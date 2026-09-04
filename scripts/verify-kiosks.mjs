#!/usr/bin/env node
/* ==========================================================================
   Verifies the kiosk family against a running site.

     node scripts/verify-kiosks.mjs http://127.0.0.1:8791
     node scripts/verify-kiosks.mjs https://terminal-delight.brownfamilysports.com

   This exists because the previous verification harness lived in a session
   scratchpad, was recommended for the repo in the theme-spine handoff, and
   then was deleted along with the scratchpad — so the next person had nothing
   to run. It asserts on computed style and on the console, not on a
   screenshot, because the thing that kept going wrong on this site was
   invisible in screenshots: a 404 on every page load.

   It takes Playwright from wherever it is already installed on this box and
   drives the system chromium, so it downloads nothing.
   ========================================================================== */

import { createRequire } from 'node:module';
import { existsSync } from 'node:fs';

const BASE = (process.argv[2] || 'http://127.0.0.1:8791').replace(/\/$/, '');

const PW_CANDIDATES = [
  '/home/parker/BROWN-FAMILY-SPORTS/Software/wellness-with-kate-site/node_modules/playwright/index.js',
  '/home/parker/ai-garrison/node_modules/playwright/index.js',
];
const pwPath = PW_CANDIDATES.find(existsSync);
if (!pwPath) {
  console.error('No playwright install found. Looked in:\n  ' + PW_CANDIDATES.join('\n  '));
  process.exit(2);
}
const { chromium } = await createRequire(import.meta.url)(pwPath);

/* id, path, and whether the theme is expected to reach :root. The cabinets
   are painted on the strip only — see assets/kiosk-chrome.js. */
const KIOSKS = [
  { id: 'info',    path: '/info.html',        root: true  },
  { id: 'omarchy', path: '/omarchy.html',     root: true  },
  { id: 'agents',  path: '/agents.html',      root: true  },
  { id: 'tv',      path: '/tv.html',          root: false },
  { id: 'global',  path: '/global.html',      root: false },
  { id: 'gamba',   path: '/gamba.html',       root: false },
  { id: 'crawl',   path: '/start-crawl.html', root: false },
];

/* Parker runs tiled panes. 968x1790 and 390x844 are real widths on this
   machine, not hypothetical ones, and a gate above them has shipped a
   feature that did not exist for him before. */
const VIEWPORTS = [
  { w: 1800, h: 1000 }, { w: 1440, h: 900 }, { w: 968, h: 1790 },
  { w: 900,  h: 900  }, { w: 760,  h: 900 }, { w: 390, h: 844  },
];

let pass = 0, fail = 0;
const failures = [];
function check(name, ok, detail) {
  if (ok) { pass++; }
  else { fail++; failures.push(`${name}${detail ? ' — ' + detail : ''}`); }
}

const browser = await chromium.launch({ executablePath: '/usr/bin/chromium' });
const ctx = await browser.newContext();

/* ---- 1. the favicon, which is the whole of #254 ------------------------ */
{
  const page = await ctx.newPage();
  const r = await page.goto(BASE + '/favicon.ico');
  check('favicon.ico serves 200', r && r.status() === 200, r ? `got ${r.status()}` : 'no response');
  const s = await page.goto(BASE + '/favicon.svg');
  check('favicon.svg serves 200', s && s.status() === 200, s ? `got ${s.status()}` : 'no response');
  await page.close();
}

