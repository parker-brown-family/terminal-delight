#!/usr/bin/env bash
# Start a built Terminal Delight on a private Xvfb display and check what a
# person would check first: it says its version, a window appears, typing
# reaches the shell in it, and typing `exit` into that shell (the last one)
# quits the window with status 0.
#
#   scripts/appimage-smoke.sh <terminal-delight-x86_64.AppImage> [out-dir]
#   scripts/appimage-smoke.sh --bin <terminal-delight binary> [out-dir]
#
# Everything it starts runs under a scratch HOME and XDG folders in out-dir,
# on display $SMOKE_DISPLAY (default :99), and is stopped before it exits.
# Vulkan comes from $SMOKE_VK_ICD, else Mesa's lavapipe if installed. It never
# touches a real desktop: it refuses to run with WAYLAND_DISPLAY or
# HYPRLAND_INSTANCE_SIGNATURE set unless SMOKE_DISPLAY is given explicitly.
set -uo pipefail

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
mode=appimage
if [[ ${1:-} == --bin ]]; then
  mode=bin
  shift
fi
target=$(realpath "${1:?usage: appimage-smoke.sh [--bin] <AppImage|binary> [out-dir]}")
out=$(realpath -m "${2:-$(mktemp -d)}")
mkdir -p "$out"
disp=${SMOKE_DISPLAY:-:99}
if [[ -z ${SMOKE_DISPLAY:-} && ( -n ${WAYLAND_DISPLAY:-} || -n ${HYPRLAND_INSTANCE_SIGNATURE:-} ) ]]; then
  echo "appimage-smoke: a desktop session is running here; set SMOKE_DISPLAY to a free display" >&2
  exit 2
fi
want=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/app/Cargo.toml" | head -1)

fails=0
ok() { echo "  ok   $*"; }
bad() { echo "  FAIL $*"; fails=$((fails + 1)); }

xvfb=""
td=""
mine() { # every process started with this run's config folder
  for p in /proc/[0-9]*; do
    { tr '\0' '\n' <"$p/environ"; } 2>/dev/null | grep -qx "XDG_CONFIG_HOME=$out/config" && echo "${p#/proc/}"
  done
}
cleanup() {
  [[ -n $td ]] && kill "$td" 2>/dev/null
  local left
  left=$(mine)
  # shellcheck disable=SC2086 # one pid per word
  [[ -n $left ]] && kill $left 2>/dev/null
  sleep 1
  left=$(mine)
  # shellcheck disable=SC2086 # one pid per word
  [[ -n $left ]] && kill -9 $left 2>/dev/null
  [[ -n $xvfb ]] && kill "$xvfb" 2>/dev/null
}
trap cleanup EXIT

shot() { # a picture of the display, for the run's artifacts
  if command -v import >/dev/null; then
    DISPLAY=$disp import -window root "$out/$1.png" 2>/dev/null
  fi
}

# 1. The binary, out of the AppImage without FUSE.
if [[ $mode == appimage ]]; then
  (cd "$out" && "$target" --appimage-extract >/dev/null) || { echo "appimage-smoke: could not extract $target"; exit 1; }
  run=$out/squashfs-root/AppRun
else
  run=$target
fi

# 2. --version names this version and its terminal core.
version=$("$run" --version 2>&1)
if [[ $version == "terminal-delight $want (terminal core: "* ]]; then
  ok "--version: $version"
else
  bad "--version said \"$version\"; Cargo.toml says $want"
fi

