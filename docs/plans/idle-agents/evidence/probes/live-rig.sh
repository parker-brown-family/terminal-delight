#!/usr/bin/env bash
# The live check for rest and sleep, off screen and away from every real pane.
#
# A hidden Terminal Delight window on a throwaway session, with its own config
# and state directories, two tabs seeded through its session file: a shell
# (active, so it is the one on screen) and, in the tab behind it, a real Claude
# Code resuming a tiny conversation made here with one Haiku call. agents.toml
# says rest after 1 minute and sleep after 1 minute of rest. The rig watches the
# pane's memory group go from adopted to frozen to asleep, photographs the sleep
# card, presses a key through Hyprland's send_shortcut, and watches the agent
# come back on its own conversation. Everything it starts is torn down on exit.
#
#   live-rig.sh <binary> <out dir>
set -u
export PATH=/home/parker/bin:$PATH
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1} XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/1000}
TD=$1
OUT=$2
CLAUDE=/home/parker/.local/bin/claude
rm -rf "$OUT"; mkdir -p "$OUT/cache" "$OUT/state" "$OUT/config/terminal-delight/sessions" "$OUT/work"
source /home/parker/Work/td-authority-lab/idle-agents/scripts/lib/hidden-window.sh
HIDDEN_SESSION_DIR="$OUT/config/terminal-delight/sessions"
SESSION="idlerig$$"
WS="special:idlerig$$"
LOG="$OUT/rig.log"
log() { printf '%s %s\n' "$(date +%T)" "$*" | tee -a "$LOG"; }
cleanup() {
  hidden_cleanup
  # The tiny conversation is this rig's own; its project folder goes with it.
  rm -rf "/home/parker/.claude/projects/$(echo "$OUT/work" | sed 's/[^A-Za-z0-9]/-/g')"
}
trap cleanup EXIT

# 1. A conversation of our own, as small as one can be.
SID=$(cd "$OUT/work" && env -u TD_PANE_ID -u TD_SESSION -u TD_TAG "$CLAUDE" -p "Reply with the single word ok." \
  --model haiku --output-format json 2>/dev/null | jq -r '.session_id // empty')
[ -n "$SID" ] || { log "could not make a conversation"; exit 1; }
log "conversation $SID"

# 2. Its config: rest after one minute, sleep after one minute of rest.
printf 'rest_after_min = 1\nsleep_after_rest_min = 1\n' > "$OUT/config/terminal-delight/agents.toml"
cat > "$HIDDEN_SESSION_DIR/$SESSION.toml" <<EOF
active = 0

[[tabs]]
name = "shell"

[tabs.node.Leaf]
cwd = "$OUT/work"

[[tabs]]
name = "agent"

[tabs.node.Leaf]
cwd = "$OUT/work"
resume = "$CLAUDE --resume $SID --model haiku"
EOF
{
  echo "export TD_SESSION=$SESSION TD_MCP_SCRIPTED=1 XDG_CONFIG_HOME=$OUT/config XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state"
  echo "exec $TD > $OUT/window.log 2>&1"
} > "$OUT/launch.sh"
hidden_launch "$SESSION" "$WS" "$OUT/window.log" "sh $OUT/launch.sh"
log "window $WIN on $WS, exe $(readlink /proc/$WIN/exe)"
HOST=$(pgrep -f "serve --session $SESSION\$" | head -1)
log "host $HOST"

