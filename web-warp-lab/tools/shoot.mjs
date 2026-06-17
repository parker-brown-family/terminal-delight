// Reusable Playwright capture: screenshot (+ optional video) a page/file using
// system Chrome. Reports WebGL2/WebGPU caps and any console/page errors.
//
// usage: node tools/shoot.mjs <url|file> <out.png> '<jsonOpts>'
//   opts: { viewport:{width,height}, dpr, wait(ms), clip:{x,y,width,height},
//           video:"dir", interactions:[...], headed:bool }
import { chromium } from 'playwright';

const [, , target, out, optsRaw] = process.argv;
const o = optsRaw ? JSON.parse(optsRaw) : {};
const url = target.startsWith('http') ? target : 'file://' + target;

const browser = await chromium.launch({
  channel: 'chrome',
  headless: o.headed ? false : true,
  args: [
    '--enable-unsafe-webgpu',
    '--enable-features=Vulkan,WebGPU',
    '--ignore-gpu-blocklist',
    '--enable-gpu',
    o.swiftshader ? '--use-angle=swiftshader' : '--use-angle=gl',
  ],
});
const ctx = await browser.newContext({
  viewport: o.viewport || { width: 1280, height: 800 },
  deviceScaleFactor: o.dpr || 1,
  ...(o.video ? { recordVideo: { dir: o.video, size: o.viewport || { width: 1280, height: 800 } } } : {}),
});
const page = await ctx.newPage();
const errs = [];
page.on('console', (m) => { if (m.type() === 'error') errs.push('CONSOLE: ' + m.text()); });
page.on('pageerror', (e) => errs.push('PAGEERROR: ' + e.message));

await page.goto(url, { waitUntil: 'networkidle' }).catch((e) => errs.push('GOTO: ' + e.message));
if (o.wait) await page.waitForTimeout(o.wait);

// optional scripted interactions (e.g. drag a slider) before the shot
for (const act of o.interactions || []) {
  try {
    if (act.move) await page.mouse.move(act.move[0], act.move[1]);
    if (act.fill) await page.fill(act.fill[0], act.fill[1]);
    if (act.eval) await page.evaluate(act.eval);
    if (act.wait) await page.waitForTimeout(act.wait);
  } catch (e) { errs.push('ACT: ' + e.message); }
}

await page.screenshot({ path: out, clip: o.clip }).catch((e) => errs.push('SHOT: ' + e.message));

const caps = await page.evaluate(() => {
  const c = document.createElement('canvas');
  return {
    webgl2: !!c.getContext('webgl2'),
    webgl1: !!c.getContext('webgl'),
    webgpu: !!navigator.gpu,
    ua: navigator.userAgent,
  };
}).catch(() => ({}));

console.log(JSON.stringify({ caps, errs }, null, 0));
await ctx.close();
await browser.close();
