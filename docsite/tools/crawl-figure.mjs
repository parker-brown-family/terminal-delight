// Draws the text-crawl page's figure: a pane's centred rows flat, and the same
// rows under the crawl at its defaults. The mapping is the one in
// docs/patches/0003-text-crawl.patch: a content row at tc (0 at the bottom, 1 at
// the top) lands at screen height vb = ln(1 + (D − 1)·tc) / ln(D), and its width
// is (1 − tc) + a·tc, where a = clamp(1 − 1.3·sin(angle), 0.2, 1) (theme.rs).
// Each row of text is drawn as a bar, so the picture shows the geometry and no
// invented words.
//
//   node docsite/tools/crawl-figure.mjs > /tmp/crawl.svg

const W = 300, H = 190, GAP = 60, PAD = 20, TOP = 34;
const ANGLE = 12, DEPTH = 2.5;
const a = Math.min(1, Math.max(0.2, 1 - 1.3 * Math.sin((ANGLE * Math.PI) / 180)));
const ROWS = [0.62, 0.8, 0.7, 0.86, 0.5, 0.0, 0.74, 0.9, 0.66, 0.82, 0.58, 0.0, 0.78, 0.7, 0.4];

const vbOf = (tc) => (Math.abs(DEPTH - 1) < 1e-3 ? tc : Math.log(1 + (DEPTH - 1) * tc) / Math.log(DEPTH));
const widthOf = (tc) => (1 - tc) + a * tc;
const fmt = (n) => n.toFixed(1);

function panel(on, ox, label, sub) {
  // content row i occupies tc in [t0, t1]; row 0 is the top row of the pane
  const n = ROWS.length, out = [];
  const pt = (u, tc) => {
    const w = on ? widthOf(tc) : 1, vb = on ? vbOf(tc) : tc;
    return [ox + (0.5 + (u - 0.5) * w) * W, TOP + (1 - vb) * H];
  };
  if (on) {
    const q = [pt(0, 0), pt(1, 0), pt(1, 1), pt(0, 1)];
    out.push(`<path class="plane" d="M${q.map((p) => p.map(fmt).join(' ')).join('L')}Z"/>`);
  }
  ROWS.forEach((len, i) => {
    if (!len) return;
    const t1 = 1 - i / n, t0 = 1 - (i + 1) / n;              // top and bottom of the row
    const b0 = t0 + (t1 - t0) * 0.22, b1 = t1 - (t1 - t0) * 0.22;
    const u0 = 0.5 - len * 0.42, u1 = 0.5 + len * 0.42;
    const q = [pt(u0, b0), pt(u1, b0), pt(u1, b1), pt(u0, b1)];
    out.push(`<path class="row" d="M${q.map((p) => p.map(fmt).join(' ')).join('L')}Z"/>`);
  });
  return [
    `<rect class="frame" x="${ox}" y="${TOP}" width="${W}" height="${H}" rx="6"/>`,
    ...out,
    `<text class="t1" x="${ox}" y="20">${label}</text>`,
    `<text class="t2" x="${ox + W}" y="20" text-anchor="end">${sub}</text>`,
  ].join('\n    ');
}

console.error(`a=${a.toFixed(3)}: the top row is ${(a * 100).toFixed(0)}% as wide as the bottom`);
console.error(`half the pane's height holds the bottom ${(((Math.pow(DEPTH, 0.5) - 1) / (DEPTH - 1)) * 100).toFixed(1)}% of the rows`);

const VW = PAD * 2 + W * 2 + GAP, VH = TOP + H + PAD;
console.log(`<svg class="crawl" viewBox="0 0 ${VW} ${VH}" role="img" aria-label="Two panes of centred text drawn as bars. On the left, crawl off: every row the same size. On the right, crawl on at its defaults: the rows lean back into a trapezoid, largest at the bottom and narrowing toward the top, with black on either side.">
  <g transform="translate(${PAD} 0)">
    ${panel(false, 0, 'crawl off', 'centred rows')}
    ${panel(true, W + GAP, 'crawl on', `${ANGLE}°, depth ${DEPTH}×`)}
  </g>
</svg>`);
