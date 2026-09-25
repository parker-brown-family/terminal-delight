#!/usr/bin/env node
/* ==========================================================================
   Verifies the vanilla-shell pages — the info kiosk and the docs site —
   against a running server. The kiosk family has its own verifier
   (verify-kiosks.mjs); this one covers what td-shell adds.

     node scripts/verify-site.mjs http://127.0.0.1:8838 [shots-dir]

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
const OUT = process.argv[3] || './site-shots';
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
  if (prefs) await page.addInitScript(p => localStorage.setItem('td-shell', JSON.stringify(p)), prefs);
  await page.goto(BASE + path, { waitUntil: 'load' });
  await page.waitForTimeout(700);
  return { ctx, page, errors };
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
  return { filter: getComputedStyle(t).filter, shown, corner, centre };
}
const overflow = page => page.evaluate(() => {
  const t = document.getElementById('tube');
  return { doc: document.documentElement.scrollWidth - innerWidth, tube: t ? t.scrollWidth - t.clientWidth : 0 };
});

for (const [path, name] of [['/info', 'info'], ['/docsite/', 'docs-index'], ['/docsite/workbench.html', 'workbench'], ['/docsite/install', 'install']]) {
  for (const w of [1440, 968, 390]) {
    for (const prefs of [{ theme: 'glass', crt: 'on' }, { theme: 'paper', crt: 'off' }, { theme: 'paper', crt: 'on' }]) {
      const { ctx, page, errors } = await open(path, { w, h: w === 390 ? 844 : 900, prefs });
      ok(`${name}@${w} ${prefs.theme}/${prefs.crt}: clean console`, errors.length === 0, errors.join(' | '));
      const o = await overflow(page);
      ok(`${name}@${w} ${prefs.theme}/${prefs.crt}: no horizontal overflow`, o.doc <= 0 && o.tube <= 1, JSON.stringify(o));
      const g = await page.evaluate(glassState);
      const expectGlass = prefs.theme === 'glass' && prefs.crt === 'on';
      /* The rule curved-glass-web wrote down and this site broke once: warp
         the glass, never the text. Nothing may put a filter on the page. */
      ok(`${name}@${w} ${prefs.theme}/${prefs.crt}: the page itself is never filtered`, g.filter === 'none', g.filter);
      ok(`${name}@${w} ${prefs.theme}/${prefs.crt}: curved glass ${expectGlass ? 'drawn' : 'absent'}`,
        expectGlass ? (g.shown && g.corner === 255 && g.centre < 80) : !g.shown, JSON.stringify(g));
      await page.screenshot({ path: `${OUT}/${name}-${w}-${prefs.theme}-${prefs.crt}.png` });
      await ctx.close();
    }
  }
}

// toggles flip state and persist; the glass follows them
{
  const { ctx, page } = await open('/info', { prefs: { theme: 'glass', crt: 'off' } });
  await page.click('.td-actions [data-td-toggle="crt"]');
  const on = await page.evaluate(glassState);
  ok('crt toggle draws the curved glass', on.shown && on.corner === 255, JSON.stringify(on));
  ok('crt toggle persists', await page.evaluate(() => JSON.parse(localStorage.getItem('td-shell')).crt === 'on'));
  await page.evaluate(() => { const t = document.getElementById('tube'); t.style.scrollBehavior = 'auto'; t.scrollTop = 900; });
  ok('scrolling leaves the page unfiltered', (await page.evaluate(glassState)).filter === 'none');
  await page.click('.td-actions [data-td-toggle="theme"]');
  ok('theme toggle to paper', await page.evaluate(() => document.documentElement.dataset.theme === 'paper'));
  ok('paper hides the glass', !(await page.evaluate(glassState)).shown);
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

// registers: deep link, radios, copy scoped to one register
{
  const { ctx, page } = await open('/docsite/workbench.html#story', { prefs: { theme: 'glass', crt: 'off' } });
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
  await ctx.close();
}
// one top bar everywhere: the same four sections, and Install where Download was
for (const path of ['/info', '/docsite/', '/docsite/workbench.html', '/docsite/install']) {
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
  const { ctx, page } = await open('/docsite/install#technical', { prefs: { theme: 'glass', crt: 'off' } });
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
  await page.goto(BASE + '/docsite/workbench.html', { waitUntil: 'load' });
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
  const { ctx, page } = await open('/docsite/workbench.html#story', { prefs: { theme: 'glass', crt: 'off' } });
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
