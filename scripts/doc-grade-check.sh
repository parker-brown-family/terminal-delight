#!/usr/bin/env bash
# The floating document's gauges, read off the screen, in a window nobody can
# see. The grade runs in a shader that compiles at runtime, so no test can say
# whether it works and a green build proves nothing: this opens a square, moves
# each dial through set_pane_config, photographs the window and compares a pixel
# in the document with the number the dial should have made.
#
# In Clean, the design a fresh install wears, on a flat screen:
#   0. a flat colour, every dial unset        -> the colour every sum starts from
#   1. brightness 25 and 75                   -> half and half as much again
#   2. contrast 75                            -> pushed away from the mid-grey
#   3. both dials back at 50                  -> the unset colour, to the byte
#   4. the terminal beside the square         -> untouched throughout
#   5. see-through                            -> between the page and the field
#   6. the document's own glow                -> held off, as Clean holds all light
#   7. Markdown, a page and a PDF             -> take the same grade
#   8. the square lent to the reading frame   -> keeps its grade there
#   9. closed and reopened with GAUGES up     -> the dial survived, and still works
#                                                with the glass flattened, and the
#                                                tray over it takes no grade
# In a design with light in it:
#  10. the halo: unset, none, its own         -> follows phosphor until moved
#  11. bent glass                             -> the grade survives the warp
#  12. see-through over a wall of text        -> the text shows through
#
#   scripts/doc-grade-check.sh [--bin PATH] [--out DIR]
#
# THE WINDOW IS HIDDEN (scripts/lib/hidden-window.sh). Its session is its own,
# and the window, its host and the session's files go when the script exits.
#
# Exit 0: every step held. 1: one did not. 2: bad arguments. 4: not hidden.
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TD="$ROOT/app/target/release/terminal-delight"
OUT="${TMPDIR:-/tmp}/td-doc-grade-$$"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
for tool in jq grim magick python3; do
  command -v "$tool" >/dev/null || { echo "$tool not found"; exit 2; }
done
RUN="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
if [ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
  HYPRLAND_INSTANCE_SIGNATURE=$(ls -t "$RUN/hypr" | head -1)
  export HYPRLAND_INSTANCE_SIGNATURE
fi
if [ -z "${WAYLAND_DISPLAY:-}" ]; then
  WAYLAND_DISPLAY=$(basename "$(ls "$RUN"/wayland-? | head -1)")
  export WAYLAND_DISPLAY
fi
# shellcheck source=scripts/lib/hidden-window.sh
. "$ROOT/scripts/lib/hidden-window.sh"

mkdir -p "$OUT/cache" "$OUT/state" "$OUT/docs" "$OUT/shots"
# Mid-tones on every channel, so a gain moves each one and none starts clipped.
FILE="96,112,144"
magick -size 1000x1000 "xc:rgb($FILE)" "$OUT/docs/flat.png"
{ echo "# A page"; for i in $(seq 1 40); do echo; echo "Line $i of a page to read."; done; } > "$OUT/docs/page.md"
printf '<!doctype html><html style="height:100%%;background:rgb(%s)"><body></body></html>\n' "$FILE" > "$OUT/docs/flat.html"
magick "$OUT/docs/flat.png" "$OUT/docs/flat.pdf"

SESSION="docgrade$$"
LIT="docgradelit$$"
WS="special:tdgrade$$"
LOG="$OUT/window.log"
launcher() { # launcher <file> [VAR=value…]: the script Hyprland runs
  local file=$1
  shift
  {
    echo "export TD_SESSION=$SESSION TD_MCP_SCRIPTED=1 XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state $*"
    echo "exec $TD > $LOG 2>&1"
  } > "$file"
}
trap 'hidden_cleanup' EXIT

fail=0
ok() { echo "   ok   $1"; }
bad() { echo "   FAIL $1"; fail=1; }

rpc() { # rpc <tool> <arguments-json>
  ctl mcp rpc "$(jq -nc --arg t "$1" --argjson a "$2" \
    '{jsonrpc:"2.0",id:1,method:"tools/call",params:{name:$t,arguments:$a}}')"
}

config() { # config <json>: patch the window's outer grade
  local r
  r=$(rpc set_pane_config "$(jq -nc --argjson c "$1" '{updates:[{target:"outer",config:$c}]}')")
  case "$r" in
    '{'*) [ "$(jq -r '.result.isError // false' <<< "$r")" = true ] && bad "set_pane_config $1: $r" ;;
    *) bad "set_pane_config $1 answered: $r" ;;
  esac
  sleep 0.6
}

