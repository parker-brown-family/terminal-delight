// Q2. Round trip: TD writes notes (and concurs) by rewriting only the islands + mirror
// comment, then the copy is reopened in Chromium with fresh storage. Then the
// localStorage precedence problem, measured rather than read off the code.
// Every file touched here is a throwaway copy under /tmp/claude-1000/td-snapshot-spike/q2.
import { readFileSync, writeFileSync, mkdirSync, copyFileSync } from 'node:fs';
import { basename } from 'node:path';
import { launch, ms, fileUrl, SAVED_EXAMPLE, SCRATCH, EXTRACT_ANCHORS, PANE } from './lib.mjs';
import { writeNotes, readIsland, untouchedOutsideEdits, findIsland } from './notes-writer.mjs';

const dir = `${SCRATCH}/q2`;
mkdirSync(dir, { recursive: true });
const browser = await launch();
const results = {};

async function openFresh(path, storage /* [[key, value], ...] */) {
  const ctx = await browser.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  if (storage) {
    await ctx.addInitScript((pairs) => {
      if (!sessionStorage.getItem('seeded')) { for (const [k, v] of pairs) localStorage.setItem(k, v); sessionStorage.setItem('seeded', '1'); }
    }, storage);
  }
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  await page.goto(fileUrl(path), { waitUntil: 'load' });
  return { ctx, page, errors };
}

const STATE = () => ({
  total: document.getElementById('note-count') ? document.getElementById('note-count').textContent : null,
  concurCount: document.getElementById('concur-count') ? document.getElementById('concur-count').textContent : null,
  withNote: Array.from(document.querySelectorAll('.notable.has-note')).map((el) => ({
    nid: el.dataset.nid, badge: (el.querySelector(':scope > .note-btn') || {}).textContent,
  })),
  concurred: Array.from(document.querySelectorAll('.has-concur')).map((el) => ({
    nid: el.dataset.nid, stamp: !!el.querySelector('.concur-stamp'),
  })),
  concurZones: document.querySelectorAll('.concur-zone').length,
});

async function anchorsOf(path) {
  const { page, ctx } = await openFresh(path);
  const a = await page.evaluate(EXTRACT_ANCHORS);
  await ctx.close();
  return a;
}

// ---- A. The saved example (older notes.js, no concurs): add a note, add one inside a
//         modal, delete one, keep the rest.
{
  const copy = `${dir}/${basename(SAVED_EXAMPLE)}`;
  copyFileSync(SAVED_EXAMPLE, copy);
  const orig = readFileSync(copy);
  const anchors = await anchorsOf(copy);
  const notes = readIsland(orig);
  const firstFree = anchors.find((a) => !notes[a.nid] && !a.dialog);
  const inDialog = anchors.find((a) => a.dialog);
  notes[firstFree.nid] = [{ text: 'TD wrote this one', title: firstFree.title, ts: '2026-09-24 20:00' }];
  notes[inDialog.nid] = [{ text: 'TD note on an element inside a modal', title: inDialog.title, ts: '2026-09-24 20:01' }];
  delete notes['ask-3-should-a-document'];
  const t0 = performance.now();
  const { out, edits } = writeNotes(orig, notes, anchors, '2026-09-24-gui-in-the-tui.html', null);
  const writeMs = ms(t0);
  writeFileSync(copy, out);
  const r = await openFresh(copy);
  const st = await r.page.evaluate(STATE);
  await r.ctx.close();
  results.A_savedExample = {
    origBytes: orig.length, newBytes: out.length, writeMs, edits: edits.map((e) => e.what),
    untouchedOutsideEdits: untouchedOutsideEdits(orig, out, edits),
    keysWritten: Object.keys(notes).length, noteCountBadge: st.total, elementsWithBadge: st.withNote.length,
    pageErrors: r.errors,
    newAnchorShown: st.withNote.some((x) => x.nid === firstFree.nid),
    dialogAnchorShown: st.withNote.some((x) => x.nid === inDialog.nid),
    deletedGone: !st.withNote.some((x) => x.nid === 'ask-3-should-a-document'),
    added: [firstFree.nid, inDialog.nid],
    concurZonesInThisBrief: st.concurZones,
  };
}

// ---- B. A pristine brief (empty island, no mirror comment) + hostile note text.
{
  const src = '/home/parker/Work/terminal-delight/reports/2026-09-11-tab-strip-code-review.html';
  const copy = `${dir}/${basename(src)}`;
  copyFileSync(src, copy);
  const orig = readFileSync(copy);
  const anchors = await anchorsOf(copy);
  const hostile = 'ends a script </script><b>LEAKED-SCRIPT</b> and a comment --> LEAKED-COMMENT <!--<script> x; and ünïcødé 🎯\nsecond line';
  const notes = { [anchors[0].nid]: [{ text: hostile, title: anchors[0].title, ts: '2026-09-24 20:02' }] };
  const { out, edits } = writeNotes(orig, notes, anchors, basename(src), null);
  writeFileSync(copy, out);
  const r = await openFresh(copy);
  const st = await r.page.evaluate(STATE);
  const roundTripText = await r.page.evaluate((nid) => JSON.parse(document.getElementById('report-notes').textContent)[nid][0].text, anchors[0].nid);
  const leaked = await r.page.evaluate(() => /LEAKED/.test(document.body.innerText));
  await r.ctx.close();
  results.B_pristineHostile = {
    edits: edits.map((e) => e.what), untouchedOutsideEdits: untouchedOutsideEdits(orig, out, edits),
    badges: st.withNote, pageErrors: r.errors, hostileTextRoundTrips: roundTripText === hostile, leakedIntoPage: leaked,
  };
}

