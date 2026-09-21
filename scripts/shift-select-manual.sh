#!/usr/bin/env bash
# Press shift+arrow at a REAL composer and photograph the highlight.
#
# The composer only draws on an AGENT pane (`shows.composer = is_agent`) and
# the smoke rig stages shells, so this puts a script NAMED `claude` on PATH:
# the host decides a pane is an agent from the process name.
#
# WHY IT USES THE REAL SCREEN. A headless output would be tidier and was
# tried first: `hyprctl output create headless` works, but nothing can be
# moved onto it, because this Hyprland takes LUA dispatchers and the config
# only ever defines `hl.dsp.exec`, `hl.dsp.focus` and `hl.dsp.window` — there
# is no working `focusmonitor` or `moveworkspacetomonitor` to reach for.
# So the window lands on whatever monitor is current, for about thirty
# seconds, and focus is handed back at the end.
#
# Safety, in the order it matters:
#   * refuses to run against a sleeping output — grim hangs to its timeout
#   * its own TD_SESSION, and cleanup kills the session HOST as well as the
#     window: the host is parented to systemd, so killing the window alone
#     orphans it and everything inside it
#   * the stand-in is launched by ABSOLUTE PATH and then verified, because
#     PATH is not ours to control — see below
#   * EVERY keystroke gated on `activewindow` being the window we launched,
#     because wtype types into whatever has focus, and that guard has already
#     caught focus drifting onto a real agent's prompt once
#
# THE PATH TRAP, paid for once. The pane's shell inherits our PATH, and then
# its interactive rc REBUILDS PATH and appends the inherited part at the END.
# So `claude` resolved to ~/.local/bin/claude — the real one — and the rig
# started an actual Claude Code session with six MCP servers under it, left
# the host orphaned when it exited, and still produced correct screenshots,
# because a real agent draws a composer too. It passed by luck. Hence both
# the absolute path and the assertion that the child is the one we meant.
#
# Usage:  scripts/shift-select-manual.sh [path-to-terminal-delight]
set -euo pipefail

