#!/usr/bin/env bash
# How fast does a TD pane take in video-sized Kitty frames today, and does the
# window's copy of the pane keep up?
#
# Launches a hidden window (the doc-float-soak.sh recipe: its own session, a
# special workspace, render_unfocused, no_initial_focus; killed at once if it
# lands anywhere visible), opens a tab running apcwriter.py through
# `ctl adopt`, and reads back the writer's own timing. TD drops the pixels
# (vte discards APC), so this is the read path alone: PTY -> host parser ->
# tee -> socket -> window replica parser.
#
# A window that falls more than SINK_DEPTH chunks behind is dropped by the
# host and re-attaches, which opens a new socket. TD logs nothing when that
# happens, so the probe diffs the window's socket inodes across the run.
#
#   apcprobe.sh OUTDIR FRAMES [hosted|owned]
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=$1; FRAMES=$2; MODE=${3:-hosted}
TD=$(readlink -f "$HOME/.local/bin/terminal-delight")
mkdir -p "$OUT"
SESSION="apcprobe$$"
WS="special:tdapc$$"
LOG="$OUT/window-$MODE.log"
EXTRA=""
[ "$MODE" = owned ] && EXTRA="TD_NO_SESSIOND=1"
WIN=""
cleanup() {
  local pids
  pids="$WIN $(pgrep -f "serve --session $SESSION" 2>/dev/null)"
  for p in $pids; do kill "$p" 2>/dev/null; done
  for _ in $(seq 1 50); do
    alive=0
    for p in $pids; do kill -0 "$p" 2>/dev/null && alive=1; done
    [ "$alive" -eq 0 ] && break
    sleep 0.1
  done
  rm -f "$HOME/.config/terminal-delight/sessions/$SESSION".*
  rm -rf "$HOME/.config/terminal-delight/sessions/backups/$SESSION"
}
trap cleanup EXIT

SAID=$(hyprctl dispatch "hl.dsp.exec_cmd(\"sh -c 'TD_SESSION=$SESSION $EXTRA exec $TD > $LOG 2>&1'\", { workspace = \"$WS silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
case "$SAID" in ok|"") ;; *) echo "hyprctl refused the launch: $SAID"; exit 1 ;; esac
for _ in $(seq 1 60); do
  WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $SESSION" '.[] | select(.title==$t) | .pid' | head -1)
  [ -n "$WIN" ] && break
  sleep 0.5
done
[ -n "$WIN" ] || { echo "no window appeared — see $LOG"; exit 1; }
LANDED=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
if [ "$LANDED" != "$WS" ]; then echo "landed on '$LANDED', not $WS — killed"; kill "$WIN"; WIN=""; exit 4; fi
echo "window $WIN hidden on $LANDED, $MODE"
ctl() { "$TD" ctl --pid "$WIN" "$@" 2>&1 | head -1 | sed 's/^[0-9]*\t//'; }
for _ in $(seq 1 40); do [ "$(ctl ping)" = "pong" ] && break; sleep 0.5; done
sleep 2
socks() { ls -l "/proc/$WIN/fd" 2>/dev/null | grep -o 'socket:\[[0-9]*\]' | sort -u; }
for kind in apc text; do
  R="$OUT/$MODE-$kind.json"; rm -f "$R"
  echo "adopt: $(ctl adopt --cwd "$OUT" --run "env APC_DELAY=3 python3 $HERE/apcwriter.py $FRAMES $R $kind")"
  sleep 2; BEFORE=$(socks)
  for _ in $(seq 1 300); do [ -s "$R" ] && break; sleep 0.2; done
  sleep 1; AFTER=$(socks)
  NEW=$(comm -13 <(echo "$BEFORE") <(echo "$AFTER") | grep -c socket)
  if [ -s "$R" ]; then cat "$R"; echo " new-sockets-during-run=$NEW"; else echo "$kind: no result after 60 s"; fi
done
