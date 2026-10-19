// Q4. Modals. For each content <dialog> (not notes.js's own d-note / d-export): open it
// through its [data-dlg] opener the way a click would, screenshot the dialog element, and
// collect the .notable anchors inside it with rects relative to the dialog's own box.
// Also record whether the dialog's content scrolls inside it (then one image is not all of it).
import { writeFileSync, mkdirSync } from 'node:fs';
import { launch, ms, fileUrl, SCRATCH, PANE, SAVED_EXAMPLE, pngSize } from './lib.mjs';

const TARGETS = [process.argv[2] || '/home/parker/Work/reports/2026-09-12-agent-native-output-surface.html', SAVED_EXAMPLE];
const OUT = `${SCRATCH}/q4`;
mkdirSync(OUT, { recursive: true });

const b = await launch();
const results = [];
for (const path of TARGETS) {
  const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
  const page = await ctx.newPage();
  await page.goto(fileUrl(path), { waitUntil: 'load' });
  await page.evaluate(() => document.fonts.ready);
  const dialogs = await page.evaluate(() => Array.from(document.querySelectorAll('dialog'))
    .filter((d) => !['d-note', 'd-export'].includes(d.id))
    .map((d) => ({ id: d.id, openers: document.querySelectorAll(`[data-dlg="${d.id}"]`).length })));
  const tAll = performance.now();
  const per = [];
  for (const d of dialogs) {
    const t0 = performance.now();
    // Click the opener as a reader would. Some briefs ship [data-dlg] buttons with no
    // script wiring them (the click does nothing in a browser either); then open the
    // dialog directly, which is what TD would do from the declarative mapping anyway.
    const how = await page.evaluate((id) => {
      const opener = document.querySelector(`[data-dlg="${id}"]`);
      const dlg = document.getElementById(id);
      if (opener) opener.click();
      if (dlg.open) return 'opener-click';
      dlg.showModal();
      return opener ? 'showModal (opener present but dead)' : 'showModal (no opener)';
    }, d.id);
    const openMs = ms(t0);
    const info = await page.evaluate((id) => {
      const dlg = document.getElementById(id);
      const r = dlg.getBoundingClientRect();
      // the element that actually scrolls: the dialog, or a body inside it
      const scrollers = [dlg, ...dlg.querySelectorAll('*')].filter((el) => el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== 'visible');
      return {
        open: dlg.open,
        rect: { x: r.x, y: r.y, w: r.width, h: r.height },
        scrolls: scrollers.map((el) => ({ el: el === dlg ? 'dialog' : el.className || el.tagName, scrollHeight: el.scrollHeight, clientHeight: el.clientHeight })),
        anchors: Array.from(dlg.querySelectorAll('.notable')).map((el) => {
          const a = el.getBoundingClientRect();
          return { nid: el.dataset.nid, title: el.dataset.ntitle, rect: { x: a.x - r.x, y: a.y - r.y, w: a.width, h: a.height }, insideVisibleBox: a.bottom <= r.bottom && a.top >= r.top };
        }),
        links: Array.from(dlg.querySelectorAll('a[href]')).length,
      };
    }, d.id);
    const t1 = performance.now();
    const png = await page.locator(`#${d.id}`).screenshot({ type: 'png' });
    const shotMs = ms(t1);
    writeFileSync(`${OUT}/${d.id}.png`, png);
    // The same capture as a raw CDP clip, without Playwright's font/stability waits.
    const cdp = await ctx.newCDPSession(page);
    const t2 = performance.now();
    await cdp.send('Page.captureScreenshot', { format: 'png', optimizeForSpeed: true, clip: { ...{ x: info.rect.x, y: info.rect.y, width: info.rect.w, height: info.rect.h }, scale: 1 } });
    const cdpShotMs = ms(t2);
    // A dialog whose body scrolls is not all in one image. Grow the viewport until the
    // dialog's max-height (a share of the viewport) stops clamping it, then capture again.
    let grown = null;
    if (info.scrolls.length) {
      const need = Math.max(...info.scrolls.map((s) => s.scrollHeight - s.clientHeight));
      const t3 = performance.now();
      let vh = PANE.height;
      for (let k = 0; k < 6; k++) {
        vh = Math.ceil(vh + need * 1.25);
        await page.setViewportSize({ width: PANE.width, height: vh });
        const still = await page.evaluate((id) => {
          const dlg = document.getElementById(id);
          return [dlg, ...dlg.querySelectorAll('*')].some((el) => el.scrollHeight > el.clientHeight + 1 && getComputedStyle(el).overflowY !== 'visible');
        }, d.id);
        if (!still) break;
      }
      const g = await page.locator(`#${d.id}`).screenshot({ type: 'png' });
      writeFileSync(`${OUT}/${d.id}-grown.png`, g);
      grown = { viewportHeightCss: vh, png: pngSize(g), ms: ms(t3) };
      await page.setViewportSize({ width: PANE.width, height: PANE.height });
    }
    await page.evaluate((id) => document.getElementById(id).close(), d.id);
    per.push({ id: d.id, how, openers: d.openers, openMs, shotMs, cdpShotMs, totalMs: ms(t0), png: pngSize(png), pngBytes: png.length, ...info,
      anchorCount: info.anchors.length, anchorsClippedByScroll: info.anchors.filter((a) => !a.insideVisibleBox).length, grown });
  }
  results.push({ file: path, contentDialogs: dialogs.length, allDialogsMs: ms(tAll), perDialog: per.map(({ anchors, ...rest }) => rest), sampleAnchors: per[0] ? per[0].anchors.slice(0, 3) : [] });
  await ctx.close();
}
await b.close();
console.log(JSON.stringify(results, null, 1));
