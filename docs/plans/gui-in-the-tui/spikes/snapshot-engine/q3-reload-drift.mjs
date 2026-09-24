// Q3 follow-up 2: q3 captured the longest brief at 25875 device px on a cold page and at
// 25902 on the third page opened in the same context. Repeat that sequence and measure
// scrollHeight and PNG height per page, to see whether two loads of one file can lay out
// differently (then anchors and image must come from the same page, in the same pass).
import { launch, fileUrl, PANE, pngSize } from './lib.mjs';

const f = process.argv[2] || '/home/parker/Work/reports/2026-09-21-microsurvey-invoicing-harness.html';
const b = await launch();
const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
let prev;
for (let i = 0; i < 5; i++) {
  const p = await ctx.newPage();
  await p.goto(fileUrl(f), { waitUntil: 'load' });
  const hLoad = await p.evaluate(() => document.documentElement.scrollHeight);
  const png = await p.screenshot({ fullPage: true });
  const hAfter = await p.evaluate(() => document.documentElement.scrollHeight);
  const bodyH = await p.evaluate(() => document.body.getBoundingClientRect().height);
  console.log(JSON.stringify({ page: i + 1, openPagesInContext: ctx.pages().length, hLoad, hAfter, bodyH, pngH: pngSize(png).h, pngHOverDsf: +(pngSize(png).h / PANE.deviceScaleFactor).toFixed(1) }));
  if (prev) await prev.close();
  prev = p;
}
await b.close();
