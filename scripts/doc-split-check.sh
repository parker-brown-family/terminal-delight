#!/usr/bin/env bash
# Open documents beside a pane without a pointer, in a window nobody can see,
# and check what the split did.
#
# Ctrl+Alt+click on a path opens it in a pane to the right of the one clicked.
# The gesture belongs to a hand, and TD has no gpui harness, so the unit tests
# can only read the code that decides it. This drives the same request through
# the control socket (`ctl doc beside`, which asks the pane exactly as the click
# does) against a real window and a real session host, and reads back what the
# window said, what it drew and what it wrote into the layout file:
#
#   1. a.png beside the shell        -> a split, and the window draws a.png
#   2. a.png again                   -> the pane already showing it is focused;
#                                       a.png is not drawn a second time
#   3. b.png, c.md                   -> two more splits, one a Markdown page:
#                                       four panes
#   4. d.png                         -> the tab is full, so it floats instead
#                                       and says why; d.png is drawn in the square
#   5. the layout file               -> four leaves
#
#   scripts/doc-split-check.sh [--bin PATH] [--out DIR]
#
# THE WINDOW IS HIDDEN, as in doc-float-soak.sh: launched onto a special
# workspace with `render_unfocused` and `no_initial_focus`, and killed at once,
# exit 4, if it lands anywhere else. Its session is its own (`docsplit<pid>`),
# and the window, its host and that session's files go when the script exits.
#
# Exit 0: every step answered and drew as expected. 1: a step did not.
# 2: bad arguments or no binary. 4: the window was not hidden; nothing ran.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
TD="$ROOT/app/target/release/terminal-delight"
OUT="/tmp/doc-split-check-$(date +%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
mkdir -p "$OUT"
SESSION="docsplit$$"
WS="special:tdsplit$$"
LAYOUT="$HOME/.config/terminal-delight/sessions/$SESSION.toml"

# Three small, distinct pictures and a short page: the check is about where
# they go, not how large they are.
python3 - "$OUT" <<'EOF'
import struct, zlib, sys
def png(path, rgb):
    W = H = 64
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    raw = b"".join(b"\0" + bytes(rgb) * W for _ in range(H))
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))
for name, rgb in [("a", (220, 40, 40)), ("b", (40, 200, 60)), ("d", (230, 200, 30))]:
    png(f"{sys.argv[1]}/{name}.png", rgb)
open(f"{sys.argv[1]}/c.md", "w").write("# Beside the prompt\n\nA page read in a pane of its own.\n")
EOF

WIN=""
cleanup() {
  local pids
  pids="$WIN $(pgrep -f "serve --session $SESSION" 2>/dev/null)"
  for p in $pids; do kill "$p" 2>/dev/null; done
  for _ in $(seq 1 50); do
    alive=0
    for p in $pids; do kill -0 "$p" 2>/dev/null && alive=1; done
    [ "$alive" -eq 0 ] && break
    sleep 0.1
  done
  rm -f "$HOME/.config/terminal-delight/sessions/$SESSION".*
}
trap cleanup EXIT

launch() { # launch <log>
  local said
  said=$(hyprctl dispatch "hl.dsp.exec_cmd(\"sh -c 'TD_SESSION=$SESSION TD_DOCDEBUG=1 exec $TD > $1 2>&1'\", { workspace = \"$WS silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
  case "$said" in ok|"") ;; *) echo "hyprctl refused the launch: $said"; exit 1 ;; esac
  WIN=""
  for _ in $(seq 1 60); do
    WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $SESSION" \
      '.[] | select(.title==$t) | .pid' | head -1)
    [ -n "$WIN" ] && break
    sleep 0.5
  done
  [ -n "$WIN" ] || { echo "no window appeared — see $1"; exit 1; }
  local landed
  landed=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
  if [ "$landed" != "$WS" ]; then
    echo "the window landed on '$landed', not $WS — killed, nothing ran"
    kill "$WIN" 2>/dev/null
    WIN=""
    exit 4
  fi
  echo "   window $WIN, hidden on $landed"
  for _ in $(seq 1 40); do [ "$(ctl ping)" = "pong" ] && break; sleep 0.5; done
}

# The client prefixes each reply with the answering window's pid and a tab.
ctl() { "$TD" ctl --pid "$WIN" "$@" 2>&1 | head -1 | sed 's/^[0-9]*\t//'; }
drew() { grep -c "\[doc\] drew $OUT/$1 " "$LOG" 2>/dev/null || true; }
wait_drew() { # wait_drew <file> <n>
  for _ in $(seq 1 50); do [ "$(drew "$1")" -ge "$2" ] && return 0; sleep 0.1; done
  return 1
}
fail=0
expect() { # expect <reply prefix> <ctl args…>
  local want=$1 r
  shift
  r=$(ctl "$@")
  case "$r" in
    "$want"*) echo "   ok   $* -> $r" ;;
    *) echo "   FAIL $* -> $r (wanted: $want…)"; fail=1 ;;
  esac
}
check() { # check <what> <command…>
  local what=$1
  shift
  if "$@"; then echo "   ok   $what"; else echo "   FAIL $what"; fail=1; fi
}

echo "== launching a hidden window on $WS (session $SESSION)"
LOG="$OUT/window.log"
launch "$LOG"
sleep 2

echo "== the split, driven through the socket"
expect "ok beside pane" doc beside "$OUT/a.png"
check "a.png was drawn on its Document face" wait_drew a.png 1
expect "ok focused pane" doc beside "$OUT/a.png"
sleep 0.5
check "a.png was not opened a second time" test "$(drew a.png)" -eq 1
expect "ok beside pane" doc beside "$OUT/b.png"
expect "ok beside pane" doc beside "$OUT/c.md"
check "b.png was drawn" wait_drew b.png 1
check "c.md, a Markdown page, was drawn" wait_drew c.md 1
expect "ok float pane" doc beside "$OUT/d.png"
check "d.png was drawn in the square instead" wait_drew d.png 1
sleep 1
cp "$LAYOUT" "$OUT/layout.toml" 2>/dev/null
LEAVES=$(grep -o 'Leaf' "$OUT/layout.toml" 2>/dev/null | wc -l)
check "the layout holds four leaves (it holds $LEAVES)" test "$LEAVES" -eq 4

echo "== window log: $LOG; layout copy: $OUT/layout.toml"
[ "$fail" -eq 0 ] && echo "== every step did what it should" || echo "== a step did not"
exit "$fail"
