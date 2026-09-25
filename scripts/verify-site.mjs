#!/usr/bin/env node
/* ==========================================================================
   Verifies the vanilla-shell pages — the info kiosk and the docs site —
   against a running server. The kiosk family has its own verifier
   (verify-kiosks.mjs); this one covers what td-shell adds.

     node scripts/verify-site.mjs <kiosk-base> <docs-base> [shots-dir]

   Serve the repository root for the kiosk and docsite/dist for the docs, each
   from a server that resolves /name to /name.html as Pages and Caddy do.

   The server must resolve /info to /info.html, as GitHub Pages does;
   `python3 -m http.server` does not, so use any server that does.

   It asserts on the console, computed style and behaviour at three widths
   and in glass, paper and green-bar, then exercises every control: the tube
   and theme toggles, the barrel suspending on scroll, the theme chips, the
   TERM/BENCH flip, the spine's where-am-I, register deep links, arrow keys,
   copy scoped to one register, the search palette, and the tabs with
   JavaScript off. Photographs go to shots-dir, default ./site-shots.
   ========================================================================== */
import { createRequire } from 'node:module';
import { existsSync, mkdirSync } from 'node:fs';
const PW_CANDIDATES = [
  '/home/parker/BROWN-FAMILY-SPORTS/Software/wellness-with-kate-site/node_modules/playwright/index.js',
  '/home/parker/ai-garrison/node_modules/playwright/index.js',
];
const pw = PW_CANDIDATES.find(existsSync);
if (!pw) { console.error('No playwright install found. Looked in:\n  ' + PW_CANDIDATES.join('\n  ')); process.exit(2); }
const { chromium } = createRequire(import.meta.url)(pw);
const BASE = (process.argv[2] || 'http://127.0.0.1:8838').replace(/\/$/, '');
/* the docs are built by docsite/build.mjs and served from their own root */
const DOCS = (process.argv[3] || 'http://127.0.0.1:8839').replace(/\/$/, '');
const OUT = process.argv[4] || './site-shots';
mkdirSync(OUT, { recursive: true });

let pass = 0, fail = 0; const bad = [];
const ok = (name, cond, detail) => { if (cond) pass++; else { fail++; bad.push(name + (detail ? ' — ' + detail : '')); } };

const browser = await chromium.launch({ executablePath: '/usr/bin/chromium' });

async function open(path, { w = 1440, h = 900, prefs = null } = {}) {
  const ctx = await browser.newContext({ viewport: { width: w, height: h } });
  await ctx.grantPermissions(['clipboard-read', 'clipboard-write']);
  const page = await ctx.newPage();
  const errors = [];
  page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
  page.on('pageerror', e => errors.push(String(e)));
  page.on('requestfailed', r => { if (!/fonts\.g/.test(r.url())) errors.push('failed ' + r.url()); });
  page.on('response', r => { if (r.status() >= 400) errors.push(r.status() + ' ' + r.url()); });
  /* the fonts are self-hosted and the policy is 'self': nothing leaves */
  const offsite = [];
  page.on('request', r => { const u = r.url(); if (!u.startsWith(BASE) && !u.startsWith(DOCS) && !/^(data|blob):/.test(u)) offsite.push(u); });
  if (prefs) await page.addInitScript(p => localStorage.setItem('td-shell', JSON.stringify(p)), prefs);
  await page.goto((/^https?:/.test(path) ? '' : BASE) + path, { waitUntil: 'load' });
  await page.waitForTimeout(700);
  return { ctx, page, errors, offsite };
}
/* The curved tube (td-glass.js) snapshots the page asynchronously; wait for
   it to go live before asserting on it. */
async function tubeLive(page) {
  try { await page.waitForFunction(() => document.documentElement.dataset.tube === 'gl', null, { timeout: 20000 }); return true; }
  catch { return false; }
}
/* The page's filter, and the glass canvas: whether it shows, the alpha of its
   top-left pixel (outside the bent screen, so black) and of its centre
   (inside, so clear or a faint scanline). Runs in the page. */
