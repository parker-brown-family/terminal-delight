# Visuals / CRT — the look is the brand

Terminal Delight looks like a beautiful CRT because it *is* one: a true GPU
post-process pipeline (forked into gpui's renderer), not CSS tricks. The aesthetic
is a feature — it's what makes a wall of agents feel like mission control instead of
a spreadsheet.

## Why it matters

"The look is free" is a North-Star bar: the effects add ≤1 ms of GPU frame time and
**zero** added input latency. You get the warmth and identity of phosphor glass with
native-terminal speed.

## Features

| Feature | What it does | Evidence | Flag / control |
|---|---|---|---|
| **Per-pane barrel warp** | Each pane bends by its own curvature (k1/k2) — a real fullscreen GPU pass via forked `crt_pass.wgsl` | `app/src/warp.rs`; `docs/patches/0001` | `Grade::warp` 0..1 |
| **Per-card overlay warp** | Card logos on the wall warp independently; the card stays a flat click target (tube cap 8→32) | `register_overlay_tube`; patch `0004` | auto on themed wall |
| **Warp-aware hit-testing** | Clicks map through the *forward* shader gather map, so selection/links land correctly on curved glass | `warp_screen_to_content` (pinned by test) | — |
| **Warp suppression** | Open a menu/dialog → glass flattens so clicks match layout | `warp::set_suppressed` | automatic |
| **👓 FOCUS reader** | Click 👓 → full-screen zoomed mirror of one pane; the rest dim + **GPU frosted blur** (golden-angle disk + rounded SDF mask) | `register_focus_tube`, `set_focus_blur`; patch `0002` | 👓 button / esc |
| **FOCUS text-size slider** | Live 0.6–3× zoom on the reader, non-destructive | reader header A↔A | per-open |
| **Text-crawl mode** | Star-Wars perspective crawl per pane (angle 2–30°, depth 0.05–15×); composes with warp | `crt_pass.wgsl`; patch `0003` | `Grade::crawl` |
| **Tracking band** | A glow bar sweeps the screen every 9–25 s — CRT-authentic | `crt.rs::Fx::band` | theme `tracking*` |
| **Scanlines / vignette** | Striped overlay + darkened edges, per-theme | `crt.rs` dials | theme; off in light themes |
| **Glow / bloom / glare** | Centre phosphor bloom + per-pane glass glare | theme `glow/bloom/screen_glare` | per-pane |
| **Flicker / jiggle** | Rare brightness dips + a 1–2 px hop (desynced per screen) | `Fx::flicker_mul`, `Fx::jiggle_px` | theme dials |
| **Bezel** | Raised metallic emboss frame; non-occluding | theme `bezel` | per-pane |

## Performance posture

Effects run a 30 fps FX clock **only mid-sweep**; static scenes are gated so idle
CPU stays near zero. Latency with the glass on: p50 ≈ 113 µs (no regression).
Kill switches: `TD_NOGLASS` (no warp), `TD_NOCANVAS` (no CRT effects).

## The fork

The shaders live in a small patch set on a pinned gpui/Zed checkout
(`docs/patches/0001-0004`), applied by `scripts/prepare-gpui.sh`. This is the only
out-of-repo code and is tracked as data files. See [packaging](09-packaging.md).

## Status

**Shipped** (all of the above; 0.4 "true shader" gate cleared).
