#!/usr/bin/env bash
set -euo pipefail
TD=$1
OUT=$2
STYLE=${3:-chalk-flower}
SESSION=${4:-flowerstudy$$}
ROOT=/home/parker/Work/td-chalk-flower
mkdir -p "$OUT/config/terminal-delight/sessions"
cat > "$OUT/config/terminal-delight/sessions/$SESSION.toml" <<EOF
active = 0
lang = "en"
left_bar = true
anchor_top = false
scale = 1.15
warp = 0.0
win = [0.0,0.0,1440.0,950.0]

[theme]
id = "$STYLE"
color = "default"

[theme.grade]
crt = false
scale = 1.15
brightness = 0.5
contrast = 0.5
colour = 0.5
text = 0.5
background = 0.5
gamma = 0.5
warp = 0.0
phosphor = 0.0

[[tabs]]
name = "Paper & pigment"
[tabs.node.Split]
dir = "Row"
ratio = 0.58
[tabs.node.Split.a.Leaf]
cwd = "/tmp"
resume = "python3 /tmp/chalk-native-fixture.py"
[tabs.node.Split.b.Leaf]
cwd = "/tmp"
resume = "python3 /tmp/chalk-native-fixture.py"

[[tabs]]
name = "The offerings page"
[tabs.node.Leaf]
cwd = "/tmp"
resume = "python3 /tmp/chalk-native-fixture.py"
EOF
cat > "$OUT/launch.sh" <<EOF
export TD_SESSION=$SESSION XDG_CONFIG_HOME=$OUT/config XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state
export SHELL=/bin/bash FLOWER_BUSY=$OUT/busy
unset TD_DEMO_STATE TD_THEME TD_PALETTE TD_SKIN TD_WALL_DEMO TD_BENCH_DEMO
exec $TD > $OUT/window.log 2>&1
EOF
. "$ROOT/scripts/lib/hidden-window.sh"
HIDDEN_SESSION_DIR="$OUT/config/terminal-delight/sessions"
trap hidden_cleanup EXIT
hidden_launch "$SESSION" "special:$SESSION" "$OUT/window.log" "sh $OUT/launch.sh"
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[]|select(.pid==$p)|.stableId')
echo "window=$WIN stable=$STABLE session=$SESSION"
ctl mcp on
ctl mcp writes on
ctl mcp expose all
sleep 2
ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' > "$OUT/panes.json"
PID=$(jq -r '.result.structuredContent.panes[0].pid' "$OUT/panes.json")
ctl mcp rpc "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"leave_note\",\"arguments\":{\"pid\":$PID,\"title\":\"PAPER & PETALS\",\"text\":\"The workshop has become a place to work.\",\"pin\":true}}}"
sleep 1
timeout 12 grim -T "$STABLE" "$OUT/terminal.png"
ctl skin status > "$OUT/skin.txt"
ctl skin default
sleep 1
timeout 12 grim -T "$STABLE" "$OUT/default-skin.png"
ctl skin paper
ctl bench on
sleep 1
timeout 12 grim -T "$STABLE" "$OUT/bench.png"
ctl bench off
sleep 1
for skin in default deco console paper; do ctl skin "$skin"; done
hidden_close || true
hidden_launch "$SESSION" "special:$SESSION" "$OUT/restart.log" "sh $OUT/launch.sh"
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[]|select(.pid==$p)|.stableId')
sleep 2
ctl skin status > "$OUT/restart-skin.txt"
timeout 12 grim -T "$STABLE" "$OUT/restart.png"
# Keep the native window available for additional targeted checks. The caller
# can release it by creating this run's own finish file.
echo "$WIN" > "$OUT/window.pid"
for _ in $(seq 1 900); do [ -f "$OUT/finish" ] && break; sleep 1; done