function glassState() {
  const t = document.getElementById('tube');
  const cv = document.querySelector('#td-fx canvas.glass');
  const fx = document.getElementById('td-fx');
  const shown = !!cv && !cv.hidden && getComputedStyle(fx).display !== 'none' && cv.width > 0;
  let corner = null, centre = null;
  if (shown) {
    const c = cv.getContext('2d');
    corner = c.getImageData(1, 1, 1, 1).data[3];
    centre = c.getImageData(cv.width >> 1, (cv.height >> 1) + 2, 1, 1).data[3];
  }
  const gl = document.querySelector('canvas.td-tube');
  return {
    filter: getComputedStyle(t).filter, shown, corner, centre,
    tube: document.documentElement.dataset.tube || null,
    curved: !!gl && !gl.hidden && getComputedStyle(gl).display !== 'none',
  };
}
const overflow = page => page.evaluate(() => {
  const t = document.getElementById('tube');
  return { doc: document.documentElement.scrollWidth - innerWidth, tube: t ? t.scrollWidth - t.clientWidth : 0 };
});

for (const [path, name] of [['/info', 'info'], [DOCS + '/', 'docs-index'], [DOCS + '/workbench', 'workbench'], [DOCS + '/install', 'install']]) {
  for (const w of [1440, 968, 390]) {
    for (const prefs of [{ theme: 'glass', crt: 'on' }, { theme: 'paper', crt: 'off' }, { theme: 'paper', crt: 'on' }]) {
      const { ctx, page, errors, offsite } = await open(path, { w, h: w === 390 ? 844 : 900, prefs });
      const tag = `${name}@${w} ${prefs.theme}/${prefs.crt}`;
      const tubeOn = prefs.theme === 'glass' && prefs.crt === 'on';
      /* The true curve runs on a pane 600px wide or more (the spine takes
         268 of the window); a narrower pane, and paper, get the flat glass. */
      const expectCurve = tubeOn && w >= 968;
      if (expectCurve) await tubeLive(page);
      ok(`${tag}: clean console`, errors.length === 0, errors.join(' | '));
      ok(`${tag}: nothing requested off-site`, offsite.length === 0, offsite.join(' '));
      const o = await overflow(page);
      ok(`${tag}: no horizontal overflow`, o.doc <= 0 && o.tube <= 1, JSON.stringify(o));
      const g = await page.evaluate(glassState);
      /* The rule curved-glass-web wrote down and this site broke once: warp
         the glass, never the text. Nothing may put a filter on the page. */
      ok(`${tag}: the page itself is never filtered`, g.filter === 'none', g.filter);
      ok(`${tag}: true curve ${expectCurve ? 'live' : 'off'}`, expectCurve ? (g.tube === 'gl' && g.curved && !g.shown) : (g.tube === null && !g.curved), JSON.stringify(g));
      if (!expectCurve) ok(`${tag}: flat glass ${tubeOn ? 'drawn' : 'absent'}`,
        tubeOn ? (g.shown && g.corner === 255 && g.centre < 80) : !g.shown, JSON.stringify(g));
      await page.screenshot({ path: `${OUT}/${name}-${w}-${prefs.theme}-${prefs.crt}.png` });
      await ctx.close();
    }
  }
}

// toggles flip state and persist; the glass follows them
{
  const { ctx, page } = await open('/info', { prefs: { theme: 'glass', crt: 'off' } });
  await page.click('.td-actions [data-td-toggle="crt"]');
  ok('crt toggle brings the true curve up', await tubeLive(page));
  ok('crt toggle persists', await page.evaluate(() => JSON.parse(localStorage.getItem('td-shell')).crt === 'on'));
  await page.evaluate(() => { const t = document.getElementById('tube'); t.style.scrollBehavior = 'auto'; t.scrollTop = 900; });
  ok('scrolling leaves the page unfiltered', (await page.evaluate(glassState)).filter === 'none');
  await page.click('.td-actions [data-td-toggle="theme"]');
  ok('theme toggle to paper', await page.evaluate(() => document.documentElement.dataset.theme === 'paper'));
  const paperState = await page.evaluate(glassState);
  ok('paper takes the curve and the glass down', !paperState.shown && !paperState.curved && paperState.tube === null, JSON.stringify(paperState));
  ok('paper paints hero window quiet-command', await page.evaluate(() => document.getElementById('win').dataset.wear === 'quiet-command'));
  await page.click('.wear button[data-wear="gamba"]');
  ok('theme chip paints both windows', await page.evaluate(() => [...document.querySelectorAll('#win,[data-wear-follow]')].every(e => e.dataset.wear === 'gamba')));
  // TERM/BENCH flip
  await page.evaluate(() => document.getElementById('bench').scrollIntoView());
  await page.click('label[for="face-term"]');
  ok('TERM face shows the terminal', await page.evaluate(() => getComputedStyle(document.querySelector('.flip .face.term')).display !== 'none' && getComputedStyle(document.querySelector('.flip .face.bench')).display === 'none'));
  // spine scrollspy
  await page.waitForTimeout(500);
  ok('spine marks the bench stop', await page.evaluate(() => !!document.querySelector('.td-spine a.is-here[href="#bench"]')));
  await ctx.close();
}

