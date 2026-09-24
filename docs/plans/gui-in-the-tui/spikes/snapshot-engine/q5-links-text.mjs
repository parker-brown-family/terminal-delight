// Q5 across the corpus, plus three things the image engine needs to get right that
// turned up while answering Q1-Q4:
//   links   — a[href] and [data-dlg] openers with page rects, so TD can route a click
//   text    — the text of every anchored element (for TD's note dialog), and every visible
//             text block with its rect (for copy later): count, bytes, time
//   fonts   — does page height move between 'load' and document.fonts.ready? (anchors
//             must be measured on the same layout the image was captured from)
//   chrome  — the page's own notes UI (fixed notebar, badge buttons, concur zones) is baked
//             into a screenshot; does hiding it with visibility:hidden move any anchor?
//   fixed   — other position:fixed/sticky elements, which a full-page capture misplaces
//   dialogs — how many content dialogs scroll inside at a 1400 CSS px viewport
import { writeFileSync } from 'node:fs';
import { launch, ms, fileUrl, corpus, SCRATCH, EXTRACT_ANCHORS, PANE } from './lib.mjs';

const HIDE_NOTES_CHROME = '.notebar, .note-btn, .concur-zone { visibility: hidden !important; }';

const LINKS = () => {
  const sx = window.scrollX, sy = window.scrollY;
  const rect = (el) => { const r = el.getBoundingClientRect(); return { x: r.x + sx, y: r.y + sy, w: r.width, h: r.height }; };
  const kind = (href) => /^https?:/i.test(href) ? 'http' : href.startsWith('#') ? 'fragment' : /^file:/i.test(href) ? 'file' : /^mailto:/i.test(href) ? 'mailto' : 'relative';
  return {
    links: Array.from(document.querySelectorAll('a[href]')).map((a) => ({ href: a.getAttribute('href'), kind: kind(a.getAttribute('href')), rect: rect(a), inDialog: !!a.closest('dialog'), text: a.textContent.trim().slice(0, 60) })),
    openers: Array.from(document.querySelectorAll('[data-dlg]')).map((b) => ({ dialog: b.dataset.dlg, rect: rect(b), inDialog: !!b.closest('dialog') })),
  };
};

const ANCHOR_TEXT = () => Array.from(document.querySelectorAll('.notable')).map((el) => {
  // innerText of the element minus the note button's glyph/count
  const t = el.innerText.replace(/\s+\n/g, '\n').trim();
  return { nid: el.dataset.nid, chars: t.length };
});

const TEXT_BLOCKS = () => {
  const sx = window.scrollX, sy = window.scrollY;
  const sel = 'h1,h2,h3,h4,h5,p,li,td,th,pre,figcaption,blockquote,dt,dd,summary,label';
  const out = [];
  for (const el of document.querySelectorAll(sel)) {
    if (el.closest('dialog:not([open])')) continue;          // closed modals are not on screen
    if (el.parentElement && el.parentElement.closest(sel)) continue; // outermost block only
    const r = el.getBoundingClientRect();
    if (!r.width || !r.height) continue;
    const t = el.innerText.trim();
    if (!t) continue;
    out.push({ y: r.y + sy, x: r.x + sx, w: r.width, h: r.height, t });
  }
  return { blocks: out.length, bytes: out.reduce((a, b) => a + b.t.length, 0) };
};

const FIXED = () => Array.from(document.querySelectorAll('body *')).filter((el) => {
  if (el.closest('dialog')) return false;
  const p = getComputedStyle(el).position;
  return p === 'fixed' || p === 'sticky';
}).map((el) => `${el.tagName.toLowerCase()}${el.id ? '#' + el.id : ''}${el.className && typeof el.className === 'string' ? '.' + el.className.split(' ')[0] : ''}:${getComputedStyle(el).position}`);

