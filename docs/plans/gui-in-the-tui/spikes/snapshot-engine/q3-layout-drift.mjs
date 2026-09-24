// Q3 follow-up: q1 measured the longest brief at 16172 CSS px after 'load'; q3 measured
// 16189 after taking full-page screenshots. Does a full-page capture change the layout
// the anchors were measured on? Measure height and every anchor rect before and after.
import { launch, fileUrl, PANE, EXTRACT_ANCHORS } from './lib.mjs';

const FILES = [
  '/home/parker/Work/reports/2026-09-21-microsurvey-invoicing-harness.html',
  '/home/parker/Work/terminal-delight/reports/2026-09-19-the-rodeo-checklist.html',
  '/home/parker/Downloads/2026-09-24-gui-in-the-tui.html',
];
const b = await launch();
for (const f of FILES) {
  const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  const p = await ctx.newPage();
  await p.goto(fileUrl(f), { waitUntil: 'load' });
  await p.evaluate(() => document.fonts.ready);
  const h0 = await p.evaluate(() => document.documentElement.scrollHeight);
  const a0 = await p.evaluate(EXTRACT_ANCHORS);
  await p.screenshot({ fullPage: true });
  const h1 = await p.evaluate(() => document.documentElement.scrollHeight);
  const a1 = await p.evaluate(EXTRACT_ANCHORS);
  const moved = a0.map((a, i) => a1[i].rect.y - a.rect.y).filter((d) => Math.abs(d) > 0.5);
  // What grew? Elements whose height changed.
  const vhUsers = await p.evaluate(() => Array.from(document.querySelectorAll('body *')).filter((el) => {
    const cs = getComputedStyle(el);
    return /vh|svh|dvh|lvh/.test(el.getAttribute('style') || '') || cs.position === 'sticky';
  }).length);
  console.log(JSON.stringify({ file: f.split('/').pop(), heightBefore: h0, heightAfterFullPageShot: h1, anchorsMoved: moved.length, maxShift: moved.length ? Math.max(...moved.map(Math.abs)) : 0, firstMovedIndex: a0.findIndex((a, i) => Math.abs(a1[i].rect.y - a.rect.y) > 0.5), vhOrStickyInline: vhUsers }));
  await ctx.close();
}
await b.close();