BIN="${1:-$(dirname "$0")/../target/release/terminal-delight}"
[ -x "$BIN" ] || { echo "no binary at $BIN" >&2; exit 2; }
BIN="$(readlink -f "$BIN")"
OUT="${OUT:-/tmp/claude-1000/shiftsel-shots}"
RIG="$(mktemp -d)"
WPID=""
WAS=""
mkdir -p "$OUT"
rm -f "$OUT"/*.png

dispatch() { hyprctl dispatch "$1" >/dev/null; }
focus_addr() { dispatch "hl.dsp.focus({ window = 'address:$1' })"; }

cleanup() {
  [ -n "$WPID" ] && kill "$WPID" 2>/dev/null || true
  sleep 1
  # THE HOST TOO. It is parented to systemd, not to the window, so killing
  # the window alone leaves it running with every pane still inside it.
  # Matched on our own TD_SESSION, which no other host can carry.
  for h in $(pgrep -u "$(id -u)" -f "serve --session ${TD_SESSION:-nope}" 2>/dev/null); do
    [ "$h" = "$$" ] && continue      # pgrep -f matches this very shell
    kill "$h" 2>/dev/null || true
  done
  sleep 1
  # hand the screen back to whatever had it
  [ -n "$WAS" ] && focus_addr "$WAS" 2>/dev/null || true
  rm -rf "$RIG"
}
trap cleanup EXIT

if ! hyprctl monitors -j | jq -e 'any(.[]; .dpmsStatus == true)' >/dev/null; then
  echo "every output reports dpmsStatus=false — grim would hang. Not staging." >&2
  exit 3
fi
WAS="$(hyprctl activewindow -j | jq -r '.address // empty')"

# ── the stand-in agent ──────────────────────────────────────────────────────
#
# It has to be a real EXECUTABLE named `claude`, not a shebang script called
# that: the host classifies a pane by the foreground process's `comm`
# (`host.rs` — `if comm == "claude" || cmdline.contains("/claude")`), and a
# `#!/usr/bin/env bash` script reports `bash`. So this is a copy of bash
# wearing the name, handed the body separately.
cp "$(command -v bash)" "$RIG/claude"
cat > "$RIG/agent.sh" <<'AGENT'
printf '\033[?1049h'          # alternate screen — what holds the pane at CLAUDE
printf 'stand-in agent. the composer below is the thing under test.\r\n'
while IFS= read -r _line; do printf 'heard it\r\n'; done
sleep 100000
AGENT

export PATH="$RIG:$PATH"
export TD_SESSION="shiftsel-$$"

setsid "$BIN" >"$RIG/window.log" 2>&1 &
WPID=$!
sleep 8

addr="$(hyprctl clients -j | jq -r --arg p "$WPID" '.[] | select(.pid==($p|tonumber)) | .address' | head -1)"
if [ -z "$addr" ]; then
  echo "the window never appeared; last of its log:" >&2
  tail -25 "$RIG/window.log" >&2
  exit 4
fi
read -r X Y W H < <(hyprctl clients -j | jq -r --arg a "$addr" \
  '.[] | select(.address==$a) | "\(.at[0]) \(.at[1]) \(.size[0]) \(.size[1])"')
echo "window $addr at ${W}x${H}+${X}+${Y}"

focus_addr "$addr"
sleep 1

ours() { [ "$(hyprctl activewindow -j | jq -r '.address // empty')" = "$addr" ]; }
press() {
  ours || { echo "focus left our window — refusing to type" >&2; exit 5; }
  wtype "$@"
  sleep 0.3
}
shot() { grim -g "${X},${Y} ${W}x${H}" "$OUT/$1.png"; echo "  -> $OUT/$1.png"; }

ours || { echo "could not focus our own window" >&2; exit 6; }

echo "0. a fresh shell pane"
shot 0-pane
echo "1. run the stand-in so the pane classifies as an agent"
# ABSOLUTE PATH. `claude` alone resolves through the pane shell's own rebuilt
# PATH, which puts ~/.local/bin ahead of ours — see the header.
press -- "$RIG/claude $RIG/agent.sh"
press -k Return
sleep 4
shot 1-agent-running

# …and prove it. A real Claude Code session also draws a composer, so without
# this the run passes on the wrong binary and nothing says so.
imposter=""
for p in $(pgrep -u "$(id -u)" -f "agent.sh" 2>/dev/null); do
  [ "$p" = "$$" ] && continue
  line="$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null)"
  case "$line" in
    "$RIG/claude"*) found=1 ;;
    *claude*agent.sh*) imposter="$p: $line" ;;
  esac
done
if [ -n "$imposter" ]; then
  echo "a REAL claude is running the stand-in script, not ours:" >&2
  echo "  $imposter" >&2
  echo "killing it now; the rig's PATH lost to the pane shell's own." >&2
  kill "${imposter%%:*}" 2>/dev/null || true
  exit 9
fi
[ -n "${found:-}" ] || { echo "the stand-in never started; see 1-agent-running.png" >&2; exit 10; }
echo "2. alt+k to the workbench, then type a draft"
press -M alt -k k -m alt
sleep 2
shot 2-bench
press -- "alpha beta gamma"
sleep 1
shot 3-draft
# THE GUARD THIS RIG EXISTS FOR. A first run photographed six identical
# frames of a SHELL pane with no composer on it and reported success,
# because nothing asserted the keystrokes had landed anywhere.
if cmp -s "$OUT/2-bench.png" "$OUT/3-draft.png"; then
  echo "the draft changed nothing on screen — there is no composer here." >&2
  echo "the pane probably never classified as an agent; check 1-agent-running.png" >&2
  exit 7
fi
echo "4. shift+Left x4 — expect 'amma' highlighted"
for _ in 1 2 3 4; do press -M shift -k Left -m shift; done
shot 4-shift-left-4
echo "5. shift+Left x2 more — the anchor holds, so six are selected"
for _ in 1 2; do press -M shift -k Left -m shift; done
shot 5-shift-left-6
if cmp -s "$OUT/4-shift-left-4.png" "$OUT/5-shift-left-6.png"; then
  echo "the selection did not grow between four and six presses." >&2
  exit 8
fi
echo "6. plain Left — collapses to the START, highlight gone"
press -k Left
shot 6-collapsed
echo "7. ctrl+a — the whole draft"
press -M ctrl -k a -m ctrl
shot 7-select-all
echo "8. type over it — one character replaces everything"
press -- "X"
shot 8-typed-over

echo
echo "shots in $OUT"
echo "a passing run: 4 highlights four characters, 5 highlights six,"
echo "6 shows none, 7 highlights everything, 8 shows a lone X."
