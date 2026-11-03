#!/bin/bash
# The prompt rig: a hidden window on a throwaway session. A scripted
# coordinator (sender.py, an MCP client in its own seated pane) prompts a real
# Claude Code worker (haiku) and a real Codex worker along prompt lines the
# layout draws, and logs what happened.
#   prompt-rig.sh <binary> <out dir>
set -u
export PATH=/home/parker/bin:$PATH
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1} XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/1000}
TD=$1
OUT=$2
RIG=$(cd "$(dirname "$0")" && pwd)  # sender.py, sender2.py and coordinator-brief.txt sit beside this script
rm -rf "$OUT"; mkdir -p "$OUT/cache" "$OUT/state" "$OUT/tmp" "$OUT/coord" "$OUT/claude-worker" "$OUT/codex-worker"
source /home/parker/Work/td-threshing-floor/wt/scripts/lib/hidden-window.sh
# A config dir of the rig's own: its sessions, and agents that rest after one
# minute and sleep after one more, so waking can be watched.
export XDG_CONFIG_HOME=$OUT/config
mkdir -p "$XDG_CONFIG_HOME/terminal-delight/sessions"
printf 'rest_after_min = 1\nsleep_after_rest_min = 1\n' > "$XDG_CONFIG_HOME/terminal-delight/agents.toml"
HIDDEN_SESSION_DIR="$XDG_CONFIG_HOME/terminal-delight/sessions"
trap 'hidden_cleanup' EXIT
SESSION="promptrig$$"
WS="special:promptrig$$"
CODEX=$(mise which codex)
mkdir -p "$HIDDEN_SESSION_DIR"
cat > "$HIDDEN_SESSION_DIR/$SESSION.toml" <<EOF
active = 0
war_org = 1

[[war_orgs]]
id = 1
name = "Crew"
seats = [11, 12, 13, 14]

[[lines]]
from = 11
to = 12
rung = "prompt"

[[lines]]
from = 11
to = 13
rung = "prompt"

[[lines]]
from = 14
to = 12
rung = "prompt"

[[tabs]]
name = "coordinator"

[tabs.node.Leaf]
cwd = "$OUT/coord"
seat = 11
resume = "python3 $RIG/sender.py $TD $OUT/sender.log"

[[tabs]]
name = "claude worker"

[tabs.node.Leaf]
cwd = "$OUT/claude-worker"
seat = 12
resume = "/home/parker/bin/claude --model haiku"

[[tabs]]
name = "codex worker"

[tabs.node.Leaf]
cwd = "$OUT/codex-worker"
seat = 13
resume = "$CODEX"

[[tabs]]
name = "second coordinator"

[tabs.node.Leaf]
cwd = "$OUT/coord"
seat = 14
resume = "python3 $RIG/sender2.py $TD $OUT/sender.log"
EOF
{
  echo "export TD_SESSION=$SESSION TD_MCP_SCRIPTED=1 XDG_CONFIG_HOME=$XDG_CONFIG_HOME XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state TMPDIR=$OUT/tmp"
  echo "exec $TD > $OUT/window.log 2>&1"
} > "$OUT/launch.sh"
hidden_launch "$SESSION" "$WS" "$OUT/window.log" "sh $OUT/launch.sh"
echo "window $WIN on $WS, exe $(readlink /proc/$WIN/exe)"
ctl mcp on > /dev/null
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
shot() { timeout 10 grim -T "$STABLE" "$OUT/$1.png" >/dev/null 2>&1 && echo "photographed $1"; }
for i in $(seq 1 2400); do
  grep -q 'SENDER DONE' "$OUT/sender.log" 2>/dev/null && break
  case $i in 40|200|400|600|900|1200) shot "t$i" ;; esac
  sleep 0.5
done
shot end
for t in 1 2; do ctl tab "$t" >/dev/null 2>&1; sleep 1; shot "tab$t"; done
cat "$OUT/sender.log" | grep -v '^{' 
cp "$HIDDEN_SESSION_DIR/$SESSION.toml" "$OUT/session-after.toml" 2>/dev/null
find "$OUT/state" -name 'asks.jsonl' -exec cp {} "$OUT/asks.jsonl" \;
grep -i -e panic "$OUT/window.log" | head -3
