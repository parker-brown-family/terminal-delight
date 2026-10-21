# Image Glow

**Difficulty: 6/10.** Parker authorized end-to-end implementation while away on 2026-10-20.

## Purpose

Make bright terminal content cast a soft, colour-preserving halo. Add an `image glow` slider beside glare in GAUGES → CRT, using the same outer/per-pane ownership and saved settings as the existing controls. The first build provides image bloom; the research study's fading history remains a separate possible effect.

## Shape

The slider and MCP use 0–100%; stored values use 0–1. `Grade::image_glow: Option<f32>` preserves an unset setting separately from `Some(0)`. At the drawing edge, an unset setting currently means off. Turning the slider pins only this channel. The CRT master switch suppresses the effect without destroying its stored strength.

The existing post-process samples nearby pixels from its existing scene texture. Samples stay inside the terminal's registered content rectangle, preserving pane boundaries. A flat terminal can glow with CRT enabled and warp at zero. Workbench faces, document controls and card artwork do not opt into the new glow. The implementation adds fixed per-pane metadata and no additional image history or blur render target.

Use a private GPUI checkout at the pinned revision with the repository patch stack applied. Carry the new renderer delta as an additional patch. Never modify the shared `zed-upstream` checkout or replace the normal installed TD binary.

## Interfaces

- `GradeKey::ImageGlow` and `GradeChannel::ImageGlow`: range, slider conversion, pinning and persistence.
- `ConfigPatch::image_glow` and `GradeReport::image_glow`: optional percentages, with an omitted PATCH field leaving the setting unchanged.
- The tube registry supplies a glow strength for terminal content; other tubes supply zero.
- GPUI's tube data carries the strength into WGSL and activates the pass for glow even with zero curvature.

## Least Confident Decisions

The sparse blur's radius and gain need native judgement at ordinary reading sizes. The desktop test instance will make that judgement possible. GPU frame cost is unmeasured until the native build runs. The current GAUGES scrim suppresses the CRT pass, a known limitation in [the glass-preview issue](https://github.com/parker-brown-family/terminal-delight/issues/944); native checks must identify whether tuning requires closing the tray. Fixing every geometric preview case is separate from shipping this image effect.

## Verification

Use meaningful tests for absent versus explicit-zero settings, one-channel pinning, API percentage round trips and the CRT switch. Compare deterministic native captures at zero and nonzero glow, including flat tubes and adjacent panes. Verify that disabling the effect returns to its baseline image, and that save/reopen preserves the chosen value. Check the isolated patch stack and the new binary, then leave the test session on an empty workspace with its identity and launch command recorded.
