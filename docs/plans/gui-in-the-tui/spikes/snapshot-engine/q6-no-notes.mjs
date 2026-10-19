// Q6. The 26 corpus files without a report-notes island.
//   1. What are they (full pages or body fragments)?
//   2. If TD injects today's notes.js at render time (in the engine's page only, not the
//      file), how many anchors does each get?
//   3. Naive path: inject notes.js and let the page's own "save into file" run. What does
//      the saved file contain, and does it work when reopened?
//   4. Explicit conversion: insert the skill's markup + islands + notes.js before </body>
//      in a scratch copy, have TD's writer add a note, reopen: does the note show?
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { basename } from 'node:path';
import { launch, ms, fileUrl, SCRATCH, PANE, EXTRACT_ANCHORS } from './lib.mjs';
import { writeNotes, readIsland } from './notes-writer.mjs';

const NOTES_JS = readFileSync('/home/parker/.claude/skills/decision-brief/assets/notes.js', 'utf8');
const NOTES_CSS = readFileSync('/home/parker/.claude/skills/decision-brief/assets/notes.css', 'utf8');
const MARKUP = readFileSync('/home/parker/.claude/skills/decision-brief/reference/notes-markup.html', 'utf8');
const corpusRows = JSON.parse(readFileSync(`${SCRATCH}/corpus.json`, 'utf8'));
const DIR = `${SCRATCH}/q6`;
mkdirSync(DIR, { recursive: true });

const b = await launch();
const rows = [];
for (const r of corpusRows.filter((x) => !x.islands)) {
  const path = '/home/parker/Work/' + r.file;
  const s = readFileSync(path, 'utf8');
  const row = {
    file: r.file, bytes: r.bytes,
    doctype: /^\s*<!doctype html/i.test(s), htmlTag: /<html\b/i.test(s), bodyTag: /<body\b/i.test(s),
    // '_name_body.html' files are partials an agent assembles into a brief; the rest are pages
    // (some omit <html>/<body>, which HTML allows).
    fragment: basename(path).startsWith('_'),
  };
  const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  const page = await ctx.newPage();
  await page.goto(fileUrl(path), { waitUntil: 'load' });
  const t = performance.now();
  await page.addScriptTag({ content: NOTES_JS });
  const anchors = await page.evaluate(EXTRACT_ANCHORS);
  row.injectMs = ms(t);
  row.anchors = anchors.length;
  row.byPrefix = anchors.reduce((m, a) => { const k = a.nid.split('-')[0]; m[k] = (m[k] || 0) + 1; return m; }, {});
  await ctx.close();
  rows.push(row);
}

