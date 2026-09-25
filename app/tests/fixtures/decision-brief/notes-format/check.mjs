// Holds every case to two writers at once: the reference writer (writer.mjs) and the
// brief's own notes.js running in Chromium.
//
//   node check.mjs                  check every case; exit 1 on any failure
//   node check.mjs --update         re-derive anchors.json, expect.json, expected.html and
//                                   expected-map.txt from brief.html and edits.json
//   node check.mjs --case <name>    one case
//   node check.mjs --notes-js <rev> run the page-driven checks of this release's cases
//                                   with another notes.js inlined, e.g. 250188f, to show
//                                   what an older release gets wrong
//
// For each case:
//   1. The page: brief.html in a fresh profile gives the anchors (with each stamp's pose),
//      NOTES_FILE, and the notes island's text as the browser reads it.
//   2. The writer: edits.json applied to brief.html's bytes gives expected.html, or the
//      refusal expect.json names. Every byte outside the three regions is untouched, and
//      writing expected.html again with no edits changes nothing.
//   3. The written file, reopened fresh: its badges, stamps and counts, and "copy map"
//      gives expected-map.txt, which is also the text of its READER NOTES comment.
//   4. The page's own save: the same edits made through the page's buttons, then
//      💾 save into file. The downloaded islands and mirror equal the writer's, and the
//      download reopens to the same notes.
import { readFileSync, writeFileSync, existsSync, readdirSync, rmSync, mkdtempSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { HERE, SKILL, release } from './brief.mjs';
import { planWrite, verify, readIslands, findIsland, findNotesJs, attr, mirrorComment, Refusal } from './writer.mjs';
import { launch, openPage, EXTRACT, STATE, stampPoses, applyEdits, saveIntoFile, copyMap, tsDate } from './browser.mjs';

const CASES_DIR = join(HERE, 'cases');
const MARK = '<!--\nREADER NOTES —';

export function mirrors(buf) {
  const out = [];
  for (let at = buf.indexOf(MARK); at >= 0; at = buf.indexOf(MARK, at + 1)) {
    const end = buf.indexOf('-->', at + 4);
    out.push(buf.subarray(at, end < 0 ? buf.length : end + 3));
  }
  return out;
}
function island(buf, id) {
  const i = findIsland(buf, id);
  return i ? { element: buf.subarray(i.tagStart, i.closeEnd), inner: buf.subarray(i.start, i.end) } : null;
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const show = (b) => JSON.stringify(b === null || b === undefined ? b : b.toString('utf8')).slice(0, 240);

async function checkCase(browser, name, { update, notesJs }) {
  const dir = join(CASES_DIR, name);
  const res = [];
  const ok = (cond, label, detail = '') => res.push({ ok: !!cond, label, detail: cond ? '' : detail });
  const brief = readFileSync(join(dir, 'brief.html'));
  const { rev, edits } = JSON.parse(readFileSync(join(dir, 'edits.json'), 'utf8'));
  const expectPath = join(dir, 'expect.json');
  const prior = JSON.parse(readFileSync(expectPath, 'utf8'));
  const current = prior.release === 'current';
  const assetsJs = join(SKILL, 'assets', 'notes.js');

  // The page-driven steps read copies, so a substituted notes.js never touches the case.
  const scratch = mkdtempSync(join(tmpdir(), `notes-format-${name}-`));
  const swap = (buf) => {
    if (!notesJs || !current) return buf;
    const mine = readFileSync(assetsJs, 'utf8');
    const s = buf.toString('utf8');
    // A download from a swapped page already carries the other release.
    return s.includes(mine) ? Buffer.from(s.replace(mine, () => release(notesJs).js), 'utf8') : buf;
  };
  const pageFile = (buf, as) => { const p = join(scratch, as); writeFileSync(p, swap(buf)); return 'file://' + p; };

  if (current && !notesJs && existsSync(assetsJs)) {
    for (const f of ['notes.js', 'notes.css']) {
      ok(brief.includes(readFileSync(join(SKILL, 'assets', f))), `brief.html inlines this skill's assets/${f} verbatim`,
        `${f} changed since the fixtures were built: run build.mjs, which rebuilds them`);
    }
  }

  // 1. The page.
  const hasJs = !!findNotesJs(brief);
  const url = pageFile(brief, 'brief.html');
  let a = await openPage(browser, url);
  const anchors = await a.page.evaluate(EXTRACT);
  const label = hasJs ? await a.page.evaluate(() => window.NOTES_FILE || (location.pathname.split('/').pop() || 'decision-brief.html')) : null;
  const notesText = await a.page.evaluate(() => { const e = document.getElementById('report-notes'); return e ? e.textContent : null; });
  const before = await a.page.evaluate(STATE);
  ok(!a.errors.length, 'brief.html loads without a page error', a.errors.join(' | '));
  await a.ctx.close();
  a = await openPage(browser, url);
  const poses = await stampPoses(a.page);
  await a.ctx.close();
  const anchorsJson = anchors.map((x) => ({ ...x, stamp: x.concurrable ? poses[x.nid] : null }));
  const isl = findIsland(brief, 'report-notes');
  const jsText = hasJs ? brief.subarray(findNotesJs(brief).start, findNotesJs(brief).end).toString('utf8') : '';
  const concurSupport = hasJs ? (jsText.includes('report-concurs') ? 'supported' : 'unsupported') : null;

  // 2. The writer.
  let plan = null, refuse = null;
  try { plan = planWrite(brief, edits, { anchors, label, concurSupport, rev, path: 'brief.html' }); }
  catch (e) { if (!(e instanceof Refusal)) throw e; refuse = e.kind; }
  const expected = plan ? plan.out : null;
  if (plan) {
    const v = verify(brief, plan);
    ok(v === null, 'writer: only the three regions changed, and the islands re-parse to the maps written', v);
    const again = planWrite(expected, [], { anchors, label, concurSupport, rev, path: 'brief.html' }).out;
    ok(again.equals(expected), 'writer: writing expected.html back with no edits changes no byte');
    const titles = Object.fromEntries(anchors.map((x) => [x.nid, x.title]));
    const bad = edits.filter((e) => e.op === 'add' && titles[e.nid] !== e.title);
    ok(!bad.length, 'edits.json: every added note carries its anchor\'s title', JSON.stringify(bad));
  }

  // 3. The written file, reopened.
  let shown = null, mapText = null;
  if (plan) {
    const c = await openPage(browser, pageFile(expected, 'expected.html'));
    shown = await c.page.evaluate(STATE);
    const back = await c.page.evaluate(() => JSON.parse(document.getElementById('report-notes').textContent || '{}'));
    ok(same(back, plan.notes), 'expected.html: the browser parses the notes island to the notes written', JSON.stringify(back));
    mapText = await copyMap(c.page);
    ok(!c.errors.length, 'expected.html loads without a page error', c.errors.join(' | '));
    await c.ctx.close();
    const last = mirrors(expected).pop();
    ok(last && last.equals(Buffer.from(mirrorComment(mapText), 'utf8')),
      'expected.html: its last READER NOTES comment is "copy map", made comment-safe', show(last));
  }

  // 4. The page's own save.
  if (plan) {
    const firstTs = edits.find((e) => e.ts);
    const d = await openPage(browser, url, { clock: firstTs ? tsDate(firstTs.ts) : rev });
    await applyEdits(d.page, edits);
    const { bytes: saved, suggested } = await saveIntoFile(d.page, rev);
    ok(!d.errors.length, 'page save: no page error while editing and saving', d.errors.join(' | '));
    await d.ctx.close();
    ok(suggested === label, 'page save: the download is named after NOTES_FILE', suggested);
    const stamping = plan.rev !== null;
    const W = { n: island(expected, 'report-notes'), c: island(expected, 'report-concurs') };
    const B = { n: island(saved, 'report-notes'), c: island(saved, 'report-concurs') };
    if (stamping) {
      ok(B.n && B.n.element.equals(W.n.element), 'page save: the notes island, open tag included, equals the writer\'s', show(B.n && B.n.element));
      ok(W.c ? B.c && B.c.element.equals(W.c.element) : !B.c, 'page save: the concurs island equals the writer\'s, or both have none', show(B.c && B.c.element));
    } else {
      ok(B.n && B.n.inner.equals(W.n.inner), 'page save: the notes island text equals the writer\'s', show(B.n && B.n.inner));
      ok(W.c ? B.c && B.c.inner.equals(W.c.inner) : !B.c || B.c.inner.toString() === '{}',
        'page save: the concurs island text equals the writer\'s (a release that always creates one may add an empty one)', show(B.c && B.c.inner));
    }
    const wm = mirrors(expected), bm = mirrors(saved);
    ok(bm.length && bm[bm.length - 1].equals(wm[wm.length - 1]), 'page save: the last READER NOTES comment equals the writer\'s', show(bm[bm.length - 1]));
    if (stamping) ok(bm.length === wm.length, 'page save: as many READER NOTES comments as the writer leaves', `${bm.length} against ${wm.length}`);
    else ok(bm.length >= wm.length, 'page save: at least as many READER NOTES comments (releases before format 1 append)', `${bm.length} against ${wm.length}`);
    const r = await openPage(browser, pageFile(saved, 'downloaded.html'));
    const st = await r.page.evaluate(STATE);
    let back;
    try { back = readIslands(saved); } catch (e) { back = { unreadable: e.message }; }
    ok(same(back.notes, plan.notes) && (!plan.concurs || same(back.concurs, plan.concurs)),
      'page save: the download\'s islands parse to the same notes and concurs', JSON.stringify(back));
    ok(same(st, shown), 'page save: the download reopens showing what expected.html shows', JSON.stringify(st));
    ok(!r.errors.length, 'page save: the download loads without a page error', r.errors.join(' | '));
    await r.ctx.close();
  }
  rmSync(scratch, { recursive: true, force: true });

  // The derived files: written with --update, compared otherwise.
  const expect = {
    release: prior.release, pins: prior.pins,
    notes_file: label, concur_support: concurSupport,
    format: isl ? attr(isl.openTag, 'data-format') : null,
    rev_before: isl ? attr(isl.openTag, 'data-rev') : null,
    notes_text: notesText,
    refuse,
    shown_before: { note_count: before.noteCount, concur_count: before.concurCount, notes: before.badges, concurs: before.stamped },
    shown_after: shown && { note_count: shown.noteCount, concur_count: shown.concurCount, notes: shown.badges, concurs: shown.stamped },
    writes: plan ? plan.splices.map((s) => s.what) : null,
  };
  const files = {
    'anchors.json': JSON.stringify(anchorsJson, null, 1) + '\n',
    'expect.json': JSON.stringify(expect, null, 1) + '\n',
    'expected.html': expected,
    'expected-map.txt': mapText,
  };
  for (const [f, content] of Object.entries(files)) {
    const p = join(dir, f);
    if (notesJs && current) continue;   // a swapped notes.js derives nothing
    if (update) {
      if (content === null) rmSync(p, { force: true }); else writeFileSync(p, content);
    } else if (content === null) {
      ok(!existsSync(p), `${f} is absent, as a refused write leaves it`);
    } else {
      const disk = existsSync(p) ? readFileSync(p) : null;
      ok(disk && disk.equals(Buffer.isBuffer(content) ? content : Buffer.from(content, 'utf8')), `${f} matches what the page and the writer give today`,
        disk ? 'differs' : 'missing');
    }
  }
  if (prior.refuse !== undefined && !update) ok(prior.refuse === refuse, `the writer ${refuse ? 'refuses: ' + refuse : 'writes'}, as expect.json says`, String(refuse));
  return res;
}

async function main() {
  const args = process.argv.slice(2);
  const flag = (f) => { const i = args.indexOf(f); return i < 0 ? null : args[i + 1]; };
  const update = args.includes('--update');
  const notesJs = flag('--notes-js');
  const only = flag('--case');
  const names = readdirSync(CASES_DIR).filter((n) => !only || n === only).sort();
  const browser = await launch();
  let failed = 0, passed = 0;
  for (const name of names) {
    const res = await checkCase(browser, name, { update, notesJs });
    const bad = res.filter((r) => !r.ok);
    console.log(`${bad.length ? 'FAIL' : 'ok  '} ${name}`);
    for (const r of res) {
      if (r.ok) passed++; else failed++;
      if (!r.ok || process.env.VERBOSE) console.log(`     ${r.ok ? 'ok  ' : 'FAIL'} ${r.label}${r.detail ? '\n          ' + r.detail : ''}`);
    }
  }
  await browser.close();
  console.log(`\n${names.length} cases, ${passed} checks passed, ${failed} failed${update ? ' (derived files rewritten)' : ''}${notesJs ? ` (notes.js ${notesJs} inlined in this release's cases)` : ''}`);
  process.exit(failed ? 1 : 0);
}

if (import.meta.url === 'file://' + process.argv[1]) await main();
