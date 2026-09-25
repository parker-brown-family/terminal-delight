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
#   5. the layout file               -> four leaves, three of them documents,
#                                       the page's place written beside it
#
# Then the layout is put to work. The window is closed — its session host and
# the terminals in it keep running — c.md is deleted, and a second window is
# opened on the same session with TD_GUARD_FORCE_MISMATCH=1, which makes the
# divergence guard throw away and rebuild every quiet pane every 30 seconds:
#
#   6. the restart                   -> a.png and b.png are drawn again on their
#                                       panes; c.md's pane comes back and says
#                                       it cannot read the file
#   7. a square over the shell, then a forced replica repair of every pane
#                                    -> no document is opened again and none is
#                                       released: the views crossed to the
#                                       rebuilt panes
#   8. the layout, saved again       -> still four leaves and three documents;
#                                       c.md is kept by path, with no place,
#                                       because a page never read was never
#                                       measured
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
. "$ROOT/scripts/lib/hidden-window.sh"
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

# The window, its host and this session's files go when the script exits
# (scripts/lib/hidden-window.sh).
trap hidden_cleanup EXIT

launch() { # launch <log> [extra environment, as NAME=value words]
  hidden_launch "$SESSION" "$WS" "$1" "sh -c 'TD_SESSION=$SESSION TD_DOCDEBUG=1 ${2:-} exec $TD > $1 2>&1'"
  echo "   window $WIN, hidden on $WS"
}

said() { grep -c "\[doc\] $1 $OUT/$2" "$LOG" 2>/dev/null || true; } # said <verb> <file>
drew() { said drew "$1 "; }
wait_drew() { # wait_drew <file> <n>
  for _ in $(seq 1 50); do [ "$(drew "$1")" -ge "$2" ] && return 0; sleep 0.1; done
  return 1
}
# The layout file, walked: "leaves=N docs=M" and one "doc <name> <scroll>" line
# per document leaf, `-` for a scroll never measured.
layout() {
  python3 - "$LAYOUT" <<'EOF'
import sys, tomllib, os
try:
    state = tomllib.load(open(sys.argv[1], "rb"))
except (OSError, tomllib.TOMLDecodeError):
    print("leaves=0 docs=0"); sys.exit()
leaves, docs = 0, []
def walk(n):
    global leaves
    if "Leaf" in n:
        leaves += 1
        d = n["Leaf"].get("document") if isinstance(n["Leaf"], dict) else None
        if d:
            docs.append((os.path.basename(d["path"]), d.get("scroll", "-")))
    elif "Split" in n:
        walk(n["Split"]["a"]); walk(n["Split"]["b"])
for t in state.get("tabs", []):
    n = t.get("node")
    if isinstance(n, dict):
        walk(n)
print(f"leaves={leaves} docs={len(docs)}")
for name, scroll in docs:
    print(f"doc {name} {scroll}")
EOF
}
# wait_layout <seconds> <python-ish grep pattern>: poll the layout until a line
# matches, for the 30-second checkpoint that writes a measured place.
wait_layout() {
  for _ in $(seq 1 "$1"); do layout | grep -q "$2" && return 0; sleep 1; done
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
check "the layout holds four leaves, three of them documents ($(layout | head -1))" \
  wait_layout 5 "^leaves=4 docs=3$"
# The split saved at once, before the page was laid out, so its place was
# unmeasured then; the checkpoint that follows writes where it is.
check "c.md's place is written once it has been measured" wait_layout 40 "^doc c.md 0"
cp "$LAYOUT" "$OUT/layout-before.toml" 2>/dev/null
layout | sed 's/^/        /'

echo "== the restart: window closed, host kept, c.md deleted"
hidden_close
rm -f "$OUT/c.md"
LOG="$OUT/window-2.log"
launch "$LOG" "TD_GUARD_FORCE_MISMATCH=1"
check "a.png came back on its pane" wait_drew a.png 1
check "b.png came back on its pane" wait_drew b.png 1
cannot() { [ "$(said "cannot read" c.md)" -ge 1 ]; }
wait_cannot() { for _ in $(seq 1 50); do cannot && return 0; sleep 0.1; done; return 1; }
check "c.md's pane came back and says it cannot read the file" wait_cannot
expect "ok pane" doc here "$OUT/d.png"
check "d.png floats over the shell" wait_drew d.png 1

echo "== a forced replica repair of every pane (the guard runs every 30 s)"
repaired() { grep -c "is being taken again anyway" "$LOG" 2>/dev/null || true; }
wait_repair() { for _ in $(seq 1 75); do [ "$(repaired)" -ge 4 ] && return 0; sleep 1; done; return 1; }
check "all four panes were taken again" wait_repair
sleep 2
for f in a.png b.png d.png; do
  check "$f was not opened again (drawn $(drew $f) time)" test "$(drew $f)" -eq 1
  check "$f was not released" test "$(said released $f)" -eq 0
done
BEFORE=$(stat -c %Y "$LAYOUT" 2>/dev/null || echo 0)
saved_again() { [ "$(stat -c %Y "$LAYOUT" 2>/dev/null || echo 0)" -gt "$BEFORE" ]; }
wait_saved() { for _ in $(seq 1 40); do saved_again && return 0; sleep 1; done; return 1; }
check "the layout was saved again after the repair" wait_saved
check "still four leaves and three documents ($(layout | head -1))" wait_layout 3 "^leaves=4 docs=3$"
check "c.md is kept by its path, with no place (a page never read was never measured)" \
  wait_layout 3 "^doc c.md -$"
cp "$LAYOUT" "$OUT/layout-after.toml" 2>/dev/null
layout | sed 's/^/        /'

echo "== window logs: $OUT/window.log, $OUT/window-2.log; layouts: $OUT/layout-before.toml, $OUT/layout-after.toml"
[ "$fail" -eq 0 ] && echo "== every step did what it should" || echo "== a step did not"
exit "$fail"
