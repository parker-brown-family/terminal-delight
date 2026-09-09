#!/usr/bin/env bash
# What does the seam cost a keystroke?
#
# Gate 2 set the condition for making the split the default: input-echo latency
# within a millisecond of today, and no visible lag at eight panes under a
# flood. This is the instrument for the first half and the stress case for the
# second, and it is the last unmeasured thing in the feature.
#
#   ./scripts/td-echo-bench.sh
#   ./scripts/td-echo-bench.sh --samples 200        (a quicker, noisier answer)
#
# It runs one measurement four times — a terminal this process owns and one a
# session host owns, each quiet and each beside eight flooding panes — and
# prints the numbers plus the verdict. The instrument is identical across the
# four, so the difference between a mode's numbers is the cost of the seam and
# nothing else.
#
# RELEASE, always. A debug build spends so long in the parser that it hides the
# thing being measured, and the gate is about what ships. Nothing here opens a
# window: it spawns pseudoterminals and a session host, and no GUI is involved.

set -uo pipefail

SAMPLES=1000
FLOOD=8
while [ $# -gt 0 ]; do
  case "$1" in
    --samples) SAMPLES="$2"; shift 2 ;;
    --flood) FLOOD="$2"; shift 2 ;;
    *) echo "td-echo-bench: unknown argument $1" >&2; exit 2 ;;
  esac
done

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APP="$(dirname "$HERE")/app"

# The gate, in microseconds: an attached window's p99 may sit at most this far
# above a local one's, at the same load. One millisecond is the number Gate 2
# settled on — near enough that a person typing cannot tell.
GATE_US=1000

echo "building release (the gate is about what ships, not what tests)…" >&2
if ! (cd "$APP" && cargo build --release --tests >/dev/null 2>&1); then
  echo "td-echo-bench: the release build failed — run it yourself to see why" >&2
  exit 2
fi
BIN="$APP/target/release/terminal-delight"

run() { # run <mode> <flood>
  (cd "$APP" && TD_ECHO_MODE="$1" TD_ECHO_FLOOD="$2" TD_ECHO_SAMPLES="$SAMPLES" TD_BIN="$BIN" \
    cargo test --release --bin terminal-delight echo_latency_bench -- --ignored --nocapture 2>/dev/null) |
    grep -E '^\{"mode"' | head -1
}

results=""
for flood in 0 "$FLOOD"; do
  for mode in local attached; do
    line="$(run "$mode" "$flood")"
    if [ -z "$line" ]; then
      echo "td-echo-bench: $mode at flood $flood produced no measurement" >&2
      exit 1
    fi
    echo "$line"
    results="$results$line"$'\n'
  done
done

verdict=$(printf '%s' "$results" | jq -s --argjson gate "$GATE_US" '
  . as $rows
  | ($rows | map(.flood) | unique) as $floods
  | {
      gate_us: $gate,
      conditions: [
        $floods[] as $f
        | ($rows | map(select(.mode == "local" and .flood == $f)) | .[0]) as $local
        | ($rows | map(select(.mode == "attached" and .flood == $f)) | .[0]) as $attached
        | {
            flood: $f,
            local_p50: $local.p50_us,
            attached_p50: $attached.p50_us,
            local_p99: $local.p99_us,
            attached_p99: $attached.p99_us,
            cost_us: ($attached.p99_us - $local.p99_us),
            within_gate: (($attached.p99_us - $local.p99_us) <= $gate)
          }
      ]
    }
  | .passes = (.conditions | map(.within_gate) | all)
')
echo "$verdict"

if [ "$(printf '%s' "$verdict" | jq -r .passes)" = "true" ]; then
  echo "td-echo-bench: within the gate — an attached keystroke costs less than a millisecond more" >&2
  exit 0
fi
echo "td-echo-bench: OVER the gate — the seam costs more than a millisecond at some load" >&2
exit 1
