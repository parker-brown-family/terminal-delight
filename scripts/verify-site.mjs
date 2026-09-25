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
const overflow = page => page.evaluate(() => {
  const t = document.getElementById('tube');
  return { doc: document.documentElement.scrollWidth - innerWidth, tube: t ? t.scrollWidth - t.clientWidth : 0 };
});

for (const [path, name] of [['/info', 'info'], ['/docsite/', 'docs-index'], ['/docsite/workbench.html', 'workbench']]) {
  for (const w of [1440, 968, 390]) {
    for (const prefs of [{ theme: 'glass', crt: 'on' }, { theme: 'paper', crt: 'off' }, { theme: 'paper', crt: 'on' }]) {
      const { ctx, page, errors } = await open(path, { w, h: w === 390 ? 844 : 900, prefs });
      ok(`${name}@${w} ${prefs.theme}/${prefs.crt}: clean console`, errors.length === 0, errors.join(' | '));
      const o = await overflow(page);
      ok(`${name}@${w} ${prefs.theme}/${prefs.crt}: no horizontal overflow`, o.doc <= 0 && o.tube <= 1, JSON.stringify(o));
      const warp = await page.evaluate(() => document.getElementById('tube').classList.contains('warp'));
      const expectWarp = prefs.theme === 'glass' && prefs.crt === 'on' && w >= 968;
      ok(`${name}@${w} ${prefs.theme}/${prefs.crt}: warp ${expectWarp ? 'on' : 'off'}`, warp === expectWarp);
      await page.screenshot({ path: `${OUT}/${name}-${w}-${prefs.theme}-${prefs.crt}.png` });
      await ctx.close();
    }
  }
}

// toggles flip state, persist, and the barrel suspends on scroll
{
  const { ctx, page } = await open('/info', { prefs: { theme: 'glass', crt: 'off' } });
  await page.click('.td-actions [data-td-toggle="crt"]');
  ok('crt toggle turns warp on', await page.evaluate(() => document.documentElement.dataset.crt === 'on' && document.getElementById('tube').classList.contains('warp')));
  ok('crt toggle persists', await page.evaluate(() => JSON.parse(localStorage.getItem('td-shell')).crt === 'on'));
  await page.evaluate(() => { const t = document.getElementById('tube'); t.scrollTop = 900; t.dispatchEvent(new Event('scroll')); });
  ok('warp suspends while scrolling', await page.evaluate(() => document.getElementById('tube').classList.contains('scrolling')));
  await page.waitForTimeout(1500);
  ok('warp returns after scrolling stops', await page.evaluate(() => !document.getElementById('tube').classList.contains('scrolling')));
  await page.click('.td-actions [data-td-toggle="theme"]');
  ok('theme toggle to paper', await page.evaluate(() => document.documentElement.dataset.theme === 'paper'));
  ok('paper drops the warp', await page.evaluate(() => !document.getElementById('tube').classList.contains('warp')));
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