// the true curve: clicks and hover land on what is drawn, and the live island moves
{
  const { ctx, page } = await open('/info', { prefs: { theme: 'glass', crt: 'on' } });
  ok('engine: live on info', await tubeLive(page));
  /* Where on the glass is an element drawn? Invert the barrel: find the
     screen point s whose sample point warp(s) is the element's centre. */
  const onGlass = sel => page.evaluate(sel => {
    const K1 = 0.39 * 0.6, K2 = 0.39 * 0.25;
    const cv = document.querySelector('canvas.td-tube').getBoundingClientRect();
    const el = document.querySelector(sel).getBoundingClientRect();
    const cu = (el.left + el.width / 2 - cv.left) / cv.width - 0.5, cvn = (el.top + el.height / 2 - cv.top) / cv.height - 0.5;
    let u = cu, v = cvn;
    for (let i = 0; i < 40; i++) { const r2 = u * u + v * v, f = 1 + K1 * r2 + K2 * r2 * r2; u = cu / f; v = cvn / f; }
    const x = cv.left + (u + 0.5) * cv.width, y = cv.top + (v + 0.5) * cv.height;
    return { x, y, shift: Math.hypot(x - (el.left + el.width / 2), y - (el.top + el.height / 2)) };
  }, sel);
  await page.evaluate(() => { const t = document.getElementById('tube'); t.style.scrollBehavior = 'auto'; t.scrollTop = document.getElementById('bench').offsetTop + 60; });
  await page.waitForTimeout(400);
  const term = await onGlass('label[for="face-term"]');
  ok('engine: the TERM label is drawn somewhere else than it sits', term.shift > 4, `shift ${term.shift.toFixed(1)}px`);
  await page.mouse.click(term.x, term.y);
  ok('engine: clicking TERM where it is drawn flips the face', await page.evaluate(() => document.getElementById('face-term').checked));
  /* The case the redirect exists for: near the rim the barrel moves an
     element so far that the page under the pointer is something else. Put
     the theme chips at the bottom of the tube and click one where it is
     drawn. */
  await page.evaluate(() => {
    const t = document.getElementById('tube'), b = document.querySelector('.wear').getBoundingClientRect();
    const tr = t.getBoundingClientRect();
    t.scrollTop += b.top - (tr.top + t.clientHeight * 0.9);
  });
  await page.waitForTimeout(400);
  const chip = await onGlass('button[data-wear="field-command"]');
  const naive = await page.evaluate(p => { const e = document.elementFromPoint(p.x, p.y); return e ? (e.closest('button[data-wear]') || {}).dataset?.wear || e.tagName : null; }, chip);
  await page.mouse.click(chip.x, chip.y);
  await page.waitForTimeout(150);
  ok('engine: a chip clicked where it is drawn paints the window', await page.evaluate(() => document.getElementById('win').dataset.wear === 'field-command'), `naive target ${naive}`);
  await page.evaluate(() => { const t = document.getElementById('tube'); t.scrollTop = document.getElementById('bench').offsetTop + 60; });
  await page.waitForTimeout(300);
  const more = await onGlass('#bench .more');
  await page.mouse.move(more.x, more.y);
  await page.waitForTimeout(250);
  ok('engine: hovering a link where it is drawn marks it clickable', await page.evaluate(() => document.documentElement.classList.contains('tube-pointer')));
  await page.evaluate(() => { const t = document.getElementById('tube'); t.scrollTop = document.getElementById('kiosks').offsetTop - 20; });
  const f0 = await page.evaluate(() => document.querySelector('canvas.babel').__tdFrame || 0);
  await page.waitForTimeout(2200);
  const isl = await page.evaluate(() => { const c = document.querySelector('canvas.babel'); return { frame: c.__tdFrame || 0, uploaded: c.__tdUploaded || '' }; });
  ok('engine: the Global title keeps drawing', isl.frame > f0, `${f0} -> ${isl.frame}`);
  ok('engine: its frames reach the curved picture', /:\d+$/.test(isl.uploaded) && +isl.uploaded.split(':')[1] > f0, isl.uploaded);
  await page.screenshot({ path: `${OUT}/engine-kiosks.png` });
  /* The case only the redirect can pass: a 10px channel dot in the footer,
     down in the bottom-right corner where the barrel moves things furthest.
     Under the pointer is something else; drawn there is the dot. */
  await page.evaluate(() => { const t = document.getElementById('tube'); t.scrollTop = t.scrollHeight; });
  await page.waitForTimeout(400);
  const dot = await onGlass('.foot .ch a[href="/tv"]');
  const under = await page.evaluate(p => { const e = document.elementFromPoint(p.x, p.y); return e && e.getAttribute('href') || (e && e.tagName); }, dot);
  ok('engine: in the corner the pointer is over something else', under !== '/tv', `under the pointer: ${under}, shift ${dot.shift.toFixed(1)}px`);
  await Promise.all([page.waitForURL(/\/tv$/, { timeout: 5000 }).catch(() => null), page.mouse.click(dot.x, dot.y)]);
  ok('engine: clicking the dot where it is drawn opens its channel', /\/tv$/.test(page.url()), page.url());
  await ctx.close();
}