serve() { # serve: the config API, once a fresh window has it up
  ctl mcp on >/dev/null
  ctl mcp writes on >/dev/null
  ctl mcp expose all >/dev/null
  for _ in $(seq 1 20); do
    # A write, because reads come up first: an empty patch changes nothing.
    case "$(rpc set_pane_config '{"updates":[{"target":"outer","config":{}}]}')" in
      *disabled*) sleep 0.5 ;;
      *) return ;;
    esac
  done
  echo "the config API never came up"
  exit 1
}

shot() { # shot <name>: photograph the window by its stable id
  local id
  id=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .stableId' | head -1)
  timeout 10 grim -T "$id" "$OUT/shots/$1.png" || bad "no photograph for $1"
}

pixel() { # pixel <shot> <x> <y>: "r,g,b"
  magick "$OUT/shots/$1.png" -format "%[fx:round(255*p{$2,$3}.r)],%[fx:round(255*p{$2,$3}.g)],%[fx:round(255*p{$2,$3}.b)]" info:
}

light() { # light <shot>: the mean of the document's rectangle, 0..255
  magick "$OUT/shots/$1.png" -crop "$(( R - L ))x$(( B - T ))+$L+$T" +repage -colorspace Gray \
    -format '%[fx:round(255*mean)]' info:
}

share() { # share <shot> <r,g,b>: the fraction of the window that is that colour
  magick "$OUT/shots/$1.png" -fuzz 1.2% -fill white -opaque "rgb($2)" -fill black +opaque white \
    -colorspace Gray -format '%[fx:mean]' info:
}

spread() { # spread <shot>: how much the document's rectangle varies, 0 for flat
  magick "$OUT/shots/$1.png" -crop "$(( R - L ))x$(( B - T ))+$L+$T" +repage -colorspace Gray \
    -format '%[fx:standard_deviation]' info:
}

locate() { # locate <shot>: the one big lit block on a dark screen, into L T R B X Y
  # The left third is blacked out first: a lit design's left bar is as bright.
  local size
  size=$(magick identify -format '%w %h' "$OUT/shots/$1.png")
  read -r L T R B < <(magick "$OUT/shots/$1.png" -colorspace Gray \
    -fill black -draw "rectangle 0,0 $(( ${size% *} / 3 )),${size#* }" -threshold 25% \
    -morphology Open Square:12 -format '%@' info: |
    sed -E 's/^([0-9]+)x([0-9]+)\+([0-9]+)\+([0-9]+)$/\3 \4 \1 \2/' |
    awk '{print $1, $2, $1 + $3, $2 + $4}')
  [ "${R:-0}" -gt $(( ${L:-0} + 200 )) ] || { echo "the picture is not on the screen; see $OUT/shots/$1.png"; exit 1; }
  X=$(( (L + R) / 2 )); Y=$(( (T + B) / 2 ))
  # Outside the square's border, which stands about 5% of a fitted picture off it.
  HX=$(( L - (R - L) / 15 )); HY=$Y
  echo "   the document is at $L,$T to $R,$B; reading $X,$Y"
}

near() { # near <what> <got> <want> [tolerance]
  python3 -c '
import sys
what, got, want, tol = sys.argv[1:5]
g = [float(v) for v in got.split(",")]
w = [float(v) for v in want.split(",")]
held = len(g) == len(w) and all(abs(a - b) <= float(tol) for a, b in zip(g, w))
tag = "ok  " if held else "FAIL"
print(f"   {tag} {what}: {got} (wanted {want})")
sys.exit(0 if held else 1)' "$1" "$2" "$3" "${4:-3}" || fail=1
}

# The file's colour through the shader's sum, then the window's own opacity,
# which the compositor lays on after the pass and which $INK against $FILE reads.
expect() { # expect <brightness gain> <contrast gain>
  python3 -c '
import sys
b, c = float(sys.argv[1]), float(sys.argv[2])
file = [int(v) / 255 for v in sys.argv[3].split(",")]
seen = [int(v) / 255 for v in sys.argv[4].split(",")]
print(",".join(str(round(255 * min(1, max(0, ((f - .5) * c + .5) * b)) * s / f))
               for f, s in zip(file, seen)))' "$1" "$2" "$FILE" "$INK"
}

half() { python3 -c "print(','.join(str(round(int(v) * .5)) for v in '$1'.split(',')))"; }

launcher "$OUT/launch.sh"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch.sh"
serve

# A flat screen first: the grade has to turn the pass on by itself.
config '{"crt":false}'
sleep 1
r=$(ctl doc here "$OUT/docs/flat.png")
case "$r" in ok*) ;; *) echo "doc here said: $r"; exit 1 ;; esac
sleep 2
shot unset
locate unset
# On the terminal, clear of the square and its halo.
FX=$(( L / 3 )); FY=$Y
FIELD=$(pixel unset "$FX" "$FY")
INK=$(pixel unset "$X" "$Y")
echo "   0    unset, the document reads $INK of the file's $FILE"