// 3. Naive path on one full page that has anchors.
const full = rows.filter((x) => !x.fragment && x.anchors > 0).sort((a, c) => c.anchors - a.anchors)[0];
let naive = null, converted = null;
if (full) {
  const path = '/home/parker/Work/' + full.file;
  const ctx = await b.newContext({ acceptDownloads: true, viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  const page = await ctx.newPage();
  await page.goto(fileUrl(path), { waitUntil: 'load' });
  await page.addScriptTag({ content: NOTES_JS });
  const anchors = await page.evaluate(EXTRACT_ANCHORS);
  // a note set the way notes.js would store it, then its own save
  await page.evaluate(([k, v]) => localStorage.setItem(k, v), ['notes:' + basename(path), JSON.stringify({ [anchors[0].nid]: [{ text: 'naive note', title: anchors[0].title, ts: '2026-09-24 20:10' }] })]);
  await page.reload({ waitUntil: 'load' });
  await page.addScriptTag({ content: NOTES_JS });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e).slice(0, 120)));
  // open() needs #note-title etc.; clicking a note button on a page without the markup:
  await page.evaluate(() => { const btn = document.querySelector('.note-btn'); if (btn) btn.click(); });
  const [dl] = await Promise.all([
    page.waitForEvent('download', { timeout: 5000 }).catch(() => null),
    page.evaluate(() => {
      // no #btn-embed on this page: fire the same delegated action notes.js listens for
      const fake = document.createElement('button');
      fake.dataset.noteAction = 'embed';
      document.body.appendChild(fake);
      fake.click();
      fake.remove();
    }),
  ]);
  let saved = null;
  if (dl) {
    const p = `${DIR}/naive-${basename(path)}`;
    await dl.saveAs(p);
    const out = readFileSync(p, 'utf8');
    saved = {
      bytesBefore: full.bytes, bytesAfter: Buffer.byteLength(out),
      hasNotesIsland: /id="report-notes"/.test(out), hasConcursIsland: /id="report-concurs"/.test(out),
      hasReaderNotesComment: out.includes('READER NOTES —'), carriesNotesJs: out.includes('function tag()'),
      noteTextOnlyInComment: out.includes('naive note') && !/id="report-notes"/.test(out),
    };
    const ctx2 = await b.newContext();
    const p2 = await ctx2.newPage();
    await p2.goto(fileUrl(p), { waitUntil: 'load' });
    saved.reopenedShowsNote = await p2.evaluate(() => document.querySelectorAll('.has-note').length > 0);
    await ctx2.close();
  }
  naive = { file: full.file, errorsWhenOpeningNoteDialog: errors, download: !!dl, saved };
  await ctx.close();

  // 4. Explicit conversion in a scratch copy.
  const copy = `${DIR}/${basename(path)}`;
  copyFileSync(path, copy);
  const orig = readFileSync(copy);
  const block = '\n<style>\n' + NOTES_CSS + '\n</style>\n' +
    MARKUP.replace("'<YYYY-MM-DD>-<topic-slug>.html'", `'${basename(path)}'`).replace('<!-- then paste the contents of assets/notes.js inline, inside a <script> tag -->', '<script>\n' + NOTES_JS + '\n</script>\n');
  const s = orig.toString('latin1');
  const at = s.toLowerCase().lastIndexOf('</body>');
  const convertedBuf = Buffer.concat([orig.subarray(0, at < 0 ? orig.length : at), Buffer.from(block, 'utf8'), orig.subarray(at < 0 ? orig.length : at)]);
  const ctx3 = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  writeFileSync(copy, convertedBuf);
  let p3 = await ctx3.newPage();
  await p3.goto(fileUrl(copy), { waitUntil: 'load' });
  const convAnchors = await p3.evaluate(EXTRACT_ANCHORS);
  const notes = { [convAnchors[0].nid]: [{ text: 'TD note in a converted page', title: convAnchors[0].title, ts: '2026-09-24 20:11' }] };
  const { out } = writeNotes(convertedBuf, notes, convAnchors, basename(path), {});
  writeFileSync(copy, out);
  await p3.close();
  const ctx4 = await b.newContext();
  p3 = await ctx4.newPage();
  await p3.goto(fileUrl(copy), { waitUntil: 'load' });
  converted = {
    bytesAdded: convertedBuf.length - orig.length, anchorsMatchInjected: convAnchors.length === full.anchors,
    reopenedShowsNote: await p3.evaluate(() => document.querySelectorAll('.has-note').length),
    noteCountBadge: await p3.evaluate(() => document.getElementById('note-count').textContent),
    islandReadsBack: JSON.stringify(readIsland(out)) === JSON.stringify(notes),
  };
  await ctx3.close(); await ctx4.close();
}
await b.close();

console.log(JSON.stringify({
  noIslandFiles: rows.length,
  fragments: rows.filter((r) => r.fragment).map((r) => r.file),
  fullPages: rows.filter((r) => !r.fragment).map((r) => ({ f: r.file, doctype: r.doctype, bodyTag: r.bodyTag, anchors: r.anchors, byPrefix: r.byPrefix, injectMs: r.injectMs })),
  fragmentAnchors: rows.filter((r) => r.fragment).map((r) => r.anchors),
  fullPagesWithZeroAnchors: rows.filter((r) => !r.fragment && r.anchors === 0).length,
  naive, converted,
}, null, 1));
