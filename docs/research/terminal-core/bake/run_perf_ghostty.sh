#!/usr/bin/env bash
# Re-run only libghostty-vt's three rounds at matched scrollback (see run_perf.sh).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
cache="$HOME/.cache/td-core-research"
out="$here/results/perf"
for round in 1 2 3; do
  GHOSTBAKE_SCROLLBACK=11000000 "$cache/targets/ghostbake/release/ghostbake" --perf "$cache/perf/text-8mb.bin" "$here/captures/icat.bin" 20 > "$out/ghostty-$round.json"
done