// the hero window: each pane bent on its own (assets/td-panes.js)
{
  const { ctx, page, errors } = await open('/info', { prefs: { theme: 'glass', crt: 'off' } });
  const drawn = await page.waitForFunction(() => { const c = document.querySelector('canvas.pane-warp'); return c && !c.hidden; }, null, { timeout: 20000 }).then(() => true, () => false);
  ok('hero: the pane-warped window is drawn', drawn);
  /* Read the canvas back: just inside the focused pane's corner the bent
     glass has pulled the content away, so it is black; the pane's centre
     carries content. The flat HTML has the pane background in both. */
  const px = await page.evaluate(() => {
    const cv = document.querySelector('canvas.pane-warp'), win = document.getElementById('win');
    const body = win.querySelector('[data-focus]').getBoundingClientRect(), box = win.getBoundingClientRect();
    const s = cv.width / box.width, c2 = document.createElement('canvas');
    c2.width = cv.width; c2.height = cv.height;
    const g = c2.getContext('2d'); g.drawImage(cv, 0, 0);
    const at = (x, y) => Array.from(g.getImageData(Math.round((x - box.left) * s), Math.round((y - box.top) * s), 1, 1).data);
    return { corner: at(body.left + 3, body.top + 3), centre: at(body.left + body.width / 2, body.top + body.height / 2), panes: win.querySelectorAll('[data-warp]').length };
  });
  ok('hero: inside a pane corner the glass is black', px.corner[3] === 255 && px.corner[0] + px.corner[1] + px.corner[2] < 12, JSON.stringify(px.corner));
  ok('hero: three panes are marked to bend', px.panes === 3, String(px.panes));
  await page.click('.wear button[data-wear="deco"]');
  const redrawn = await page.waitForFunction(() => (document.querySelector('canvas.pane-warp').__tdFrame || 0) >= 2, null, { timeout: 10000 }).then(() => true, () => false);
  ok('hero: a theme chip redraws the bent window', redrawn);
  ok('hero: clean console', errors.length === 0, errors.join(' | '));
  await ctx.close();
}
{
  const { ctx, page } = await open('/info', { prefs: { theme: 'glass', crt: 'on' } });
  await tubeLive(page);
  const up = await page.waitForFunction(() => /:\d+$/.test((document.querySelector('canvas.pane-warp') || {}).__tdUploaded || ''), null, { timeout: 20000 }).then(() => true, () => false);
  ok('hero: under the tube the bent window reaches the curved page', up);
  await ctx.close();
}

