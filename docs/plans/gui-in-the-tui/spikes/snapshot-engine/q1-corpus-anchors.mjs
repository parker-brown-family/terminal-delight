// Q1 across the whole corpus, plus the page heights Q3's tiling depends on.
// For every brief:
//   - page height at 968 x 1.6 (all 119 files)
//   - anchors from the brief's OWN notes.js at 968 and at 600 wide: identical?
//   - anchors from TODAY's skill notes.js run in place of the brief's own copy:
//     identical? (answers whether TD could compute anchors itself from one
//     canonical algorithm instead of running the copy inside each file)
//   - island keys that no anchor carries (an orphaned note)
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { basename } from 'node:path';
import { launch, ms, fileUrl, corpus, SCRATCH, EXTRACT_ANCHORS, PANE } from './lib.mjs';

const CURRENT_NOTES_JS = readFileSync('/home/parker/.claude/skills/decision-brief/assets/notes.js', 'utf8');
const SCRIPT_RE = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;

function stripOwnNotesJs(html) {
  return html.replace(SCRIPT_RE, (whole, attrs, body) =>
    /reader notes/i.test(body) && /function tag\(\)/.test(body) ? '<script>/* own notes.js removed */</script>' : whole);
}

const browser = await launch();
const rows = [];
for (const { path, bytes } of corpus()) {
  const html = readFileSync(path, 'utf8');
  const islandM = html.match(/<script\b[^>]*\bid="report-notes"[^>]*>([\s\S]*?)<\/script>/);
  const island = islandM ? JSON.parse(islandM[1].trim() || '{}') : null;
  const row = { file: path.replace('/home/parker/Work/', ''), bytes, hasNotes: !!island };

  const ctx = await browser.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  const page = await ctx.newPage();
  const t0 = performance.now();
  await page.goto(fileUrl(path), { waitUntil: 'load' });
  row.loadMs = ms(t0);
  const t1 = performance.now();
  const a968 = await page.evaluate(EXTRACT_ANCHORS);
  row.firstExtractMs = ms(t1);
  row.docHeightCss = await page.evaluate(() => document.documentElement.scrollHeight);
  row.docWidthCss = await page.evaluate(() => document.documentElement.scrollWidth);
  row.anchors = a968.length;
  row.anchorsInDialogs = a968.filter((a) => a.dialog).length;
  row.contentDialogs = await page.evaluate(() => Array.from(document.querySelectorAll('dialog')).filter((d) => !['d-note', 'd-export'].includes(d.id)).length);
  row.links = await page.evaluate(() => document.querySelectorAll('a[href]').length);
  if (island) {
    const ids = new Set(a968.map((a) => a.nid));
    row.islandKeys = Object.keys(island).length;
    row.orphanKeys = Object.keys(island).filter((k) => !ids.has(k));
  }
  await page.setViewportSize({ width: 600, height: PANE.height });
  // notes.js tags once at load; a resize does not re-tag, so reload at the new width.
  await page.reload({ waitUntil: 'load' });
  const a600 = await page.evaluate(EXTRACT_ANCHORS);
  row.sameIdsAt600 = JSON.stringify(a600.map((a) => a.nid)) === JSON.stringify(a968.map((a) => a.nid));
  await ctx.close();

  if (island) {
    const ctx2 = await browser.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
    const p2 = await ctx2.newPage();
    // page.route does not intercept file:// in Chromium, so the stripped copy is written
    // to scratch under the same basename (NOTES_FILE falls back to the basename).
    const strippedDir = `${SCRATCH}/stripped/${path.includes('/terminal-delight/') ? 'td' : 'reports'}`;
    mkdirSync(strippedDir, { recursive: true });
    const strippedPath = `${strippedDir}/${basename(path)}`;
    writeFileSync(strippedPath, stripOwnNotesJs(html));
    await p2.goto(fileUrl(strippedPath), { waitUntil: 'load' });
    await p2.addScriptTag({ content: CURRENT_NOTES_JS });
    const cur = await p2.evaluate(EXTRACT_ANCHORS);
    const own = a968.map((a) => a.nid), now = cur.map((a) => a.nid);
    row.currentAlgoSameIds = JSON.stringify(own) === JSON.stringify(now);
    if (!row.currentAlgoSameIds) {
      const so = new Set(own), sn = new Set(now);
      row.currentAlgoDelta = { own: own.length, current: now.length, onlyOwn: own.filter((x) => !sn.has(x)).length, onlyCurrent: now.filter((x) => !so.has(x)).length };
      if (island) row.orphanedByCurrentAlgo = Object.keys(island).filter((k) => !sn.has(k));
    }
    await ctx2.close();
  }
  rows.push(row);
  process.stderr.write('.');
}
await browser.close();
writeFileSync(`${SCRATCH}/q1-corpus.json`, JSON.stringify(rows, null, 1));

const briefs = rows.filter((r) => r.hasNotes);
const q = (arr, p) => { const s = arr.slice().sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(p * s.length))]; };
const h = rows.map((r) => r.docHeightCss);
const hd = h.map((x) => Math.ceil(x * PANE.deviceScaleFactor));
console.log('\n' + JSON.stringify({
  files: rows.length,
  briefsWithNotes: briefs.length,
  sameIdsAt600: briefs.filter((r) => r.sameIdsAt600).length,
  differentIdsAt600: briefs.filter((r) => !r.sameIdsAt600).map((r) => r.file),
  currentAlgoSameIds: briefs.filter((r) => r.currentAlgoSameIds).length,
  currentAlgoDiffers: briefs.filter((r) => !r.currentAlgoSameIds).map((r) => ({ f: r.file, ...r.currentAlgoDelta, orphaned: r.orphanedByCurrentAlgo })),
  orphanKeysUnderOwnAlgo: briefs.filter((r) => r.orphanKeys && r.orphanKeys.length).map((r) => ({ f: r.file, orphan: r.orphanKeys })),
  anchorsPerBrief: { min: q(briefs.map((r) => r.anchors), 0), median: q(briefs.map((r) => r.anchors), 0.5), max: q(briefs.map((r) => r.anchors), 0.999) },
  heightCss: { min: q(h, 0), p50: q(h, 0.5), p90: q(h, 0.9), max: q(h, 0.999) },
  heightDevicePx: { p50: q(hd, 0.5), p90: q(hd, 0.9), max: q(hd, 0.999) },
  over8192dev: hd.filter((x) => x > 8192).length,
  over16384dev: hd.filter((x) => x > 16384).length,
  over32768dev: hd.filter((x) => x > 32768).length,
  horizontalOverflow: rows.filter((r) => r.docWidthCss > PANE.width).map((r) => `${r.file} ${r.docWidthCss}`),
  loadMs: { p50: q(rows.map((r) => r.loadMs), 0.5), max: q(rows.map((r) => r.loadMs), 0.999) },
  firstExtractMs: { p50: q(rows.map((r) => r.firstExtractMs), 0.5), max: q(rows.map((r) => r.firstExtractMs), 0.999) },
  tallest: rows.slice().sort((a, b) => b.docHeightCss - a.docHeightCss).slice(0, 5).map((r) => `${r.file} ${r.docHeightCss}`),
  shortestBriefs: briefs.slice().sort((a, b) => a.docHeightCss - b.docHeightCss).slice(0, 3).map((r) => `${r.file} ${r.docHeightCss}`),
}, null, 1));
