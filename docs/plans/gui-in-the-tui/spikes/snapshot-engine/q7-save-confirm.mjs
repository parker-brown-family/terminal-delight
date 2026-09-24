// Q7b. Could a live engine delegate saving to the page? Drive notes.js's own "save into
// file" on a scratch copy of Parker's saved brief and intercept Playwright's download event.
// Then: what the page's own save does to the bytes, what it does with hostile note text,
// and what happens on confirm() (the "delete every note" button).
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { basename } from 'node:path';
import { launch, ms, fileUrl, SCRATCH, SAVED_EXAMPLE, PANE, EXTRACT_ANCHORS } from './lib.mjs';
import { islandJson, findIsland, readIsland, writeNotes } from './notes-writer.mjs';

const DIR = `${SCRATCH}/q7`;
mkdirSync(DIR, { recursive: true });
const copy = `${DIR}/${basename(SAVED_EXAMPLE)}`;
copyFileSync(SAVED_EXAMPLE, copy);
const orig = readFileSync(copy);
const b = await launch();
const res = {};
const CTX = { acceptDownloads: true, viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor };

function lineDelta(a, c) {
  const count = (s) => s.split('\n').reduce((m, l) => { m.set(l, (m.get(l) || 0) + 1); return m; }, new Map());
  const A = count(a), C = count(c);
  let removed = 0, added = 0;
  const samplesRemoved = [], samplesAdded = [];
  for (const [l, n] of A) { const d = n - (C.get(l) || 0); if (d > 0) { removed += d; if (samplesRemoved.length < 6) samplesRemoved.push(l.trim().slice(0, 110)); } }
  for (const [l, n] of C) { const d = n - (A.get(l) || 0); if (d > 0) { added += d; if (samplesAdded.length < 6) samplesAdded.push(l.trim().slice(0, 110)); } }
  return { linesBefore: a.split('\n').length, linesAfter: c.split('\n').length, removed, added, samplesRemoved, samplesAdded };
}

// ---- 1. Add a note through the page's own UI, then its own save, intercepted.
{
  const ctx = await b.newContext(CTX);
  const page = await ctx.newPage();
  await page.goto(fileUrl(copy), { waitUntil: 'load' });
  await page.locator('[data-nid="fig-01-what-a-ctrl"] > .note-btn').click();
  await page.fill('#note-text', 'added through the page UI');
  await page.click('[data-note-action="add"]');
  await page.click('#d-note [data-close]');
  const t0 = performance.now();
  const [dl] = await Promise.all([page.waitForEvent('download'), page.click('#btn-embed')]);
  const saved = `${DIR}/downloaded-${basename(copy)}`;
  await dl.saveAs(saved);
  const saveMs = ms(t0);
  const out = readFileSync(saved);
  const N = await page.evaluate(() => JSON.parse(localStorage.getItem('notes:2026-09-24-gui-in-the-tui.html')));
  const anchors = await page.evaluate(EXTRACT_ANCHORS);
  const pageIsland = out.subarray(findIsland(out).start, findIsland(out).end).toString('utf8');
  const td = writeNotes(orig, N, anchors, '2026-09-24-gui-in-the-tui.html', null);
  res.pageSave = {
    suggestedFilename: dl.suggestedFilename(), downloadMs: saveMs,
    bytesOriginal: orig.length, bytesDownloaded: out.length,
    fullDocument: out.toString('utf8').startsWith('<!DOCTYPE html>') && out.includes('function tag()') && (out.toString('utf8').match(/<dialog\b/g) || []).length === (orig.toString('utf8').match(/<dialog\b/g) || []).length,
    readerNotesCommentsBefore: (orig.toString('utf8').match(/READER NOTES —/g) || []).length,
    readerNotesCommentsAfter: (out.toString('utf8').match(/READER NOTES —/g) || []).length,
    concursIslandCreated: !findIsland(orig, 'report-concurs') && !!findIsland(out, 'report-concurs'),
    islandBytesEqualTdWriter: pageIsland === islandJson(N),
    wholeFileEqualTdWriter: out.equals(td.out),
    tdWriterBytes: td.out.length,
    lineDeltaVsOriginal: lineDelta(orig.toString('utf8'), out.toString('utf8')),
  };
  await ctx.close();

  // a second save, starting from the downloaded file
  mkdirSync(`${DIR}/second`, { recursive: true });
  const second = `${DIR}/second/${basename(copy)}`;
  copyFileSync(saved, second);
  const ctx2 = await b.newContext(CTX);
  const p2 = await ctx2.newPage();
  await p2.goto(fileUrl(second), { waitUntil: 'load' });
  const [dl2] = await Promise.all([p2.waitForEvent('download'), p2.click('#btn-embed')]);
  const saved2 = `${DIR}/second/downloaded-${basename(copy)}`;
  await dl2.saveAs(saved2);
  const out2 = readFileSync(saved2, 'utf8');
  res.secondPageSave = { readerNotesComments: (out2.match(/READER NOTES —/g) || []).length, bytes: Buffer.byteLength(out2), lineDeltaVsFirstSave: lineDelta(out.toString('utf8'), out2) };
  await ctx2.close();
}

