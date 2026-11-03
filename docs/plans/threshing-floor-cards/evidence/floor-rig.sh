#!/bin/bash
# Photograph the Threshing Floor's cards in a hidden window on a throwaway
# session. Two seated panes in one org with a line between them, and a seat
# whose pane is gone. The worker gets a brief with a saved concur, handed over
# through MCP declare_deliverable, and a latest message through present_surface;
# the lead hands over nothing. A second run shows an org whose only seat is gone.
#   floor-rig.sh <binary> <out dir> <org id to show: 1 crew, 2 gone>
set -u
export PATH=/home/parker/bin:$PATH
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1} XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/1000}
TD=$1
OUT=$2
ORG=${3:-1}
rm -rf "$OUT"; mkdir -p "$OUT/cache" "$OUT/state"
source /home/parker/Work/td-threshing-floor/wt/scripts/lib/hidden-window.sh
trap 'hidden_cleanup' EXIT
SESSION="floorrig$$"
WS="special:floorrig$$"
mkdir -p "$HIDDEN_SESSION_DIR"
cat > "$HIDDEN_SESSION_DIR/$SESSION.toml" <<EOF
active = 0
war_org = $ORG

[[war_orgs]]
id = 1
name = "Crew"
seats = [11, 12, 13]

[[war_orgs]]
id = 2
name = "Gone"
seats = [14]

[[lines]]
from = 11
to = 12
rung = "ask"

[[tabs]]
name = "lead"

[tabs.node.Leaf]
cwd = "/home/parker"
seat = 11

[[tabs]]
name = "worker"

[tabs.node.Leaf]
cwd = "/home/parker"
seat = 12
resume = "sh $OUT/worker.sh"
EOF
BRIEF="$OUT/order-page-brief.html"
cat > "$BRIEF" <<'EOF'
<html><body><h1>Order page</h1><p id="x">x</p>
<script type="application/json" id="report-notes" data-format="1">{}</script>
<script type="application/json" id="report-concurs" data-format="1">{"ask-1":"2026-10-27 12:00"}</script>
</body></html>
EOF
cat > "$OUT/worker.sh" <<WEOF
#!/bin/sh
# The worker announces its own work, the way an agent does: its latest message
# into its pane's surface mailbox, then a printed Deliverable line.
d="$OUT/state/terminal-delight/surfaces/\$TD_SESSION/\$TD_PANE_ID"
mkdir -p "\$d"
printf '%s' '{"td":"0.4","kind":"response","title":"Order page","model":{"layman":"Wrote the order page and its brief.","brief":"Order page written; two pages left to wire."}}' > "\$d/message.json"
printf '%s' '{"td":"0.4","kind":"artifact","title":"Order page brief","model":{"href":"$BRIEF","mime":"text/html","summary":"the order page, with a concur"}}' > "\$d/artifact.json"
printf '%s\n' "  Deliverable: Order page brief ($BRIEF)"
WEOF
{
  echo "export TD_SESSION=$SESSION TD_MCP_SCRIPTED=1 TD_THRESHING_FLOOR_DEMO=open XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state"
  echo "exec $TD > $OUT/window.log 2>&1"
} > "$OUT/launch.sh"
hidden_launch "$SESSION" "$WS" "$OUT/window.log" "sh $OUT/launch.sh"
echo "window $WIN on $WS, exe $(readlink /proc/$WIN/exe)"
ctl mcp on > /dev/null
ctl mcp writes on > /dev/null
ctl mcp expose all > /dev/null
rpc() { "$TD" ctl --pid "$WIN" mcp rpc "$1" 2>&1 | sed 's/^[0-9]*\t//' | head -1; }
WORKER=""
for _ in $(seq 1 60); do
  WORKER=$(rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' |
    jq -r '[.result.structuredContent.panes[]?] | sort_by(.tab) | last | .pid // empty' 2>/dev/null | head -1)
  [ -n "$WORKER" ] && break
  sleep 0.25
done
echo "worker pane pid: ${WORKER:-none}"
rpc '{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' | jq -c '[.result.structuredContent.panes[]? | {tab, pid, pane_id, title}], .result.structuredContent.hidden' > "$OUT/census.json"; cat "$OUT/census.json"
if [ "$ORG" = 1 ] && [ -n "$WORKER" ]; then
  rpc "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"declare_deliverable\",\"arguments\":{\"pid\":$WORKER,\"label\":\"Order page brief\",\"href\":\"$BRIEF\"}}}" | jq -c '{err: .result.isError, said: .result.content[0].text}'
  rpc "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"present_surface\",\"arguments\":{\"pid\":$WORKER,\"surface\":{\"td\":\"0.4\",\"kind\":\"response\",\"title\":\"Order page\",\"model\":{\"layman\":\"Wrote the order page and its brief.\",\"brief\":\"Order page written; two pages left to wire.\"}}}}}" | jq -c '{err: .result.isError, said: .result.content[0].text}'
fi
sleep 3
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
timeout 10 grim -T "$STABLE" "$OUT/floor.png" && echo "photographed $OUT/floor.png"
grep -i -e panic "$OUT/window.log" | head -3
