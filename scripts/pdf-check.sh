#!/usr/bin/env bash
# scripts/pdf-check.sh [TD=binary] — a PDF in the floating square and on the
# Document face, in a hidden window (scripts/lib/hidden-window.sh), read back
# from the window's own TD_DOCDEBUG lines and its GPU memory. Needs poppler,
# python3, jq, nvidia-smi and grim.
#
# Against a twelve-page PDF built here — ten letter pages, each labelled with
# its number, a landscape page, and a page turned a quarter — it checks:
#
#   1. `ctl doc here` floats it, and the window reads all twelve pages;
#   2. the page in view is drawn, each tile in well under a second;
#   3. scrolling to the end draws the pages on the way, and gives back the
#      tiles it leaves behind, so the GPU never holds more than a few;
#   4. rewriting the file while it is open is read again, in place;
#   5. closing gives back every tile, and twenty opens and closes leave the
#      window's GPU memory where the first left it;
#   6. `ctl doc beside` shows it on a Document face;
#   7. nothing went to the desktop.
#
# Photographs land in $OUT. Exits non-zero if any check fails.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/hidden-window.sh"
TD=${TD:-$ROOT/app/target/release/terminal-delight}
OUT=${OUT:-/tmp/pdf-check-$(date +%H%M%S)}
CYCLES=${CYCLES:-20}
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
for tool in pdfinfo pdftoppm python3 jq nvidia-smi grim; do
  command -v "$tool" >/dev/null || { echo "$tool is needed and not on PATH"; exit 2; }
done
rm -rf "$OUT"; mkdir -p "$OUT/bin" "$OUT/state" "$OUT/cache"

