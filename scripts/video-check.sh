#!/usr/bin/env bash
# scripts/video-check.sh [TD=binary] — video in the floating square and on the
# bench, driven the way a hand drives it, in a hidden window
# (scripts/lib/hidden-window.sh). Needs libmpv, ffmpeg, jq and grim.
#
# On an AGENT pane (a stand-in `claude`) with two surfaces on its bench — an
# artifact whose href is a clip, and a Markdown card whose link label hides
# another — it checks, reading back what the pane holds after each:
#
#   1. a plain click on the artifact's rail row floats the clip over the bench;
#   2. Alt+hover on the Markdown label shows the chip over the label alone;
#   3. Alt+click on the label floats the clip the label hides;
#   4. Ctrl+Alt+click on it opens the clip in a pane beside;
#   5. while a clip plays: its player's threads exist, the window's GPU memory
#      holds still, and closing the square ends the threads.
#
# Sound is routed nowhere: the window gets no PulseAudio, no PipeWire and an
# empty ALSA config, so a clip with a soundtrack plays silently here.
# Photographs land in $OUT. Exits non-zero on the first check that fails.
set -uo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/hidden-window.sh"
TD=${TD:-$ROOT/app/target/release/terminal-delight}
OUT=${OUT:-/tmp/video-check-$(date +%H%M%S)}
rm -rf "$OUT"; mkdir -p "$OUT/bin" "$OUT/state" "$OUT/cache"
: > "$OUT/empty-alsa.conf"
ffmpeg -v error -y -f lavfi -i testsrc2=size=640x360:rate=30 -t 6 \
  -c:v libx264 -pix_fmt yuv420p "$OUT/bars.mp4" || { echo "ffmpeg could not make a clip"; exit 2; }
FILM="$ROOT/assets/clips/decision-briefs.mp4"
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
SESSION="vidcheck$$"
WS="special:tdvideo$$"
trap 'pkill -f "$OUT/bin/claude" 2>/dev/null; hidden_cleanup' EXIT
LOG="$OUT/window.log"
{
  echo "export TD_SESSION=$SESSION PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state TD_DOCDEBUG=1"
  echo "export PULSE_SERVER=unix:/nonexistent PIPEWIRE_REMOTE=/nonexistent ALSA_CONFIG_PATH=$OUT/empty-alsa.conf"
  echo "exec $TD > $LOG 2>&1"
} > "$OUT/launch.sh"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch.sh"
echo "window $WIN"
STABLE=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
shot() { timeout 10 grim -T "$STABLE" "$OUT/$1.png" && echo "   photographed $OUT/$1.png"; }
threads() { cat /proc/"$WIN"/task/*/comm 2>/dev/null | grep -c '^td-video'; }
gpu() { nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader,nounits 2>/dev/null |
  awk -F', ' -v p="$WIN" '$1==p {print $2}' | head -1; }
fail=0
check() { # check <what> <condition-exit-status>
  if [ "$2" -eq 0 ]; then echo "   ok   $1"; else echo "   FAIL $1"; fail=1; fi
}

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
cat > "$SURF/film.json" <<EOF
{ "td": "0.4", "kind": "artifact", "id": "film", "title": "The film",
  "model": { "href": "file://$FILM", "mime": "video/mp4", "summary": "A clip to play." } }
EOF
cat > "$SURF/notes.json" <<EOF
{ "td": "0.4", "kind": "markdown", "id": "notes", "title": "Notes",
  "model": { "body": "Watch [the bars clip](file://$OUT/bars.mp4) first." } }
EOF
echo "bench on: $(ctl bench on)"
PROBE="$OUT/probe.txt"
probe() { "$TD" ctl --pid "$WIN" bench probe 2>&1 | sed "s/ \xe2\x80\x96 /\n/g" > "$PROBE"; }
for i in $(seq 1 20); do
  sleep 0.5; probe
  [ "$(grep -c OpenRow "$PROBE")" -gt 0 ] && [ "$(grep -c '^label ' "$PROBE")" -gt 0 ] && break
  if [ "$i" -eq 4 ]; then
    SH=$(grep -m1 'Shelf(Artifacts)' "$PROBE" | awk '{printf "%d %d", $2 + $4/2, $3 + $5/2}')
    [ -n "$SH" ] && echo "   the Artifacts shelf: $(ctl bench click $SH)"
  fi
done
grep -E 'OpenRow|^label ' "$PROBE" | sed 's/^/   /'
center() { awk '{printf "%d %d", $2 + $4/2, $3 + $5/2}'; }

echo "== 1 · plain click on the film's rail row"
ROW=$(grep 'OpenRow' "$PROBE" | grep -m1 'film' | center)
[ -n "$ROW" ] || ROW=$(grep -m1 'OpenRow' "$PROBE" | center)
said=$(ctl bench click $ROW); echo "   $said"
case "$said" in *decision-briefs.mp4*) check "the film floats over the bench" 0 ;; *) check "the film floats over the bench" 1 ;; esac
sleep 3
shot bench-film
check "its player's threads are running ($(threads))" "$([ "$(threads)" -ge 2 ]; echo $?)"
g0=$(gpu); sleep 8; g1=$(gpu)
echo "   GPU memory ${g0:-?} MB, then ${g1:-?} MB eight seconds later"
if [ -n "$g0" ] && [ -n "$g1" ]; then
  check "GPU memory holds still while it plays" "$([ $((g1 - g0)) -le 16 ]; echo $?)"
fi
echo "   close: $(ctl doc close)"
sleep 1.5
check "closing ends its threads" "$([ "$(threads)" -eq 0 ]; echo $?)"
probe

echo "== 2-4 · the Markdown label that hides a clip"
# The film's card is the one open now; the notes card has to be on screen
# for its label to be there to press.
ROW=$(grep 'OpenRow' "$PROBE" | grep -m1 'notes' | center)
echo "   open the notes card at [$ROW]: $(ctl bench click $ROW)"
sleep 0.8
probe
LABEL=$(grep -m1 '^label ' "$PROBE")
echo "   $LABEL"
[ -n "$LABEL" ] || { echo "   FAIL no label on the bench to press"; exit 1; }
AT=$(echo "$LABEL" | center)
hover=$(ctl bench hover $AT alt); echo "   alt hover: $hover"
case "$hover" in *"hint=Some"*) check "Alt over the label shows the chip" 0 ;; *) check "Alt over the label shows the chip" 1 ;; esac
said=$(ctl bench click $AT alt); echo "   alt click: $said"
case "$said" in *bars.mp4*) check "Alt+click floats the clip the label hides" 0 ;; *) check "Alt+click floats the clip the label hides" 1 ;; esac
sleep 2
shot bench-label
echo "   close: $(ctl doc close)"
said=$(ctl bench click $AT ctrl-alt); echo "   ctrl-alt click: $said"
sleep 1.5
# The square is closed, so a player running now is the pane beside's, and
# the frame it drew is logged under the clip's name.
drew=$(grep -c "\[doc\] drew .*bars.mp4" "$LOG")
echo "   player threads: $(threads); frames drawn from bars.mp4 so far, by first-frame line: $drew"
check "Ctrl+Alt+click opens it beside, playing" "$([ "$(threads)" -ge 2 ] && [ "$drew" -ge 2 ]; echo $?)"
shot bench-beside

echo "== nothing went to the desktop"
if [ -s "$OUT/opened.log" ]; then cat "$OUT/opened.log"; check "no desktop opener" 1; else check "no desktop opener" 0; fi
exit $fail
