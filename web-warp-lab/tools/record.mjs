// Record a webm of a warp demo while sweeping the BARREL slider, so the live
// curvature animation is captured. usage: node tools/record.mjs <file> <outdir>
import { chromium } from 'playwright';
const [, , target, outdir] = process.argv;
const browser = await chromium.launch({ channel: 'chrome', headless: true,
  args: ['--ignore-gpu-blocklist', '--enable-gpu', '--use-angle=gl'] });
const ctx = await browser.newContext({ viewport: { width: 1040, height: 680 },
  recordVideo: { dir: outdir, size: { width: 1040, height: 680 } } });
const page = await ctx.newPage();
await page.goto('file://' + target, { waitUntil: 'networkidle' });
await page.waitForTimeout(600);
const setCurv = (v) => page.evaluate((val) => {
  const s = document.getElementById('curv'); s.value = val;
  s.dispatchEvent(new Event('input', { bubbles: true }));
}, v);
// sweep up, hold, down — a smooth curvature breathing
const seq = [];
for (let v = 0; v <= 100; v += 4) seq.push(v);
for (let v = 100; v >= 30; v -= 4) seq.push(v);
for (let v = 30; v <= 60; v += 3) seq.push(v);
for (const v of seq) { await setCurv(v); await page.waitForTimeout(70); }
await page.waitForTimeout(400);
await ctx.close(); // finalizes the video
await browser.close();
console.log('recorded to', outdir);
