// Q2 guard check: does the byte-level island finder pick the same element the browser
// does (document.getElementById('report-notes')), in every corpus brief? Compares the raw
// text the writer would replace with the textContent the page's notes.js reads.
import { readFileSync } from 'node:fs';
import { launch, fileUrl, corpus } from './lib.mjs';
import { findIsland } from './notes-writer.mjs';

const b = await launch(['--blink-settings=scriptEnabled=false']);
const ctx = await b.newContext({ javaScriptEnabled: false });
const page = await ctx.newPage();
let checked = 0, agree = 0;
const disagree = [];
for (const { path } of corpus()) {
  const buf = readFileSync(path);
  for (const id of ['report-notes', 'report-concurs']) {
    const isl = findIsland(buf, id);
    if (!isl) continue;
    checked++;
    const bytesText = buf.subarray(isl.start, isl.end).toString('utf8');
    await page.goto(fileUrl(path), { waitUntil: 'domcontentloaded' });
    const domText = await page.evaluate((i) => { const e = document.getElementById(i); return e ? e.textContent : null; }, id);
    if (domText === bytesText) agree++; else disagree.push(`${path} ${id}`);
  }
}
await b.close();
console.log(JSON.stringify({ islandsChecked: checked, writerAndBrowserAgree: agree, disagree }, null, 1));
