#!/usr/bin/env bash
# Twenty panes, four cores, three rounds, one core per process, run one after another.
#   bash run_perf.sh    → results/perf/<core>-<round>.json
# Builds live in ~/.cache/td-core-research/targets (see the harness READMEs in each Cargo.toml).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cache="$HOME/.cache/td-core-research"
text="$cache/perf/text-8mb.bin"
[ -f "$text" ] || python3 "$here/text_stream.py" "$text" 8
pic="$here/captures/icat.bin"
out="$here/results/perf"
mkdir -p "$out"
for round in 1 2 3; do
  "$cache/targets/corebake/release/corebake" --perf alacritty "$text" "$pic" 20 > "$out/alacritty-$round.json"
  "$cache/targets/corebake/release/corebake" --perf rio "$text" "$pic" 20 > "$out/rio-$round.json"
  # libghostty-vt's max_scrollback behaves as bytes: 11,000,000 keeps 10,164 rows, the page
  # step nearest the other cores' 10,040 (ghost_scrollback_sweep.sh). 10,000 kept 489.
  GHOSTBAKE_SCROLLBACK=11000000 "$cache/targets/ghostbake/release/ghostbake" --perf "$text" "$pic" 20 > "$out/ghostty-$round.json"
  "$cache/targets/wezbake/release/wezbake" --perf "$text" "$pic" 20 > "$out/wezterm-$round.json"
  echo "round $round done"
done
