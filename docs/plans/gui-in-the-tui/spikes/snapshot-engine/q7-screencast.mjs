// Q7a. A live engine, cheaply: Page.startScreencast on one brief at 968x1400 CSS, scale 1.6.
// Measures frames per second while scrolling with the mouse wheel, frame sizes, and the
// CPU of the whole Chromium process tree (browser + gpu + renderer), idle and scrolling.
// CPU percent is per core (100 = one core busy); USER_HZ is 100 here.
// Two launches: headless defaults (software GL), and the Vulkan flags that gave WebGPU
// the NVIDIA adapter in q3-gpu-limits-browser.mjs.
import { launch, fileUrl, PANE, SAVED_EXAMPLE, chromeTree, cpuTicks, pssKiB } from './lib.mjs';

const CONFIGS = {
  headlessDefault: [],
  headlessVulkan: ['--enable-features=Vulkan,VulkanFromANGLE,DefaultANGLEVulkan', '--use-angle=vulkan', '--ignore-gpu-blocklist', '--enable-gpu'],
};
const FORMATS = [
  { format: 'jpeg', quality: 80 },
  { format: 'png' },
];
const sleep = (t) => new Promise((r) => setTimeout(r, t));
const out = {};

for (const [cname, args] of Object.entries(CONFIGS)) {
  for (const fmt of FORMATS) {
    const marker = `--td-spike-marker=q7-${cname}-${fmt.format}`;
    const b = await launch([...args, marker]);
    const ctx = await b.newContext({ viewport: { width: PANE.width, height: PANE.height }, deviceScaleFactor: PANE.deviceScaleFactor });
    const page = await ctx.newPage();
    await page.goto(fileUrl(SAVED_EXAMPLE), { waitUntil: 'load' });
    const cdp = await ctx.newCDPSession(page);
    const frames = [];
    cdp.on('Page.screencastFrame', (f) => {
      const head = Buffer.from(f.data.slice(0, 44), 'base64');
      const png = head[1] === 0x50 && head[2] === 0x4e ? `${head.readUInt32BE(16)}x${head.readUInt32BE(20)}` : null;
      frames.push({ t: performance.now(), bytes: Math.round(f.data.length * 3 / 4), w: f.metadata.deviceWidth, h: f.metadata.deviceHeight, scrollY: f.metadata.scrollOffsetY, png });
      cdp.send('Page.screencastFrameAck', { sessionId: f.sessionId }).catch(() => {});
    });
    await cdp.send('Page.startScreencast', { ...fmt, maxWidth: Math.round(PANE.width * PANE.deviceScaleFactor), maxHeight: Math.round(PANE.height * PANE.deviceScaleFactor), everyNthFrame: 1 });
    await sleep(800);
    const pids = chromeTree(marker);

    // idle: screencast running, nothing changing
    let f0 = frames.length, c0 = cpuTicks(pids), t0 = performance.now();
    await sleep(3000);
    const idle = { seconds: (performance.now() - t0) / 1000, frames: frames.length - f0, cpuPercent: +((cpuTicks(pids) - c0) / ((performance.now() - t0) / 1000)).toFixed(0) };

    // scrolling: wheel 60 steps/s of 40 CSS px for 5 s
    f0 = frames.length; c0 = cpuTicks(pids); t0 = performance.now();
    await page.mouse.move(400, 600);
    for (let i = 0; i < 300; i++) { await page.mouse.wheel(0, 40); await sleep(16); }
    const secs = (performance.now() - t0) / 1000;
    const scrolled = frames.slice(f0);
    const scroll = {
      seconds: +secs.toFixed(2), frames: scrolled.length, fps: +(scrolled.length / secs).toFixed(1),
      cpuPercent: +((cpuTicks(pids) - c0) / secs).toFixed(0),
      medianFrameBytes: scrolled.map((f) => f.bytes).sort((a, b) => a - b)[scrolled.length >> 1],
      pngPixels: scrolled[0] ? scrolled[0].png : null,
      frameSize: scrolled[0] ? `${Math.round(scrolled[0].w * PANE.deviceScaleFactor)}x${Math.round(scrolled[0].h * PANE.deviceScaleFactor)} (metadata ${scrolled[0].w}x${scrolled[0].h} CSS)` : null,
      scrolledCss: scrolled.length ? scrolled[scrolled.length - 1].scrollY - scrolled[0].scrollY : 0,
      maxGapMs: scrolled.slice(1).reduce((m, f, i) => Math.max(m, f.t - scrolled[i].t), 0).toFixed(0),
    };
    // ceiling: the page scrolls itself every animation frame (no input round trips), 5 s
    await page.evaluate(() => window.scrollTo(0, 0));
    await sleep(300);
    f0 = frames.length; c0 = cpuTicks(pids); t0 = performance.now();
    await page.evaluate(() => new Promise((res) => {
      const end = performance.now() + 5000;
      const step = () => { window.scrollBy(0, 6); if (performance.now() < end) requestAnimationFrame(step); else res(); };
      requestAnimationFrame(step);
    }));
    const rs = (performance.now() - t0) / 1000;
    const rafFrames = frames.length - f0;
    const rafScroll = { seconds: +rs.toFixed(2), frames: rafFrames, fps: +(rafFrames / rs).toFixed(1), cpuPercent: +((cpuTicks(pids) - c0) / rs).toFixed(0) };
    const pss = +(pssKiB(pids) / 1024).toFixed(0);
    await cdp.send('Page.stopScreencast');
    await b.close();
    out[`${cname}/${fmt.format}`] = { processes: pids.length, pssMiB: pss, idle, wheelScroll: scroll, rafScroll };
    process.stderr.write(`${cname}/${fmt.format} `);
  }
}
console.log('\n' + JSON.stringify(out, null, 1));