// registers: deep link, radios, copy scoped to one register
{
  const { ctx, page } = await open(DOCS + '/workbench#story', { prefs: { theme: 'glass', crt: 'off' } });
  const vis = () => page.evaluate(() => [...document.querySelectorAll('.reg-panel')].filter(p => p.offsetParent !== null).map(p => p.id));
  ok('deep link opens Story', JSON.stringify(await vis()) === '["story"]', JSON.stringify(await vis()));
  ok('every register is in the source', await page.evaluate(() => ['brief', 'story', 'technical'].every(id => document.getElementById(id))));
  await page.click('label[for="r-technical"]');
  ok('tab click shows Technical only', JSON.stringify(await vis()) === '["technical"]');
  ok('hash follows the tab', await page.evaluate(() => location.hash === '#technical'));
  await page.click('#technical .td-copy');
  const txt = await page.evaluate(() => window.__tdLastCopy || '');
  ok('copy carries the metadata header', /^Workbench\n[\s\S]*Register: Technical[\s\S]*Source URL: .*#technical/.test(txt), txt.slice(0, 200));
  ok('copy holds Technical', txt.includes('five real transcripts'));
  ok('copy excludes Story and Brief', !txt.includes('The doorway') && !txt.includes('Every agent pane in Terminal Delight has a second face'));
  await page.focus('#r-technical');
  await page.keyboard.press('ArrowLeft');
  ok('arrow keys move between registers', JSON.stringify(await vis()) === '["story"]', JSON.stringify(await vis()));
  await page.keyboard.press('Control+k');
  ok('ctrl+k opens the search palette', await page.evaluate(() => !!document.querySelector('.td-pal.open')));
  await page.keyboard.type('bench');
  ok('palette finds Workbench', await page.evaluate(() => [...document.querySelectorAll('.td-pal li')].some(li => /Workbench/.test(li.textContent))));
  /* Full text: a phrase that appears only in a table row of the Install
     page's Technical register is found, and opening it lands in that
     register at that heading. */
  await page.fill('.td-pal input', 'bracketed paste');
  await page.waitForTimeout(150);
  const hit = await page.evaluate(() => { const li = document.querySelector('.td-pal li'); return li ? li.textContent : ''; });
  ok('search finds body text, not only titles', /Workbench/.test(hit) && /bracketed paste/i.test(hit), hit.slice(0, 120));
  await page.fill('.td-pal input', 'cargo install');
  await page.waitForTimeout(150);
  await Promise.all([page.waitForURL(/\/install#/, { timeout: 5000 }).catch(() => null), page.keyboard.press('Enter')]);
  await page.waitForTimeout(500);
  const landed = await page.evaluate(() => ({ url: location.pathname + location.hash, tech: document.getElementById('r-technical') && document.getElementById('r-technical').checked }));
  ok('a search hit opens its page in the right register', /^\/install#technical/.test(landed.url) && landed.tech, JSON.stringify(landed));
  await ctx.close();
}
// one top bar everywhere: the same four sections, and Install where Download was
for (const path of ['/info', DOCS + '/', DOCS + '/workbench', DOCS + '/install']) {
  const { ctx, page } = await open(path, { prefs: { theme: 'glass', crt: 'off' } });
  const bar = await page.evaluate(() => ({
    sections: [...document.querySelectorAll('.td-sections a')].map(a => a.textContent.trim()),
    button: (document.querySelector('.td-actions .td-btn.primary') || {}).textContent,
    href: (document.querySelector('.td-actions .td-btn.primary') || {}).href || '',
    download: [...document.querySelectorAll('.td-top a, .td-top button')].some(el => /download/i.test(el.textContent)),
  }));
  ok(`${path}: top bar reads Overview · Docs · Omarchy · Global`, bar.sections.join('|') === 'Overview|Docs|Omarchy|Global', bar.sections.join('|'));
  ok(`${path}: the bar's button is Install`, (bar.button || '').trim() === 'Install', bar.button);
  ok(`${path}: Install opens the install page`, /\/install$/.test(bar.href), bar.href);
  ok(`${path}: nothing in the bar says Download`, !bar.download);
  await ctx.close();
}

// the install page: two registers, deep link to technical, command copy is commands only
{
  const { ctx, page } = await open(DOCS + '/install#technical', { prefs: { theme: 'glass', crt: 'off' } });
  const vis = () => page.evaluate(() => [...document.querySelectorAll('.reg-panel')].filter(p => p.offsetParent !== null).map(p => p.id));
  ok('install: deep link opens Technical', JSON.stringify(await vis()) === '["technical"]', JSON.stringify(await vis()));
  ok('install: two register tabs', await page.evaluate(() => document.querySelectorAll('.reg-tabs label').length === 2));
  await page.click('label[for="r-brief"]');
  ok('install: Brief tab shows Brief', JSON.stringify(await vis()) === '["brief"]', JSON.stringify(await vis()));
  await page.click('#brief pre.cmd [data-copy-code]');
  const cmd = await page.evaluate(() => window.__tdLastCopy || '');
  ok('install: copying the commands copies three lines of commands', cmd.split('\n').length === 3 && cmd.startsWith('curl -LO https://') && !/copy/i.test(cmd), JSON.stringify(cmd));
  await page.click('#brief .reg-head .td-copy');
  const reg = await page.evaluate(() => window.__tdLastCopy || '');
  ok('install: register copy carries the commands without a button label', reg.includes('chmod +x terminal-delight-x86_64.AppImage') && !/\nCopy\n|Copy$/.test(reg), reg.slice(-200));
  await ctx.close();
}

// no-JS: tabs still work, default is Brief
{
  const ctx = await browser.newContext({ viewport: { width: 1200, height: 900 }, javaScriptEnabled: false });
  const page = await ctx.newPage();
  await page.goto(DOCS + '/workbench', { waitUntil: 'load' });
  const v = await page.evaluate(() => 0);
  const shown = await page.$$eval('.reg-panel', ps => ps.filter(p => p.offsetParent !== null).map(p => p.id));
  ok('no JS: lands on Brief', JSON.stringify(shown) === '["brief"]', JSON.stringify(shown));
  await page.click('label[for="r-story"]');
  const shown2 = await page.$$eval('.reg-panel', ps => ps.filter(p => p.offsetParent !== null).map(p => p.id));
  ok('no JS: Story tab works', JSON.stringify(shown2) === '["story"]', JSON.stringify(shown2));
  await ctx.close();
}

// photographs of specific states
{
  const { ctx, page } = await open(DOCS + '/workbench#story', { prefs: { theme: 'glass', crt: 'off' } });
  await page.screenshot({ path: `${OUT}/workbench-story.png` });
  await page.click('label[for="r-technical"]');
  await page.evaluate(() => document.querySelector('.td-fig').scrollIntoView({ block: 'center' }));
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${OUT}/workbench-figure.png` });
  await ctx.close();
}
for (const [id, prefs] of [['wall', { theme: 'glass', crt: 'on' }], ['bench', { theme: 'glass', crt: 'on' }], ['glass', { theme: 'paper', crt: 'on' }], ['kiosks', { theme: 'glass', crt: 'off' }]]) {
  const { ctx, page } = await open('/info', { prefs });
  await page.evaluate(i => { const t = document.getElementById('tube'); t.style.scrollBehavior = 'auto'; t.scrollTop = document.getElementById(i).offsetTop - 20; }, id);
  await page.waitForTimeout(500);
  await page.screenshot({ path: `${OUT}/info-stop-${id}-${prefs.theme}.png` });
  await ctx.close();
}

await browser.close();
console.log(`${pass} passed, ${fail} failed`);
if (bad.length) console.log(bad.map(b => '  ✗ ' + b).join('\n'));
process.exit(fail ? 1 : 0);
