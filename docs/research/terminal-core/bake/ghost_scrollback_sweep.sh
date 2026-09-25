#!/usr/bin/env bash
# Find the libghostty-vt max_scrollback that keeps the same rows as the other cores (10,040).
#   bash ghost_scrollback_sweep.sh → one line per value: rows kept, kB per pane full
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
bin="$HOME/.cache/td-core-research/targets/ghostbake/release/ghostbake"
text="$HOME/.cache/td-core-research/perf/text-8mb.bin"
for sb in "$@"; do
  printf '%s: ' "$sb"
  GHOSTBAKE_SCROLLBACK="$sb" "$bin" --perf "$text" "$here/captures/icat.bin" 2 | grep -E '"rows_kept"|"kb_per_pane_full"' | tr -d ' \n'
  echo
done
