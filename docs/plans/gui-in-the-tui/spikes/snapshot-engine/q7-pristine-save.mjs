// Q7c. How much does the page's own "save into file" rewrite an AUTHORED brief (one never
// saved before)? The saved example had already been through one save, so its second save
// looked small. Here: one note added through the page UI on pristine copies, then the
// page's save, compared with TD's two-span write of the same notes.
import { readFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { basename } from 'node:path';
import { launch, fileUrl, SCRATCH, PANE, EXTRACT_ANCHORS } from './lib.mjs';
import { writeNotes } from './notes-writer.mjs';

const FILES = [
  '/home/parker/Work/reports/2026-09-15-attention-levels.html',
  '/home/parker/Work/terminal-delight/reports/2026-09-21-bench-tenancy.html',
  '/home/parker/Work/reports/2026-09-24-concur-stamp.html',
];
const DIR = `${SCRATCH}/q7-pristine`;
mkdirSync(DIR, { recursive: true });
const b = await launch();
for (const src of FILES) {
  const copy = `${DIR}/${basename(src)}`;
  copyFileSync(src, copy);
  const orig = readFileSync(copy);
  const ctx = await b.newContext({ acceptDownloads: true, viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  const page = await ctx.newPage();
  await page.goto(fileUrl(copy), { waitUntil: 'load' });
  await page.locator('.notable > .note-btn').first().click();
  await page.fill('#note-text', 'one note');
  await page.click('[data-note-action="add"]');
  await page.click('#d-note [data-close]');
  const [dl] = await Promise.all([page.waitForEvent('download'), page.click('#btn-embed')]);
  const saved = `${DIR}/downloaded-${basename(src)}`;
  await dl.saveAs(saved);
  const out = readFileSync(saved);
  const key = await page.evaluate(() => Object.keys(localStorage).find((k) => k.startsWith('notes:')));
  const N = await page.evaluate((k) => JSON.parse(localStorage.getItem(k)), key);
  const anchors = await page.evaluate(EXTRACT_ANCHORS);
  await ctx.close();
  const td = writeNotes(orig, N, anchors, basename(src), null);
  const a = orig.toString('utf8').split('\n'), c = out.toString('utf8').split('\n');
  const setA = new Set(a);
  const changedOrNew = c.filter((l) => !setA.has(l)).length;
  const o = orig.toString('utf8'), s = out.toString('utf8');
  console.log(JSON.stringify({
    file: basename(src),
    bytes: { original: orig.length, pageSave: out.length, tdWrite: td.out.length },
    pageSaveLinesNotInOriginal: changedOrNew, pageSaveLines: c.length, originalLines: a.length,
    dataNidAttrs: { original: (o.match(/data-nid=/g) || []).length, pageSave: (s.match(/data-nid=/g) || []).length },
    emptyClassAttrs: { original: (o.match(/class=""/g) || []).length, pageSave: (s.match(/class=""/g) || []).length },
  }));
}
await b.close();