/* ---- 2. every kiosk: clean console, head parity, strip present --------- */
for (const k of KIOSKS) {
  const page = await ctx.newPage();
  const errors = [];
  page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
  page.on('pageerror', (e) => errors.push('pageerror: ' + e.message));
  page.on('requestfailed', (rq) => { if (rq.url().startsWith(BASE)) errors.push('requestfailed: ' + rq.url()); });

  await page.goto(BASE + k.path, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(350);   // let the strip mount and the wall paint

  check(`${k.id}: no console errors`, errors.length === 0, errors.slice(0, 3).join(' | '));

  const head = await page.evaluate(() => ({
    icons: document.querySelectorAll('link[rel="icon"]').length,
    og: document.querySelectorAll('meta[property^="og:"]').length,
    csp: !!document.querySelector('meta[http-equiv="Content-Security-Policy"]'),
    title: document.title,
  }));
  check(`${k.id}: declares an icon`, head.icons >= 1, `${head.icons} found`);
  check(`${k.id}: has social tags`, head.og >= 4, `${head.og} og tags`);
  check(`${k.id}: has a CSP`, head.csp);

  const strip = await page.evaluate(() => {
    const s = document.querySelector('.kiosk-strip');
    if (!s) return null;
    return {
      links: s.querySelectorAll('a.k-link').length,
      here: (s.querySelector('.k-here') || {}).textContent || null,
      rail: s.querySelectorAll('.kiosk-chip').length,
    };
  });
  check(`${k.id}: family strip renders`, !!strip);
  if (strip) {
    /* Six links plus the page itself named as current: no kiosk is a dead
       end any more, and none of them links to itself. */
    check(`${k.id}: strip links the other six`, strip.links === 6, `${strip.links} links`);
    check(`${k.id}: strip marks the current page`, !!strip.here);
  }
  await page.close();
}

/* ---- 3. the pick travels, and lands where it should -------------------- */
{
  const page = await ctx.newPage();
  await page.goto(BASE + '/info.html', { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(350);

  /* Probe the role variable AND a real element's rendered colour. The obvious
     probe — body's backgroundColor — is useless here and quietly so: the page
     paints itself with a radial-gradient, so background-color is transparent
     before and after, and an assertion on it reads "no repaint" on a page that
     is repainting perfectly. */
  const snap = () => page.evaluate(() => ({
    bg: getComputedStyle(document.documentElement).getPropertyValue('--bg').trim(),
    ink: getComputedStyle(document.querySelector('p, a, li') || document.body).color,
  }));
  const before = await snap();
  await page.evaluate(() => {
    const chip = document.querySelector('.kiosk-chip[data-name="tokyo-night"]');
    if (chip) chip.click();
  });
  await page.waitForTimeout(120);
  const after = await snap();
  check('info repaints when a theme is picked',
    after.bg === '#1a1b26' && before.ink !== after.ink,
    `--bg ${before.bg} -> ${after.bg}; ink ${before.ink} -> ${after.ink}`);

  /* Same storage, different page: this is the thing that did not exist
     before — a theme chosen on one kiosk being worn by the next. */
  await page.goto(BASE + '/agents.html', { waitUntil: 'domcontentloaded' });
  const carried = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
  check('the pick travels to agents', carried === 'tokyo-night', `data-theme=${carried}`);

  await page.goto(BASE + '/omarchy.html', { waitUntil: 'domcontentloaded' });
  const carried2 = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
  check('the pick travels to omarchy', carried2 === 'tokyo-night', `data-theme=${carried2}`);

  /* And the cabinets do NOT get repainted: GAMBA keeps its own red. A root
     paint would have silently replaced it with the palette's. */
  await page.goto(BASE + '/gamba.html', { waitUntil: 'domcontentloaded' });
  const gamba = await page.evaluate(() => ({
    red: getComputedStyle(document.documentElement).getPropertyValue('--red').trim(),
    stripFg: getComputedStyle(document.querySelector('.kiosk-strip')).getPropertyValue('--fg').trim(),
  }));
  check('gamba keeps its own --red', gamba.red === '#f75a33', `--red=${gamba.red}`);
  check('gamba strip wears the theme', gamba.stripFg.length > 0, `--fg on strip = "${gamba.stripFg}"`);

  await page.evaluate(() => localStorage.clear());
  await page.close();
}

/* ---- 4. the wall's vitals ---------------------------------------------- */
{
  const page = await ctx.newPage();
  await page.goto(BASE + '/agents.html', { waitUntil: 'domcontentloaded' });
  const wall = await page.evaluate(() => {
    const cards = [...document.querySelectorAll('.card')];
    const rows = [...document.querySelectorAll('.vrow')];
    const bad = cards.filter((c) => {
      const call = c.querySelector('.call');
      if (!call) return false;
      /* The rule from #275: a bar cannot look calm while the chip says act. */
      const acting = ['STOP', 'HAND OFF', 'COMPACT'].includes(call.textContent.trim());
      const coloured = [...c.querySelectorAll('.vrow')].some((r) => r.classList.contains('t-warn') || r.classList.contains('t-bad'));
      return acting && !coloured;
    });
    return {
      cards: cards.length,
      bars: rows.length,
      calls: [...document.querySelectorAll('.call')].map((e) => e.textContent.trim()),
      labels: [...new Set([...document.querySelectorAll('.vlab')].map((e) => e.textContent))],
      disagreeing: bad.length,
      codexBlank: document.querySelectorAll('.vitals.none').length,
    };
  });
  check('wall renders cards', wall.cards > 0, `${wall.cards} cards`);
  check('vitals bars render', wall.bars >= 30, `${wall.bars} bars`);
  check('the three bars are the right three',
    ['CTX WINDOW', 'FATIGUE', 'RELEVANCE'].every((l) => wall.labels.includes(l)), wall.labels.join('/'));
  check('no card acts while every bar reads calm', wall.disagreeing === 0, `${wall.disagreeing} disagree`);
  check('the wall shows more than one verdict', new Set(wall.calls).size >= 3, wall.calls.join(','));
  check('codex panes draw no bars', wall.codexBlank > 0, `${wall.codexBlank} blank`);
  await page.close();
}

/* ---- 5. no page scrolls sideways, at any width Parker uses ------------- */
for (const k of KIOSKS) {
  const page = await ctx.newPage();
  for (const v of VIEWPORTS) {
    await page.setViewportSize({ width: v.w, height: v.h });
    await page.goto(BASE + k.path, { waitUntil: 'domcontentloaded' });
  await page.waitForTimeout(350);   // let the strip mount and the wall paint
    const over = await page.evaluate(() =>
      Math.max(0, document.documentElement.scrollWidth - document.documentElement.clientWidth));
    check(`${k.id} @ ${v.w}x${v.h}: no horizontal overflow`, over <= 1, `${over}px`);
  }
  await page.close();
}

await browser.close();

console.log(`\n${pass} passed, ${fail} failed`);
if (fail) {
  console.log('\nFailures:');
  for (const f of failures) console.log('  ✕ ' + f);
  process.exit(1);
}
console.log('All kiosk checks green.');