const b = await launch();
const rows = [];
for (const { path } of corpus()) {
  const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  const page = await ctx.newPage();
  await page.goto(fileUrl(path), { waitUntil: 'load' });
  const row = { file: path.replace('/home/parker/Work/', '') };
  const hLoad = await page.evaluate(() => document.documentElement.scrollHeight);
  await page.evaluate(() => document.fonts.ready);
  const hFonts = await page.evaluate(() => document.documentElement.scrollHeight);
  row.heightLoad = hLoad; row.heightFontsReady = hFonts; row.fontShiftCss = hFonts - hLoad;

  let t = performance.now();
  const L = await page.evaluate(LINKS);
  row.linksMs = ms(t);
  row.links = L.links.length; row.openers = L.openers.length;
  row.linkKinds = L.links.reduce((m, l) => { m[l.kind] = (m[l.kind] || 0) + 1; return m; }, {});
  row.linksInDialogs = L.links.filter((l) => l.inDialog).length;

  t = performance.now();
  const A = await page.evaluate(ANCHOR_TEXT);
  row.anchorTextMs = ms(t);
  row.anchorTextChars = A.reduce((a, x) => a + x.chars, 0);

  t = performance.now();
  const T = await page.evaluate(TEXT_BLOCKS);
  row.textBlocksMs = ms(t);
  Object.assign(row, { textBlocks: T.blocks, textBytes: T.bytes });

  row.fixedOrSticky = await page.evaluate(FIXED);

  const before = await page.evaluate(EXTRACT_ANCHORS);
  await page.addStyleTag({ content: HIDE_NOTES_CHROME });
  const after = await page.evaluate(EXTRACT_ANCHORS);
  row.hideChromeMovedAnchors = before.filter((a, i) => {
    const b2 = after[i];
    return !b2 || Math.abs(a.rect.y - b2.rect.y) > 0.5 || Math.abs(a.rect.h - b2.rect.h) > 0.5 || Math.abs(a.rect.x - b2.rect.x) > 0.5;
  }).length;

  // dialogs that scroll inside at this viewport
  row.dialogOverflow = await page.evaluate(() => {
    let n = 0, total = 0;
    for (const d of document.querySelectorAll('dialog')) {
      if (['d-note', 'd-export'].includes(d.id)) continue;
      total++;
      try { d.showModal(); } catch { continue; }
      const scrolls = [d, ...d.querySelectorAll('*')].some((el) => el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== 'visible');
      if (scrolls) n++;
      d.close();
    }
    return { total, scrolling: n };
  });
  await ctx.close();
  rows.push(row);
  process.stderr.write('.');
}
await b.close();
writeFileSync(`${SCRATCH}/q5-corpus.json`, JSON.stringify(rows, null, 1));

const q = (arr, p) => { const s = arr.slice().sort((a, c) => a - c); return s[Math.min(s.length - 1, Math.floor(p * s.length))]; };
const sum = (f) => rows.reduce((a, r) => a + f(r), 0);
const kinds = rows.reduce((m, r) => { for (const [k, v] of Object.entries(r.linkKinds)) m[k] = (m[k] || 0) + v; return m; }, {});
const fixedTally = rows.flatMap((r) => r.fixedOrSticky).reduce((m, k) => { m[k] = (m[k] || 0) + 1; return m; }, {});
console.log('\n' + JSON.stringify({
  files: rows.length,
  links: { total: sum((r) => r.links), filesWithAny: rows.filter((r) => r.links).length, kinds, inDialogs: sum((r) => r.linksInDialogs), maxPerFile: q(rows.map((r) => r.links), 0.999) },
  dataDlgOpeners: { total: sum((r) => r.openers), filesWithAny: rows.filter((r) => r.openers).length },
  linksMs: { p50: q(rows.map((r) => r.linksMs), 0.5), max: q(rows.map((r) => r.linksMs), 0.999) },
  anchorTextMs: { p50: q(rows.map((r) => r.anchorTextMs), 0.5), max: q(rows.map((r) => r.anchorTextMs), 0.999) },
  textBlocks: { p50: q(rows.map((r) => r.textBlocks), 0.5), max: q(rows.map((r) => r.textBlocks), 0.999) },
  textBytes: { p50: q(rows.map((r) => r.textBytes), 0.5), max: q(rows.map((r) => r.textBytes), 0.999) },
  textBlocksMs: { p50: q(rows.map((r) => r.textBlocksMs), 0.5), max: q(rows.map((r) => r.textBlocksMs), 0.999) },
  fontShift: { filesWhereHeightMoved: rows.filter((r) => r.fontShiftCss !== 0).length, maxCss: q(rows.map((r) => Math.abs(r.fontShiftCss)), 0.999), examples: rows.filter((r) => r.fontShiftCss).slice(0, 4).map((r) => `${r.file} ${r.fontShiftCss}`) },
  hideNotesChrome: { filesWhereAnAnchorMoved: rows.filter((r) => r.hideChromeMovedAnchors).length },
  fixedOrSticky: fixedTally,
  filesWithFixedOrStickyBesidesNotebar: rows.filter((r) => r.fixedOrSticky.some((k) => !k.includes('notebar'))).length,
  dialogs: { total: sum((r) => r.dialogOverflow.total), scrollingInsideAt1400: sum((r) => r.dialogOverflow.scrolling) },
}, null, 1));
