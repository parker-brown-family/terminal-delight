#!/usr/bin/env bash
# Open and close a floating document a hundred times, and count what the GPU
# keeps.
#
# A floating square holds a decoded image and its texture. gpui would keep
# both for the life of the window unless they are handed back, and nothing in
# a unit test can see a texture: TD has no gpui harness, and gpui's test atlas
# does not say what it holds. So this runs the real window against the real
# GPU. It opens a 4096 × 4096 image (64 MiB as a texture) through the control
# socket, waits until the window has actually drawn it, closes it, waits
# until the view has actually released it, and reads the window's own GPU
# memory from nvidia-smi after every step. A leak of one texture per cycle is
# 6.4 GiB over a hundred cycles, so it cannot hide in the noise.
#
#   scripts/doc-float-soak.sh [--cycles N] [--bin PATH] [--image PATH] [--out DIR]
#
# THE WINDOW IS HIDDEN. It is launched onto a special workspace with
# `render_unfocused` and `no_initial_focus`, so nothing appears on screen, no
# tile is resized and focus never moves. If the window lands anywhere but that
# workspace, the script kills it at once and exits 4 without retrying.
#
# A window that never draws cannot leak a texture, and a soak that never drew
# would report a perfect result. So every cycle waits for the window's own
# `[doc] drew` line (TD_DOCDEBUG) and the run fails if any cycle did not draw.
#
# Exit 0: every cycle drew and released, and the GPU grew by less than one
# texture across all cycles after the first. 1: it leaked, or a step failed.
# 2: bad arguments or no binary. 4: the window was not hidden; nothing ran.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CYCLES=100
TD="$ROOT/app/target/release/terminal-delight"
IMG=""
OUT="/tmp/doc-float-soak-$(date +%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --cycles) CYCLES=$2; shift 2 ;;
    --bin) TD=$2; shift 2 ;;
    --image) IMG=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
command -v nvidia-smi >/dev/null || { echo "nvidia-smi is how this counts GPU memory; not found"; exit 2; }
mkdir -p "$OUT"
SESSION="docsoak$$"
WS="special:tdsoak$$"
TEXTURE_MIB=64

if [ -z "$IMG" ]; then
  IMG="$OUT/big.png"
  python3 - "$IMG" <<'EOF'
import struct, zlib, sys
W = H = 4096
def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
row = bytearray()
for x in range(W):
    row += bytes((0, (x * 255) // (W - 1), 128, 255))
rows = bytearray()
for y in range(H):
    rows.append(0)
    row[0::4] = bytes([(y * 255) // (H - 1)]) * W
    rows += row
open(sys.argv[1], "wb").write(b"\x89PNG\r\n\x1a\n"
    + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(bytes(rows), 6)) + chunk(b"IEND", b""))
EOF
fi
case "$IMG" in /*) ;; *) IMG="$(pwd)/$IMG" ;; esac

WIN=""
# Everything this run made goes with it: the window, the session host it
# spawned, and the host's lock files and layout, which are written as those
# processes exit, so they are removed only once both are gone.
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

echo "== launching a hidden window on $WS (session $SESSION)"
LOG="$OUT/window.log"
SAID=$(hyprctl dispatch "hl.dsp.exec_cmd(\"sh -c 'TD_SESSION=$SESSION TD_DOCDEBUG=1 exec $TD > $LOG 2>&1'\", { workspace = \"$WS silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
case "$SAID" in ok|"") ;; *) echo "hyprctl refused the launch: $SAID"; exit 1 ;; esac

for _ in $(seq 1 60); do
  WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $SESSION" \
    '.[] | select(.title==$t) | .pid' | head -1)
  [ -n "$WIN" ] && break
  sleep 0.5
done
[ -n "$WIN" ] || { echo "no window appeared — see $LOG"; exit 1; }
LANDED=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
if [ "$LANDED" != "$WS" ]; then
  echo "the window landed on '$LANDED', not $WS — killed, nothing ran"
  kill "$WIN" 2>/dev/null
  WIN=""
  exit 4
fi
echo "   window $WIN, hidden on $LANDED"

# The client prefixes each reply with the answering window's pid and a tab.
ctl() { "$TD" ctl --pid "$WIN" "$@" 2>&1 | head -1 | sed 's/^[0-9]*\t//'; }
for _ in $(seq 1 40); do [ "$(ctl ping)" = "pong" ] && break; sleep 0.5; done
gpu() { nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader,nounits \
  | awk -F', ' -v p="$WIN" '$1==p {print $2}' | head -1; }
rss() { awk '/VmRSS/ {print int($2/1024)}' "/proc/$WIN/status"; }
count() { grep -c "\[doc\] $1 " "$LOG" 2>/dev/null || true; }
wait_for() { # wait_for <drew|released> <n>
  for _ in $(seq 1 100); do [ "$(count "$1")" -ge "$2" ] && return 0; sleep 0.1; done
  return 1
}

sleep 2
BASE_GPU=$(gpu); BASE_RSS=$(rss)
echo "== baseline: gpu ${BASE_GPU:-?} MiB, rss ${BASE_RSS} MiB; $CYCLES cycles of $IMG"
CSV="$OUT/samples.csv"
echo "cycle,open_gpu_mib,closed_gpu_mib,closed_rss_mib" > "$CSV"
fail=0
for i in $(seq 1 "$CYCLES"); do
  r=$(ctl doc here "$IMG")
  case "$r" in ok*) ;; *) echo "   cycle $i: doc here said: $r"; fail=1; break ;; esac
  wait_for drew "$i" || { echo "   cycle $i: the window never drew the image"; fail=1; break; }
  OPEN=$(gpu)
  r=$(ctl doc close)
  case "$r" in ok*) ;; *) echo "   cycle $i: doc close said: $r"; fail=1; break ;; esac
  wait_for released "$i" || { echo "   cycle $i: the view was never released"; fail=1; break; }
  sleep 0.15
  CLOSED=$(gpu)
  echo "$i,$OPEN,$CLOSED,$(rss)" >> "$CSV"
  [ $((i % 10)) -eq 0 ] && echo "   cycle $i: open ${OPEN} MiB, closed ${CLOSED} MiB, rss $(rss) MiB"
done

DREW=$(count drew); RELEASED=$(count released)
FIRST=$(awk -F, 'NR==2 {print $3}' "$CSV")
LAST=$(awk -F, 'END {print $3}' "$CSV")
PEAK=$(awk -F, 'NR>1 && $2>m {m=$2} END {print m}' "$CSV")
echo "== drew $DREW, released $RELEASED of $CYCLES"
echo "   gpu after the first close ${FIRST:-?} MiB, after the last ${LAST:-?} MiB, peak open ${PEAK:-?} MiB (baseline ${BASE_GPU:-?})"
echo "   samples: $CSV"
if [ "$fail" -ne 0 ] || [ "$DREW" -ne "$CYCLES" ] || [ "$RELEASED" -ne "$CYCLES" ]; then
  echo "FAIL: not every cycle drew and released"
  exit 1
fi
GROWTH=$(( ${LAST:-0} - ${FIRST:-0} ))
if [ "$GROWTH" -ge "$TEXTURE_MIB" ]; then
  echo "FAIL: the GPU grew ${GROWTH} MiB after the first cycle — at least one ${TEXTURE_MIB} MiB texture was kept"
  exit 1
fi
echo "PASS: ${GROWTH} MiB of growth across $((CYCLES - 1)) cycles after the first, under one texture (${TEXTURE_MIB} MiB)"