// ---- E. Concurs round trip, on the one brief whose notes.js has concurs.
const CONCUR_SRC = '/home/parker/Work/reports/2026-09-24-concur-stamp.html';
const concurCopy = `${dir}/${basename(CONCUR_SRC)}`;
{
  copyFileSync(CONCUR_SRC, concurCopy);
  const orig = readFileSync(concurCopy);
  const anchors = await anchorsOf(concurCopy);
  const decisions = anchors.filter((a) => a.concurrable);
  const notes = { [decisions[1].nid]: [{ text: 'a note beside a concur', title: decisions[1].title, ts: '2026-09-24 20:03' }] };
  const concurs = { [decisions[0].nid]: '2026-09-24 20:03', [decisions[1].nid]: '2026-09-24 20:04' };
  const { out, edits } = writeNotes(orig, notes, anchors, basename(CONCUR_SRC), concurs);
  writeFileSync(concurCopy, out);
  const r = await openFresh(concurCopy);
  const st = await r.page.evaluate(STATE);
  await r.ctx.close();
  const mirror = out.toString('utf8').match(/<!--\nREADER NOTES —\n([\s\S]*?)\n-->/)[1];
  results.E_concurRoundTrip = {
    decisionsInBrief: decisions.length, concurrableAnchors: decisions.map((d) => d.nid),
    edits: edits.map((e) => e.what), untouchedOutsideEdits: untouchedOutsideEdits(orig, out, edits),
    concurredShown: st.concurred, concurCountBadge: st.concurCount, noteCountBadge: st.total,
    rereadConcurs: readIsland(out, 'report-concurs'), pageErrors: r.errors,
    mirrorHead: mirror.split('\n').slice(0, 2), mirrorConcurLines: mirror.split('\n').filter((l) => l.includes('✓ concur')),
  };
}

// ---- F. Concurs written into a brief whose notes.js predates them: does anything show?
{
  const copy = `${dir}/F-${basename(SAVED_EXAMPLE)}`;
  copyFileSync(SAVED_EXAMPLE, copy);
  const orig = readFileSync(copy);
  const anchors = await anchorsOf(copy);
  const ask = anchors.find((a) => a.nid.startsWith('ask-'));
  const { out, edits } = writeNotes(orig, readIsland(orig), anchors, '2026-09-24-gui-in-the-tui.html', { [ask.nid]: '2026-09-24 20:05' });
  writeFileSync(copy, out);
  const r = await openFresh(copy);
  const st = await r.page.evaluate(STATE);
  await r.ctx.close();
  results.F_concurIntoOldBrief = {
    edits: edits.map((e) => e.what), concurZones: st.concurZones, concurredShown: st.concurred.length,
    notesStillShown: st.withNote.length, pageErrors: r.errors,
  };
}

// ---- C. localStorage precedence: the TD-written copy, opened in a context that already
//         holds an older map for the same filename.
{
  const copy = `${dir}/2026-09-24-gui-in-the-tui.html`; // the TD-written copy from A
  const key = 'notes:2026-09-24-gui-in-the-tui.html';
  const older = JSON.stringify({ 'fig-02-two-ways-a': [{ text: 'OLDER BROWSER NOTE', title: 'x', ts: '2026-09-20 10:00' }] });
  const cases = {};
  for (const [name, stored] of [['olderMapInStorage', older], ['emptyMapInStorage', '{}'], ['nullInStorage', 'null']]) {
    const r = await openFresh(copy, [[key, stored]]);
    const st = await r.page.evaluate(STATE);
    cases[name] = { notesShownOn: st.withNote.map((x) => x.nid), total: st.total };
    await r.ctx.close();
  }
  // Concurs follow the same rule under their own key.
  const ckey = 'concurs:' + basename(CONCUR_SRC);
  for (const [name, stored] of [['concursOlderInStorage', JSON.stringify({ 'nobody-home': '2026-09-01 09:00' })], ['concursEmptyInStorage', '{}']]) {
    const r = await openFresh(concurCopy, [[ckey, stored]]);
    const st = await r.page.evaluate(STATE);
    cases[name] = { concurredShown: st.concurred.map((x) => x.nid), concurCount: st.concurCount };
    await r.ctx.close();
  }
  results.C_precedence = cases;

  // Does localStorage cross directories? Two copies with the same basename in different dirs.
  mkdirSync(`${dir}/other-dir`, { recursive: true });
  copyFileSync(copy, `${dir}/other-dir/2026-09-24-gui-in-the-tui.html`);
  const ctx = await browser.newContext();
  const p = await ctx.newPage();
  await p.goto(fileUrl(copy));
  await p.evaluate(([k, v]) => localStorage.setItem(k, v), [key, older]);
  await p.goto(fileUrl(`${dir}/other-dir/2026-09-24-gui-in-the-tui.html`));
  const seen = await p.evaluate((k) => localStorage.getItem(k), key);
  const origin = await p.evaluate(() => location.origin);
  await ctx.close();
  results.C_fileOriginSharing = { sameBasenameOtherDirSeesStorage: seen === older, origin };
}

// ---- D. Shapes: the two-island brief.
{
  const buf = readFileSync('/home/parker/Work/reports/2026-09-15-one-terminal-one-pane.html');
  results.D_twoIslands = { islandsInFile: findIsland(buf).count, writerTargets: 'first — the element getElementById and querySelector return' };
}
await browser.close();
console.log(JSON.stringify(results, null, 1));
