#!/usr/bin/env bash
# Run commands inside a real, hidden TD pane and wait for each to finish.
#
# Same hidden-window recipe as apcprobe.sh. Each command runs in its own tab
# through `ctl adopt`, wrapped so its exit code and wall time land in
# OUTDIR/<n>.done; stderr goes to OUTDIR/<n>.err. The pane's tty stays the
# program's stdin and stdout, so it talks to TD exactly as it would by hand.
#
#   tdrun.sh OUTDIR 'cmd one' 'cmd two' ...
set -uo pipefail
OUT=$1; shift
TD=$(readlink -f "$HOME/.local/bin/terminal-delight")
mkdir -p "$OUT"
SESSION="tdrun$$"
WS="special:tdrun$$"
LOG="$OUT/window.log"
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
SAID=$(hyprctl dispatch "hl.dsp.exec_cmd(\"sh -c 'TD_SESSION=$SESSION exec $TD > $LOG 2>&1'\", { workspace = \"$WS silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
case "$SAID" in ok|"") ;; *) echo "hyprctl refused the launch: $SAID"; exit 1 ;; esac
for _ in $(seq 1 60); do
  WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $SESSION" '.[] | select(.title==$t) | .pid' | head -1)
  [ -n "$WIN" ] && break
  sleep 0.5
done
[ -n "$WIN" ] || { echo "no window appeared — see $LOG"; exit 1; }
LANDED=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
if [ "$LANDED" != "$WS" ]; then echo "landed on '$LANDED', not $WS — killed"; kill "$WIN"; WIN=""; exit 4; fi
echo "window $WIN hidden on $LANDED ($(basename "$TD"))"
ctl() { "$TD" ctl --pid "$WIN" "$@" 2>&1 | head -1 | sed 's/^[0-9]*\t//'; }
for _ in $(seq 1 40); do [ "$(ctl ping)" = "pong" ] && break; sleep 0.5; done
sleep 2
n=0
for cmd in "$@"; do
  n=$((n + 1))
  D="$OUT/$n.done"; rm -f "$D" "$OUT/$n.err"
  printf '%s\n' "$cmd" > "$OUT/$n.cmd"
  wrapped="s=\$(date +%s%N); $cmd 2>$OUT/$n.err; rc=\$?; e=\$(date +%s%N); echo rc=\$rc ms=\$(( (e - s) / 1000000 )) > $D"
  echo "[$n] adopt: $(ctl adopt --cwd "$OUT" --run "sh -c '$wrapped'")"
  for _ in $(seq 1 200); do [ -s "$D" ] && break; sleep 0.2; done
  echo "[$n] $cmd"
  echo "    $(cat "$D" 2>/dev/null || echo 'did not finish in 40 s')"
  [ -s "$OUT/$n.err" ] && echo "    stderr: $(head -c 300 "$OUT/$n.err" | tr '\n' ' ')"
done
