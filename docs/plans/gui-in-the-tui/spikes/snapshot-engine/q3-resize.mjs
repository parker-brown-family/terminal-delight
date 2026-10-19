// Q3 follow-up 3: a TD pane changes width (split, drag). The page must lay out again at
// the new width; anchors keep their ids (q1) but every rect moves. What does a relayout
// plus re-extraction plus the first visible tile cost on a page that is already loaded?
import { launch, ms, fileUrl, PANE, SAVED_EXAMPLE, EXTRACT_ANCHORS } from './lib.mjs';

const b = await launch();
const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
const page = await ctx.newPage();
await page.goto(fileUrl(SAVED_EXAMPLE), { waitUntil: 'load' });
await page.evaluate(EXTRACT_ANCHORS);
const cdp = await ctx.newCDPSession(page);
const rows = [];
for (const w of [700, 968, 1200, 600, 968]) {
  const t0 = performance.now();
  await page.setViewportSize({ width: w, height: PANE.height });
  const anchors = await page.evaluate(EXTRACT_ANCHORS);
  const relayoutAndAnchorsMs = ms(t0);
  const h = await page.evaluate(() => document.documentElement.scrollHeight);
  const t1 = performance.now();
  await cdp.send('Page.captureScreenshot', { format: 'png', optimizeForSpeed: true });
  rows.push({ width: w, heightCss: h, anchors: anchors.length, relayoutAndAnchorsMs, firstViewportCaptureMs: ms(t1), totalMs: ms(t0) });
}
await b.close();
console.log(JSON.stringify(rows, null, 1));
