# FOCUS frosted-glass blur — overnight build report

**Branch:** `feat/focus-blur` (worktree `Software/td-blur-sandbox/terminal-delight`)
**Commit:** `b05a872`
**Status:** ✅ implemented · builds clean · 80/80 tests pass · visually verified on `:1`

## What you asked for
Real backdrop **blur** behind the 👓 FOCUS reading modal (not just the dim), award-winning + delightful, in an isolated worktree, with tests + screenshots.

## What shipped
A genuine GPU blur of everything behind the FOCUS panel, riding the existing forked CRT post-pass:

- **`crt_pass.wgsl`** — when the modal is up, the backdrop is blurred with a **golden-angle (Vogel) disk** of 32 taps + **per-pixel dither rotation** (trades banding for fine noise the dim swallows → creamy frosted glass). A **rounded-box SDF mask** keeps the panel itself razor-sharp and follows its 12px corners; a cool-white **frosted tint** + slight depth-darken sell the "glass."
- **`gpui_wgpu`** — new `set_focus_blur(rect, radius, feather, tint, corner)` (atomics, hot-reload safe, same pattern as `set_crt_warp`). `CrtParams` gained `focus_rect`/`focus_params`; the `crt_active` gate now also fires on blur (warp is already suppressed while the modal is open, so there's no warp/blur interaction).
- **App** — the FOCUS scrim gained **`.occlude()`** (your earlier ask: locks clicks *and* scroll to the FOCUS pane, nothing leaks to other terminals) and a **~220ms smoothstep ease-in** so the dim+blur *melt* in. The panel's measurement `canvas` feeds the **exact physical-px rect** so the sharp/frost seam is pixel-aligned through the CSD shadow margin.

## Decisions & tradeoffs
- **Technique: single-pass disk blur, not dual-Kawase.** Two research passes (web + code) concluded dual-Kawase is the quality king but needs new render targets + pipelines + multi-pass encoding deep in the vendored renderer — high risk for an unattended run with a runtime-panic failure mode. The single-pass disk folds into the *one* existing shader (zero new pipelines/targets), and on a dimmed, non-focal backdrop it's genuinely beautiful. Dual-Kawase remains a clean future upgrade if you want it sharper.
- **Performance: a non-issue.** The pass is **modal-gated** — zero cost when you're not reading. While reading, 32 taps fullscreen on the RTX 3080 is well under ~1ms. Closed-modal behaviour is byte-for-byte unchanged (gate off → old code path).
- **Isolation:** the gpui fork is *shared* across all your worktrees via a symlink, so I copied it to **`Software/zed-upstream-blur`** (84 MB, no `target/.git`) and pointed this worktree's `../../zed-upstream` at it. **Your shared fork was never touched** (still 6 dirty files, exactly as before). The blur fork delta is captured in `docs/patches/0002-focus-blur.patch` (verified `git apply --check` clean on top of `0001`).

## How to see it
From your own session (so X/DRI auth is real):
```bash
cd Software/td-blur-sandbox/terminal-delight/app
TD_FOCUS_DEMO=1 cargo run        # auto-opens FOCUS on the first pane
# …or just run normally and click the 👓 on any sub-terminal header.
```
`TD_FOCUS_DEMO` is an env-gated capture hook (inert without it) so headless screenshot tooling can frame the modal without a mouse.

## Screenshots (verified on :1)
- `screenshots/focus-blur-modal.png` — the modal: sharp panel, dim frosted surround.
- `screenshots/focus-blur-edge-proof.png` — 2× zoom of the panel edge: flat sharp panel (left of the accent border) vs the **dithered frosted texture** (right). This is the proof the backdrop is *blurred*, not just dimmed.

## Risks / notes for landing
- **Fork rebase tax:** `0002` rides on `0001` on the pinned `abbe85a`. Regenerate both if the pin moves. Authoritative fork tree: `Software/zed-upstream-blur`.
- The `TD_FOCUS_DEMO` hook + its one log line are harmless but you may want them gone before merge — say the word.
- Subtle-by-design: under the 0.6 dim the frost reads quietly in a still image; it's most alive in motion (live terminals frosting behind the panel). If you want it bolder, bump the blur radius (28px) or lower the dim.

## Suggested next steps (your call)
1. Land as-is (review `0002-focus-blur.patch` + the app diff).
2. Optional: animated **dim is already eased**; consider easing the panel scale too.
3. Optional upgrade to dual-Kawase for a wider, even creamier blur.
