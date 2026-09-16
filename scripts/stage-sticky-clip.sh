#!/usr/bin/env bash
# stage-sticky-clip — drive the "an agent writes a sticky note" gesture while
# td-clip.sh records it. The whole point of this shot is that no human touches
# anything: the note lands on the glass because an agent said so, through the MCP
# `leave_note` tool carried over the ctl socket.
#
#   scripts/stage-sticky-clip.sh <pane-pid> <window-geometry> [workspace]
#   scripts/stage-sticky-clip.sh 2282003 "12,38 1576x950" 1
#
# Beats, from the first recorded frame:
#   0s  bare glass (any existing note is peeled before recording starts)
#   2s  the note lands, handwritten
#   5s  a pin goes through it — and shows up on the pane's tab in the mother bar
#   8s  hold, then stop
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
pid="${1:?pane pid, from list_panes}"
geom="${2:?window geometry, as 'X,Y WxH'}"
ws="${3:-active}"
dur="${TD_STAGE_DUR:-9}"

rpc() {
  terminal-delight ctl mcp rpc \
    "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"leave_note\",\"arguments\":$1}}" \
    --workspace "$ws" >/dev/null 2>&1 || true
}

# Peel whatever is on the glass, so the clip opens on a bare pane.
rpc "{\"pid\":$pid,\"clear\":true}"
sleep 1

"$here/scripts/td-clip.sh" sticky-note-agent -g ${geom} -d "$dur" --gif >/tmp/td-clip-out.$$ 2>/tmp/td-clip-err.$$ &
rec=$!

sleep 5      # 3s countdown + 2s of bare glass on tape
rpc "{\"pid\":$pid,\"title\":\"TESTS GREEN\",\"text\":\"deploy paused, waiting on DNS\"}"
sleep 3
rpc "{\"pid\":$pid,\"title\":\"TESTS GREEN\",\"text\":\"deploy paused, waiting on DNS\",\"pin\":true}"

wait "$rec"
cat /tmp/td-clip-err.$$ >&2
cat /tmp/td-clip-out.$$
rm -f /tmp/td-clip-out.$$ /tmp/td-clip-err.$$
