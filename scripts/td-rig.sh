#!/usr/bin/env bash
# A real Terminal Delight window on a session of its own, for photographs.
#
# A demo window hides the left bar (#421), so anything behind the bar — the
# tree, the rail, the chrome around them — can only be photographed in a REAL
# window. That window must not join the live session, so it gets its own
# TD_SESSION and a layout written by hand. This is that rig, once. It had been
# rewritten in a scratchpad by every agent that needed it, and one copy's kill
# left the session host running with six shells for an hour and a half (#766).
#
#   bash scripts/td-rig.sh seed   <key> [--shut] [--cwd DIR] [--layout FILE]
#   bash scripts/td-rig.sh launch <key> [VAR=value ...]
#   bash scripts/td-rig.sh where  <key>
#   bash scripts/td-rig.sh shoot  <key> <out.png>
#   bash scripts/td-rig.sh kill   <key>
#   bash scripts/td-rig.sh list
#
# `seed` writes the layout: three projects, three groups under one of them, six
# tabs, the tree open unless --shut. Two tabs share the repository root, so the
# engineering badge reads SHARED and the ticker has several frames to turn.
# `launch` starts the window and waits for it to appear; extra VAR=value pairs
# go into its environment — the capture hooks (TD_TRAY_DEMO=1, TD_OSD_DEMO=1,
# TD_RAIL_TABLE=1 …) are how a state that needs a gesture gets photographed.
# `kill` stops the window and the session host, PROVES both are gone along
# with the host's shells, and removes every file the session left behind.
#
# Keys must start with `rig-`. Real sessions are numbered, so a rig can never
# be launched into one, and `kill` can never reach one.
#
# Run it as `bash scripts/td-rig.sh …`: lean-ctx's shell allowlist refuses
# `./scripts/td-rig.sh` as an unknown command word.
#
# The binary is this worktree's release build when there is one, else the
# launcher; TD_BIN overrides. The window lands wherever Hyprland tiles it,
# usually beside whatever is focused, on the person's own screen. Launching one
# is accepted practice here, but it is a live process on their desk and they
# can and do click it. Kill it when you are done.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CFG="${XDG_CONFIG_HOME:-$HOME/.config}/terminal-delight/sessions"
RT="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/terminal-delight"
STATE="${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/surfaces"

die() { echo "td-rig: $*" >&2; exit 2; }

key_ok() { # a rig key, never a real session's
  [[ "${1:-}" =~ ^rig-[a-z0-9][a-z0-9-]*$ ]] ||
    die "the key must look like rig-<name> (lowercase, digits, dashes); got '${1:-}'"
}

binary() {
  if [ -n "${TD_BIN:-}" ]; then echo "$TD_BIN"
  elif [ -x "$ROOT/app/target/release/terminal-delight" ]; then echo "$ROOT/app/target/release/terminal-delight"
  else echo "$HOME/.local/bin/terminal-delight"
  fi
}

win() { # "<x>,<y> <w>x<h> <pid> <monitor-id>" for the key's window, or nothing
  hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $1" \
    '.[] | select(.title==$t) | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1]) \(.pid) \(.monitor)"' | head -1
}

hosts() { # pids of the key's session host — the command line ENDS with the key
  pgrep -f -- "serve --session $1\$" || true
}

alive() { # the subset of the given pids that still exist
  local p
  for p in "$@"; do [ -d "/proc/$p" ] && echo "$p"; done
}

stop() { # stop <pid...>: TERM, wait up to 5s, then KILL what is left
  [ $# -eq 0 ] && return 0
  kill "$@" 2>/dev/null
  local _ left
  for _ in $(seq 1 50); do
    [ -z "$(alive "$@")" ] && return 0
    sleep 0.1
  done
  mapfile -t left < <(alive "$@")
  [ "${#left[@]}" -gt 0 ] && kill -9 "${left[@]}" 2>/dev/null
  sleep 0.5
}

seed() {
  local key=$1; shift
  local tree=true cwd="$ROOT" layout=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --shut) tree=false ;;
      --cwd) cwd=${2:?--cwd needs a directory}; shift ;;
      --layout) layout=${2:?--layout needs a file}; shift ;;
      *) die "seed: unknown option $1" ;;
    esac
    shift
  done
  [ -n "$(win "$key")$(hosts "$key")" ] && die "$key is running — kill it before re-seeding"
  mkdir -p "$CFG"
  if [ -n "$layout" ]; then
    cp "$layout" "$CFG/$key.toml"
  else
    cat > "$CFG/$key.toml" <<EOF
active = 3
lang = "en"
left_bar = $tree
left_bar_w = 220.0
scale = 0.85
warp = 0.0

[[groups]]
collapsed = false
color = "#40bfa8"
id = 11
name = "WORKBENCH"
project = 12

[[groups]]
collapsed = false
color = "#40bfa8"
id = 12
name = "GUI-TUI"
project = 12

[[groups]]
collapsed = false
color = "#40bfa8"
id = 13
name = "LITTLE STUFF"
project = 12

[[projects]]
collapsed = false
color = "#cc7e5c"
id = 7
name = "GLOBAL"

[[projects]]
collapsed = false
color = "#5c70cc"
id = 9
name = "JEV"

[[projects]]
collapsed = false
color = "#5cc1cc"
id = 12
name = "TERMINAL DELIGHT"

[[tabs]]
project = 7
name = "home"

[tabs.node.Leaf]
cwd = "$HOME"

[[tabs]]
project = 9
name = "ideas"

