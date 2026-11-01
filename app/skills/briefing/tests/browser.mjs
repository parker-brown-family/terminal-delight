// Isolated headless Chromium; no live desktop, extension, browser profile or dependency.
// Exit 0 passed, 1 failed, 77 skipped because this machine lacks Chromium or Node 22+.
// A skip is reported as a skip: it is never a pass.
// `node tests/browser.mjs path/to/brief.html` also checks that page: a note control on every h2 and
// figure, no page overflow and no clipped SVG text at 968 and 390 px, with screenshots in BRIEF_QA_DIR.
import assert from 'node:assert/strict';
import { spawn, spawnSync, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { accessSync, constants, mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { delimiter, dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

function executable(path) { try { accessSync(path, constants.X_OK); return true; } catch { return false; } }
function findChromium() {
  if (process.env.CHROMIUM) return executable(process.env.CHROMIUM) ? process.env.CHROMIUM : null;
  for (const name of ['chromium', 'chromium-browser', 'google-chrome', 'google-chrome-stable', 'chrome'])
    for (const dir of (process.env.PATH || '').split(delimiter)) if (dir && executable(join(dir, name))) return join(dir, name);
  const mac = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
  return executable(mac) ? mac : null;
}
const browserPath = findChromium();
if (typeof WebSocket === 'undefined' || !browserPath) {
  console.error('SKIPPED: ' + (typeof WebSocket === 'undefined' ? 'Node ' + process.versions.node + ' has no built-in WebSocket; use Node 22+'
    : 'no Chromium or Chrome found; set CHROMIUM=/path/to/browser') + '. Nothing was verified.');
  process.exit(77);
}
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const work = mkdtempSync(join(tmpdir(), 'briefing-browser-'));
const report = process.env.BRIEF_QA_DIR || join(tmpdir(), 'briefing-qa');
mkdirSync(report, { recursive: true });
const helper = join(root, 'scripts/brief.py');
const file = join(work, 'example.html');
copyFileSync(join(root, 'examples/engineering.html'), file);
const chromium = spawn(browserPath, [
  '--headless=new', '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
  '--no-first-run', '--no-default-browser-check', '--remote-debugging-port=0',
  '--user-data-dir=' + join(work, 'profile'), 'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
let ws;
try {
  const endpoint = await new Promise((done, fail) => {
    let log = '';
    const timer = setTimeout(() => fail(new Error('Chromium endpoint timeout: ' + log.slice(-600))), 15000);
    chromium.once('error', fail);
    chromium.stderr.on('data', data => {
      log += data;
      const m = log.match(/DevTools listening on (ws:\/\/\S+)/);
      if (m) { clearTimeout(timer); done(m[1]); }
    });
  });
  ws = new WebSocket(endpoint);
  await once(ws, 'open');
  const pending = new Map();
  let serial = 0;
  ws.onmessage = ({ data }) => {
    const msg = JSON.parse(String(data));
    if (pending.has(msg.id)) {
      const { done, fail, timer } = pending.get(msg.id);
      pending.delete(msg.id); clearTimeout(timer);
      if (msg.error) fail(new Error(JSON.stringify(msg.error)));
      else done(msg.result);
    }
  };
  function send(method, params = {}, sessionId) {
    const id = ++serial;
    return new Promise((done, fail) => {
      const timer = setTimeout(() => { pending.delete(id); fail(new Error('CDP timeout: ' + method)); }, 10000);
      pending.set(id, { done, fail, timer });
      ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
    });
  }
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const call = (method, params) => send(method, params, sessionId);
  await call('Page.enable');
  async function evaluate(expression) {
    const out = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
    if (out.exceptionDetails) throw new Error(JSON.stringify(out.exceptionDetails));
    return out.result.value;
  }
  async function navigate(path) {
    await call('Page.navigate', { url: pathToFileURL(path).href });
    const until = Date.now() + 10000;
    while (Date.now() < until) {
      if (await evaluate("document.readyState==='complete' && !!document.querySelector('h2.notable')")) return;
      await new Promise(done => setTimeout(done, 50));
    }
    throw new Error('page did not initialize');
  }
  await navigate(file);
  const coverage = await evaluate(`({
    headings:[...document.querySelectorAll('h2')].map(e=>[e.id,e.dataset.nid,!!e.querySelector(':scope > .note-btn')]),
    figures:[...document.querySelectorAll('figure')].map(e=>[e.id,e.dataset.nid,!!e.querySelector(':scope > .note-btn')]),
    duplicates:[...document.querySelectorAll('[id]')].map(e=>e.id).filter((id,i,a)=>a.indexOf(id)!==i)
  })`);
  assert.equal(coverage.headings.length, 4);
  assert.equal(coverage.figures.length, 3);
  for (const [id, target, button] of [...coverage.headings, ...coverage.figures]) {
    assert.equal(target, id); assert.equal(button, true);
  }
  assert.deepEqual(coverage.duplicates, []);
  const words = "Keep this comment on the return section. <script id='report-notes'> is a literal example.";
  await evaluate(`document.querySelector('#sec-return > .note-btn').click();
    document.getElementById('note-text').value=${JSON.stringify(words)};
    document.querySelector('#d-note [data-note-action="add"]').click();
    document.getElementById('d-note').close();`);
  await navigate(file);
  assert.equal(await evaluate("document.getElementById('note-count').textContent"), '1');
  const map = await evaluate(`document.getElementById('btn-export').click();
    document.getElementById('export-out').value;`);
  assert.match(map, /\[sec-return\]/);
  assert.match(map, /Keep this comment on the return section\./);
  await evaluate("document.getElementById('d-export').close()");
  // The text a reader pastes back into a chat resolves against the page it came from.
  const pasted = spawnSync('python3', [helper, 'resolve', file, '-'], { input: 'My notes:\n' + map, encoding: 'utf8' });
  assert.equal(pasted.status, 0, pasted.stderr);
  const pastedMap = JSON.parse(pasted.stdout);
  assert.equal(pastedMap.source, 'copy-map');
  assert.deepEqual(pastedMap.comments.map(c => [c.target, c.status]), [['sec-return', 'resolved']]);
  await evaluate(`document.getElementById('evidence-depth').value='1';document.getElementById('evidence-depth').dispatchEvent(new Event('change'));`);
  const snapshot = await evaluate(`(()=>{const detail={};document.dispatchEvent(new CustomEvent('decision-brief:snapshot',{detail}));return detail.html;})()`);
  writeFileSync(file, snapshot);
  writeFileSync(join(report, 'snapshot.html'), snapshot);
  const exported = JSON.parse(execFileSync('python3', [helper, 'feedback', file], { encoding: 'utf8' }));
  const savedMap = join(work, 'example.feedback.json');
  writeFileSync(savedMap, JSON.stringify(exported));
  let resolved = JSON.parse(execFileSync('python3', [helper, 'resolve', file, savedMap], { encoding: 'utf8' }));
  assert.equal(resolved.comments[0].status, 'resolved');
  await navigate(file);
  assert.equal(await evaluate("document.querySelectorAll('#sec-return > .note-btn').length"), 1);
  const layout = [];
  for (const width of [968, 390]) {
    await call('Emulation.setDeviceMetricsOverride', { width, height: 1000, deviceScaleFactor: 1, mobile: false });
    const metrics = await evaluate(`({width:innerWidth,overflow:document.documentElement.scrollWidth>innerWidth,
      clipped:[...document.querySelectorAll('svg[viewBox]')].flatMap(svg=>{const v=svg.viewBox.baseVal;return [...svg.querySelectorAll('text')].filter(t=>{const b=t.getBBox();return b.x<v.x-1||b.x+b.width>v.x+v.width+1||b.y<v.y-1||b.y+b.height>v.y+v.height+1;}).map(t=>t.textContent);})})`);
    assert.equal(metrics.width, width); assert.equal(metrics.overflow, false); assert.deepEqual(metrics.clipped, []);
    const image = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
    writeFileSync(join(report, 'brief-' + width + '.png'), Buffer.from(image.data, 'base64'));
    await evaluate("document.getElementById('fig-resolution').scrollIntoView({behavior:'instant',block:'start'})");
    const mechanism = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
    writeFileSync(join(report, 'brief-mechanism-' + width + '.png'), Buffer.from(mechanism.data, 'base64'));
    await evaluate("scrollTo({top:0,behavior:'instant'})");
    layout.push(metrics);
  }
  await evaluate(`document.getElementById('evidence-depth').value='1';document.getElementById('evidence-depth').dispatchEvent(new Event('change'));`);
  assert.deepEqual(await evaluate("[...document.querySelectorAll('[data-depth]')].map(e=>!e.hidden)"), [true, false, false]);
  // Rebuild the saved file after a rename. The old comment key and text must remain.
  const source = join(work, 'renamed.src.html');
  writeFileSync(source, readFileSync(join(root, 'examples/engineering.src.html'), 'utf8').replace('02</span> The Return', '02</span> Returning Feedback'));
  execFileSync('python3', [helper, 'html', source, file, '--id', 'reader-feedback-html', '--purpose', 'engineering']);
  await navigate(file);
  assert.equal(await evaluate("document.getElementById('sec-return').dataset.nid"), 'sec-return');
  assert.equal(await evaluate("document.getElementById('note-count').textContent"), '1');
  const retained = JSON.parse(execFileSync('python3', [helper, 'feedback', file], { encoding: 'utf8' }));
  assert.equal(retained.document.revision, null);
  assert.equal(retained.comments[0].target, 'sec-return');
  assert.equal(retained.comments[0].text, words);
  assert.match(await evaluate("document.getElementById('brief-feedback-status').textContent"), /Earlier feedback needs review/);
  writeFileSync(source, readFileSync(source, 'utf8').replaceAll('sec-return', 'sec-replacement'));
  execFileSync('python3', [helper, 'html', source, file, '--id', 'reader-feedback-html', '--purpose', 'engineering']);
  await navigate(file);
  assert.match(await evaluate("document.getElementById('brief-feedback-status').textContent"), /sec-return/);
  const orphanMap = await evaluate("document.getElementById('btn-export').click();document.getElementById('export-out').value");
  assert.match(orphanMap, /\[sec-return\] UNRESOLVED/);
  assert.match(orphanMap, /Keep this comment on the return section\./);
  await evaluate("document.getElementById('d-export').close()");
  const orphaned = spawnSync('python3', [helper, 'resolve', file, '-'], { input: orphanMap, encoding: 'utf8' });
  assert.equal(orphaned.status, 2, orphaned.stderr);
  const kept = JSON.parse(orphaned.stdout).comments.find(c => c.target === 'sec-return');
  assert.deepEqual(kept.reasons, ['revision_unknown', 'target_missing']);
  assert.match(kept.text, /Keep this comment on the return section\./);
  const result = { copyMapResolves: true, copyMapOrphanNeedsReview: true, passed: true, coverage, layout, commentReloadExport: true, snapshotRevision: true, evidenceDepthSnapshot: true, literalScriptComment: true, rebuildAfterRename: true, missingTargetVisibleAndExported: true, isolation: 'separate headless Chromium with temporary profile' };
  if (process.argv[2]) {
    await navigate(resolve(process.argv[2]));
    // Decisions as well as headings and figures: a grill that cannot carry a note has failed at its one job.
    const targets = await evaluate("[...document.querySelectorAll('h2,figure,.grill .ask,.grill-q,.grill .q:not(.ask .q)')].map(e=>({id:e.id,target:e.dataset.nid,button:!!e.querySelector(':scope > .note-btn'),concur:e.matches('.ask,.grill-q,.grill .q')?e.classList.contains('concurrable'):null}))");
    assert.ok(targets.length > 0);
    for (const t of targets) { assert.equal(t.id, t.target); assert.equal(t.button, true); if (t.concur !== null) assert.equal(t.concur, true, t.id + ' takes no CONCUR'); }
    const extraLayout = [];
    for (const width of [968, 390]) {
      await call('Emulation.setDeviceMetricsOverride', { width, height: 1000, deviceScaleFactor: 1, mobile: false });
      const m = await evaluate(`({width:innerWidth,overflow:document.documentElement.scrollWidth>innerWidth,
        clipped:[...document.querySelectorAll('svg[viewBox]')].flatMap(svg=>{const v=svg.viewBox.baseVal;return [...svg.querySelectorAll('text')].filter(t=>{const b=t.getBBox();return b.x<v.x-1||b.x+b.width>v.x+v.width+1||b.y<v.y-1||b.y+b.height>v.y+v.height+1;}).map(t=>t.textContent);})})`);
      assert.equal(m.width, width); assert.equal(m.overflow, false); assert.deepEqual(m.clipped, [], 'SVG text outside its viewBox');
      await evaluate("scrollTo({top:0,behavior:'instant'})");
      const shot = await call('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
      writeFileSync(join(report, 'review-' + width + '.png'), Buffer.from(shot.data, 'base64'));
      extraLayout.push(m);
    }
    result.additionalPage = { targets, layout: extraLayout };
  }
  writeFileSync(join(report, 'browser.json'), JSON.stringify(result, null, 2) + '\n');
  console.log(JSON.stringify(result, null, 2));
  console.log('QA images and result: ' + report);
} finally {
  if (ws) ws.close();
  chromium.kill('SIGTERM');
  await Promise.race([once(chromium, 'exit'), new Promise(done => setTimeout(done, 1500))]);
  if (chromium.exitCode === null) chromium.kill('SIGKILL');
  rmSync(work, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
}
