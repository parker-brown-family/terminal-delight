#!/usr/bin/env bash
# `open_document`, driven the way an agent drives it, in a window nobody can
# see: does a document open beside the agent that asked, through the same
# router a Ctrl+Alt+click goes through, and nowhere else?
#
# The agent is a stand-in: a script called `claude`, run by absolute path so no
# `claude` on anyone's PATH is ever started, which records whatever reaches its
# stdin. It asks through `ctl mcp from <session> <pane> rpc`, the line an
# agent's relay sends, naming its own pane as the relay would:
#
#   1. a.md beside it                 -> a split in its tab
#   2. a.md again                     -> the pane already showing it is focused
#   3. b.md, c.md                     -> two more splits: four panes
#   4. d.md                           -> the tab is full, so it floats instead
#   5. e.md, placement "here"         -> a square over the agent's own pane
#   6. the first tab's shell, by pid  -> refused, and nothing opens
#   7. a relative path, a .txt file   -> refused in words
#   8. the first tab, and the agent   -> the first tab still one pane; the
#                                        stand-in's stdin empty
#   9. with writes off                -> refused
#
#   scripts/doc-open-check.sh [--bin PATH] [--out DIR]
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
OUT="/tmp/doc-open-check-$(date +%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
command -v jq >/dev/null || { echo "jq is how this reads the answers; not found"; exit 2; }
mkdir -p "$OUT/bin" "$OUT/cache" "$OUT/state" "$OUT/docs"
for n in a b c d e; do printf '# Page %s\n\nMade this turn.\n' "$n" > "$OUT/docs/$n.md"; done
echo "plain text" > "$OUT/docs/notes.txt"
: > "$OUT/typed.bin"

for opener in xdg-open uwsm-app; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/opened.log"\n' "$opener" "$OUT" > "$OUT/bin/$opener"
  chmod +x "$OUT/bin/$opener"
done
cat > "$OUT/bin/claude" <<EOF
#!/usr/bin/env python3
import os, sys, tty
sys.stdout.write("\x1b[?2004hstand-in agent\r\n> ")
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

SESSION="docopen$$"
WS="special:tdopen$$"
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
# ask <arguments-json>: open_document as the stand-in, the whole reply.
ask() {
  ctl mcp from "$SESSION" "${AGENT:--}" rpc "$(jq -nc --argjson a "$1" \
    '{jsonrpc:"2.0",id:1,method:"tools/call",params:{name:"open_document",arguments:$a}}')"
}
said() { jq -r '.result.structuredContent.said // ("ERROR " + (.result.content[0].text // "no answer"))' <<< "$1"; }
opened() { # opened <arguments-json> <what the router must say, a prefix>
  local r s
  r=$(ask "$1")
  s=$(said "$r")
  case "$s" in "$2"*) echo "   ok   $1 -> $s" ;; *) echo "   FAIL $1 -> $s (wanted: $2…)"; fail=1 ;; esac
}
refused() { # refused <arguments-json> <words the refusal must contain>
  local r
  r=$(ask "$1")
  if [ "$(jq -r .result.isError <<< "$r")" = true ] && grep -q -- "$2" <<< "$r"; then
    echo "   ok   $1 refused ($2)"
  else
    echo "   FAIL $1 was not refused with \"$2\": $(said "$r")"; fail=1
  fi
}
panes() { # the exposed panes, one "tab mode" line each
  ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' |
    jq -r '.result.structuredContent.panes[]? | "\(.tab) \(.mode)"'
}
doc() { printf '{"path":"%s"%s}' "$OUT/docs/$1" "${2:-}"; }

echo "== launching a hidden window on $WS (session $SESSION)"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch.sh"
echo "   window $WIN, hidden on $WS"
ctl mcp on > /dev/null
ctl mcp writes on > /dev/null
"$TD" ctl adopt --pid "$WIN" --cwd "$OUT" --run "$OUT/bin/claude" > /dev/null 2>&1
AGENT=""
for _ in $(seq 1 60); do
  AGENT=$(ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' |
    jq -r '.result.structuredContent.panes[]? | select(.mode=="CLAUDE") | .pane_id' | head -1)
  [ -n "$AGENT" ] && break
  sleep 0.25
done
check "the stand-in is running as an agent (pane ${AGENT:-none})" test -n "$AGENT"

echo "== the router, asked by the agent"
opened "$(doc a.md)" "beside pane $AGENT — opened in pane"
opened "$(doc a.md)" "focused pane"
opened "$(doc b.md)" "beside pane $AGENT — opened in pane"
opened "$(doc c.md)" "beside pane $AGENT — opened in pane"
opened "$(doc d.md)" "float pane $AGENT — the tab has four panes"
opened "$(doc e.md ',"placement":"here"')" "float pane $AGENT"

echo "== what it refuses"
# The shell in the first tab, seen once every pane is exposed. A policy change
# is applied on the window's next tick, so it is waited for.
ctl mcp expose all > /dev/null
for _ in $(seq 1 40); do [ "$(panes | grep -c '^0 ')" -ge 1 ] && break; sleep 0.1; done
SHELL_PID=$(ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' |
  jq -r '.result.structuredContent.panes[]? | select(.tab==0) | .pid' | head -1)
refused "$(doc a.md ",\"pid\":${SHELL_PID:-0}")" "not \`pid\`"
refused '{"path":"docs/a.md"}' "relative"
refused "$(doc notes.txt)" "not a file TD can draw"

echo "== nothing else moved"
LISTED=$(panes)
check "the first tab is still one pane ($(grep -c '^0 ' <<< "$LISTED") there)" test "$(grep -c '^0 ' <<< "$LISTED")" -eq 1
check "the agent's tab holds four panes ($(grep -c '^1 ' <<< "$LISTED") there)" test "$(grep -c '^1 ' <<< "$LISTED")" -eq 4
sleep 1
check "the stand-in's stdin is empty ($(stat -c %s "$OUT/typed.bin") bytes)" test ! -s "$OUT/typed.bin"
[ -s "$OUT/opened.log" ] && { echo "   FAIL something went to the desktop:"; cat "$OUT/opened.log"; fail=1; }

echo "== with writes off"
ctl mcp writes off > /dev/null
for _ in $(seq 1 40); do
  [ "$(jq -r .result.isError <<< "$(ask "$(doc b.md)")")" = true ] && break
  sleep 0.1
done
refused "$(doc b.md)" "writes are disabled"

echo "== window log: $LOG"
[ "$fail" -eq 0 ] && echo "== every step held" || echo "== a step did not"
exit "$fail"
