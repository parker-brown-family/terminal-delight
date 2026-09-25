#!/usr/bin/env bash
# Notes on a Markdown file, in a window nobody can see: are they kept in TD's
# store and never in the file, does the map name lines, and does the agent
# beside the file read the same map the bar would copy?
#
# Nothing here presses anything: this repository never synthesises input. The
# notes are written the way a script writes a brief's, through `ctl doc note
# add`, which drives the same layer the note box does, and read back through
# `ctl doc notes` and the agent's own `document_notes`.
#
#   1. a Markdown file floating over an agent, nothing written  -> notes 0, kept in the store,
#                                                                  "↪ send to agent"
#   2. a note added on a heading                                -> kept (0 unsaved), in the
#                                                                  store file, the map names
#                                                                  its line, the .md unchanged
#   2b. a note on one item of a list                            -> named by that item's own line
#   3. the agent's document_notes                               -> the same map, byte for byte
#   4. the file closed and opened again                         -> the note is still there
#   5. the heading's words changed on disk                      -> the note is kept, under
#                                                                  "On words no longer in the file"
#   5b. that note deleted                                       -> gone from the map and the store
#   6. the agent's stdin afterwards                             -> empty
#
# The agent is a stand-in run by absolute path, as in doc-send-check.sh. The
# window's XDG_STATE_HOME is its own, so the store it writes is under --out.
#
#   scripts/doc-md-notes-check.sh [--bin PATH] [--out DIR]
#
# THE WINDOW IS HIDDEN (scripts/lib/hidden-window.sh). Exit 0: every step
# held. 1: one did not. 2: bad arguments. 4: not hidden.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/hidden-window.sh"
TD="$ROOT/app/target/release/terminal-delight"
OUT="/tmp/doc-md-notes-check-$(date +%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
command -v jq >/dev/null || { echo "jq is how this reads the answers; not found"; exit 2; }
mkdir -p "$OUT/bin" "$OUT/cache" "$OUT/state"
PLAN="$OUT/plan.md"
cat > "$PLAN" <<'EOF'
# The plan

First we read the file.

---

## Slice 2 — the agent side

- one
- two
EOF
cp "$PLAN" "$OUT/plan.before"
: > "$OUT/typed.bin"

for opener in xdg-open uwsm-app; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/opened.log"\n' "$opener" "$OUT" > "$OUT/bin/$opener"
  chmod +x "$OUT/bin/$opener"
done
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

SESSION="docmd$$"
WS="special:tdmd$$"
LOG="$OUT/window.log"
{
  echo "export TD_SESSION=$SESSION PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state"
  echo "exec $TD > $LOG 2>&1"
} > "$OUT/launch.sh"

fail=0
check() {
  local what=$1
  shift
  if "$@"; then echo "   ok   $what"; else echo "   FAIL $what"; fail=1; fi
}
rpc() { printf '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"%s","arguments":{}}}' "$1"; }
# The notes report, once the file and its notes have been read.
notes_now() {
  local r
  for _ in $(seq 1 200); do
    r=$(ctl doc notes)
    case "$r" in
      ok*"{"*) echo "{${r#*\{}"; return 0 ;;
      *"not read yet"*|*"not a laid-out"*) sleep 0.1 ;;
      *) echo "$r"; return 1 ;;
    esac
  done
  echo "timed out waiting for the document"; return 1
}
# field_is <jq path> <want>: polled, since a keep lands a moment later.
field_is() {
  local got=""
  for _ in $(seq 1 50); do
    got=$(notes_now | jq -r "$1")
    [ "$got" = "$2" ] && return 0
    sleep 0.1
  done
  echo "        $1 is \"$got\""
  return 1
}
# map_has <text>: polled, since a change on disk is seen on the watcher's next tick.
map_has() {
  for _ in $(seq 1 50); do
    notes_now | jq -r .map | grep -qF -- "$1" && return 0
    sleep 0.1
  done
  echo "        the map is:"; notes_now | jq -r .map | sed 's/^/        | /'
  return 1
}
store_has() { cat "$OUT"/state/terminal-delight/notes/plan-md-*.json 2>/dev/null | grep -qF -- "$1"; }
map_lacks() {
  for _ in $(seq 1 50); do
    notes_now | jq -r .map | grep -qF -- "$1" || return 0
    sleep 0.1
  done
  return 1
}
store_lacks() {
  for _ in $(seq 1 50); do
    store_has "$1" || return 0
    sleep 0.1
  done
  return 1
}
agent_pane() {
  ctl mcp rpc "$(rpc list_panes)" |
    jq -r '.result.structuredContent.panes[]? | select(.mode=="CLAUDE") | .pane_id' | head -1
}

