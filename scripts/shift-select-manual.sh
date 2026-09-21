#!/usr/bin/env bash
# Press shift+arrow at a REAL composer and photograph the highlight.
#
# Runs on a HEADLESS OUTPUT it creates and destroys, so it never touches the
# operator's monitors and does not care whether they are asleep — `grim`
# hangs to its timeout against a dark screen, which is what made the obvious
# version of this unrunnable half the time.
#
# The composer only draws on an AGENT pane (`shows.composer = is_agent`) and
# the smoke rig stages shells, so this puts a script NAMED `claude` on PATH:
# the host decides a pane is an agent from the process name.
#
# Safety, in the order it matters:
#   * its own headless output, created and removed here
#   * its own TD_SESSION, so nothing can reach the operator's live panes
#   * every keystroke gated on `activewindow` being the window we launched,
#     because wtype types into whatever has focus
#
# Usage:  scripts/shift-select-manual.sh [path-to-terminal-delight]
set -euo pipefail

BIN="${1:-$(dirname "$0")/../app/target/release/terminal-delight}"
[ -x "$BIN" ] || { echo "no binary at $BIN" >&2; exit 2; }
BIN="$(readlink -f "$BIN")"
OUT="${OUT:-/tmp/claude-1000/shiftsel-shots}"
RIG="$(mktemp -d)"
MON=""
WPID=""
mkdir -p "$OUT"
rm -f "$OUT"/*.png

cleanup() {
  [ -n "$WPID" ] && kill "$WPID" 2>/dev/null || true
  sleep 1
  [ -n "$MON" ] && hyprctl output remove "$MON" >/dev/null 2>&1 || true
  rm -rf "$RIG"
}
trap cleanup EXIT

# ── a screen of our own ─────────────────────────────────────────────────────
before="$(hyprctl monitors -j | jq -r '.[].name' | sort)"
hyprctl output create headless >/dev/null
sleep 2
MON="$(comm -13 <(echo "$before") <(hyprctl monitors -j | jq -r '.[].name' | sort) | head -1)"
[ -n "$MON" ] || { echo "no headless output appeared" >&2; exit 3; }
read -r MW MH < <(hyprctl monitors -j | jq -r --arg m "$MON" '.[] | select(.name==$m) | "\(.width) \(.height)"')
echo "staging on $MON (${MW}x${MH})"

WS=91
hyprctl dispatch moveworkspacetomonitor "$WS $MON" >/dev/null 2>&1 || true
hyprctl keyword workspace "$WS,monitor:$MON" >/dev/null 2>&1 || true
hyprctl dispatch focusmonitor "$MON" >/dev/null
hyprctl dispatch workspace "$WS" >/dev/null
sleep 1

# ── the stand-in agent ──────────────────────────────────────────────────────
cat > "$RIG/claude" <<'AGENT'
#!/usr/bin/env bash
printf '\033[?1049h'          # alternate screen — what holds the pane at CLAUDE
printf 'stand-in agent. the composer below is the thing under test.\r\n'
while IFS= read -r _line; do printf 'heard it\r\n'; done
sleep 100000
AGENT
chmod +x "$RIG/claude"

export PATH="$RIG:$PATH"
export TD_SESSION="shiftsel-$$"
export TD_NO_SESSIOND=1        # never adopt or spawn the operator's host

setsid "$BIN" >"$RIG/window.log" 2>&1 &
WPID=$!
sleep 7

addr="$(hyprctl clients -j | jq -r --arg p "$WPID" '.[] | select(.pid==($p|tonumber)) | .address' | head -1)"
if [ -z "$addr" ]; then
  echo "the window never appeared; last of its log:" >&2
  tail -25 "$RIG/window.log" >&2
  exit 4
fi
hyprctl dispatch movetoworkspacesilent "$WS,address:$addr" >/dev/null 2>&1 || true
hyprctl dispatch focuswindow "address:$addr" >/dev/null
sleep 1

ours() { [ "$(hyprctl activewindow -j | jq -r '.address // empty')" = "$addr" ]; }
press() {
  ours || { echo "focus left our window — refusing to type" >&2; exit 5; }
  wtype "$@"
  sleep 0.25
}
shot() { grim -o "$MON" "$OUT/$1.png"; echo "  -> $OUT/$1.png"; }

ours || { echo "could not focus our own window" >&2; exit 6; }

echo "0. a pane running the stand-in agent"
shot 0-pane
echo "1. alt+k to the workbench, then type a draft"
press -M alt -k k -m alt
sleep 1
press -- "alpha beta gamma"
sleep 1
shot 1-draft
echo "2. shift+Left x4 — expect 'amma' highlighted"
for _ in 1 2 3 4; do press -M shift -k Left -m shift; done
shot 2-shift-left-4
echo "3. shift+Left x2 more — the anchor holds, so six are selected"
for _ in 1 2; do press -M shift -k Left -m shift; done
shot 3-shift-left-6
echo "4. plain Left — collapses to the START, highlight gone"
press -k Left
shot 4-collapsed
echo "5. ctrl+a — the whole draft"
press -M ctrl -k a -m ctrl
shot 5-select-all
echo "6. type over it — one character replaces everything"
press -- "X"
shot 6-typed-over

echo
echo "shots in $OUT"
echo "a passing run: 2 highlights four characters, 3 highlights six,"
echo "4 shows none, 5 highlights everything, 6 shows a lone X."
