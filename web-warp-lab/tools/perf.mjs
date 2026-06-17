// Measure real render FPS + frame-time of a warp demo over a window of time,
// sampling rAF deltas in-page. usage: node tools/perf.mjs <url> '<jsonOpts>'
//   opts: { viewport, seconds(default 5), label }
import { chromium } from 'playwright';
const [, , target, optsRaw] = process.argv;
const o = optsRaw ? JSON.parse(optsRaw) : {};
const url = target.startsWith('http') ? target : 'file://' + target;
const browser = await chromium.launch({
  channel: 'chrome', headless: true,
  args: ['--enable-unsafe-webgpu', '--ignore-gpu-blocklist', '--enable-gpu', '--use-angle=gl'],
});
const page = await browser.newPage({ viewport: o.viewport || { width: 1280, height: 900 } });
const errs = [];
page.on('pageerror', (e) => errs.push('PAGEERROR: ' + e.message));
await page.goto(url, { waitUntil: 'networkidle' });
// install an rAF frame-time recorder
await page.evaluate(() => {
  window.__ft = []; let prev = performance.now();
  function tick(now) { window.__ft.push(now - prev); prev = now; requestAnimationFrame(tick); }
  requestAnimationFrame(tick);
});
const secs = o.seconds || 5;
await page.waitForTimeout(secs * 1000);
const stats = await page.evaluate(() => {
  const ft = window.__ft.slice(5); // drop warmup
  if (!ft.length) return null;
  const sorted = [...ft].sort((a, b) => a - b);
  const sum = ft.reduce((a, b) => a + b, 0);
  const pct = (p) => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))];
  return {
    frames: ft.length,
    avgMs: +(sum / ft.length).toFixed(2),
    avgFps: +(1000 * ft.length / sum).toFixed(1),
    p50ms: +pct(0.5).toFixed(2),
    p95ms: +pct(0.95).toFixed(2),
    worstMs: +sorted[sorted.length - 1].toFixed(2),
    cssFps: window.__fps || null,
  };
});
console.log(JSON.stringify({ label: o.label || target.split('/').pop(), stats, errs }, null, 0));
await browser.close();
