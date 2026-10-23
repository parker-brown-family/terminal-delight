#!/bin/bash
# The War Room's landing trial, on the merged release build, in hidden windows
# on their own sessions. Nothing appears on screen and the live window is never
# touched.
#   bash land-trial.sh <release binary> <out dir>
# Part 1 — the default press limit (0): the release lead asks two seats for
# copies. Each waits on the rail; the rig presses y for each and reads both
# answers back.
# Part 2 — the War Room and the artifact rail open together: Esc closes the
# rail and leaves the War Room. (The War Room's demo hook reopens it on every
# frame, so the rig cannot show the second Esc closing it.)
set -u
TD=$1; OUT=$2; mkdir -p "$OUT"
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t /run/user/1000/hypr/ | head -1)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-$(ls /run/user/1000/ | grep -E "^wayland-[0-9]+$" | head -1)}
R=$HOME/.cache/war-room-rig
HERE=/home/parker/Work/td-authority-lab/war-room/docs/plans/pane-agent-access/evidence/rig/copies
export TMPDIR=$R/tmp; mkdir -p "$TMPDIR" "$R/copybin"
gcc -O1 -Wall -o "$R/copybin/claude" "$HERE/claude.c" || exit 1
cp "$HERE/claude.sh" "$HERE/claude-copy.sh" "$R/copybin/"
: > "$R/copies.log"; : > "$R/copies.out"
source /home/parker/Work/td-authority-lab/war-room/scripts/lib/hidden-window.sh
trap hidden_cleanup EXIT
shoot() {
  local id; id=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId')
  timeout 10 grim -T "$id" "$OUT/$1.png" && echo "shot $1"
}
key() { hyprctl dispatch "hl.dsp.send_shortcut({ mods = '$1', key = '$2', window = 'pid:$WIN' })" >/dev/null; }

echo "== part 1: every copy waits for a press ($TD)"
S=warroomland1
rec=${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/warroom/$S/asks.jsonl
rm -f "$rec"
sed -e 's#/war-room-rig/bin/claude#/war-room-rig/copybin/claude#g' \
    -e "s#ASK_TITLES='backend lead,docs pages,auth refactor,scratch'#ASK_COPY='backend lead,auth refactor'#" \
    -e "s#TD=/home/parker/Work/td-authority-lab/war-room-target/debug/terminal-delight#TD=$TD#g" \
    "$R/warroomrig1.toml" > "$HIDDEN_SESSION_DIR/$S.toml"
grep -q "ASK_COPY" "$HIDDEN_SESSION_DIR/$S.toml" || { echo "layout edit failed"; exit 1; }
grep -q "ask_press_tokens" "$HIDDEN_SESSION_DIR/$S.toml" && { echo "the layout sets a press limit; this part tests the default"; exit 1; }
hidden_launch "$S" special:warroomrig "$OUT/land1.log" "env TD_SESSION=$S $TD > $OUT/land1.log 2>&1"
echo "window $WIN"
press_for() {
  # Wait for the copy ask of $1 to be on the rail, photograph it, press y.
  for i in $(seq 1 60); do grep -q "ask_agent copy -> $1" "$R/copies.out" 2>/dev/null && break; sleep 1; done
  sleep 2
  # The rail stays open after a press, and Ctrl+Shift+N toggles it: open it
  # only for the first card.
  [ "$2" = 1 ] && { key 'CTRL SHIFT' N; sleep 1.5; }
  shoot "card-$2"
  key '' y; echo "pressed y for $1"
  for i in $(seq 1 60); do grep -q "ask_answer [0-9]* ($1)" "$R/copies.out" 2>/dev/null && break; sleep 1; done
  sleep 1
}
press_for 'backend lead' 1
press_for 'auth refactor' 2
for i in $(seq 1 30); do grep -q 'ALL-ASKS-FINISHED' "$R/copies.out" 2>/dev/null && break; sleep 1; done
shoot after-presses
echo "--- what the asker read back"; cat "$R/copies.out"
echo "--- what each copy was started with"; cat "$R/copies.log"
echo "--- the record"; cat "$rec"
hidden_close

echo "== part 2: the War Room and the artifact rail together"
S2=warroomland2
sed -e "s#ASK_TITLES='backend lead,docs pages,auth refactor,scratch' ##" \
    -e "s#TD=/home/parker/Work/td-authority-lab/war-room-target/debug/terminal-delight#TD=$TD#g" \
    "$R/warroomrig1.toml" > "$HIDDEN_SESSION_DIR/$S2.toml"
hidden_launch "$S2" special:warroomrig "$OUT/land2.log" "env TD_SESSION=$S2 TD_WAR_ROOM_DEMO=open TD_SKIN_SCENE=artifacts $TD > $OUT/land2.log 2>&1"
echo "window $WIN"
sleep 8
shoot both-open
key '' Escape; sleep 1.5
shoot after-one-esc
echo "PART-2-FINISHED"
