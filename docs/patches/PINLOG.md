# PINLOG — the zed_rev pin, managed

`app/Cargo.toml` pins gpui to one Zed commit (`zed_rev`), prepared as a sibling
`zed-upstream/` checkout by `scripts/prepare-gpui.sh`, carrying the five-patch
stack in this directory. This file is the pin's ledger: every bump gets an
entry, and the cadence/trigger policy lives here so the pin always has an
owner. Background: the 2026-08-29 foundation interrogation
(`docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md`, §2/§4) and
issue #179.

## Why a bump is not scary

A bump is one rebase in a throwaway worktree, gated before it ships:

1. `git -C zed-upstream fetch && git checkout <candidate-rev>` (a sibling
   checkout — the working build's checkout is untouched).
2. Re-apply the five patches; rebase whichever ones conflict (the CRT pass is
   the likely one — it lives in `gpui_wgpu`, where upstream moves).
3. `cargo test` + `scripts/release-smoke.sh` (fmt, clippy, tests, deny,
   AppImage). Red anywhere → stay on the old pin, file what broke, no harm
   done. Green → update `zed_rev`, commit, PR.

We never ship a red bump, and the old pin keeps working the whole time. The
risk isn't bumping — it's NOT bumping until a forced bump (security fix,
driver workaround) has to cross a year of drift at the worst moment.

## Cadence and triggers

- **Scheduled:** quarterly, or sooner when the drift watcher
  (`.github/workflows/zed-pin-watch.yml`, weekly) reports more than
  16 weeks / raises its issue.
- **Off-cycle triggers:** a gpui/wgpu security advisory · a renderer or driver
  fix TD needs · an `alacritty_terminal` release TD wants that needs newer
  gpui · Zed publishing `gpui_platform` + a post-process seam to crates.io
  (that one triggers a re-evaluation of the pin strategy itself — see the
  interrogation's §4 option C).

## Ledger

| Date | zed_rev | Zed state at pin | Patches | Notes |
|---|---|---|---|---|
| 2026-06-12 | `abbe85a3321bf6cb7f5b241e623d9c2e16c29187` | post-wgpu Linux renderer (PR #46758), pre-1.0 → Zed 1.0 shipped 2026-04-29 | 5 (td-crt-pass, focus-blur, sever-gpl-crates, text-crawl, warp-tube-cap-32) | The founding pin (G0a day). Known drift banked upstream since: wgpu fork → upstream `wgpu 29.0.4`, taffy `=0.10.1` → `=0.13.0`, accesskit landed, WebGL backend landed 2026-08-04. First bump crosses these — budget accordingly. |

Patches added on a pin, without a bump, are listed here too, because the next
bump has to rebase them:

- 2026-10-15, on `abbe85a`: `0006-lazy-path-targets` — gpui's two path render
  targets wait for the first frame that draws a path (#953, ~96 MiB of GPU
  memory on an idle window). One call moved in `WgpuRenderer::draw`; applies
  after 0001-0004.
- 2026-10-16, on `abbe85a`: `0007-path-targets-fit-the-paths` — the same two
  targets are sized to the largest batch of paths a frame draws (grown in
  64-pixel steps, never shrunk until a resize) and each batch is rasterised at
  a whole-pixel offset into them, which the path sprite now carries (#962). A
  window showing sticky notes stops paying ~96 MiB for them. Touches
  `WgpuRenderer::{draw, ensure_intermediate_textures, draw_paths_to_intermediate,
  draw_paths_from_intermediate}`, `PathSprite` and `vs_path` in `shaders.wgsl`;
  applies after 0006. Edge pixels of a note can differ by 1/255 from the
  full-surface target (a different divisor in the clip-space conversion).
- 2026-10-19, on `abbe85a`: `0008-glass-rim` — a bent tube's glass is seated
  in a bezel: its visible edge is a rounded rectangle in source space (corner
  5.5% of the tube's short side, 10–72 physical px), so the warp bends the
  corners along with everything else, and the bezel's lip throws a soft shadow
  onto the glass that is heavier under the top edge and doubles where two edges
  meet, with a faint highlight on the lip just outside. Replaces the hard
  rectangular clip and its 2.5-pixel edge fade in `fs_crt`. Round that, a
  faint moulding: a ring in a per-tube colour, shaded as a slope lit from
  above so it turns with each corner's curve, fading out before the tube's
  straight boundary. Its colour (0xRRGGBB as an f32) and strength ride the two
  spare uniform lanes, `glare.w` and `crawl.w`, set through a new
  `set_crt_rects_tubes_bezel`; `set_crt_rects_tubes` and the uniform's size are
  unchanged and send no bezel. Applies after 0001-0004 and 0007.
- 2026-11-01, on `abbe85a`: `0012-tube-blur` — each tube can soften its own
  picture. A blur radius in physical px rides the spare `image_glow[i].y` lane,
  set through a new `set_crt_rects_tubes_blur`; every older setter sends 0, so
  the uniform's size and every existing caller are unchanged. `tube_blur` in
  `fs_crt` is the FOCUS backdrop's 32-tap golden-angle disk with each tap
  clamped inside the tube, used on the flat path and in place of the single
  sample on the bent one; a blurred tube keeps the pass live like a glowing
  one. Applies after 0009-0011. It does not apply to a checkout rustfmt has
  reflowed (the lab's) or to one missing 0009-0011 (`~/Work/zed-upstream` on
  this date, which already refuses 0009).
