// Q3. The page image: full-page render at 968 CSS px x scale 1.6 for representative briefs.
//   cold  = browser launch + context + goto + full-page PNG (3 runs, median)
//   warm  = browser already running: new page + goto + full-page PNG (3 runs, median)
//   again = page already loaded: full-page PNG only (3 runs, median)
//   viewport = page loaded, scrolled: 968x1400 viewport PNG (10 positions, median)
//   tile  = page loaded: clip of one 4096-device-px-tall tile (968 x 2560 CSS) (5 runs, median).
//           Playwright clips relative to the viewport unless fullPage is set, so clips use fullPage.
// PNGs land in /tmp/claude-1000/td-snapshot-spike/q3 for the Rust decode probe.
// A correctness check follows: is the bottom of the full-page PNG the same pixels as a
// clip screenshot of that region (tall captures have been known to repeat or go blank)?
import { writeFileSync, mkdirSync, readFileSync } from 'node:fs';
import { basename } from 'node:path';
import { launch, ms, fileUrl, SCRATCH, SAVED_EXAMPLE, PANE, chromeTree, pssKiB, pngSize } from './lib.mjs';

const BRIEFS = {
  short: '/home/parker/Work/reports/2026-09-24-concur-stamp.html',
  median: '/home/parker/Work/terminal-delight/reports/2026-09-19-a-closed-pipe-is-not-a-panic.html',
  savedExample: SAVED_EXAMPLE,
  long: '/home/parker/Work/terminal-delight/reports/2026-09-19-the-rodeo-checklist.html',
  longest: '/home/parker/Work/reports/2026-09-21-microsurvey-invoicing-harness.html',
  heaviestBytes: '/home/parker/Work/reports/2026-09-12-theme-foundry-interview.html',
};
const OUT = `${SCRATCH}/q3`;
mkdirSync(OUT, { recursive: true });
const med = (a) => { const s = a.slice().sort((x, y) => x - y); return s[s.length >> 1]; };
const CTX = { viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor };
const TILE_DEV = 4096, TILE_CSS = TILE_DEV / PANE.deviceScaleFactor;

const rows = {};
for (const [label, path] of Object.entries(BRIEFS)) {
  const row = { file: basename(path) };
  // cold
  const cold = [];
  for (let i = 0; i < 3; i++) {
    const t0 = performance.now();
    const b = await launch();
    const tl = ms(t0);
    const ctx = await b.newContext(CTX);
    const p = await ctx.newPage();
    await p.goto(fileUrl(path), { waitUntil: 'load' });
    const buf = await p.screenshot({ fullPage: true, type: 'png' });
    cold.push({ total: ms(t0), launch: tl });
    if (i === 0) writeFileSync(`${OUT}/${label}.png`, buf);
    await b.close();
  }
  row.coldMs = med(cold.map((c) => c.total));
  row.coldLaunchMs = med(cold.map((c) => c.launch));

  const marker = `--td-spike-marker=q3-${label}`;
  const b = await launch([marker]);
  const ctx = await b.newContext(CTX);
  const warm = [], again = [];
  let page;
  for (let i = 0; i < 3; i++) {
    const t0 = performance.now();
    const p = await ctx.newPage();
    await p.goto(fileUrl(path), { waitUntil: 'load' });
    await p.screenshot({ fullPage: true, type: 'png' });
    warm.push(ms(t0));
    if (page) await page.close();
    page = p;
  }
  let full;
  for (let i = 0; i < 3; i++) { const t0 = performance.now(); full = await page.screenshot({ fullPage: true, type: 'png' }); again.push(ms(t0)); }
  row.warmMs = med(warm);
  row.againMs = med(again);
  row.pssMiBWithOnePage = +(pssKiB(chromeTree(marker)) / 1024).toFixed(0);

  const hCss = await page.evaluate(() => document.documentElement.scrollHeight);
  const { w, h } = pngSize(full);
  Object.assign(row, {
    heightCss: hCss, pngW: w, pngH: h, pngBytes: full.length,
    rgbaBytes: w * h * 4, rgbaMiB: +((w * h * 4) / 1048576).toFixed(1),
  });

  // viewport screenshots at 10 scroll positions
  const vp = [];
  let vpBytes = 0;
  for (let i = 0; i < 10; i++) {
    const y = Math.floor((hCss - PANE.height) * i / 9);
    await page.evaluate((yy) => window.scrollTo(0, yy), y);
    const t0 = performance.now();
    const s = await page.screenshot({ type: 'png' });
    vp.push(ms(t0));
    vpBytes = s.length;
  }
  row.viewportShotMs = med(vp);
  row.viewportPngBytes = vpBytes;
  await page.evaluate(() => window.scrollTo(0, 0));

  // one 4096-device-px tile via clip (captureBeyondViewport)
  if (hCss > TILE_CSS) {
    const tl = [];
    let tb;
    for (let i = 0; i < 5; i++) {
      const t0 = performance.now();
      tb = await page.screenshot({ type: 'png', fullPage: true, clip: { x: 0, y: 0, width: PANE.width, height: TILE_CSS } });
      tl.push(ms(t0));
    }
    const ts = pngSize(tb);
    row.tileShotMs = med(tl);
    row.tile = `${ts.w}x${ts.h}`;
    row.tilePngBytes = tb.length;
    writeFileSync(`${OUT}/${label}-tile.png`, tb);
  }

  // correctness: bottom viewport of the full PNG vs a clip of the same region
  const yBottom = hCss - PANE.height;
  const clip = await page.screenshot({ type: 'png', fullPage: true, clip: { x: 0, y: yBottom, width: PANE.width, height: PANE.height } });
  writeFileSync(`${OUT}/${label}-bottom-clip.png`, clip);
  const cmp = await page.evaluate(async ([fullB64, clipB64, yDev]) => {
    const load = async (b64) => createImageBitmap(await (await fetch('data:image/png;base64,' + b64)).blob());
    const [f, c] = await Promise.all([load(fullB64), load(clipB64)]);
    const cv = new OffscreenCanvas(c.width, c.height);
    const g = cv.getContext('2d');
    g.drawImage(f, 0, -yDev);
    const a = g.getImageData(0, 0, c.width, c.height).data;
    g.clearRect(0, 0, c.width, c.height);
    g.drawImage(c, 0, 0);
    const bb = g.getImageData(0, 0, c.width, c.height).data;
    let diff = 0, blank = 0;
    for (let i = 0; i < a.length; i += 4) {
      if (Math.abs(a[i] - bb[i]) + Math.abs(a[i + 1] - bb[i + 1]) + Math.abs(a[i + 2] - bb[i + 2]) > 24) diff++;
      if (a[i + 3] === 0) blank++;
    }
    return { pixels: a.length / 4, differing: diff, transparentInFull: blank };
  }, [full.toString('base64'), clip.toString('base64'), Math.round(yBottom * PANE.deviceScaleFactor)]);
  row.bottomMatchesClip = cmp;

  await b.close();
  rows[label] = row;
  process.stderr.write(`${label} `);
}
writeFileSync(`${SCRATCH}/q3-render.json`, JSON.stringify(rows, null, 1));
console.log('\n' + JSON.stringify(rows, null, 1));