// ---- 2. Hostile note text through the page's own save.
{
  const hostile = 'ends a script </script><b>LEAKED-SCRIPT</b> and a comment --> LEAKED-COMMENT';
  const ctx = await b.newContext(CTX);
  await ctx.addInitScript(([k, v]) => { if (!sessionStorage.getItem('s')) { localStorage.setItem(k, v); sessionStorage.setItem('s', '1'); } },
    ['notes:2026-09-24-gui-in-the-tui.html', JSON.stringify({ 'fig-02-two-ways-a': [{ text: hostile, title: '02', ts: '2026-09-24 20:20' }] })]);
  const page = await ctx.newPage();
  await page.goto(fileUrl(copy), { waitUntil: 'load' });
  const [dl] = await Promise.all([page.waitForEvent('download'), page.click('#btn-embed')]);
  const saved = `${DIR}/hostile-${basename(copy)}`;
  await dl.saveAs(saved);
  await ctx.close();
  const ctx2 = await b.newContext(CTX);
  const p2 = await ctx2.newPage();
  const errors = [];
  p2.on('pageerror', (e) => errors.push(String(e).slice(0, 140)));
  await p2.goto(fileUrl(saved), { waitUntil: 'load' });
  res.pageSaveHostile = {
    leakedIntoVisiblePage: await p2.evaluate(() => /LEAKED/.test(document.body.innerText)),
    notesShown: await p2.evaluate(() => document.querySelectorAll('.has-note').length),
    anchorsTagged: await p2.evaluate(() => document.querySelectorAll('.notable').length),
    pageErrors: errors,
  };
  await ctx2.close();
}

// ---- 3. confirm() on the clear button.
{
  const ctx = await b.newContext(CTX);
  const page = await ctx.newPage();
  await page.goto(fileUrl(copy), { waitUntil: 'load' });
  // a. Playwright's default: no dialog listener -> auto-dismiss
  await page.click('#btn-clear');
  const afterDefault = await page.evaluate(() => document.getElementById('note-count').textContent);
  // b. a listener that does not answer: is the page's main thread blocked?
  let dialog = null;
  page.on('dialog', (d) => { dialog = d; });
  await page.evaluate(() => setTimeout(() => document.getElementById('btn-clear').click(), 0));
  const t0 = performance.now();
  const probe = await Promise.race([page.evaluate(() => 'answered').catch((e) => 'error'), new Promise((r) => setTimeout(() => r('blocked'), 2000))]);
  const blockedMs = ms(t0);
  const info = dialog ? { type: dialog.type(), message: dialog.message() } : null;
  if (dialog) await dialog.accept();
  const afterAccept = await page.evaluate(() => ({ count: document.getElementById('note-count').textContent, stored: localStorage.getItem('notes:2026-09-24-gui-in-the-tui.html') }));
  await ctx.close();
  // c. reopen the untouched file in the same context: do the baked notes come back?
  const ctx2 = await b.newContext(CTX);
  const p2 = await ctx2.newPage();
  await p2.goto(fileUrl(copy));
  await p2.evaluate(() => localStorage.setItem('notes:2026-09-24-gui-in-the-tui.html', '{}'));
  await p2.reload({ waitUntil: 'load' });
  const afterReload = await p2.evaluate(() => document.getElementById('note-count').textContent);
  await ctx2.close();
  res.confirm = { autoDismissedByDefault: afterDefault, dialog: info, evaluateWhileDialogUp: probe, waitedMs: blockedMs, afterAccept, bakedNotesAfterClearAndReload: afterReload };
}
await b.close();
console.log(JSON.stringify(res, null, 1));