agent_pid() { for p in $(pgrep -x claude); do grep -q "$SID" /proc/$p/cmdline 2>/dev/null && [ "$(readlink /proc/$p/cwd)" = "$OUT/work" ] && echo $p; done | head -1; }
scope_dir() { ls -d /sys/fs/cgroup/user.slice/user-1000.slice/user@1000.service/app.slice/td-pane-$SESSION-* 2>/dev/null; }
report() {
  local a=$(agent_pid) st="-"
  [ -n "$a" ] && st=$(jq -r '.status' ~/.claude/sessions/$a.json 2>/dev/null)
  local line="agent=${a:-none} status=$st"
  for d in $(scope_dir); do
    line="$line | $(basename $d) frozen=$(awk '/^frozen/{print $2}' $d/cgroup.events) mem=$(( $(cat $d/memory.current)/1048576 ))M swap=$(( $(cat $d/memory.swap.current)/1048576 ))M procs=$(wc -l < $d/cgroup.procs)"
  done
  local store=$OUT/state/terminal-delight/rest-$SESSION.toml
  [ -f "$store" ] && line="$line | store: $(grep -c '^\[asleep' $store) asleep"
  log "$line"
}
shoot() {
  local stable=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
  local said; said=$(timeout 10 grim -T "$stable" "$OUT/$1.png" 2>&1) && log "photographed $1.png" || log "grim failed for $1 (stableId=$stable): $said"
}
key() { hyprctl dispatch "hl.dsp.send_shortcut({ mods = '$1', key = '$2', window = 'pid:$WIN' })" > /dev/null; }

# 3. Watch: adopted, then resting, then asleep. Bounded at six minutes.
STAGE=adopted
for i in $(seq 1 72); do
  sleep 5
  report
  d=$(scope_dir | tail -1)
  case $STAGE in
    adopted) [ -n "$d" ] && [ "$(awk '/^frozen/{print $2}' $d/cgroup.events)" = 1 ] && { STAGE=resting; log "RESTING"; } ;;
    resting) [ -z "$(agent_pid)" ] && { STAGE=asleep; log "ASLEEP"; break; } ;;
  esac
done
[ "$STAGE" = asleep ] || { log "did not reach sleep (stage $STAGE)"; tail -20 "$OUT/window.log" | tee -a "$LOG"; exit 3; }
ls -la "$OUT/state/terminal-delight/wake/" | tee -a "$LOG"
cat "$OUT/state/terminal-delight/wake/$SID.sh" 2>/dev/null | tail -1 | cut -c1-200 | tee -a "$LOG"

# 4. Look at the sleeper: switch to its tab (focus does not wake a sleeper).
key CTRL Page_Down; sleep 1.5; shoot asleep; report
# 5. Wake it with a key, and watch the agent come back on the same conversation.
key '' x
for i in $(seq 1 30); do
  sleep 1
  a=$(agent_pid)
  [ -n "$a" ] && { log "WOKE: agent $a back, argv: $(tr '\0' ' ' < /proc/$a/cmdline | cut -c1-160)"; break; }
done
sleep 4; shoot woke; report
# 6. Rest again. The woken agent was born inside its pane's group (its wake
#    script waited for it), so this time the push-out reaches its pages.
pss() { awk '/^Pss:/{p=$2} /^SwapPss:/{w=$2} END{printf "pss=%dM swappss=%dM", p/1024, w/1024}' /proc/$1/smaps_rollup 2>/dev/null; }
# Look away: back to the shell tab, so the agent's pane is no longer on screen.
key CTRL Page_Up; sleep 1
A=$(agent_pid); log "second look: agent $A $(pss $A)"
for i in $(seq 1 30); do
  sleep 5
  d=$(scope_dir | tail -1); A=$(agent_pid)
  log "agent $A $(pss $A) | $(basename "$d") frozen=$(awk '/^frozen/{print $2}' $d/cgroup.events) charged=$(( $(cat $d/memory.current)/1048576 ))M"
  [ "$(awk '/^frozen/{print $2}' $d/cgroup.events)" = 1 ] && { log "RESTING AGAIN"; sleep 10; log "after: agent $A $(pss $A) charged=$(( $(cat $d/memory.current)/1048576 ))M swap=$(( $(cat $d/memory.swap.current)/1048576 ))M"; shoot resting; break; }
done
log "host still $HOST: $(kill -0 $HOST 2>/dev/null && echo alive || echo GONE)"
grep -iE 'panic|terminal-delight: (not waking|could not)' "$OUT/window.log" | head -5 | tee -a "$LOG"
log "done"