# 3. A private display and private homes.
if [[ ! -e /tmp/.X11-unix/X${disp#:} ]]; then
  Xvfb "$disp" -screen 0 1280x800x24 -nolisten tcp >"$out/xvfb.log" 2>&1 &
  xvfb=$!
  for _ in $(seq 50); do [[ -e /tmp/.X11-unix/X${disp#:} ]] && break; sleep 0.1; done
fi
mkdir -p "$out"/{config,state,data,cache,tmp,home,rt}
chmod 700 "$out/rt"
icd=${SMOKE_VK_ICD:-}
if [[ -z $icd ]]; then
  icd=$(find /usr/share/vulkan/icd.d -name 'lvp_icd*.json' 2>/dev/null | head -1)
fi
envs=(env -i HOME="$out/home" USER="${USER:-runner}" LOGNAME="${USER:-runner}" PATH=/usr/bin:/bin
  SHELL=/bin/bash TERM=xterm-256color LANG=C.UTF-8 DISPLAY="$disp"
  XDG_CONFIG_HOME="$out/config" XDG_STATE_HOME="$out/state" XDG_DATA_HOME="$out/data"
  XDG_CACHE_HOME="$out/cache" XDG_RUNTIME_DIR="$out/rt" TMPDIR="$out/tmp" TD_SESSION=smoke)
[[ -n $icd ]] && envs+=(VK_ICD_FILENAMES="$icd" VK_DRIVER_FILES="$icd")

# 4. It starts, and a window appears.
(cd "$out/home" && exec "${envs[@]}" "$run" >"$out/td.log" 2>&1 </dev/null) &
td=$!
win=""
for _ in $(seq 120); do
  win=$(DISPLAY=$disp xdotool search --onlyvisible --name . 2>/dev/null | head -1)
  [[ -n $win ]] && break
  kill -0 "$td" 2>/dev/null || break
  sleep 1
done
if [[ -n $win ]]; then
  ok "a window appeared ($(DISPLAY=$disp xdotool getwindowname "$win" 2>/dev/null))"
else
  bad "no window within 120 s (the process is $(kill -0 "$td" 2>/dev/null && echo running || echo gone))"
  tail -20 "$out/td.log"
  shot no-window
  exit 1
fi
sleep 3

# 5. Typing reaches the shell: a typed command writes a file. A new pane may
# refuse keys for its first moments, so the line is cleared and typed again
# until the file appears or 40 s pass.
token=$RANDOM$RANDOM
marker=$out/typed.txt
DISPLAY=$disp xdotool windowfocus --sync "$win" 2>/dev/null
for _ in $(seq 10); do
  DISPLAY=$disp xdotool key --window "$win" ctrl+u 2>/dev/null
  DISPLAY=$disp xdotool type --window "$win" --delay 25 "echo typed-$token > $marker" 2>/dev/null
  DISPLAY=$disp xdotool key --window "$win" Return 2>/dev/null
  for _ in $(seq 8); do [[ -s $marker ]] && break 2; sleep 0.5; done
done
if [[ $(cat "$marker" 2>/dev/null) == "typed-$token" ]]; then
  ok "typing reached the shell (it wrote typed-$token)"
else
  bad "a typed command never ran (marker: $(cat "$marker" 2>/dev/null || echo absent))"
  shot not-typed
fi
shot typed

# 6. `exit` in the last shell quits the window, with status 0.
DISPLAY=$disp xdotool key --window "$win" ctrl+u 2>/dev/null
DISPLAY=$disp xdotool type --window "$win" --delay 25 "exit" 2>/dev/null
DISPLAY=$disp xdotool key --window "$win" Return 2>/dev/null
for _ in $(seq 60); do kill -0 "$td" 2>/dev/null || break; sleep 0.5; done
if kill -0 "$td" 2>/dev/null; then
  bad "the window was still running 30 s after exit was typed"
  shot no-quit
else
  wait "$td"
  code=$?
  td=""
  if [[ $code == 0 ]]; then ok "exit quit the window (status 0)"; else bad "the window quit with status $code"; fi
fi
grep -iE "panicked|segfault|fatal" "$out/td.log" && bad "the window's log reports a crash"

# What it left behind. The session host may outlive its window by design;
# it is named here and stopped by the cleanup.
left=$(mine | tr '\n' ' ')
# shellcheck disable=SC2086 # one pid per word
[[ -n ${left// } ]] && echo "  note still running after the window quit: $(for p in $left; do printf '%s ' "$(cat "/proc/$p/comm" 2>/dev/null)"; done)"

if ((fails)); then
  echo "appimage-smoke: $fails check(s) failed; log and pictures in $out"
  exit 1
fi
echo "appimage-smoke: passed"
