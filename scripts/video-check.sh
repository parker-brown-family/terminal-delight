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
#      holds still, and closing the square ends the threads;
#   6. a press on the <video> in a page open in the square opens the file as a
#      clip in its own box over the page, the page is covered and kept, and at
#      the clip's end the box closes, its threads end and the page is on top
#      again; `doc here` on a video (an agent handing over a render) does the
#      same.
#   7. the same press on a page that carries its film inside itself, as a
#      data: address, opens the copy the page cache kept of that film.
#   8. a press on an <audio> whose sound the page carries opens it as a clip
#      too, a band over the page drawing the sound's waveform, which closes
#      at the sound's end; `doc here` on a sound does the same.
#
# Sound is routed nowhere: mpv is told to play into no output at all
# (TD_MPV_AO=null), so a clip with a soundtrack plays silently here, and a
# sound plays its waveform in silence. The window also gets no PulseAudio, no
# PipeWire and an empty ALSA config, for anything else that might make one.
# Breaking those alone is not enough: mpv then probes each output in turn for
# every clip with sound, and in a window whose servers point at nothing the
# probe sooner or later hangs (the sixth or seventh clip in a row, measured).
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
ffmpeg -v error -y -f lavfi -i testsrc2=size=640x360:rate=30 -t 2 \
  -c:v libx264 -pix_fmt yuv420p "$OUT/short.mp4" || { echo "ffmpeg could not make a clip"; exit 2; }
# A tone that swells and fades, so its waveform visibly moves: Opus in Ogg, as
# the cinema-delight takes pages carry theirs.
ffmpeg -v error -y -f lavfi -i 'aevalsrc=0.8*sin(2*PI*220*t)*(0.55+0.45*sin(2*PI*1.5*t)):d=2.5:s=48000' \
  -c:a libopus "$OUT/take.ogg" || { echo "ffmpeg could not make a sound"; exit 2; }
cat > "$OUT/page.html" <<'EOF'
<!doctype html><html><body style="margin:0;background:#111;color:#ddd;font:18px sans-serif">
<h1 style="font-size:24px;margin:16px">A page with a film in it</h1>
<video controls preload="metadata" src="short.mp4" style="display:block;width:90%;margin:0 auto"></video>
<p style="margin:16px">The page stays under the clip, and is back when it ends.</p>
</body></html>
EOF
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
  echo "export TD_MPV_AO=null PULSE_SERVER=unix:/nonexistent PIPEWIRE_REMOTE=/nonexistent ALSA_CONFIG_PATH=$OUT/empty-alsa.conf"
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

echo "== 6 · a video on a page opens as a clip over the page, and closes at its end"
# What floats, read without changing anything: a hover in the corner, where
# neither the square nor a bench control is.
floating() { ctl bench hover 3 3 | sed -n 's/.*float=\([^ ]*\).*/\1/p'; }
echo "   page: $(ctl doc here "$OUT/page.html")"
# The press is aimed a fifth of the body's width below the page's heading,
# inside the film's box at any width the square can be; retried while the
# page is still being laid out, when a press finds nothing. What floats is
# read BEFORE each press, never from the press's own answer: that answer is
# written before the pane routes the link the press raised, so it still
# names the page, and a second press would land on the clip's picture and
# pause it.
opened=""
for _ in $(seq 1 30); do
  sleep 0.5
  case "$(floating)" in *short.mp4) opened=$(floating); break ;; esac
  probe
  BODY=$(grep -m1 '^float .* Body$' "$PROBE")
  [ -n "$BODY" ] || continue
  AT=$(echo "$BODY" | awk '{printf "%d %d", $2 + $4/2, $3 + 70 + $4/5}')
  ctl bench click $AT >/dev/null
done
echo "   press on the film: ${opened:-nothing opened}"
check "a press on the page's video opens it as a clip" "$([ -n "$opened" ]; echo $?)"
sleep 0.6; probe
check "the page is under the clip, covered and kept" "$(grep -q '^float .* Covered$' "$PROBE"; echo $?)"
check "the clip's player is running ($(threads))" "$([ "$(threads)" -ge 2 ]; echo $?)"
shot clip-over-page
sleep 3
now=$(floating); echo "   after its end: float=$now"
check "at its end the clip closes and the page is on top again" "$(case "$now" in *page.html) echo 0 ;; *) echo 1 ;; esac)"
check "the clip's threads end with it ($(threads))" "$([ "$(threads)" -eq 0 ]; echo $?)"
echo "   an agent hands over a render: $(ctl doc here "$OUT/short.mp4")"
sleep 0.6
check "doc here on a video opens it over the page" "$(case "$(floating)" in *short.mp4) echo 0 ;; *) echo 1 ;; esac)"
sleep 3
check "and it too goes at its end, leaving the page" "$(case "$(floating)" in *page.html) echo 0 ;; *) echo 1 ;; esac)"
echo "   close: $(ctl doc close)"
probe

