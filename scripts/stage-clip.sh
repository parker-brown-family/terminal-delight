#!/usr/bin/env bash
# stage-clip — drive one demo gesture over the ctl socket while td-clip.sh records.
#
# Everything here is socket-driven, which is the whole point: no keyboard, no mouse,
# no human hitting a mark. It runs against a THROWAWAY terminal-delight instance
# (launch it with TD_SESSION=<name>, which gives it its own session state instead of
# restoring your real panes) so nothing real is ever in frame.
#
#   TD_PID=2773048 TD_GEOM="807,38 781x950" scripts/stage-clip.sh tabs
#   TD_PID=… TD_GEOM=… scripts/stage-clip.sh panes
#   TD_PID=… TD_GEOM=… scripts/stage-clip.sh note <pane-pid>
#
# Setup the instance needs once (also socket-driven):
#   terminal-delight ctl mcp on        --pid <TD_PID>
#   terminal-delight ctl mcp writes on --pid <TD_PID>
#   terminal-delight ctl mcp expose all --pid <TD_PID>
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# NB: no apostrophes or quotes inside these ${VAR:?message} texts — bash parses the
# message as shell words, and a lone apostrophe opens a quote that never closes.
pid="${TD_PID:?export TD_PID — the demo instance process id}"
geom="${TD_GEOM:?export TD_GEOM — X,Y WxH of the demo window}"
dur="${TD_STAGE_DUR:-9}"
what="${1:?tabs | panes | note}"

ctl()  { terminal-delight ctl "$1" --pid "$pid" >/dev/null 2>&1 || true; }
rpc()  { terminal-delight ctl mcp rpc "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"$1\",\"arguments\":$2}}" --pid "$pid" >/dev/null 2>&1 || true; }
tabs() { ctl "tabs $1"; }

record() {
  "$here/scripts/td-clip.sh" "$1" -g ${geom} -d "$dur" >/tmp/stage-clip.out 2>/tmp/stage-clip.err &
  rec=$!
  sleep 5     # 3s countdown, then two seconds of the scene as it was
}
finish() { wait "$rec"; cat /tmp/stage-clip.err >&2; cat /tmp/stage-clip.out; }

case "$what" in
  # The tab strip relabelling itself while the app runs — #304's whole point.
  tabs)
    tabs '[{"op":"name","tab":0,"name":"shell"},{"op":"name","tab":1,"name":"shell"},{"op":"name","tab":2,"name":"shell"},{"op":"ungroup"}]'
    record ctl-tabs
    tabs '[{"op":"name","tab":0,"name":"DEPLOY"}]';  sleep 1
    tabs '[{"op":"name","tab":1,"name":"BUILD"}]';   sleep 1
    tabs '[{"op":"name","tab":2,"name":"REVIEW"}]';  sleep 1
    tabs '[{"op":"group","tab":0,"group":"RELEASE","color":"green"},{"op":"group","tab":1,"group":"RELEASE","color":"green"},{"op":"group","tab":2,"group":"RELEASE","color":"green"}]'
    finish
    ;;

  # Per-pane appearance: one pane bends and warms while its neighbours stay flat.
  panes)
    target="${2:?pane pid to bend}"
    rpc set_pane_config "{\"pid\":$target,\"warp\":0,\"brightness\":50}"
    record per-pane
    for w in 10 25 40 55 70 85; do
      rpc set_pane_config "{\"pid\":$target,\"warp\":$w}"
      sleep 0.4
    done
    rpc set_pane_config "{\"pid\":$target,\"colour\":85,\"brightness\":65}"
    finish
    ;;

  # An agent writing on the glass, then pinning it.
  note)
    target="${2:?pane pid to note}"
    rpc leave_note "{\"pid\":$target,\"clear\":true}"
    record sticky-note-agent
    rpc leave_note "{\"pid\":$target,\"title\":\"TESTS GREEN\",\"text\":\"deploy paused, waiting on DNS\"}"
    sleep 3
    rpc leave_note "{\"pid\":$target,\"title\":\"TESTS GREEN\",\"text\":\"deploy paused, waiting on DNS\",\"pin\":true}"
    finish
    ;;

  # The MCP CONTROL / agent wall filling up: cards arrive as panes are adopted, the
  # exposure count climbs, the tabs take real names. All of it over the socket — this
  # is the one wall shot an agent can stage without a human, because the panel is
  # what a fresh TD_SESSION instance opens on.
  wall)
    record agent-wall
    ctl 'adopt {"cwd":"/home/parker/Work","run":"bash"}';           sleep 1
    tabs '[{"op":"name","tab":1,"name":"DEPLOY"}]';                 sleep 1
    ctl 'adopt {"cwd":"/home/parker/Work/conclave","run":"bash"}';  sleep 1
    tabs '[{"op":"name","tab":2,"name":"BUILD"}]';                  sleep 1
    ctl 'adopt {"cwd":"/home/parker/Work","run":"bash"}';           sleep 1
    tabs '[{"op":"name","tab":3,"name":"REVIEW"},{"op":"group","tab":1,"group":"RELEASE","color":"green"},{"op":"group","tab":2,"group":"RELEASE","color":"green"},{"op":"group","tab":3,"group":"RELEASE","color":"green"}]'
    finish
    ;;

  # The safety model, moving: exposure policy and the write gate flipping live on the
  # MCP control panel while panes arrive. Read-only is the default and the panel says
  # so in words — this is the clip that shows the moat rather than describing it.
  policy)
    ctl "mcp expose agents"; ctl "mcp writes off"; sleep 1
    record mcp-policy
    ctl 'adopt {"cwd":"/home/parker/Work","run":"bash"}';  sleep 2
    ctl "mcp expose all";                                  sleep 2
    ctl 'adopt {"cwd":"/home/parker/Work/conclave","run":"bash"}'; sleep 2
    ctl "mcp writes on";                                   sleep 2
    ctl "mcp writes off"
    finish
    ;;

  *) echo "stage-clip: unknown gesture '$what'" >&2; exit 2 ;;
esac
