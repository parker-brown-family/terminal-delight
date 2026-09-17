#!/usr/bin/env bash
# The workbench, driven end to end without a pointer.
#
# Every gesture on the bench is a mouse gesture, and the shell that builds it
# has a keyboard and no pointer — so the surface could be photographed and not
# operated. The `ctl bench` verbs close that gap by reaching the same methods
# the chips do, and this script is the run that proves it, repeatably.
#
# It launches a window on a session of its own, seeds one of every kind of
# surface through the real file transport, then reads, answers and types —
# photographing each step. Nothing here touches your live session.
#
#   scripts/workbench-smoke.sh [outdir]
#
# Exit 0 means every step reported ok and every photograph was taken. It does
# NOT mean the pixels are right: a human still has to look at the images, and
# the point of the script is that there is something to look at.
set -uo pipefail

TD="${TD_BIN:-$(cd "$(dirname "$0")/.." && pwd)/app/target/release/terminal-delight}"
OUT="${1:-/tmp/workbench-smoke-$(date +%H%M%S)}"
SESSION="wbsmoke$$"
mkdir -p "$OUT"

fail=0
step() { printf '\n\033[1m== %s\033[0m\n' "$*"; }
ok()   { printf '   ok   %s\n' "$*"; }
bad()  { printf '   FAIL %s\n' "$*"; fail=1; }

[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }

step "launching a window on session $SESSION"
setsid env TD_SESSION="$SESSION" TD_WORKBENCH_DEMO=1 TD_MCP_WRITE=1 \
  "$TD" > "$OUT/window.log" 2>&1 < /dev/null &
sleep 14

PID=$(pgrep -f "TD_SESSION=$SESSION" | head -1)
# The env is not in the cmdline for the window itself; find it by title.
GEOM=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $SESSION" \
  '.[] | select(.title==$t) | "\(.pid) \(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"' | head -1)
if [ -z "$GEOM" ]; then
  bad "no window appeared — see $OUT/window.log"
  exit 1
fi
PID=${GEOM%% *}; REST=${GEOM#* }; AT=${REST%% *}; SIZE=${REST##* }
ok "window $PID at $AT size $SIZE"
grep -q "seeded" "$OUT/window.log" && ok "demo surfaces seeded" || bad "nothing seeded"

shoot() { # shoot <name>
  if command -v grim >/dev/null && timeout 12 grim -g "$AT ${SIZE}" "$OUT/$1.png" 2>/dev/null; then
    ok "shot $1.png"
  else
    bad "could not photograph $1 (grim missing, or the window is on a hidden workspace)"
  fi
}

ctl() { # ctl <words...>  — a verb that must succeed
  expect 'ok' "$@"
}

# expect <reply pattern> <words...> — the row is what is checked, not the
# exit code. The bench verbs now answer with what actually happened (`ok pane
# 3`, `queued pane 3 …`, `err no pane …`), and a step written to exercise a
# refusal has to assert the refusal, or it passes for the wrong reason: this
# script once reported every step green on a run whose window log said no
# pane had shown a bench at all.
expect() {
  local want=$1; shift
  local reply
  reply=$("$TD" ctl --pid "$PID" "$@" 2>&1 | head -1)
  case "$reply" in
    *"$want"*) ok "ctl $* -> $reply" ;;
    *)         bad "ctl $* -> $reply (wanted: $want)" ;;
  esac
}

step "the terminal face"
ctl bench off
sleep 1; shoot 1-terminal

step "the bench face"
ctl bench on
sleep 1; shoot 2-bench

step "walking the rail, and the shelves"
ctl bench off   # keys need the bench up; this proves off/on both answer
ctl bench on
sleep 1; shoot 3-bench-again

step "answering the demo decision from the bench"
# The demo opens on its decision, whose first action is approve. `choose` is
# for questions, so this one exercises the refusal path: a decision has no
# options to choose, and the window must say so rather than doing something.
expect 'err no' bench choose 1
sleep 1; shoot 4-after-choose

step "typing into the agent from the bench"
# A shell pane has no agent, so this reports honestly rather than pretending.
expect 'err no' bench say "hello from the smoke test"
sleep 2; shoot 5-after-say

step "the window survives all of it"
if kill -0 "$PID" 2>/dev/null; then ok "still running"; else bad "the window died"; fi

step "cleaning up"
kill "$PID" 2>/dev/null
sleep 2
pkill -f "serve --session $SESSION" 2>/dev/null
rm -rf "${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/surfaces/$SESSION"
ok "session $SESSION gone"

printf '\n%s\n' "images in $OUT"
if [ "$fail" = 0 ]; then
  printf '\033[32mall steps reported ok — now LOOK at the images\033[0m\n'
else
  printf '\033[31msomething failed above\033[0m\n'
fi
exit "$fail"