config '{"doc_brightness":25}'; shot bright25
near "1 brightness 25 halves it" "$(pixel bright25 "$X" "$Y")" "$(expect 0.5 1)"
config '{"doc_brightness":75}'; shot bright75
near "1 brightness 75 lifts it past what the file holds" "$(pixel bright75 "$X" "$Y")" "$(expect 1.5 1)"
near "4 the terminal is left alone" "$(pixel bright75 "$FX" "$FY")" "$FIELD" 1

config '{"doc_brightness":50,"doc_contrast":75}'; shot contrast75
near "2 contrast 75 pushes away from grey" "$(pixel contrast75 "$X" "$Y")" "$(expect 1 1.5)"

config '{"doc_contrast":50}'; shot middle
near "3 back at the middle" "$(pixel middle "$X" "$Y")" "$INK" 0

config '{"doc_clear":50}'; shot clear50
got=$(pixel clear50 "$X" "$Y")
if python3 -c '
import sys
g, ink, field = ([float(v) for v in a.split(",")] for a in sys.argv[1:4])
moved = sum(abs(a - b) for a, b in zip(g, ink))
gap = sum(abs(a - b) for a, b in zip(field, ink))
sys.exit(0 if 0.2 * gap < moved < gap else 1)' "$got" "$INK" "$FIELD"; then
  ok "5 see-through sits between the page and the field: $got"
else
  bad "5 see-through: $got, between $INK and $FIELD"
fi
config '{"doc_clear":0}'

config '{"doc_glow":100}'; shot clean_glow
near "6 Clean holds the document's own glow off" "$(pixel clean_glow "$HX" "$HY")" "$(pixel unset "$HX" "$HY")" 1

config '{"doc_brightness":25}'
ctl doc here "$OUT/docs/page.md" >/dev/null
sleep 2
shot md_dim
config '{"doc_brightness":50}'; shot md_plain
dim=$(light md_dim); plain=$(light md_plain)
[ "$plain" -gt 3 ] || bad "7 the page drew almost nothing to grade: $plain"
near "7 Markdown takes the grade" "$dim" "$(( plain / 2 ))" 1

# Each kind reaches the screen by its own road: Chromium's tiles, poppler's page.
for kind in html pdf; do
  r=$(ctl doc here "$OUT/docs/flat.$kind")
  case "$r" in ok*) ;; *) bad "7 doc here $kind said: $r" ;; esac
  sleep 5
  shot "$kind-plain"
  config '{"doc_brightness":25}'; shot "$kind-dim"
  config '{"doc_brightness":50}'
  plain=$(pixel "$kind-plain" "$X" "$Y")
  [ "$plain" != "$FIELD" ] || bad "7 the $kind drew nothing at $X,$Y"
  near "7 $kind takes the grade" "$(pixel "$kind-dim" "$X" "$Y")" "$(half "$plain")"
done

# The frame a person's own open lands in. The window has no pointer here, so the
# reader comes up over the square with its key, and borrows the square's view.
ctl doc here "$OUT/docs/flat.html" >/dev/null
sleep 5
config '{"doc_brightness":25}'
hyprctl dispatch "hl.dsp.send_shortcut({ mods = 'ALT', key = 'r', window = 'pid:$WIN' })" >/dev/null
sleep 3
shot reader
graded=$(share reader "$(expect 0.5 1)"); plain=$(share reader "$INK")
if python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) > .03 and float(sys.argv[2]) < .001 else 1)' "$graded" "$plain"; then
  ok "8 the reading frame keeps the grade: $graded of the window graded, $plain not"
else
  bad "8 the reading frame: $graded of the window graded, $plain not"
fi
hyprctl dispatch "hl.dsp.send_shortcut({ mods = '', key = 'Escape', window = 'pid:$WIN' })" >/dev/null
sleep 1

hidden_close
launcher "$OUT/launch-tray.sh" "TD_OSD_DEMO=open,over"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch-tray.sh"
serve
ctl doc here "$OUT/docs/flat.png" >/dev/null
sleep 3
shot tray
# A reopened window is free to come up another size, so this asks how much of it
# is the graded colour, wherever that is, and how much is the ungraded one.
graded=$(share tray "$(expect 0.5 1)"); plain=$(share tray "$INK")
if python3 -c 'import sys; sys.exit(0 if float(sys.argv[1]) > .03 and float(sys.argv[2]) < .001 else 1)' "$graded" "$plain"; then
  ok "9 reopened under GAUGES, the dial held and still works: $graded of the window graded, $plain not"
else
  bad "9 reopened under GAUGES: $graded of the window graded, $plain not"
