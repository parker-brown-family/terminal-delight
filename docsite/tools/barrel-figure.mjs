// Draws the CRT page's barrel figure: a pane's content grid as the app's
// renderer bends it, flat beside the default warp. The mapping is the one in
// docs/patches/0001-td-crt-pass.patch: a screen point q, in the pane's own
// 0..1 space, shows the content at 0.5 + c·f where c = q − 0.5 and
// f = 1 + k1·|c|² + k2·|c|⁴, with k1 = 0.14·warp and k2 = 0.06·warp
// (theme.rs). To draw where a content line lands on screen, each content point
// is pushed back through that mapping by fixed-point iteration.
//
//   node docsite/tools/barrel-figure.mjs > /tmp/barrel.svg
//
// Paste the output into pages/crt.html; the numbers it prints to stderr are
// the ones the caption quotes.

const W = 300, H = 190, GAP = 60, PAD = 20, TOP = 34;
const coef = (warp) => ({ k1: 0.14 * warp, k2: 0.06 * warp });
const f = ({ k1, k2 }, cx, cy) => { const r2 = cx * cx + cy * cy; return 1 + k1 * r2 + k2 * r2 * r2; };

// content point (lx, ly) → screen point, or null if it falls outside the tube
function toScreen(k, lx, ly) {
  const tx = lx - 0.5, ty = ly - 0.5;
  let cx = tx, cy = ty;
  for (let i = 0; i < 40; i++) { const m = f(k, cx, cy); cx = tx / m; cy = ty / m; }
  return [cx + 0.5, cy + 0.5];
}

const fmt = (n) => n.toFixed(1);
function panel(warp, ox, label, sub) {
  const k = coef(warp);
  const px = (p) => [ox + p[0] * W, TOP + p[1] * H];
  const line = (pts) => 'M' + pts.map((p) => px(p).map(fmt).join(' ')).join('L');
  const N = warp ? 24 : 1, paths = [];                     // a flat line needs two points
  const rim = [];                                           // the content's edge, clockwise
  for (let i = 0; i <= N; i++) rim.push(toScreen(k, i / N, 0));
  for (let j = 1; j <= N; j++) rim.push(toScreen(k, 1, j / N));
  for (let i = N - 1; i >= 0; i--) rim.push(toScreen(k, i / N, 1));
  for (let j = N - 1; j > 0; j--) rim.push(toScreen(k, 0, j / N));
  paths.push(`<path class="tube" d="${line(rim)}Z"/>`);
  for (let i = 0; i <= 12; i++) {                           // verticals
    const x = i / 12, pts = [];
    for (let j = 0; j <= N; j++) pts.push(toScreen(k, x, j / N));
    paths.push(`<path class="${i % 6 ? 'g' : 'g2'}" d="${line(pts)}"/>`);
  }
  for (let j = 0; j <= 8; j++) {                            // horizontals
    const y = j / 8, pts = [];
    for (let i = 0; i <= N; i++) pts.push(toScreen(k, i / N, y));
    paths.push(`<path class="${j % 4 ? 'g' : 'g2'}" d="${line(pts)}"/>`);
  }
  return [
    `<rect class="frame" x="${ox}" y="${TOP}" width="${W}" height="${H}" rx="6"/>`,
    ...paths,
    `<text class="t1" x="${ox}" y="20">${label}</text>`,
    `<text class="t2" x="${ox + W}" y="20" text-anchor="end">${sub}</text>`,
  ].join('\n    ');
}

const warp = 1.43, k = coef(warp);
const corner = f(k, 0.5, 0.5), side = f(k, 0.5, 0);
const edgeMid = toScreen(k, 1, 0.5)[0], edgeTop = toScreen(k, 1, 0)[0];
console.error(`k1=${k.k1.toFixed(4)} k2=${k.k2.toFixed(4)}`);
console.error(`screen corner samples ${((corner - 1) * 100).toFixed(1)}% further out; mid-edge ${((side - 1) * 100).toFixed(1)}%`);
console.error(`content right edge lands at x=${(edgeMid * 100).toFixed(1)}% mid-height, ${(edgeTop * 100).toFixed(1)}% at the top`);

const VW = PAD * 2 + W * 2 + GAP, VH = TOP + H + PAD;
console.log(`<svg class="barrel" viewBox="0 0 ${VW} ${VH}" role="img" aria-label="Two panes' content grids. On the left, warp 0: a flat, square grid. On the right, warp 1.43, the default: the same grid bowed outward, pulled in at the corners, with black glass left in each corner.">
  <g transform="translate(${PAD} 0)">
    ${panel(0, 0, 'warp 0', 'flat')}
    ${panel(warp, W + GAP, 'warp 1.43', 'the default')}
  </g>
</svg>`);
