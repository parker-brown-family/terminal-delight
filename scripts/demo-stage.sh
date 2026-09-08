#!/usr/bin/env bash
# demo-stage — stand up (and tear down) a clean, full-screen Terminal Delight on a
# spare workspace, for filming. Nothing of yours is ever in frame.
#
#   scripts/demo-stage.sh up      # spare workspace, big window, prints TD_PID/TD_GEOM
#   scripts/demo-stage.sh down    # kills it, restores your workspace and cursor
#
# Why a spare workspace: a demo instance tiled beside your real terminal squeezes it
# to half width, and one stray tab switch puts your actual work in a marketing clip.
#
# Why TD_SESSION: without it a second terminal-delight RESTORES your live panes —
# measured 2026-09-07, a "clean" instance came up wearing 22 real agents.
#
# Placement semantics are the ones the shoot-staged-terminals skill measured on this
# box, and they are not guessable:
#   * drive them with `hyprctl dispatch`, NOT `hyprctl eval` — eval answers ok and
#     does nothing for these
#   * hl.dsp.focus({workspace=N}) pulls that workspace to the FOCUSED monitor, so
#     focus the monitor first
#   * float → resize → move, in that order: resize is exact but RE-CENTRES the
#     float, so moving first is undone
#   * all coordinates are logical and global (monitor origin + offset)
#
# Resolution: gpu-screen-recorder writes PHYSICAL pixels, so a 1560x940 logical
# window on this 1.6x display records at 2496x1504 — film the whole window and
# downscale later, rather than filming small and upscaling never.
set -euo pipefail

SESSION="${TD_DEMO_SESSION:-tdclip}"
MON="${TD_DEMO_MON:-eDP-1}"
WS="${TD_DEMO_WS:-5}"
STATE=/tmp/td-demo-stage.state

dispatch() { hyprctl dispatch "$1" >/dev/null 2>&1 || true; }

win_addr() {
  hyprctl -j clients | jq -r --arg t "terminal-delight — $SESSION" \
    '.[] | select(.title == $t) | .address' | head -1
}
win_pid() {
  hyprctl -j clients | jq -r --arg t "terminal-delight — $SESSION" \
    '.[] | select(.title == $t) | .pid' | head -1
}

case "${1:-up}" in
up)
  # Remember where to put everything back.
  read -r cx cy < <(hyprctl cursorpos | tr -d ',')
  home_ws="$(hyprctl -j monitors | jq -r --arg m "$MON" '.[] | select(.name==$m) | .activeWorkspace.name')"
  printf 'cursor=%s,%s\nhome_ws=%s\n' "$cx" "$cy" "$home_ws" > "$STATE"

  # Monitor geometry, logical: physical width / scale (portrait monitors swap).
  read -r mx my mw mh < <(hyprctl -j monitors | jq -r --arg m "$MON" \
    '.[] | select(.name==$m) | "\(.x) \(.y) \((.width/.scale)|floor) \((.height/.scale)|floor)"')
  w=$(( mw - 40 )); h=$(( mh - 60 )); x=$(( mx + 20 )); y=$(( my + 40 ))

  dispatch "hl.dsp.focus({ monitor = \"$MON\" })"
  dispatch "hl.dsp.focus({ workspace = $WS })"
  sleep 1

  TD_SESSION="$SESSION" TD_DEMO=1 TD_WALL_DEMO=1 TD_DEMO_LOGOS=1 \
    setsid terminal-delight >/tmp/td-demo-$SESSION.log 2>&1 &
  for _ in $(seq 1 40); do [ -n "$(win_addr)" ] && break; sleep 0.5; done
  a="$(win_addr)"; p="$(win_pid)"
  [ -n "$a" ] || { echo "demo-stage: the demo window never appeared" >&2; exit 1; }

  # float → resize → move. Order matters; see the header.
  dispatch "hl.dsp.window.float({ action = \"on\", window = \"address:$a\" })"; sleep 0.3
  dispatch "hl.dsp.window.resize({ x = $w, y = $h, window = \"address:$a\" })"; sleep 0.3
  dispatch "hl.dsp.window.move({ x = $x, y = $y, window = \"address:$a\" })";  sleep 0.5

  # Park the pointer on the other output — grim composites the cursor only on the
  # output it is physically on, and gsr honours -cursor no but the still does not.
  park="$(hyprctl -j monitors | jq -r --arg m "$MON" '.[] | select(.name!=$m) | "\(.x+100) \(.y+100)"' | head -1)"
  [ -n "$park" ] && dispatch "hl.dsp.cursor.move({ x = ${park% *}, y = ${park#* } })"

  # Turn on the socket surface the gestures ride.
  terminal-delight ctl mcp on         --pid "$p" >/dev/null 2>&1 || true
  terminal-delight ctl mcp writes on  --pid "$p" >/dev/null 2>&1 || true
  terminal-delight ctl mcp expose all --pid "$p" >/dev/null 2>&1 || true

  # Evidence dump: what we asked for vs what the compositor did.
  hyprctl -j clients | jq -r --arg t "terminal-delight — $SESSION" \
    '.[] | select(.title==$t) | "staged: \(.at[0]),\(.at[1]) \(.size[0])x\(.size[1]) ws=\(.workspace.name) floating=\(.floating)"'
  echo "TD_PID=$p"
  echo "TD_GEOM=$x,$y ${w}x${h}"
  ;;

down)
  p="$(win_pid)"; [ -n "$p" ] && kill "$p" 2>/dev/null || true
  rm -f "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/terminal-delight/ctl-$p.sock" 2>/dev/null || true
  sleep 1
  if [ -f "$STATE" ]; then
    home_ws="$(sed -n 's/^home_ws=//p' "$STATE")"
    cur="$(sed -n 's/^cursor=//p' "$STATE")"
    dispatch "hl.dsp.focus({ monitor = \"$MON\" })"
    [ -n "$home_ws" ] && dispatch "hl.dsp.focus({ workspace = $home_ws })"
    [ -n "$cur" ] && dispatch "hl.dsp.cursor.move({ x = ${cur%,*}, y = ${cur#*,} })"
    rm -f "$STATE"
  fi
  echo "demo-stage: down"
  ;;

*) echo "demo-stage: up | down" >&2; exit 2 ;;
esac
