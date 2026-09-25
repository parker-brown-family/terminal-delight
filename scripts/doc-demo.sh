#!/usr/bin/env bash
# Print a handful of documents to click, for trying documents-in-the-pane by hand.
#
# Run it inside a Terminal Delight window on a build that has the feature
# (Ctrl+Alt+T opens a fresh window from the launcher). It opens no window and
# drives nothing: it only prints absolute paths, each at the start of its own
# line, so the click finds them whatever the pane's width.
#
#   scripts/doc-demo.sh
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)

b=$'\e[1m'; d=$'\e[2m'; r=$'\e[0m'
say() { printf '%s\n' "$*"; }
path() { # path <label> <file>
  if [ -e "$2" ]; then say "${d}$1${r}"; say "$2"; else say "${d}$1 — missing: $2${r}"; fi
}

say ""
say "${b}Documents in the pane${r}"
say ""
say "  alt+click         open it in a floating square beside its line"
say "  ctrl+alt+click    open it in a pane beside this one"
say "  esc               close the square (every other key still reaches the shell)"
say "  shift+click       show it in the file manager"
say "  ctrl+click        open it with the desktop, as before"
say "  on the square     drag the strip · − fit + to zoom · wheel or drag to pan · ↗ desktop · ✕"
say "  in a split        alt+k between the document and its shell · 0 1 + − arrows · PgUp PgDn"
say ""
path "a picture, 2000 × 1180" "$ROOT/assets/crt-wall.png"
path "a wide picture, 2560 × 1573" "$ROOT/assets/crawl-hero.png"
path "Markdown with pictures in it" "$ROOT/README.md"
path "Markdown, the plan's product page" "$ROOT/docs/plans/gui-in-the-tui/01-product.md"
path "an HTML brief, drawn by headless Chromium" "$ROOT/reports/2026-09-24-floating-square.html"
path "a taller brief, with figures and modals" "$ROOT/reports/2026-09-24-ten-slices.html"
say ""
