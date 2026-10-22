#!/bin/bash
# Photograph the War Room in a hidden window on its own session.
#   NAME=<shot name> PICK=<seat id or 1> bash rig.sh
set -u
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-$(ls /run/user/1000/ | grep -E "^wayland-[0-9]+$" | head -1)}
R=$HOME/.cache/war-room-rig
export TMPDIR=$R/tmp; mkdir -p "$TMPDIR"
TD=/home/parker/Work/td-authority-lab/war-room-target/debug/terminal-delight
source /home/parker/Work/td-authority-lab/war-room/scripts/lib/hidden-window.sh
trap hidden_cleanup EXIT
S=warroomrig1
cp "$R/warroomrig1.toml" "$HIDDEN_SESSION_DIR/$S.toml"
hidden_launch "$S" special:warroomrig "$R/rig.log" "env TD_SESSION=$S TD_WAR_ROOM_DEMO=${PICK:-1} $TD > $R/rig.log 2>&1"
echo "window $WIN"
sleep "${WAIT:-16}"
id=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId')
timeout 10 grim -T "$id" "$R/shots/${NAME:-war-room}.png" && echo "shot $R/shots/${NAME:-war-room}.png"
"$TD" ctl --pid "$WIN" mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' >/dev/null 2>&1
