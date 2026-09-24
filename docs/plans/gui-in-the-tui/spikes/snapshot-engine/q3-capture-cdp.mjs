// Q3, second half: is rendering only the visible region on scroll fast enough?
// Playwright's page.screenshot adds its own work (font wait, scroll-into-view). This
// talks to Page.captureScreenshot directly over a CDP session, the call a TD engine
// would make, across encodings. Longest brief, page already loaded, 10 positions each.
//   viewport: capture the 968x1400 viewport after window.scrollTo (1549x2240 device px)
//   tile:     clip a 968x2560 CSS region anywhere with captureBeyondViewport (1549x4096)
import { launch, ms, fileUrl, PANE } from './lib.mjs';

const PATH = '/home/parker/Work/reports/2026-09-21-microsurvey-invoicing-harness.html';
const med = (a) => { const s = a.slice().sort((x, y) => x - y); return s[s.length >> 1]; };
const b = await launch();
const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
const page = await ctx.newPage();
await page.goto(fileUrl(PATH), { waitUntil: 'load' });
const cdp = await ctx.newCDPSession(page);
const H = await page.evaluate(() => document.documentElement.scrollHeight);

const FORMATS = {
  png: { format: 'png' },
  pngFast: { format: 'png', optimizeForSpeed: true },
  jpeg90: { format: 'jpeg', quality: 90 },
  webp90: { format: 'webp', quality: 90 },
  webp100: { format: 'webp', quality: 100 },
};
const out = { file: PATH, heightCss: H };
for (const [name, opts] of Object.entries(FORMATS)) {
  const vp = [], vb = [], tl = [], tb = [];
  for (let i = 0; i < 10; i++) {
    const y = Math.floor((H - PANE.height) * i / 9);
    await page.evaluate((yy) => window.scrollTo(0, yy), y);
    let t0 = performance.now();
    const r = await cdp.send('Page.captureScreenshot', opts);
    vp.push(ms(t0)); vb.push(Math.round(r.data.length * 3 / 4));
    const ty = Math.floor((H - 2560) * i / 9);
    t0 = performance.now();
    const t = await cdp.send('Page.captureScreenshot', { ...opts, captureBeyondViewport: true, clip: { x: 0, y: ty, width: PANE.width, height: 2560, scale: 1 } });
    tl.push(ms(t0)); tb.push(Math.round(t.data.length * 3 / 4));
  }
  out[name] = { viewportMs: med(vp), viewportBytes: med(vb), tileMs: med(tl), tileBytes: med(tb) };
}
await b.close();
console.log(JSON.stringify(out, null, 1));
