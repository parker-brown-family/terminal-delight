# Chalk native review evidence — second pass

`proof.json` binds the tested source files, binary hash, test totals and lossless captures. Native windows were 1576 × 950 logical pixels at display scale 1.6. Terminal content and attention cases are staged examples. The actual terminal applications are not measurements of a real build session.

`praus-theme.json`, `light-theme.json` and `warped-theme.json` record the captured palette and display settings. Restart receipts prove a chosen Chalk skin survived ordinary session restoration without a supplied skin file. `install.json` distinguishes the installed next-launch binary from the GUI still running.

## Repeat the native capture

Run from a checkout containing this evidence directory on the Hyprland desktop. Requires Python 3.11+, jq, grim with stable-window capture, and the repository’s existing hidden-window helper. It reads the saved session `~/.config/terminal-delight/sessions/1.toml` for display settings; it does not edit that session. Praus scenes follow the desktop palette current at capture time. For exact colour reproduction restore the recorded palette through a separate fixture, rather than changing the user’s desktop.

Pass an absolute verified binary path and a fresh output directory. `CHALK_CAPTURE_ONLY=1` makes the helper close its own fixture after capture. Without it, create `finish` in the output directory to release the fixture, or let its bounded wait expire. Do not rewrite a runner while it executes.

```sh
CHALK_CAPTURE_ONLY=1 bash reports/2026-10-19-chalk-surface-v02-evidence/capture.sh /tmp/td-chalk-surface-v02 /tmp/chalk-review-new praus workspace
```

Palettes: `praus`, `light`, `warped`. Surfaces: `workspace`, `tray`, `gauges`, `rail`, `paint`. Workspace captures also switch Chalk → Default → Chalk, then restart the isolated fixture. The helper uses a hidden workspace and closes only its own GUI and host. Save the seed and restored profile before cleanup.

## Repeat the HTML check

Requires Chromium and Node with native WebSocket. The isolated browser profile and downloads stay under `/tmp/chalk-report-browser-qa`. It checks desktop/mobile overflow, eight figures, two decisions, capture zoom and note add/export/save/clear, including the real download filename. It never modifies the delivered HTML or Parker’s browser storage.

```sh
node reports/2026-10-19-chalk-surface-v02-evidence/report-qa.mjs /home/parker/Work/terminal-delight/reports/2026-10-19-chalk-surface-second-pass.html
```

The narrative measurement is retained as a diagnostic, not a passing visual test. After revising the transitions from the palette correction through the rendering gaps to the next review, it classified the page as a journey. Its prose-only reader check does not evaluate the screenshots; the review is intended for Parker, who supplied the first-pass feedback.
