// Drives a real Chromium over a brief the way a reader does: through the page's own
// buttons. Shared by check.mjs, build.mjs and ../../tests/notes-js.test.mjs.
//
// playwright-core is found through normal resolution, else through PLAYWRIGHT_FROM (any
// directory whose node_modules holds playwright-core 1.45 or later, which added page.clock).
// Chromium is CHROMIUM, else the first of the usual system paths that exists.
import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';

function loadPlaywright() {
  const from = [process.env.PLAYWRIGHT_FROM, import.meta.url, process.cwd() + '/'].filter(Boolean);
  for (const f of from) {
    const base = f.startsWith('file:') || f.endsWith('.json') || f.endsWith('/') ? f : f + '/';
    try { return createRequire(base)('playwright-core'); } catch { /* try the next place */ }
  }
  throw new Error('playwright-core not found. Set PLAYWRIGHT_FROM to a directory whose node_modules holds it.');
}

export const { chromium } = loadPlaywright();

export function chromiumPath() {
  if (process.env.CHROMIUM) return process.env.CHROMIUM;
  return ['/usr/bin/chromium', '/usr/bin/chromium-browser', '/usr/bin/google-chrome-stable', '/usr/bin/google-chrome']
    .find((p) => existsSync(p));
}

export async function launch() {
  return chromium.launch({ executablePath: chromiumPath() });
}

// A fresh profile: its own localStorage, downloads accepted, no animation, so a peeled
// stamp is gone at once. Seed storage with [[key, value], ...] before the first load.
export async function openPage(browser, url, { storage = null, clock = null } = {}) {
  const ctx = await browser.newContext({ acceptDownloads: true, reducedMotion: 'reduce',
    viewport: { width: 1100, height: 900 } });
  // A textarea hands back its value with every CR turned into LF, so reading #export-out
  // would lose a CR that buildMap() kept. Remember the string the page actually set.
  await ctx.addInitScript(() => {
    const d = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value');
    Object.defineProperty(HTMLTextAreaElement.prototype, 'value', {
      configurable: true, enumerable: d.enumerable, get() { return d.get.call(this); },
      set(v) { this.__fixtureRaw = String(v); d.set.call(this, v); },
    });
  });
  if (storage) {
    await ctx.addInitScript((pairs) => {
      if (sessionStorage.getItem('fixture-seeded')) return;
      for (const [k, v] of pairs) localStorage.setItem(k, v);
      sessionStorage.setItem('fixture-seeded', '1');
    }, storage);
  }
  const page = await ctx.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  // A native confirm() left unanswered blocks the page's main thread and wedges the tab
  // (agent-skills#18). Answer every one; the ✕ test depends on accepting.
  page.on('dialog', (d) => d.accept().catch(() => {}));
  if (clock) await page.clock.setFixedTime(new Date(clock));
  await page.goto(url, { waitUntil: 'load' });
  return { ctx, page, errors };
}

// ts "YYYY-MM-DD HH:MM" (UTC, as notes.js writes it) → a Date for the page clock.
export function tsDate(ts) { return new Date(ts.replace(' ', 'T') + ':00.000Z'); }

// Every anchor notes.js tagged, in document order, with what TD needs from each.
export const EXTRACT = () => Array.from(document.querySelectorAll('.notable')).map((el) => ({
  nid: el.dataset.nid,
  title: el.dataset.ntitle,
  concurrable: el.classList.contains('concurrable'),
}));

// What the page shows: badge counts per anchor, stamps, and the two counters.
export const STATE = () => ({
  noteCount: (document.getElementById('note-count') || {}).textContent ?? null,
  concurCount: (document.getElementById('concur-count') || {}).textContent ?? null,
  badges: Object.fromEntries(Array.from(document.querySelectorAll('.notable.has-note'))
    .map((el) => [el.dataset.nid, (el.querySelector(':scope > .note-btn') || {}).textContent])),
  stamped: Array.from(document.querySelectorAll('.has-concur'))
    .filter((el) => el.querySelector('.concur-stamp')).map((el) => el.dataset.nid),
});

// The angle and offset a stamp lands at, read off the stamp the page draws. Concurs each
// decision in turn and peels it off again, so run it on a throwaway page.
export async function stampPoses(page) {
  return page.evaluate(() => {
    const out = {};
    for (const el of document.querySelectorAll('.concurrable')) {
      const zone = el.querySelector(':scope > .concur-zone');
      const was = el.classList.contains('has-concur');
      if (!was) zone.click();
      const s = el.querySelector('.concur-stamp:not(.peel)');
      const st = s ? s.getAttribute('style') : '';
      const num = (k) => { const m = st.match(new RegExp('--' + k + ':(-?[0-9.]+)')); return m ? +m[1] : null; };
      out[el.dataset.nid] = { r: num('r'), dx: num('dx'), dy: num('dy') };
      if (!was) zone.click();
    }
    return out;
  });
}

// Apply fixture edits through the page's own UI, with the page clock set to each edit's
// time so the page stamps what the fixture says. Returns nothing; read state afterwards.
export async function applyEdits(page, edits) {
  for (const e of edits) {
    const sel = `[data-nid="${e.nid}"]`;
    if (e.ts) await page.clock.setFixedTime(tsDate(e.ts));
    if (e.op === 'add') {
      await page.locator(`${sel} > .note-btn`).click();
      await page.fill('#note-text', e.text);
      await page.click('#d-note [data-note-action="add"]');
      await page.click('#d-note .note-actions [data-close]');
    } else if (e.op === 'delete') {
      await page.locator(`${sel} > .note-btn`).click();
      const i = await page.evaluate(({ text, ts }) => {
        const lis = Array.from(document.querySelectorAll('#note-list li'));
        return lis.findIndex((li) => {
          const body = li.lastChild && li.lastChild.nodeType === 3 ? li.lastChild.textContent : '';
          const when = li.querySelector('.nts span').textContent;
          return body === text && (!ts || when === ts);
        });
      }, { text: e.text, ts: e.ts || null });
      if (i < 0) throw new Error(`delete: no note on ${e.nid} reads ${JSON.stringify(e.text)}`);
      await page.locator('#note-list .ndel').nth(i).click();
      await page.click('#d-note .note-actions [data-close]');
    } else if (e.op === 'concur' || e.op === 'unconcur') {
      const on = await page.evaluate((s) => document.querySelector(s).classList.contains('has-concur'), sel);
      if (on !== (e.op === 'unconcur')) throw new Error(`${e.op}: ${e.nid} is ${on ? 'already' : 'not'} concurred`);
      await page.locator(`${sel} > .concur-zone`).click();
    } else throw new Error('unknown op ' + e.op);
  }
}

// Click 💾 and return the downloaded bytes. The page clock is set first, because the
// revision a save stamps is the time it happened.
export async function saveIntoFile(page, rev) {
  if (rev) await page.clock.setFixedTime(new Date(rev));
  const [dl] = await Promise.all([page.waitForEvent('download'), page.click('#btn-embed')]);
  const path = await dl.path();
  const { readFileSync } = await import('node:fs');
  return { bytes: readFileSync(path), suggested: dl.suggestedFilename() };
}

// The map "copy map" builds: the string exportNotes() put into #export-out, as buildMap()
// returned it. What the reader copies is the same with any CR read back as LF.
export async function copyMap(page) {
  await page.click('#btn-export');
  const txt = await page.evaluate(() => document.getElementById('export-out').__fixtureRaw);
  await page.click('#d-export .note-actions [data-close]');
  return txt;
}
