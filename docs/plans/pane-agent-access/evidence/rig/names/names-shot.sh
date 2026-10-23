#!/bin/bash
# Photograph the War Room naming panes by the left bar and their header, in a
# hidden window on its own session. Nothing appears on screen and the live
# window is never touched.
#   bash names-shot.sh <binary> <out dir>
set -u
TD=$1; OUT=$2; mkdir -p "$OUT"
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-$(ls /run/user/1000/ | grep -E "^wayland-[0-9]+$" | head -1)}
R=$HOME/.cache/war-room-rig
export TMPDIR=$R/tmp; mkdir -p "$TMPDIR"
source /home/parker/Work/td-authority-lab/war-room/scripts/lib/hidden-window.sh
trap hidden_cleanup EXIT
shoot() {
  local id; id=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId')
  timeout 10 grim -T "$id" "$OUT/$1.png" && echo "shot $1"
}
S=warroomnames1
cp "$R/names/names-rig.toml" "$HIDDEN_SESSION_DIR/$S.toml"
# 102 picks the backend lead's seat, so its card and its lines show too.
hidden_launch "$S" special:warroomrig "$OUT/names.log" "env TD_SESSION=$S TD_WAR_ROOM_DEMO=102 $TD > $OUT/names.log 2>&1"
echo "window $WIN"
sleep 10
shoot war-room-names
hidden_close
echo "NAMES-SHOT-FINISHED"
