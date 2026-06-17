# Web Barrel-Warp Lab — research log

Goal: reproduce terminal-delight's signature **barrel "curved glass" CRT warp** in
the browser. Ideal = CSS/HTML/JS; open to WebGL/WebGPU/Rust→WASM. Two distinct
use-cases, judged separately:

- **(A) Marketing HERO** — visual only, interactivity optional. Pixel-accuracy wins.
- **(B) Interactive web terminal** — clicks/selection/typing must still work.

The reference shader (gpui `crt_pass.wgsl` `fs_crt`), the math every approach is
matched against:

```
local = (p - rect.xy) / rect.wh           // 0..1 within the screen rect
c     = local - 0.5
r2    = dot(c, c)
f     = 1 + k1*r2 + k2*r2*r2               // k1≈curv*0.14, k2≈curv*0.06
l2    = 0.5 + c*f                          // CONTENT shown at screen `local`
sample content at clamp(l2, 0, 1)
```

It's a **gather**: content displayed at screen point `local` is sampled from `l2`.

## Environment (verified 2026-06-14)
- node 24.13, npm 11.6, Playwright 1.60 (system **Chrome 145**, **Firefox 151**), ffmpeg 4.4.
- Headless Chrome 145 reports **WebGL2 = true AND WebGPU = true** (no display needed) —
  big enabler; verified by rendering a WebGL canvas + screenshotting (`shots/00-smoke.png`).
- Harness: `tools/shoot.mjs <url|file> <out.png> '<jsonOpts>'` → screenshot (+optional
  video), prints `{caps, errs}`. Drives system Chrome via Playwright `channel:'chrome'`.
- Isolated git worktree `td-web-warp` on branch `experiment/web-barrel-warp`. NOT committed.

## Approaches (status)
| # | Technique | Live DOM? | Pixel-accurate? | Status |
|---|-----------|-----------|-----------------|--------|
| 01 | SVG `feDisplacementMap` on live DOM | ✅ yes | approx | building |
| 02 | WebGL2 shader on `<foreignObject>` snapshot | ❌ snapshot | ✅ exact | pending |
| 03 | WebGPU (WGSL) — the real shader | ❌ snapshot | ✅ exact | pending |
| 04 | CSS `perspective`/transform (cylinder fake) | ✅ yes | ✗ no true barrel | pending |
| 05 | Rust→WASM (wgpu) — same shader in browser | ❌ snapshot | ✅ exact | pending |
| 06 | Hybrid (live flat input + warped GPU mirror) | ✅/❌ | ✅ | pending |

Findings appended below as each is built & screenshotted.

---

## Research synthesis (3 parallel agents, all cited in `docs/research-*.md`)

**Live-DOM warp = SVG `feDisplacementMap` ONLY.** `filter:url(#barrel)` on the
container, fed by a canvas-generated radial map (R=x-offset, G=y-offset, 128=neutral),
`color-interpolation-filters="sRGB"`, filter region overscanned ~140%. Verified working
(Approach 01). Caveats: (1) **hit-testing is paint-only** — clicks hit un-warped geometry;
keep curve gentle OR inverse-map pointer coords (we have the forward field). (2) edges go
transparent → overscan + oversize content. (3) `backdrop-filter:url()` is DEAD x-browser
(Safari bug 245510 open 4yrs; FF unsupported). (4) author map at exact px size / userSpaceOnUse
to dodge FXTF#596 non-square bug. Refs: Smashing feDisplacementMap deep-dive, kube.io liquid-glass.

**Pure CSS cannot do a true 2-axis barrel.** `perspective`+`rotateX/Y` = flat-plane tilt only;
slices = single-axis cylinder (and wreck text flow). `border-radius`/`clip-path` = outline only.
CSS is for the bezel/vignette/scanlines, not curvature. (CSS-Tricks perspective.)

