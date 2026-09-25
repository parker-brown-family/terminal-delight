#!/usr/bin/env bash
# ↪ send to agent, in a window nobody can see: is the button drawn beside an
# agent pane and nowhere else, and does anything reach the agent unasked?
#
# ↪ is a press, and this repository never synthesises input, so nothing here
# presses it. What a press does is held by unit tests in pane.rs: the bytes are
# a bracketed paste with no carriage return (`notes_paste`), and the one caller
# of `send_notes` is the handler of the bar's own event. What a unit test cannot
# reach is the wiring that decides whether the button is drawn at all — the
# workspace's render pass working out, from the tab, who each brief sits beside,
# and handing that down through the pane and the view to the bar. This reads it
# back through `ctl doc notes`, whose `send_to` is the button's own words:
#
#   1. a brief floating over a shell, in a tab with no agent  -> no button
#   2. the same brief floating over an agent pane             -> "↪ send to agent"
#   3. the same brief in a pane opened beside the agent        -> "↪ send to agent"
#   4. the agent's stdin afterwards                            -> empty: nothing
#                                                                 was typed at it
#
# The agent is a stand-in: a script called `claude` that turns bracketed paste
# on, as Claude Code does, and writes every byte it is sent to a file. It is run
# by absolute path, so no `claude` on anyone's PATH is ever started.
#
#   scripts/doc-send-check.sh [--bin PATH] [--out DIR]
#
# THE WINDOW IS HIDDEN (scripts/lib/hidden-window.sh): a special workspace,
# `render_unfocused` and `no_initial_focus`, killed at once, exit 4, if it lands
# anywhere else. Its session is its own, and the window, its host, the stand-in
# and the session's files go when the script exits.
#
# Exit 0: every step held. 1: one did not. 2: bad arguments. 4: not hidden.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/hidden-window.sh"
TD="$ROOT/app/target/release/terminal-delight"
OUT="/tmp/doc-send-check-$(date +%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
command -v jq >/dev/null || { echo "jq is how this reads the answers; not found"; exit 2; }
BRIEF="$ROOT/app/tests/fixtures/decision-brief/notes-format/cases/current-pristine/expected.html"
[ -f "$BRIEF" ] || { echo "no fixture at $BRIEF"; exit 2; }
mkdir -p "$OUT/bin" "$OUT/cache" "$OUT/state"
cp "$BRIEF" "$OUT/brief.html"
: > "$OUT/typed.bin"

# Nothing may reach the desktop from here.
for opener in xdg-open uwsm-app; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/opened.log"\n' "$opener" "$OUT" > "$OUT/bin/$opener"
  chmod +x "$OUT/bin/$opener"
done

# The stand-in agent. Its path ends in /claude, which is what the pane's
# classifier reads, and it records whatever arrives on its stdin.
cat > "$OUT/bin/claude" <<EOF
#!/usr/bin/env python3
import os, sys, tty
sys.stdout.write("\x1b[?2004hstand-in agent, bracketed paste on\r\n> ")
sys.stdout.flush()
tty.setraw(0)
with open("$OUT/typed.bin", "ab", buffering=0) as f:
    while True:
        b = os.read(0, 4096)
        if not b:
            break
        f.write(b)
EOF
chmod +x "$OUT/bin/claude"

trap 'pkill -f "$OUT/bin/claude" 2>/dev/null; hidden_cleanup' EXIT

SESSION="docsend$$"
WS="special:tdsend$$"
LOG="$OUT/window.log"
{
  echo "export TD_SESSION=$SESSION PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state"
  echo "exec $TD > $LOG 2>&1"
} > "$OUT/launch.sh"

fail=0
check() { # check <what> <command…>
  local what=$1
  shift
  if "$@"; then echo "   ok   $what"; else echo "   FAIL $what"; fail=1; fi
}
rpc() { printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"%s","arguments":{}}}' "$1"; }
# The notes report, once the page is laid out.
notes_now() {
  local r
  for _ in $(seq 1 200); do
    r=$(ctl doc notes)
    case "$r" in
      ok*"{"*) echo "{${r#*\{}"; return 0 ;;
      "err the document is not a laid-out HTML page yet") sleep 0.1 ;;
      *) echo "$r"; return 1 ;;
    esac
  done
  echo "timed out waiting for the page"; return 1
}
# send_to_is <want>: the bar's ↪, read until it says <want> ("null" for no
# button). The workspace sets it on its next frame, so it is polled.
send_to_is() {
  local got=""
  for _ in $(seq 1 50); do
    got=$(notes_now | jq -r '.send_to // "null"')
    [ "$got" = "$1" ] && return 0
    sleep 0.1
  done
  echo "        send_to is \"$got\""
  return 1
}
agent_pane() {
  ctl mcp rpc "$(rpc list_panes)" |
    jq -r '.result.structuredContent.panes[] | select(.mode=="CLAUDE") | .pane_id' | head -1
}

echo "== launching a hidden window on $WS (session $SESSION)"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch.sh"
echo "   window $WIN, hidden on $WS"
ctl mcp on > /dev/null
ctl mcp expose all > /dev/null

echo "== 1. a brief over a shell, with no agent in the tab"
r=$(ctl doc here "$OUT/brief.html")
check "doc here opened it ($r)" test "${r%% *}" = ok
check "no ↪ is drawn" send_to_is null
ctl doc close > /dev/null

echo "== 2. the same brief over an agent pane"
"$TD" ctl adopt --pid "$WIN" --cwd "$OUT" --run "$OUT/bin/claude" > /dev/null 2>&1
AGENT=""
for _ in $(seq 1 60); do AGENT=$(agent_pane); [ -n "$AGENT" ] && break; sleep 0.25; done
check "the stand-in is running as an agent (pane ${AGENT:-none})" test -n "$AGENT"
r=$(ctl doc here "$OUT/brief.html")
check "doc here opened it over the agent ($r)" test "${r%% *}" = ok
check "↪ says it sends to the agent" send_to_is "↪ send to agent"
found=$(ctl mcp from "$SESSION" "${AGENT:--}" rpc "$(rpc document_notes)" | jq -r .result.structuredContent.found)
check "and the agent, asking, finds the square over its own pane ($found)" test "$found" = float-over-you
ctl doc close > /dev/null

echo "== 3. the same brief in a pane opened beside the agent"
r=$(ctl doc beside "$OUT/brief.html")
check "doc beside split it off the agent ($r)" test "${r#ok beside pane }" != "$r"
check "↪ on the split says it sends to the agent" send_to_is "↪ send to agent"
found=$(ctl mcp from "$SESSION" "${AGENT:--}" rpc "$(rpc document_notes)" | jq -r .result.structuredContent.found)
check "and the agent, asking, finds the split it opened ($found)" test "$found" = your-split

echo "== 4. nothing reached the agent unasked"
sleep 1
check "the stand-in's stdin is empty ($(stat -c %s "$OUT/typed.bin") bytes)" test ! -s "$OUT/typed.bin"
[ -s "$OUT/opened.log" ] && { echo "   FAIL something went to the desktop:"; cat "$OUT/opened.log"; fail=1; }

echo "== window log: $LOG"
[ "$fail" -eq 0 ] && echo "== every step held" || echo "== a step did not"
exit "$fail"
