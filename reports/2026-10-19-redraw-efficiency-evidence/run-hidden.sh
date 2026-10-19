#!/usr/bin/env bash
# Adapted from the shipped-performance and styling experiments in this repository.
set -euo pipefail
ROOT=$(git rev-parse --show-toplevel)
SOURCE="$ROOT/reports/2026-10-17-pane-sleep-evidence"
OUT="${TD_SLEEP_OUT:-$SOURCE}"
ROUNDS="${TD_SLEEP_ROUNDS:-5}"
EXPECTED_MODE=CODEX
FIXTURE=codex-fixture.py
if [ "${TD_SLEEP_KIND:-agent}" = plain ]; then EXPECTED_MODE=python3; FIXTURE=worker.py; fi
[[ "$ROUNDS" =~ ^[1-9][0-9]?$ ]]
mkdir -p "$OUT"
if [ "$OUT" != "$SOURCE" ]; then
  cp "$SOURCE/layout.py" "$SOURCE/codex-fixture.py" "$SOURCE/sample.py" "$OUT/"
fi
if [ "$FIXTURE" = worker.py ]; then cp "$OUT/codex-fixture.py" "$OUT/worker.py"; fi
BASE=${1:?baseline normal production binary}
CAND=${2:?candidate normal production binary}
HOST_BIN=${3:?same host binary for both fixtures}
for path in "$ROOT" "$OUT" "$BASE" "$CAND" "$HOST_BIN"; do
  [[ "$path" =~ ^/[a-zA-Z0-9_./-]+$ ]] || { echo "launch path needs simple absolute characters"; exit 1; }
done
export XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1
export HYPRLAND_INSTANCE_SIGNATURE=efb50993780079460b0cbed1363e2166a2de1d9f_1791921895_961721468
. "$ROOT/reports/2026-10-19-redraw-efficiency-evidence/headless-window.sh"
RUN=tdsleep-$(date +%s)-$$
WS="special:$RUN"
SWITCH="/tmp/$RUN-working"
COUNTERS="/tmp/$RUN-counts"
mkdir -p "$COUNTERS/baseline" "$COUNTERS/candidate"
test ! -e "$OUT/samples.jsonl"
cleanup() { rm -f "$SWITCH"; hidden_cleanup; rm -rf "$COUNTERS"; }
trap cleanup EXIT
WINDOWS=(); HOSTS=(); SESSIONS=()
sha256sum "$BASE" "$CAND" "$HOST_BIN" > "$OUT/binaries.sha256"
LABELS=(baseline candidate)
if [ "${TD_SLEEP_REVERSE:-0}" = 1 ]; then LABELS=(candidate baseline); fi
for label in "${LABELS[@]}"; do
  if [ "$label" = baseline ]; then TD=$BASE; SLOT=0; else TD=$CAND; SLOT=1; fi
  session="$RUN-$label"
  SESSIONS[SLOT]="$session"
  python3 "$OUT/layout.py" "$session" "$HIDDEN_SESSION_DIR/$session.toml" "$OUT/$FIXTURE" "$SWITCH" "$COUNTERS/$label"
  "$HOST_BIN" serve --session "$session" > "$OUT/$label-host.log" 2>&1 &
  HOSTS[SLOT]="$!"
  HIDDEN_SESSIONS="$HIDDEN_SESSIONS $session"
  for _ in $(seq 1 50); do [ -S "$XDG_RUNTIME_DIR/terminal-delight/session-$session.sock" ] && break; sleep .1; done
  printf 'export TD_SESSION=%s\ncd /tmp\nexec %s > %s/%s-window.log 2>&1\n' "$session" "$TD" "$OUT" "$label" > "$OUT/$label-launch.sh"
  hidden_launch "$session" "$WS" "$OUT/$label-window.log" "bash $OUT/$label-launch.sh"
  WINDOWS[SLOT]="$WIN"
  # Both fixtures occupy the same private hidden workspace, so they share a
  # monitor and scale even if the user changes focus during startup. Only
  # these owned windows are floated; canonical float -> exact resize order.
  address=$(hyprctl clients -j | jq -r --argjson pid "$WIN" '.[]|select(.pid==$pid)|.address')
  [[ "$address" =~ ^0x[0-9a-fA-F]+$ ]]
  hyprctl dispatch "hl.dsp.window.float({ action = \"on\", window = \"address:$address\" })"
  hyprctl dispatch "hl.dsp.window.resize({ x = 1000, y = 700, window = \"address:$address\" })"
  # Mode pushes and shell startup are asynchronous. Require actual readiness,
  # rather than assuming a fixed four-second pause started all thirty agents.
  ready=0
  for _ in $(seq 1 60); do
    ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' | sed 's/^[^{]*//' > "$OUT/$label-panes.json"
    count=$(jq --arg mode "$EXPECTED_MODE" '[.result.structuredContent.panes[]?|select(.mode == $mode)]|length' "$OUT/$label-panes.json")
    echo "$label readiness: $count/30 host-classified $EXPECTED_MODE fixtures"
    if [ "$count" = 30 ]; then ready=1; break; fi
    sleep 1
  done
  test "$ready" = 1
  test "$(jq '.result.structuredContent.panes | length' "$OUT/$label-panes.json")" = 31
  cp "$HIDDEN_SESSION_DIR/$session.toml" "$OUT/$label-layout.toml"
done
# Check geometry before sampling, not after choosing which pairs to retain.
hyprctl clients -j | jq --argjson a "${WINDOWS[0]}" --argjson b "${WINDOWS[1]}" '[.[]|select(.pid==$a or .pid==$b)|{pid,at,size,workspace,monitor,floating}]' > "$OUT/geometry.json"
test "$(jq --arg ws "$WS" 'length == 2 and .[0].size == [1000,700] and .[1].size == [1000,700] and .[0].monitor == .[1].monitor and all(.[]; .floating == true and .workspace.name == $ws)' "$OUT/geometry.json")" = true
if [ "${TD_SLEEP_PREFLIGHT:-0}" = 1 ]; then
  echo "PASS: two equal hidden windows, each with one visible quiet pane and thirty host-classified Codex fixtures; no CPU samples taken."
  exit 0
fi
# Quiet precedes working: paused output must not retain a stale "working"
# footer in a sample labelled quiet, or synthesize thirty audible completions.
# Both builds are sampled simultaneously, so neither runs first within a pair.
for condition in idle working; do
  if [ "$condition" = working ]; then touch "$SWITCH"; else rm -f "$SWITCH"; fi
  for pair in $(seq 1 "$ROUNDS"); do
    sleep 5
    python3 "$OUT/sample.py" "${WINDOWS[0]}" "${HOSTS[0]}" "${WINDOWS[1]}" "${HOSTS[1]}" "$condition" "$pair" "$OUT/samples.jsonl" "$COUNTERS" "${SESSIONS[0]}" "${SESSIONS[1]}"
  done
done
echo "$ROUNDS paired idle/working observations complete; cleaning only isolated fixtures."