echo "== 7 · a video the page carries inside itself opens as a clip the same way"
# A page made to travel as one file carries its film as a data: address. The
# page cache keeps it as pages/clip-<key>/clip.mp4, here under $OUT/cache.
{
  echo '<!doctype html><html><body style="margin:0;background:#111;color:#ddd;font:18px sans-serif">'
  echo '<h1 style="font-size:24px;margin:16px">A page that carries its film</h1>'
  printf '<video controls preload="metadata" src="data:video/mp4;base64,%s" style="display:block;width:90%%;margin:0 auto"></video>\n' \
    "$(base64 -w0 "$OUT/short.mp4")"
  echo '</body></html>'
} > "$OUT/carried.html"
echo "   page: $(ctl doc here "$OUT/carried.html")"
opened=""
for _ in $(seq 1 30); do
  sleep 0.5
  case "$(floating)" in */clip.mp4) opened=$(floating); break ;; esac
  probe
  BODY=$(grep -m1 '^float .* Body$' "$PROBE")
  [ -n "$BODY" ] || continue
  AT=$(echo "$BODY" | awk '{printf "%d %d", $2 + $4/2, $3 + 70 + $4/5}')
  ctl bench click $AT >/dev/null
done
echo "   press on the film: ${opened:-nothing opened}"
check "a press on the carried video opens it as a clip" "$([ -n "$opened" ]; echo $?)"
check "from the copy the page cache kept" \
  "$(case "$opened" in "$OUT/cache/terminal-delight/pages/clip-"*) echo 0 ;; *) echo 1 ;; esac)"
check "which is the film itself" "$(cmp -s "$opened" "$OUT/short.mp4"; echo $?)"
shot carried-clip-over-page
sleep 3
check "at its end the page is on top again" "$(case "$(floating)" in *carried.html) echo 0 ;; *) echo 1 ;; esac)"
echo "   close: $(ctl doc close)"
probe

echo "== 8 · a sound the page carries opens as a clip, drawn as its waveform"
# The page cache keeps it as pages/clip-<key>/clip.ogg: named as a sound, so
# it opens as one. TD_DOCDEBUG logs the first frame drawn, at the waveform's
# own size, which says mpv drew the sound rather than nothing.
{
  echo '<!doctype html><html><body style="margin:0;background:#111;color:#ddd;font:18px sans-serif">'
  echo '<h1 style="font-size:24px;margin:16px">A page that carries its takes</h1>'
  printf '<audio controls preload="none" src="data:audio/ogg;base64,%s" style="display:block;width:90%%;height:360px;margin:0 auto"></audio>\n' \
    "$(base64 -w0 "$OUT/take.ogg")"
  echo '</body></html>'
} > "$OUT/takes.html"
echo "   page: $(ctl doc here "$OUT/takes.html")"
opened=""
for _ in $(seq 1 30); do
  sleep 0.5
  case "$(floating)" in */clip.ogg) opened=$(floating); break ;; esac
  probe
  BODY=$(grep -m1 '^float .* Body$' "$PROBE")
  [ -n "$BODY" ] || continue
  AT=$(echo "$BODY" | awk '{printf "%d %d", $2 + $4/2, $3 + 70 + $4/10}')
  ctl bench click $AT >/dev/null
done
echo "   press on the sound: ${opened:-nothing opened}"
check "a press on the page's sound opens it as a clip" "$([ -n "$opened" ]; echo $?)"
check "from the copy the page cache kept, named as a sound" \
  "$(case "$opened" in "$OUT/cache/terminal-delight/pages/clip-"*/clip.ogg) echo 0 ;; *) echo 1 ;; esac)"
check "which is the sound itself" "$(cmp -s "$opened" "$OUT/take.ogg"; echo $?)"
sleep 0.6; probe
check "the page is under it, covered and kept" "$(grep -q '^float .* Covered$' "$PROBE"; echo $?)"
check "its player is running ($(threads))" "$([ "$(threads)" -ge 2 ]; echo $?)"
drew=$(grep -m1 "\[doc\] drew .*clip.ogg" "$LOG"); echo "   ${drew:-no frame drawn}"
check "it draws the sound's waveform" "$(case "$drew" in *" 1200x480 as "*) echo 0 ;; *) echo 1 ;; esac)"
shot sound-over-page
sleep 3
now=$(floating); echo "   after its end: float=$now"
check "at its end it closes and the page is on top again" "$(case "$now" in *takes.html) echo 0 ;; *) echo 1 ;; esac)"
check "its threads end with it ($(threads))" "$([ "$(threads)" -eq 0 ]; echo $?)"
echo "   an agent hands over a sound: $(ctl doc here "$OUT/take.ogg")"
sleep 0.6
check "doc here on a sound opens it over the page" "$(case "$(floating)" in *take.ogg) echo 0 ;; *) echo 1 ;; esac)"
sleep 3.5
check "and it too goes at its end, leaving the page" "$(case "$(floating)" in *takes.html) echo 0 ;; *) echo 1 ;; esac)"
echo "   close: $(ctl doc close)"
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
