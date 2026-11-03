#!/bin/bash
# Drive a real copy ask end to end in a hidden window, nothing on screen.
#   bash copy-rig.sh
# The release lead (seat 101) asks two seats for copies, from inside its own
# pane through MCP. The press limit is 300k: backend lead (290,211 tokens,
# drawn line) runs at once; auth refactor (360,090, bypass) waits on the rail.
# The rig opens the rail with Ctrl+Shift+N, photographs the copy card, presses
# y, and keeps what the asker read back, the copies' own log, and the record.
set -u
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-$(ls /run/user/1000/ | grep -E "^wayland-[0-9]+$" | head -1)}
R=$HOME/.cache/war-room-rig
HERE=$(cd "$(dirname "$0")" && pwd)
export TMPDIR=$R/tmp; mkdir -p "$TMPDIR" "$R/copybin" "$R/shots"
TD=/home/parker/Work/td-authority-lab/war-room-target/debug/terminal-delight
gcc -O1 -Wall -o "$R/copybin/claude" "$HERE/claude.c" || exit 1
cp "$HERE/claude.sh" "$HERE/claude-copy.sh" "$R/copybin/"
: > "$R/copies.log"; : > "$R/copies.out"
source /home/parker/Work/td-authority-lab/war-room/scripts/lib/hidden-window.sh
trap hidden_cleanup EXIT
S=warroomrig2
rec=${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/warroom/$S/asks.jsonl
rm -f "$rec"
sed -e 's#/war-room-rig/bin/claude#/war-room-rig/copybin/claude#g' \
    -e "s#ASK_TITLES='backend lead,docs pages,auth refactor,scratch'#ASK_COPY='backend lead,auth refactor'#" \
    -e 's#^panes = 6#panes = 6\nask_press_tokens = 300000#' \
    "$R/warroomrig1.toml" > "$HIDDEN_SESSION_DIR/$S.toml"
grep -q "ASK_COPY" "$HIDDEN_SESSION_DIR/$S.toml" || { echo "layout edit failed"; exit 1; }
hidden_launch "$S" special:warroomrig "$R/rig2.log" "env TD_SESSION=$S $TD > $R/rig2.log 2>&1"
echo "window $WIN"
shoot() {
  local id; id=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId')
  timeout 10 grim -T "$id" "$R/shots/$1.png" && echo "shot $1"
}
key() { hyprctl dispatch "hl.dsp.send_shortcut({ mods = '$1', key = '$2', window = 'pid:$WIN' })" >/dev/null; }
# Cheap asks are off in this layout; the lead starts its copy asks at 8 s.
for i in $(seq 1 40); do grep -q 'auth refactor: OK' "$R/copies.out" 2>/dev/null && break; sleep 1; done
sleep 2
key 'CTRL SHIFT' N; sleep 1.5
shoot copy-card
key '' y; echo "pressed y"
# Its own marker: an answer may well say "done".
for i in $(seq 1 60); do grep -q 'ALL-ASKS-FINISHED' "$R/copies.out" 2>/dev/null && break; sleep 1; done
sleep 1
shoot after-press
echo "--- what the asker read back"; cat "$R/copies.out"
echo "--- what each copy was started with"; cat "$R/copies.log"
echo "--- the record"; cat "$rec"
"$TD" ctl --pid "$WIN" mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' >/dev/null 2>&1
