# Chalk marker and depth — native evidence

The third-pass review retains Parker’s images and words under `docs/plans/chalk-flower/feedback-v03/` and `03-marker-and-depth.md`. The earlier report remains unchanged. This evidence compares the previous native build with the current saved Last Horizon settings, then checks the revised material on pale stock, tree rows, pane controls, the attention queue, paint cards, CRT warp and Subscriptions in both dark and pale palettes. The final selection has one pigment pass and no solid selector tile.

`proof.json` binds the binary, source hashes, integrated base commit and pixel-identical WebP captures. The installed binary receipt is separate in `install.json`. Profile files record the captured skin, theme, grade and window size. Test and build logs are compressed without changing their contents. Terminal output and attention cases are synthetic examples.

## Repeat the native recipe

Requires this checkout, Hyprland, Python 3.11+, jq, and grim with stable-window capture. The existing hidden-window helper keeps the fixture on a special workspace; the recipe adds an explicit landscape-monitor launch rule and asserts the geometry before capture. It resizes only its own window to 1576 × 950 logical pixels. It never moves focus or edits compositor configuration. Cleanup closes only the fixture’s own GUI and host.

The default source of display settings is `~/.config/terminal-delight/sessions/1.toml`. Set `CHALK_SOURCE_SESSION` to a frozen fixture profile to keep a multi-scene comparison constant while Parker changes themes. For the recorded run it was the baseline scene’s seed, copied before cleanup. Modes `current` and `warped` copy its complete theme; `light` uses Flexoki Light with a neutral grade.

```sh
CHALK_CAPTURE_ONLY=1 bash reports/2026-10-20-chalk-marker-evidence/capture.sh /tmp/td-chalk-single-marker-v8 /tmp/chalk-marker-new current tray
```

Surfaces: `tray`, `workspace`, `rail`, `paint`, `gauges`, `usage`. Workspace also switches Chalk → Default → Chalk and restarts the isolated fixture. Seed and restored profiles are exported before the helper cleans up. Do not rewrite the runner while it executes.

## Repeat the HTML check

The isolated headless browser verifies eleven figures, two annotatable choices, desktop/mobile width, native capture zoom, and note add/export/save/clear with the correct filename. It uses its own profile and temporary downloads, leaving the delivered file and Parker’s browser storage untouched.

```sh
node reports/2026-10-20-chalk-marker-evidence/report-qa.mjs /home/parker/Work/terminal-delight/reports/2026-10-20-chalk-marker-and-depth.html
```

The narrative measurement is retained as a diagnostic for Parker’s visual review. It does not judge the screenshots or establish that the material feels tactile; that remains the reader’s visual judgement.

The Subscriptions demo deliberately has no reset instants, so it draws no pace estimate. Its two captures establish the shared shell, option and meter material. The pace track and fill use the same grain in the real view; the usage collector and arithmetic are unchanged.