[tabs.node.Leaf]
cwd = "$HOME"

[[tabs]]
group = 11
name = "model version"

[tabs.node.Leaf]
cwd = "$cwd"

[[tabs]]
group = 12
name = "Research"

[tabs.node.Leaf]
cwd = "$cwd"

[[tabs]]
group = 13
name = "UX"

[tabs.node.Leaf]
cwd = "$HOME"

[[tabs]]
group = 13
name = "little stuff"

[tabs.node.Leaf]
cwd = "$HOME"
EOF
  fi
  echo "seeded $CFG/$key.toml (tree $([ "$tree" = true ] && echo open || echo shut))"
}

launch() {
  local key=$1; shift
  [ -f "$CFG/$key.toml" ] || die "no layout for $key — run: bash scripts/td-rig.sh seed $key"
  [ -n "$(win "$key")" ] && die "$key already has a window: $(win "$key")"
  local bin
  bin=$(binary)
  [ -x "$bin" ] || die "no binary at $bin — cargo build --release, or set TD_BIN"
  local log="${TMPDIR:-/tmp}/td-rig-$key.log"
  setsid env TD_SESSION="$key" "$@" "$bin" > "$log" 2>&1 < /dev/null &
  echo "launched $bin on session $key (log $log)"
  local _
  for _ in $(seq 1 60); do
    if [ -n "$(win "$key")" ]; then
      echo "window: $(win "$key")"
      return 0
    fi
    sleep 0.5
  done
  echo "td-rig: no window appeared in 30s — see $log" >&2
  return 1
}

shoot() {
  local key=$1 out=${2:?shoot needs an output .png}
  local w
  w=$(win "$key")
  [ -n "$w" ] || die "$key has no window"
  local at size mon
  read -r at size _ mon <<< "$w"
  # A window partly off its monitor grabs as garbage — grim reads the output,
  # not the surface — so refuse rather than hand back a smear.
  local inside
  inside=$(hyprctl monitors -j | jq -r --argjson m "$mon" --arg at "$at" --arg size "$size" '
    .[] | select(.id==$m) |
    ($at | split(",") | map(tonumber)) as [$x, $y] |
    ($size | split("x") | map(tonumber)) as [$w, $h] |
    (.width / .scale) as $mw | (.height / .scale) as $mh |
    ($x >= .x and $y >= .y and ($x + $w) <= (.x + $mw) and ($y + $h) <= (.y + $mh))')
  [ "$inside" = "true" ] || die "$key's window ($at $size) is not wholly on its monitor — cannot photograph it"
  grim -g "$at $size" "$out" || die "grim failed"
  echo "shot $out ($at $size)"
}

kill_rig() {
  local key=$1
  local w wpid=""
  w=$(win "$key")
  [ -n "$w" ] && wpid=$(echo "$w" | awk '{print $3}')
  local hs kids=() h
  mapfile -t hs < <(hosts "$key")
  for h in "${hs[@]}"; do
    [ -n "$h" ] && mapfile -t -O "${#kids[@]}" kids < <(pgrep -P "$h" || true)
  done
  [ -n "$wpid" ] && stop "$wpid"
  [ "${#hs[@]}" -gt 0 ] && [ -n "${hs[0]}" ] && stop "${hs[@]}"
  # The host's shells lose their terminal when it goes; give them a moment.
  if [ "${#kids[@]}" -gt 0 ]; then
    sleep 0.5
    local stragglers
    mapfile -t stragglers < <(alive "${kids[@]}")
    stop "${stragglers[@]}"
  fi

  local left
  left=$( { [ -n "$wpid" ] && alive "$wpid"; hosts "$key"; alive "${kids[@]}"; } 2>/dev/null | sort -u | tr '\n' ' ')
  rm -f "$RT/session-$key".{sock,window,tag,log} \
    "$CFG/$key".{toml,host.lock,hostspawn.lock} \
    "${TMPDIR:-/tmp}/td-rig-$key.log"
  [ -n "$wpid" ] && rm -f "$RT/ctl-$wpid.sock"
  rm -rf "${STATE:?}/$key"
  if [ -n "${left// /}" ]; then
    echo "td-rig: STILL RUNNING after kill: $left" >&2
    return 1
  fi
  echo "killed $key: window ${wpid:-none}, host ${hs[*]:-none}, ${#kids[@]} shell(s); files removed"
}

list() { # every rig session with anything left of it
  local keys
  keys=$( { ls "$CFG" 2>/dev/null; ls "$RT" 2>/dev/null; } |
    grep -oE 'rig-[a-z0-9-]+' | sed -E 's/\.(toml|host|hostspawn)$//' | sort -u)
  if [ -z "$keys" ]; then echo "no rig sessions"; return 0; fi
  local k
  for k in $keys; do
    printf '%-24s window:%-28s host:%s\n' "$k" "$(win "$k" | awk '{print $3}')" "$(hosts "$k" | tr '\n' ' ')"
  done
}

cmd=${1:-}
case "$cmd" in
  seed|launch|where|shoot|kill) key_ok "${2:-}" ;;
esac
case "$cmd" in
  seed) shift; seed "$@" ;;
  launch) shift; launch "$@" ;;
  where)
    w=$(win "$2")
    if [ -n "$w" ]; then echo "$w"; else echo "$2 has no window"; exit 1; fi
    ;;
  shoot) shoot "$2" "${3:-}" ;;
  kill) kill_rig "$2" ;;
  list) list ;;
  *) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 2 ;;
esac