fi
# The tray stands over the square (TD_OSD_DEMO's `over`): its right edge at three
# quarters of the window and its top 156 down. A spot in its padding, clear of
# the halo it casts onto the page, which is the page's and is graded, must not
# move when the dial does, and must really be over the document.
config '{"doc_brightness":75}'; shot tray_lit
config '{"doc_brightness":25}'
locate tray_lit
ww=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .size[0]' | head -1)
iw=$(magick identify -format '%w' "$OUT/shots/tray.png")
TX=$(( (ww * 3 / 4 - 24) * iw / ww )); TY=$(( 196 * iw / ww ))
beside=$(( TX + 100 ))
dim=$(pixel tray "$beside" "$TY"); lit=$(pixel tray_lit "$beside" "$TY")
if [ "$dim" != "$lit" ]; then
  ok "9 the tray stands over the document at $TX,$TY: the page beside it went $dim to $lit"
else
  bad "9 the tray is not over the document: beside it reads $dim and $lit"
fi
near "9 and takes no grade from it" "$(pixel tray_lit "$TX" "$TY")" "$(pixel tray "$TX" "$TY")" 1
hidden_close

# A design with light in it, on a session of its own.
# From the file the first window saved: a seed that does not parse is dropped
# without a word, and the window opens in Clean. The dials this run moved stay
# behind, so the halo starts unset.
sed -E -e '/^\[theme\]/,/^\[/ s/^id = .*/id = "deco"/' -e '/^doc_/d' \
  "$HIDDEN_SESSION_DIR/$SESSION.toml" > "$HIDDEN_SESSION_DIR/$LIT.toml"
cp "$HIDDEN_SESSION_DIR/$LIT.toml" "$OUT/lit-seed.toml"
SESSION=$LIT
launcher "$OUT/launch-lit.sh"
hidden_launch "$SESSION" "$WS" "$LOG" "sh $OUT/launch-lit.sh"
serve
config '{"crt":false,"warp":0}'
ctl doc here "$OUT/docs/flat.png" >/dev/null
sleep 2
shot lit
locate lit
config '{"phosphor":100}'; shot halo_loud
config '{"phosphor":0}'; shot halo_none
loud=$(pixel halo_loud "$HX" "$HY"); none=$(pixel halo_none "$HX" "$HY")
if [ "$loud" != "$none" ]; then
  ok "10 unset, the halo follows phosphor: $loud against $none"
else
  bad "10 the halo ignored phosphor: $loud against $none"
fi
config '{"doc_glow":100}'; shot halo_own
near "10 its own dial lights it with phosphor at nought" "$(pixel halo_own "$HX" "$HY")" "$loud" 2
config '{"phosphor":100,"doc_glow":0}'; shot halo_off
near "10 and puts it out with phosphor up" "$(pixel halo_off "$HX" "$HY")" "$none" 2

config '{"crt":true,"warp":40}'; shot bent_plain
config '{"doc_brightness":25}'; shot bent_dim
flat=$(pixel halo_off "$X" "$Y"); plain=$(pixel bent_plain "$X" "$Y"); dim=$(pixel bent_dim "$X" "$Y")
if [ "$plain" != "$flat" ]; then
  ok "11 the glass really is bent: $flat flat, $plain through it"
else
  bad "11 the glass did not bend: $plain"
fi
near "11 bent glass keeps the grade" "$dim" "$(half "$plain")" 5

# A wall of text behind the square. Solid, the picture is one flat colour; clear,
# the letters have to come through it.
config '{"crt":false,"warp":0,"doc_brightness":50}'
printf 'yes "the quick brown fox jumps over the lazy dog 0123456789 the quick brown fox" | head -300; exec cat\n' > "$OUT/wall.sh"
ctl adopt --cwd "$OUT" --run "sh $OUT/wall.sh" >/dev/null
sleep 2
ctl doc here "$OUT/docs/flat.png" >/dev/null
sleep 3
shot wall_solid
config '{"doc_clear":50}'; shot wall_clear
solid=$(spread wall_solid); clear=$(spread wall_clear)
ctl doc here "$OUT/docs/page.md" >/dev/null
sleep 3
shot wall_page
if python3 -c 'import sys; sys.exit(0 if float(sys.argv[2]) - float(sys.argv[1]) > .01 else 1)' "$solid" "$clear"; then
  ok "12 the text shows through: spread $solid solid, $clear clear"
else
  bad "12 see-through hides the text: spread $solid solid, $clear clear"
fi

echo
if [ "$fail" -eq 0 ]; then echo "every step held; photographs in $OUT/shots"; else echo "FAILED; photographs in $OUT/shots"; fi
exit "$fail"
