# Handoff — per-card logo warp + CI green (2026-06-22)

## Status
Landed + deployed. `main` = `0dc950f`, in sync with origin, tree clean. Binary
hot-swapped (14:26). Tag **v0.2.1** cut (release AppImage building). All CI green.

## What's done (this session)
- **#118** SWCCG ⚡POWER corner (model·effort) + demo-logo wall — verified build/clippy/192 tests.
- **#119** demo logos on "Spin up a demo" (gate now includes `TD_DEMO=1`).
- **#120** graveyard text-size scrubber (reuses `card_scale`) + 2× bottom-right status pip.
- **#121** generated default card art (group wash · title monogram · kind · state).
- **#122** PER-CARD LOGO-SQUARE WARP — theme-on bends each card's art only; whole card stays a flat
  click target. CRT tube cap 8→32 (`crt_pass.wgsl` arrays + `gpui_wgpu` packing/buffer + `warp.rs`
  `MAX_TUBES`/`register_overlay_tube`). Fork change = `docs/patches/0004-warp-tube-cap-32.patch`.
  **Verified via headless GPU smoke test** (CRT pass active, zero wgpu validation lines) + 192 tests.
- **#123** CI green: `cargo fmt` main.rs + prepare-gpui sentinels `git grep`→`grep -rqF`. Confirmed by
  the post-merge main run going fully green (AppImage incl.).
- **v0.2.1** tag + **issue #124** (macOS support, falsifiable).

## How to run / verify
```bash
cd app && cargo fmt --check && cargo clippy --release --all-targets -- -D warnings && cargo test --release
bash scripts/prepare-gpui.sh           # idempotent; 5 patches incl. 0004
# GPU smoke (uniform layout): launch on a throwaway :2 with the CRT pass active, grep stderr:
DISPLAY=:2 TD_FOCUS_DEMO=1 TD_SCRATCH=1 app/target/release/terminal-delight  # → no wgpu validation lines
# deploy: cp app/target/release/terminal-delight → ~/.local/bin/terminal-delight-main.bin (write-then-mv)
```

## Not done / next (Parker queued — START HERE)
1. **Tall-terminal warp click miss** (#88): WHY do clicks ~15px off the near-top/bottom links in
   tall panes? Hit-test math is self-consistent in repro; top suspects (per #88) = stale `self.warp_k`
   vs live `theme::warp_coeffs` at click time, OR `grid_pad` corner-overscan approx (evaluated at
   r²=0.25, but the bottom bows to r²→0.5). Needs `TD_HITDEBUG=1` interactive repro on the REAL :1.
2. **`anchor_top` redesign**: anchor-top should put the PROMPT/input area at the TOP of the terminal,
   with the agent's RECENT messages at the top and older messages going DOWN; then FOCUS reader mirrors
   the same behaviour. (`anchor_top` already exists as a global toggle from #115 — this changes its
   semantics from content-top-anchor to a full inverted-flow + prompt-at-top.) "Award-winning."

## Watch out
- **Scripted edits bypass rustfmt-on-save** → run `cargo fmt` before any push (this caused #123).
- **gpui fork is read-only to `ctx_edit`** (sibling `../zed-upstream`) → native Bash/Python is the
  fallback for shader/renderer edits; capture as a patch + wire into `prepare-gpui.sh`.
- **Uniform cap bumps are byte-layout surgery** — mismatch only errors at DRAW time; always GPU-smoke.
- Shared-tree hazard: concurrent agents edit `main.rs`; stage only your hunks, never blind reset/checkout.

## Where it's recorded
APES episode `apes/projects/terminal-delight/episodes/2026-06-22-per-card-logo-warp-and-the-ci-green-fix.md` ·
lean-ctx (gotcha `git grep`→`grep -rqF`, fact `crt-warp-tube-cap-32`, session decision) ·
memory `ci-and-prepare-gpui-gotchas.md`, `crt-warp-tube-architecture.md`, `demo-logos-and-power-corner.md` ·
PRs #118–#123 · tag v0.2.1 · issue #124.