**Pixel-accurate = WebGL2/WebGPU shader on a texture** — our formula `uv'=0.5+(uv-0.5)*(1+k1 r²+k2 r⁴)`
ports verbatim (GLSL & WGSL). DOM underneath is dead → hero / or render glyphs to canvas.
Refs: CRT-Lottes (public domain, Shadertoy MsjXzh), gingerbeardman/webgl-crt-shader (mobile-tuned),
Ghostty crt.glsl, cool-retro-term-webgl, langterm.

**DOM→texture options:** (a) `foreignObject`→dataURL→`Image`→texture = SNAPSHOT; drops external
fonts/imgs (must inline), CORS-taints (Safari worst). (b) **HTML-in-Canvas API** (`canvas layoutsubtree`
+ `onpaint`/`texElementImage2D`) = LIVE DOM→GPU, but Chromium 147+ experimental, behind a flag, no FF/Safari.
(c) **Best for a terminal: render glyphs straight to canvas** (xterm.js-WebGL style) → warp → live 60fps,
no snapshot cost. Perf: the warp pass itself is sub-ms; only DOM-snapshot upload is slow.

**Rust→WASM (wgpu):** our WGSL runs UNMODIFIED on WebGPU + (via naga→GLSL) WebGL2. But bundle is
~1–3MB+ (naga). gpui-on-web exists (PR#50228, Feb 2026) but immature (no tree-sitter/PTY). Verdict:
plain JS+WebGL2 wins for a warp; Rust→WASM only worth it for bit-exact single-source parity. Toolchain
is no-sudo (rustup/cargo).

### Approach 01 result — SVG feDisplacementMap (live DOM)  ✅ WORKS
`shots/01-svg-k40.png`: grid lines bow outward in a clean barrel; text live & selectable.
TODO: overscan content to kill the bottom edge artifact; add inverse-map pointer hit-testing.

---
# RESULTS — all approaches built & verified (Chrome 145, RTX 3080 via ANGLE)

| # | Technique | Live? | Pixel-exact? | Interactive-correct? | Verdict |
|---|-----------|-------|--------------|----------------------|---------|
| 01 | SVG `feDisplacementMap` on live DOM | ✅ | approx (8-bit, soft edges) | ✗ (browser hit-tests un-warped) | **Ship for: real arbitrary DOM you must keep live.** Gentle curve only. `shots/01-svg-k40.png` |
| 02 | WebGL2 warp of `foreignObject` snapshot | ❌ snapshot | ✅ | n/a (static) | **Ship for: arbitrary-HTML hero** that updates rarely. Font/CORS caveats. `shots/02-foreignobject.png` |
| 03 | **WebGL2 barrel on a canvas-rendered terminal** | ✅ 60fps | ✅ | ✅ (we own it) | **★ THE ANSWER for a terminal.** Live, gorgeous, trivial GPU cost. `shots/03-webgl-terminal.png`, video `shots/03-warp-sweep.mp4/.gif` |
| 04 | **WebGPU (WGSL)** — literal desktop shader | ✅ 60fps | ✅ bit-exact | ✅ | Same look from the *same shader*; ship as progressive enhancement over 03. `shots/04-webgpu.png` |
| 05 | **Canvas terminal + gather-map hit-test** | ✅ | ✅ | ✅ **rim-accurate** | ★ Proves click-to-select is correct UNDER the warp, same math as desktop `warp_screen_to_content`. `shots/05-hittest-rim.png` |
| 06 | CSS `perspective`/cylinder/glass | ✅ | ✗ | ✅ | **Documents the CSS ceiling** — tilt or 1-axis cylinder or cosmetic rim only; NO true barrel. `shots/06-css-limits.png` |

## Performance (stress, Approach 03) — could not break it
rAF-locked **60fps at 960×600, 1920×1080, 2560×1440, and 3840×2160 supersampled (7680×4320 internal)**, zero dropped frames — every-frame canvas re-rasterize + texture upload + warp pass. Overdraw ramp at 1080p: **60fps through ~50 full-screen warp passes/frame**, dips at 200 (58fps), buckles at 600+ (21fps). The real effect needs **1** pass ⇒ **~50× headroom** on this GPU; research says the pass targets an iPhone A12. **The warp is GPU-free; the only real cost is DOM-snapshot upload (approaches 01/02), which is why rendering glyphs straight to canvas (03/04/05) wins.**

## Notable bug found & fixed (WGSL parity)
Approach 04 rendered black on WebGPU until I changed `textureSample(...)` → `textureSampleLevel(..., 0.0)`. WGSL requires `textureSample` (it computes derivatives) to run in **uniform control flow**; my fragment sampled *after* an `if (edge<=0) { return }` early-out → non-uniform → undefined → SwiftShader returns 0. WebGL2/GLSL is lenient (03 worked), WGSL is strict. Our desktop `crt_pass.wgsl` already uses `textureSampleLevel` — the same lesson. (Isolated proofs: gradient readback + solid-texture sample both pass; scene canvas verified non-black; fix verified by readback: centre `[28,190,104]`, corner `[0,0,0]` = vignette working.)

## RECOMMENDATION
- **Marketing hero (the site's `assets/crt-wall.png` replacement):** ship **Approach 03** (WebGL2 canvas terminal) — a *live, breathing* curved-CRT hero instead of a PNG, 60fps, ~zero deps. Optionally **04 (WebGPU)** as progressive enhancement for bit-exact parity with the desktop, WebGL2 as the fallback (≈96% support; WebGPU ≈82%, default everywhere but Firefox).
- **Genuinely interactive web terminal:** **Approach 05** — render cells to canvas + the gather-map hit-test. It's the only path that's warped AND click-correct (proven at the rim). If you must warp *foreign* live DOM (not a terminal), it's **01** with a gentle curve (accept un-warped hit-testing) — or wait for the **HTML-in-Canvas API** (Chromium 147+, experimental) to feed live DOM into 03/04.
- **Rust→WASM:** skip unless you want one shader source of truth; then a thin wgpu crate runs the WGSL unmodified (1–3MB naga bundle). Not worth it here — JS+WebGL2 is ~40 lines and 0 download.

## Ship-it sketch (hero)
Drop a `<canvas>`; render the terminal scene (or an `xterm.js` WebGL buffer) to an offscreen 2D/GL canvas; run the Approach 03 fragment as a post-pass; expose `curvature/scan/aberr/glow` as CSS-driven uniforms; feature-detect WebGPU→use 04, else 03. Bezel/scanline glare can stay CSS (06-C) for cheap polish.

## References (key, from the 3 research agents)
- feDisplacementMap deep-dive (Smashing); canvas displacement map encoding (kube.io liquid-glass); Safari backdrop bug 245510; FXTF#596 non-square scaling.
- CRT-Lottes (Shadertoy MsjXzh, public domain); gingerbeardman/webgl-crt-shader (mobile-tuned); Ghostty crt.glsl; cool-retro-term-webgl; statico/langterm.
- foreignObject→texture limits (semisignal); HTML-in-Canvas API (html-in-canvas.dev, Chromium 147+); WebGPU support (web.dev, Nov 2025); WebGPU textures (webgpufundamentals).
- wgpu-on-web (wgpu wiki + docs.rs); gpui-on-web PR zed#50228 (immature); naga WGSL→GLSL; ~10MB wasm size (wgpu#2278).

### Approach 07 — WHOLE-SITE warp (real homepage in a filtered iframe)
`shots/07-whole-site.png`: the entire live homepage (nav/hero/buttons/image) bows like
curved glass; scanline+vignette overlay on top; site stays interactive underneath.
Perf (RTX 3080, Chrome 145): **IDLE ~60fps** (an occasional 33ms hitch from the page's
blink animation re-filtering), **SCROLL ~30fps with 50ms hitches** — feDisplacementMap
re-rasterizes the full viewport on every content repaint. Click drift exists at the rim
(gentle curve keeps it sub-button near centre; nav links at the top edge drift most).
∴ Whole-scrolling-site warp = usable but janky on scroll. Mitigations: (a) warp a fixed
ONE-SCREEN landing (no scroll → buttery), (b) warp only the hero/above-the-fold, leave the
scrolling body flat, (c) suspend the filter during scroll + restore on idle. Pixel-perfect
WebGL can't do a live scrolling site (needs a texture = snapshot).

### Approach 08 — KIOSK LANDING (the product) ★
One-screen terminal-delight landing rendered to a canvas, warped in-shader:
**barrel ~1.4, ramped flicker, an obvious rolling tracking bar, scanlines, vignette,
glow, chromatic aberration** — all animated IN THE SHADER (scene texture re-uploads only
on cursor blink). CTAs (Get v0.1.0 / GitHub / Docs) are **clickable through the warp** via
the gather-map hit-test (verified: clicks at the 3 buttons return 0/1/2, empty=-1).
Perf: **locked 60fps at 1280×800 and 1920×1080** with every effect animating — vs the
feDisplacementMap whole-scrolling-site at ~30fps. `shots/08-kiosk-a.png`, `08-kiosk-b.png`,
video `shots/08-kiosk.mp4`. This is the recommended shape for the homepage: a non-scrolling
curved-CRT hero that's buttery and interactive.

## DECISION — obfuscation: NO.
The repo is public + the project is open-source MIT (and the openness is part of the
appeal), it's not a sellable artifact, and client code is never truly secret. Obfuscating
the deployed bundle buys nothing while the source is public and adds build friction that
quietly contradicts the open-source identity. If wanted later, a *minify* build is a
load-speed choice, not secrecy. Skipped.

### Approach 09 — SCROLLABLE curved-CRT site (real homepage) ★ scroll-friendly
Real homepage in a fixed full-viewport "tube" with the barrel filter; content scrolls
inside, warp stays glued to the screen. **suspend-on-scroll**: drop `filter` during active
scroll → native **60fps** (verified, p95/worst 16.8ms), restore the curve ~180ms after the
last scroll (confirmed restored at every depth, `shots/09-scroll-0..3.png`, video
`shots/09-scroll.mp4`). So a scrollable warped site IS viable and smooth.
**Gotcha:** under `file://` the iframe gets a `null` opaque origin → cross-origin wall
(parent can't read/drive the iframe scroll), so the demo must be served over **http**
(same-origin) — exactly how a real deployment runs. Served locally via
`python3 -m http.server 8099` from the `Software/` dir.
Alternative (no iframe, no scroll-suspend): the WebGL tall-canvas path could keep the curve
ON during scroll at 60fps by sampling a scroll-offset window of one tall scene texture, at
the cost of canvas-rendered (non-DOM) content — a future option if you want the curve to
persist through the scroll motion.

### Approach 10 — 1950s CONSOLE-TV KIOSK ★★ THE CHOSEN DIRECTION
Shipped as `terminal-delight/tv.html` (main repo working tree, NOT committed/pushed).
A mid-century wooden TV cabinet (CSS art: wood grain, rounded CRT, speaker grille, brass
knobs, splayed legs). The screen is the WebGL warp (barrel + flicker + tracking + scanlines
+ vignette + glow + chromatic aberration). **The cabinet KNOBS are the controls** (outside
the glass): BARREL warp, TEXT FADE (phosphor brightness), TRACKING (speed+wobble combo) —
drag or wheel to turn; the indicator rotates ±135°. **All links live INSIDE the screen**
(Get v0.1.0 / GitHub / Docs / ▸ Explore the full site → index.html), clickable through the
warp via the gather-map hit-test (verified at high barrel). 60fps. `shots/10-tv-a.png`,
`10-tv-b.png` (knobs cranked), video `shots/10-tv-knobs.mp4`. Existing `index.html` (hero +
demo) kept intact; the TV links out to it. Research worktree preserved per request.
