#!/usr/bin/env bash
# scripts/bench-doc-check.sh [TD=binary] — the bench's document gestures, driven
# the way a hand drives them, in a hidden window (scripts/lib/hidden-window.sh).
#
# Drive the bench's document gestures the way a hand does, in a hidden window,
# on an AGENT pane (a stand-in `claude`), with an artifact on its bench:
# a plain click on the artifact's rail row, Alt+hover and Alt+click on its
# TARGET text, Ctrl+Alt+click. Reads back what the pane holds after each.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/hidden-window.sh"
TD=${TD:-$ROOT/app/target/release/terminal-delight}
OUT=${OUT:-/tmp/bench-doc-check-$(date +%H%M%S)}
rm -rf "$OUT"; mkdir -p "$OUT/bin" "$OUT/state" "$OUT/cache"
printf '<!doctype html><html><body><h1>Bench click</h1></body></html>\n' > "$OUT/brief.html"
cat > "$OUT/bin/claude" <<EOF
#!/usr/bin/env python3
import os, sys, tty
sys.stdout.write("stand-in agent\r\n> ")
sys.stdout.flush()
tty.setraw(0)
while os.read(0, 4096):
    pass
EOF
chmod +x "$OUT/bin/claude"
for opener in xdg-open uwsm-app; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/opened.log"\n' "$opener" "$OUT" > "$OUT/bin/$opener"
  chmod +x "$OUT/bin/$opener"
done
SESSION="benchclk$$"
WS="special:tdclick$$"
trap 'pkill -f "$OUT/bin/claude" 2>/dev/null; hidden_cleanup' EXIT
LOG="$OUT/window.log"
{
  echo "export TD_SESSION=$SESSION PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state TD_DOCDEBUG=1 TD_HITDEBUG=1"
  echo "exec $TD > $LOG 2>&1"
} > "$OUT/launch.sh"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch.sh"
echo "window $WIN"
ctl mcp on >/dev/null; ctl mcp writes on >/dev/null
"$TD" ctl adopt --pid "$WIN" --cwd "$OUT" --run "$OUT/bin/claude" >/dev/null 2>&1
AGENT=""
for _ in $(seq 1 60); do
  AGENT=$(ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' |
    jq -r '.result.structuredContent.panes[]? | select(.mode=="CLAUDE") | .pane_id' | head -1)
  [ -n "$AGENT" ] && break
  sleep 0.25
done
echo "agent pane: ${AGENT:-NONE}"
[ -n "$AGENT" ] || exit 1
SURF="$OUT/state/terminal-delight/surfaces/$SESSION/$AGENT"
mkdir -p "$SURF"
cat > "$SURF/art.json" <<EOF
{ "td": "0.4", "kind": "artifact", "id": "art-click", "title": "Click me",
  "model": { "href": "file://$OUT/brief.html", "mime": "text/html",
             "summary": "A test artifact.", "served": "http://127.0.0.1:8611/brief.html" } }
EOF
echo "bench on: $(ctl bench on)"
PROBE="$OUT/probe.txt"
probe() { "$TD" ctl --pid "$WIN" bench probe 2>&1 | sed "s/ \xe2\x80\x96 /\n/g" > "$PROBE"; }
for i in $(seq 1 20); do
  sleep 0.5; probe
  n=$(grep -c OpenRow "$PROBE"); z=$(grep -c '^zone' "$PROBE")
  echo "   t=$i zones=$z openrows=$n"
  [ "$n" -gt 0 ] && break
  if [ "$i" -eq 4 ]; then
    grep '^zone' "$PROBE" | sed 's/^/     /'
    SH=$(grep -m1 'Shelf(Artifacts)' "$PROBE" | awk '{printf "%d %d", $2 + $4/2, $3 + $5/2}')
    echo "   click the Artifacts shelf at [$SH]: $(ctl bench click $SH)"
  fi
done
echo "tabs: $(ctl mcp rpc '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}' | jq -c '[.result.structuredContent.panes[]? | {tab,mode,pane_id}]')"
echo "== probe:"; head -1 "$PROBE"; grep -E 'OpenRow|^text ' "$PROBE" | head -8
center() { awk '{printf "%d %d", $2 + $4/2, $3 + $5/2}'; }
ROW=$(grep -m1 'OpenRow' "$PROBE" | center)
echo "== plain click on the rail row at [$ROW]:"
echo "   $(ctl bench click $ROW)"
echo "   doc close: $(ctl doc close)"
sleep 0.8
probe
TXT=$(grep -m1 '^text .*file://' "$PROBE" | awk '{printf "%d %d", $2 + 30, $3 + $5/2}')
echo "== TARGET text at [$TXT]"
echo "   alt hover:      $(ctl bench hover $TXT alt)"
echo "   alt click:      $(ctl bench click $TXT alt)"
echo "   doc close:      $(ctl doc close)"
echo "   ctrl-alt click: $(ctl bench click $TXT ctrl-alt)"
sleep 1
echo "== opened by the desktop (should be empty):"; cat "$OUT/opened.log" 2>/dev/null
echo "== [doc-hit] / [doc] lines:"
grep -E '\[doc-hit\]|\[doc\] (drew|page)' "$LOG" | cut -c1-220 | tail -14