echo "== launching a hidden window on $WS (session $SESSION)"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch.sh"
echo "   window $WIN, hidden on $WS"
ctl mcp on > /dev/null
ctl mcp expose all > /dev/null
"$TD" ctl adopt --pid "$WIN" --cwd "$OUT" --run "$OUT/bin/claude" > /dev/null 2>&1
AGENT=""
for _ in $(seq 1 60); do AGENT=$(agent_pane); [ -n "$AGENT" ] && break; sleep 0.25; done
check "the stand-in is running as an agent (pane ${AGENT:-none})" test -n "$AGENT"

echo "== 1. a Markdown file over the agent, nothing written on it"
r=$(ctl doc here "$PLAN")
check "doc here opened it ($r)" test "${r%% *}" = ok
check "it shows notes" field_is .state notes
check "none yet" field_is .notes 0
check "kept in TD's store" field_is .kept store
check "↪ says it sends to the agent" field_is .send_to "↪ send to agent"

echo "== 2. a note on the second heading"
r=$(ctl doc note add h-slice-2-the-agent "Before slice 1, please.")
check "the note was added ($(printf %.30s "$r"))" test "${r%% *}" = ok
check "and kept at once" field_is .unsaved 0
check "the bar counts it" field_is .notes 1
check "the map names its line" map_has "[L7] ## Slice 2 — the agent side"
check "with the note under it" map_has "  · Before slice 1, please."
check "the map names the file whole" map_has "NOTES — $PLAN"
check "the store holds it" store_has "Before slice 1, please."
check "the Markdown file is byte for byte as it was" cmp -s "$PLAN" "$OUT/plan.before"

echo "== 2b. a note on one item of a list"
r=$(ctl doc note add li-two "Only this one.")
check "the note was added ($(printf %.30s "$r"))" test "${r%% *}" = ok
check "the map names the item by its own line" map_has "[L10] • two"
check "two notes now" field_is .notes 2

echo "== 3. the agent reads the same map"
ours=$(notes_now | jq -r .map)
theirs=$(ctl mcp from "$SESSION" "${AGENT:--}" rpc "$(rpc document_notes)" | jq -r .result.structuredContent.map)
check "document_notes' map is the bar's, byte for byte" test "$ours" = "$theirs"

echo "== 4. closed and opened again"
ctl doc close > /dev/null
r=$(ctl doc here "$PLAN")
check "opened again ($r)" test "${r%% *}" = ok
check "the notes are still there" field_is .notes 2

echo "== 5. the heading's words change on disk"
sed -i 's/^## Slice 2 — the agent side$/## The agent, second/' "$PLAN"
check "the note is kept, under the words it was written on" map_has "On words no longer in the file:"
check "still counted" field_is .notes 2
check "and still in the store" store_has "Before slice 1, please."

echo "== 5b. the note on gone words, deleted"
r=$(ctl doc note delete h-slice-2-the-agent "Before slice 1, please.")
check "deleted ($(printf %.30s "$r"))" test "${r%% *}" = ok
check "one note left" field_is .notes 1
check "the map has no gone words now" map_lacks "On words no longer in the file:"
check "and the store has let it go" store_lacks "Before slice 1, please."

echo "== 6. nothing reached the agent unasked"
sleep 1
check "the stand-in's stdin is empty ($(stat -c %s "$OUT/typed.bin") bytes)" test ! -s "$OUT/typed.bin"
[ -s "$OUT/opened.log" ] && { echo "   FAIL something went to the desktop:"; cat "$OUT/opened.log"; fail=1; }

echo "== window log: $LOG"
[ "$fail" -eq 0 ] && echo "== every step held" || echo "== a step did not"
exit "$fail"
