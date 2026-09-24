// Q1. Load Parker's saved brief at a pane-like viewport, let its own notes.js run,
// extract every .notable element's data-nid / data-ntitle / page rect, and check:
//   - the ids equal the keys of the brief's own report-notes island
//   - the ids do not change with viewport width or device scale
//   - how long extraction takes
import { readFileSync, writeFileSync } from 'node:fs';
import { launch, ms, fileUrl, SAVED_EXAMPLE, SCRATCH, EXTRACT_ANCHORS, PANE } from './lib.mjs';

const target = process.argv[2] || SAVED_EXAMPLE;
const src = readFileSync(target, 'utf8');
const islandMatch = src.match(/<script\b[^>]*\bid="report-notes"[^>]*>([\s\S]*?)<\/script>/);
const island = islandMatch ? JSON.parse(islandMatch[1].trim() || '{}') : null;

const t0 = performance.now();
const browser = await launch();
const launchMs = ms(t0);

async function anchorsAt(width, dsf, repeats = 1) {
  const ctx = await browser.newContext({ viewport: { width, height: PANE.height }, deviceScaleFactor: dsf });
  const page = await ctx.newPage();
  const tl = performance.now();
  await page.goto(fileUrl(target), { waitUntil: 'load' });
  const loadMs = ms(tl);
  const times = [];
  let anchors;
  for (let i = 0; i < repeats; i++) {
    const te = performance.now();
    anchors = await page.evaluate(EXTRACT_ANCHORS);
    times.push(ms(te));
  }
  const docH = await page.evaluate(() => document.documentElement.scrollHeight);
  await ctx.close();
  return { width, dsf, loadMs, extractMs: times, anchors, docH };
}

const base = await anchorsAt(PANE.width, PANE.deviceScaleFactor, 20);
const variants = [];
for (const [w, d] of [[600, 1.6], [968, 1], [1400, 1.6], [2000, 1], [390, 3]]) variants.push(await anchorsAt(w, d));
await browser.close();

const ids = base.anchors.map((a) => a.nid);
const sameIds = variants.map((v) => ({
  width: v.width, dsf: v.dsf, docH: v.docH,
  identicalOrderedIds: JSON.stringify(v.anchors.map((a) => a.nid)) === JSON.stringify(ids),
  identicalTitles: JSON.stringify(v.anchors.map((a) => a.title)) === JSON.stringify(base.anchors.map((a) => a.title)),
}));

let islandCheck = null;
if (island) {
  const keys = Object.keys(island);
  const byId = Object.fromEntries(base.anchors.map((a) => [a.nid, a]));
  islandCheck = {
    islandKeys: keys.length,
    keysFoundAsAnchors: keys.filter((k) => byId[k]).length,
    missing: keys.filter((k) => !byId[k]),
    titleMismatches: keys.filter((k) => byId[k]).flatMap((k) => island[k]
      .filter((n) => n.title !== byId[k].title)
      .map((n) => ({ k, baked: n.title, live: byId[k].title }))),
    anchorsForKeys: keys.filter((k) => byId[k]).map((k) => ({ k, dialog: byId[k].dialog, rect: byId[k].rect, rendered: byId[k].rendered })),
  };
}

const sorted = base.extractMs.slice(1).sort((a, b) => a - b);
const out = {
  file: target,
  browserLaunchMs: launchMs,
  loadMs: base.loadMs,
  docHeightCss: base.docH,
  anchorCount: base.anchors.length,
  anchorsInDialogs: base.anchors.filter((a) => a.dialog).length,
  anchorsNotRendered: base.anchors.filter((a) => !a.rendered).length,
  extractMs: { first: base.extractMs[0], median: sorted[sorted.length >> 1], max: sorted[sorted.length - 1] },
  anchorsJsonBytes: Buffer.byteLength(JSON.stringify(base.anchors)),
  widthInvariance: sameIds,
  islandCheck,
  sample: base.anchors.slice(0, 4),
};
writeFileSync(`${SCRATCH}/q1-anchors.json`, JSON.stringify({ ...out, anchors: base.anchors }, null, 1));
console.log(JSON.stringify(out, null, 1));