cat > "$OUT/make.py" <<'EOF'
# make.py <out.pdf> <letter pages>: that many letter pages labelled with their
# number, then a landscape page and a page turned 90°, in Helvetica, which
# every PDF reader has without it being embedded.
import sys
out, n = sys.argv[1], int(sys.argv[2])
objs = [None, None, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
kids = []
def page(w, h, label, rotate=0):
    body = "BT /F1 64 Tf 60 %d Td (%s) Tj ET\n" % (h - 130, label)
    for i in range(16):
        body += "BT /F1 13 Tf 60 %d Td (Line %d of %s: the quick brown fox jumps over the lazy dog.) Tj ET\n" % (h - 190 - i * 22, i + 1, label)
    body += "0.2 0.4 0.8 RG 6 w 20 20 %d %d re S\n" % (w - 40, h - 40)
    objs.append("<< /Length %d >>\nstream\n%sendstream" % (len(body), body))
    content = len(objs)
    turn = " /Rotate %d" % rotate if rotate else ""
    objs.append("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %d %d]%s "
                "/Resources << /Font << /F1 3 0 R >> >> /Contents %d 0 R >>" % (w, h, turn, content))
    kids.append(len(objs))
for i in range(n):
    page(612, 792, "Page %d" % (i + 1))
page(792, 612, "Page %d, landscape" % (n + 1))
page(612, 792, "Page %d, turned" % (n + 2), 90)
objs[0] = "<< /Type /Catalog /Pages 2 0 R >>"
objs[1] = "<< /Type /Pages /Kids [%s] /Count %d >>" % (" ".join("%d 0 R" % k for k in kids), len(kids))
data = b"%PDF-1.4\n"
offsets = []
for i, o in enumerate(objs):
    offsets.append(len(data))
    data += ("%d 0 obj\n%s\nendobj\n" % (i + 1, o)).encode()
xref = len(data)
data += ("xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)).encode()
for o in offsets:
    data += ("%010d 00000 n \n" % o).encode()
data += ("trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)).encode()
open(out, "wb").write(data)
EOF
PDF="$OUT/the paper.pdf"
python3 "$OUT/make.py" "$PDF" 10 || { echo "could not make the PDF"; exit 2; }
echo "made $PDF: $(pdfinfo "$PDF" | awk '/^Pages:/ {print $2}') pages"

for opener in xdg-open uwsm-app; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/opened.log"\n' "$opener" "$OUT" > "$OUT/bin/$opener"
  chmod +x "$OUT/bin/$opener"
done
SESSION="pdfcheck$$"
WS="special:tdpdf$$"
trap hidden_cleanup EXIT
LOG="$OUT/window.log"
{
  echo "export TD_SESSION=$SESSION PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state TD_DOCDEBUG=1"
  echo "exec $TD > $LOG 2>&1"
} > "$OUT/launch.sh"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch.sh"
echo "window $WIN"
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
shot() { timeout 10 grim -T "$STABLE" "$OUT/$1.png" && echo "   photographed $OUT/$1.png"; }
gpu() { nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader,nounits 2>/dev/null |
  awk -F', ' -v p="$WIN" '$1==p {print $2}' | head -1; }
count() { grep -cF "$1" "$LOG" 2>/dev/null || true; }
wait_count() { # wait_count <fixed text> <at least> [tenths of a second]
  for _ in $(seq 1 "${3:-100}"); do [ "$(count "$1")" -ge "$2" ] && return 0; sleep 0.1; done
  return 1
}
fail=0
check() { # check <what> <condition-exit-status>
  if [ "$2" -eq 0 ]; then echo "   ok   $1"; else echo "   FAIL $1"; fail=1; fi
}
sleep 2
BASE=$(gpu)
echo "   GPU memory at rest: ${BASE:-?} MiB"

echo "== 1-2 · ctl doc here"
said=$(ctl doc here "$PDF"); echo "   $said"
wait_count "[doc] pdf read $PDF: 12 pages" 1
check "the window reads all twelve pages" $?
wait_count "[doc] drew $PDF 12 pages" 1
check "and draws the page in view" $?
sleep 0.5
shot float-first
tiles() { grep -F '[doc] pdf page ' "$LOG"; }
slowest=$(tiles | sed -n 's/.* in \([0-9]*\) ms$/\1/p' | sort -n | tail -1)
echo "   $(tiles | wc -l) tiles so far, the slowest ${slowest:-?} ms"
check "each tile in under a second" "$([ -n "$slowest" ] && [ "$slowest" -lt 1000 ]; echo $?)"
OPEN=$(gpu)
echo "   GPU memory with it open: ${OPEN:-?} MiB"

echo "== 3 · scroll to the end"
# A quarter of a second a step: slower than a hand on a wheel, so a page
# skipped here was never going to be drawn, not merely scrolled past too fast.
for _ in $(seq 1 40); do ctl doc scroll 400 >/dev/null; sleep 0.25; done
wait_count "[doc] pdf page 12 " 1 50
check "the last page is drawn" $?
sleep 0.5
shot float-end
drawn=$(tiles | sed -n 's/.*pdf page \([0-9]*\) .*/\1/p' | sort -un | tr '\n' ' ')
echo "   pages drawn on the way: $drawn"
check "every page on the way was drawn" "$([ "$(echo "$drawn" | wc -w)" -eq 12 ]; echo $?)"
held=$(grep -F '[doc] pdf gave back' "$LOG" | tail -1 | sed -n 's/.*, \([0-9]*\) on the GPU/\1/p')
most=$(grep -F '[doc] pdf gave back' "$LOG" | sed -n 's/.*, \([0-9]*\) on the GPU/\1/p' | sort -n | tail -1)
echo "   tiles given back $(grep -cF '[doc] pdf gave back' "$LOG") times; on the GPU after the last ${held:-?}, at most ${most:-?}"
check "the tiles left behind are given back" "$([ -n "$most" ] && [ "$most" -le 12 ]; echo $?)"

echo "== 4 · the file is rewritten while it is open"
python3 "$OUT/make.py" "$OUT/next.pdf" 8 && mv "$OUT/next.pdf" "$PDF"
wait_count "[doc] pdf read $PDF: 10 pages" 1 50
check "it is read again, ten pages now" $?
sleep 1
shot float-rewritten

echo "== 5 · close, then open and close $CYCLES times"
echo "   close: $(ctl doc close)"
wait_count "[doc] released $PDF (" 1 50
check "closing releases the view" $?
sleep 0.5
FIRST=$(gpu)
for i in $(seq 1 "$CYCLES"); do
  ctl doc here "$PDF" >/dev/null
  wait_count "[doc] drew $PDF" "$((i + 1))" 50 || { echo "   cycle $i never drew"; break; }
  ctl doc close >/dev/null
  wait_count "[doc] released $PDF (" "$((i + 1))" 50 || { echo "   cycle $i never released"; break; }
done
sleep 0.5
LAST=$(gpu)
echo "   GPU memory after the first close ${FIRST:-?} MiB, after the last ${LAST:-?} MiB (at rest ${BASE:-?}, open ${OPEN:-?})"
if [ -n "$FIRST" ] && [ -n "$LAST" ]; then
  check "$CYCLES opens and closes keep no tiles" "$([ $((LAST - FIRST)) -lt 16 ]; echo $?)"
fi

echo "== 6 · ctl doc beside"
before=$(count "[doc] drew $PDF")
said=$(ctl doc beside "$PDF"); echo "   $said"
wait_count "[doc] drew $PDF" "$((before + 1))" 50
check "it is drawn on a Document face beside" $?
sleep 0.8
shot face

echo "== 7 · nothing went to the desktop"
if [ -s "$OUT/opened.log" ]; then cat "$OUT/opened.log"; check "no desktop opener" 1; else check "no desktop opener" 0; fi
echo "log: $LOG"
exit $fail
