#!/usr/bin/env bash
# Open a real HTML brief in a floating square a hundred times, walk it top to
# bottom and back each time, close it, and count what the GPU keeps — then
# watch the browser TD started go away on its own.
#
# A brief is drawn as tiles, 2,048 device rows each, at most five of them on
# the GPU at once. Scrolling evicts tiles and brings them back; closing the
# square must give every one of them back. Nothing in a unit test can see a
# texture, so this runs the real window against the real GPU, the way
# scripts/doc-float-soak.sh does for an image, and reads the window's GPU
# memory from nvidia-smi after every step.
#
#   scripts/doc-page-soak.sh --brief ABSOLUTE.html [--cycles N] [--bin PATH] [--out DIR] [--idle SECS]
#
# THE WINDOWS ARE HIDDEN. Each is launched onto a special workspace with
# `render_unfocused` and `no_initial_focus`, so nothing appears on screen and
# focus never moves; a window that lands anywhere else is killed at once and
# the run exits 4. NOTHING REACHES THE DESKTOP: `xdg-open` and `uwsm-app` are
# replaced, on the windows' PATH only, by stubs that write down what they were
# asked to open, so the no-Chromium phases below hand the brief to a log
# rather than to a browser on somebody's screen.
#
# Phases:
#   1. soak     — N cycles: open, wait for the window's own "drew", walk the
#                 brief down and up, close, wait for "released". GPU growth
#                 across the cycles after the first must stay under one tile.
#   2. idle     — with TD_PAGE_IDLE_SECS shortened, the browser this window
#                 started must close itself, by pid, and take its profile.
#   2b. beside  — the brief opened in a pane of its own (`doc beside`, what
#                 ctrl+alt+click does) is drawn on that pane's Document face.
#   3. missing  — a window whose documents.toml names a Chromium that is not
#                 there: `doc here` and `doc beside` must answer "desktop …"
#                 with the sentence, and the brief must reach the (stub)
#                 desktop opener, with no square and no pane made.
#   4. broken   — a Chromium that exists and will not start (/bin/false): the
#                 square opens, says why, and hands the brief over.
#
# Exit 0: all four held. 1: one did not. 2: bad arguments. 4: a window was not
# hidden; the run stopped there.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/lib/hidden-window.sh"
CYCLES=100
TD="$ROOT/app/target/release/terminal-delight"
BRIEF=""
OUT="/tmp/doc-page-soak-$(date +%H%M%S)"
IDLE=20
while [ $# -gt 0 ]; do
  case "$1" in
    --cycles) CYCLES=$2; shift 2 ;;
    --bin) TD=$2; shift 2 ;;
    --brief) BRIEF=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    --idle) IDLE=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
case "$BRIEF" in /*) ;; *) echo "--brief takes an absolute path to an .html file"; exit 2 ;; esac
[ -f "$BRIEF" ] || { echo "no brief at $BRIEF"; exit 2; }
command -v nvidia-smi >/dev/null || { echo "nvidia-smi is how this counts GPU memory; not found"; exit 2; }
mkdir -p "$OUT/bin" "$OUT/cache"

# The desktop, replaced for these windows only.
for opener in xdg-open uwsm-app; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/opened.log"\n' "$opener" "$OUT" > "$OUT/bin/$opener"
  chmod +x "$OUT/bin/$opener"
done

CHROMIUMS=""
# Everything this run made goes with it: the windows, their session hosts,
# their lock files, layouts and layout backups (scripts/lib/hidden-window.sh).
# The browsers are the windows' children and go with them; each one's pid is
# checked after.
cleanup() {
  hidden_stop
  left=""
  for _ in $(seq 1 30); do
    left=""
    for c in $CHROMIUMS; do kill -0 "$c" 2>/dev/null && left="$left $c"; done
    [ -z "$left" ] && break
    sleep 0.1
  done
  [ -n "$left" ] && echo "!! chromium this run started is still running:$left"
  hidden_sweep
  # The windows given a config of their own keep their sessions in it.
  for s in $HIDDEN_SESSIONS; do
    rm -f "$OUT"/config-*/terminal-delight/sessions/"$s".*
    rm -rf "$OUT"/config-*/terminal-delight/sessions/backups/"$s"
  done
}
trap cleanup EXIT

# launch <name> <extra env lines...> — a hidden window, its pid in $WIN.
launch() {
  local name=$1; shift
  local session="docpage$$$name" ws="special:tdpage$$"
  local log="$OUT/$name.log" script="$OUT/launch-$name.sh"
  {
    echo "export TD_SESSION=$session TD_DOCDEBUG=1 PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache"
    for line in "$@"; do echo "export $line"; done
    echo "exec $TD > $log 2>&1"
  } > "$script"
  hidden_launch "$session" "$ws" "$log" "sh $script"
  LOG=$log
  echo "   $name: window $WIN, hidden on $ws"
}

gpu() { nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader,nounits \
  | awk -F', ' -v p="$WIN" '$1==p {print $2}' | head -1; }
rss() { awk '/VmRSS/ {print int($2/1024)}' "/proc/$WIN/status"; }
count() { grep -c "\[doc\] $1 " "$LOG" 2>/dev/null || true; }
wait_for() { # wait_for <drew|released> <n> [tenths]
  for _ in $(seq 1 "${3:-150}"); do [ "$(count "$1")" -ge "$2" ] && return 0; sleep 0.1; done
  return 1
}
# Every process under a pid, from /proc.
tree() {
  local root=$1 out=$1 grew=1
  while [ $grew -eq 1 ]; do
    grew=0
    for s in /proc/[0-9]*/stat; do
      local pid ppid
      read -r pid _ _ ppid _ 2>/dev/null < "$s" || continue
      case " $out " in *" $pid "*) continue ;; esac
      case " $out " in *" $ppid "*) out="$out $pid"; grew=1 ;; esac
    done
  done
  echo "$out"
}
tree_rss() { local t=0; for p in $(tree "$1"); do r=$(awk '/VmRSS/ {print int($2/1024)}' "/proc/$p/status" 2>/dev/null); t=$((t + ${r:-0})); done; echo $t; }

fail=0

# ── 1. soak ──────────────────────────────────────────────────────────────────
echo "== 1. soak: $CYCLES cycles of $BRIEF"
launch soak "TD_PAGE_IDLE_SECS=$IDLE"
sleep 2
BASE_GPU=$(gpu); BASE_RSS=$(rss)
echo "   baseline: gpu ${BASE_GPU:-?} MiB, rss ${BASE_RSS} MiB"
CSV="$OUT/samples.csv"
STEPS="$OUT/walk.csv"
echo "cycle,open_gpu_mib,walked_gpu_mib,closed_gpu_mib,closed_rss_mib,chromium_procs,chromium_rss_mib" > "$CSV"
echo "cycle,step,gpu_mib" > "$STEPS"
CHROME=""
for i in $(seq 1 "$CYCLES"); do
  r=$(ctl doc here "$BRIEF")
  case "$r" in ok*) ;; *) echo "   cycle $i: doc here said: $r"; fail=1; break ;; esac
  wait_for drew "$i" || { echo "   cycle $i: the window never drew the brief"; fail=1; break; }
  OPEN=$(gpu)
  # Down the whole brief and back: every band leaves the GPU and comes back.
  # Slowly enough that the window paints between steps, so what is resident
  # is really on the GPU when it is counted.
  step=0
  for dy in $(printf '2400 %.0s' $(seq 1 14)) $(printf -- '-2400 %.0s' $(seq 1 14)); do
    r=$(ctl doc scroll "$dy")
    case "$r" in ok*) ;; *) echo "   cycle $i: doc scroll $dy said: $r"; fail=1 ;; esac
    sleep 0.15
    step=$((step + 1))
    echo "$i,$step,$(gpu)" >> "$STEPS"
  done
  WALKED=$(gpu)
  if [ -z "$CHROME" ]; then
    CHROME=$(grep -m1 -o "\[page\] chromium pid [0-9]*" "$LOG" | awk '{print $4}')
    [ -n "$CHROME" ] && CHROMIUMS="$CHROMIUMS $CHROME"
  fi
  PROCS=0; CRSS=0
  if [ -n "$CHROME" ] && kill -0 "$CHROME" 2>/dev/null; then
    PROCS=$(tree "$CHROME" | wc -w); CRSS=$(tree_rss "$CHROME")
  fi
  r=$(ctl doc close)
  case "$r" in ok*) ;; *) echo "   cycle $i: doc close said: $r"; fail=1; break ;; esac
  wait_for released "$i" || { echo "   cycle $i: the view was never released"; fail=1; break; }
  sleep 0.2
  CLOSED=$(gpu)
  echo "$i,$OPEN,$WALKED,$CLOSED,$(rss),$PROCS,$CRSS" >> "$CSV"
  [ $((i % 10)) -eq 0 ] && echo "   cycle $i: open ${OPEN} MiB, walked ${WALKED} MiB, closed ${CLOSED} MiB, rss $(rss) MiB, chromium ${PROCS} procs ${CRSS} MiB"
done

DREW=$(count drew); RELEASED=$(count released)
TILE_W=$(grep -m1 -o "\[doc\] tile at [0-9]* is [0-9]*x" "$LOG" | grep -o "is [0-9]*" | awk '{print $2}')
TILE_MIB=$(( ${TILE_W:-1280} * 2048 * 4 / 1048576 ))
FIRST=$(awk -F, 'NR==2 {print $4}' "$CSV")
LAST=$(awk -F, 'END {print $4}' "$CSV")
PEAK=$(awk -F, 'NR>1 { if ($2>m) m=$2; if ($3>m) m=$3 } END {print m}' "$CSV")
WALK_PEAK=$(awk -F, 'NR>1 && $3>m {m=$3} END {print m}' "$STEPS")
[ "${WALK_PEAK:-0}" -gt "${PEAK:-0}" ] && PEAK=$WALK_PEAK
LAUNCHES=$(grep -c "\[page\] chromium pid .* profile" "$LOG" || true)
echo "   drew $DREW, released $RELEASED of $CYCLES; one tile is ${TILE_MIB} MiB (${TILE_W:-?} px wide)"
echo "   gpu after the first close ${FIRST:-?} MiB, after the last ${LAST:-?} MiB, peak while open ${PEAK:-?} MiB"
echo "   peak while open is $(( (${PEAK:-0} - ${FIRST:-0}) / (TILE_MIB > 0 ? TILE_MIB : 1) )) tiles' worth above closed"
echo "   chromium launched $LAUNCHES time(s); samples: $CSV"
if [ "$fail" -ne 0 ] || [ "$DREW" -ne "$CYCLES" ] || [ "$RELEASED" -ne "$CYCLES" ]; then
  echo "FAIL 1: not every cycle drew and released"
  fail=1
fi
GROWTH=$(( ${LAST:-0} - ${FIRST:-0} ))
if [ "$GROWTH" -ge "$TILE_MIB" ]; then
  echo "FAIL 1: the GPU grew ${GROWTH} MiB after the first cycle — at least one ${TILE_MIB} MiB tile was kept"
  fail=1
else
  echo "PASS 1: ${GROWTH} MiB of growth across $((CYCLES - 1)) cycles after the first, under one tile (${TILE_MIB} MiB)"
fi

# ── 2. idle ──────────────────────────────────────────────────────────────────
echo "== 2. idle: the browser this window started closes itself after ${IDLE}s"
if [ -z "$CHROME" ]; then
  echo "FAIL 2: no chromium pid in the log"
  fail=1
else
  PROFILE=$(grep -m1 -o "\[page\] chromium pid $CHROME profile .*" "$LOG" | sed 's/.* profile //')
  gone=0
  for _ in $(seq 1 $(( (IDLE + 20) * 10 ))); do
    if ! kill -0 "$CHROME" 2>/dev/null && [ ! -e "$PROFILE" ]; then gone=1; break; fi
    sleep 0.1
  done
  CLOSED_LINE=$(grep -c "\[page\] chromium pid $CHROME closed" "$LOG" || true)
  if [ "$gone" -eq 1 ] && [ "$CLOSED_LINE" -ge 1 ]; then
    echo "PASS 2: chromium $CHROME is gone and $PROFILE is removed, with the window still open"
  else
    echo "FAIL 2: chromium $CHROME alive=$(kill -0 "$CHROME" 2>/dev/null && echo yes || echo no), profile exists=$([ -e "$PROFILE" ] && echo yes || echo no)"
    fail=1
  fi
fi

# ── 2b. beside ───────────────────────────────────────────────────────────────
echo "== 2b. beside: the same brief in a pane of its own, as ctrl+alt+click opens it"
before=$(count drew)
r=$(ctl doc beside "$BRIEF")
if [ "${r%% *}" = "ok" ] && wait_for drew $((before + 1)); then
  echo "PASS 2b: \"$r\", and the Document face drew the brief"
else
  echo "FAIL 2b: doc beside said '$r'; drew lines $before -> $(count drew)"
  fail=1
fi
for c in $(grep -o "\[page\] chromium pid [0-9]*" "$LOG" | awk '{print $4}' | sort -u); do
  case " $CHROMIUMS " in *" $c "*) ;; *) CHROMIUMS="$CHROMIUMS $c" ;; esac
done

# ── 3. missing ───────────────────────────────────────────────────────────────
echo "== 3. missing: no Chromium where documents.toml says"
mkdir -p "$OUT/config-missing/terminal-delight"
printf '[html]\nchromium = "/nonexistent/chromium"\n' > "$OUT/config-missing/terminal-delight/documents.toml"
launch missing "XDG_CONFIG_HOME=$OUT/config-missing"
r=$(ctl doc here "$BRIEF")
sleep 1
case "$r" in
  "desktop No Chromium at /nonexistent/chromium"*"Opened with the desktop.")
    if grep -q -- "$BRIEF" "$OUT/opened.log" 2>/dev/null; then
      echo "PASS 3: said \"${r#desktop }\", and the brief went to the desktop opener"
    else
      echo "FAIL 3: said the right thing but nothing was handed to the desktop"; fail=1
    fi ;;
  *) echo "FAIL 3: doc here said: $r"; fail=1 ;;
esac
[ "$(ctl doc close)" = "err no pane has a floating document open" ] || { echo "FAIL 3: a square was opened anyway"; fail=1; }
handed=$(grep -c -- "$BRIEF" "$OUT/opened.log" 2>/dev/null || echo 0)
r=$(ctl doc beside "$BRIEF")
sleep 1
case "$r" in
  "desktop No Chromium at /nonexistent/chromium"*)
    if [ "$(grep -c -- "$BRIEF" "$OUT/opened.log")" -gt "$handed" ]; then
      echo "PASS 3: the split was refused the same way, before any pane was made"
    else
      echo "FAIL 3: doc beside said the right thing but nothing went to the desktop"; fail=1
    fi ;;
  *) echo "FAIL 3: doc beside said: $r"; fail=1 ;;
esac

# ── 4. broken ────────────────────────────────────────────────────────────────
echo "== 4. broken: a Chromium that will not start"
mkdir -p "$OUT/config-broken/terminal-delight"
printf '[html]\nchromium = "/bin/false"\n' > "$OUT/config-broken/terminal-delight/documents.toml"
before=$(grep -c -- "$BRIEF" "$OUT/opened.log" 2>/dev/null || echo 0)
launch broken "XDG_CONFIG_HOME=$OUT/config-broken"
r=$(ctl doc here "$BRIEF")
said=""
for _ in $(seq 1 150); do
  said=$(grep -m1 "\[doc\] page failed" "$LOG" || true)
  [ -n "$said" ] && break
  sleep 0.1
done
sleep 0.5
after=$(grep -c -- "$BRIEF" "$OUT/opened.log" 2>/dev/null || echo 0)
if [ "${r%% *}" = "ok" ] && [ -n "$said" ] && [ "$after" -gt "$before" ]; then
  echo "PASS 4: the square opened, said \"${said#*: }\", and handed the brief to the desktop"
else
  echo "FAIL 4: doc here said '$r'; failure line '${said}'; handed over $((after - before)) time(s)"
  fail=1
fi

[ "$fail" -eq 0 ] && echo "ALL PASS" || echo "SOMETHING FAILED — logs in $OUT"
exit "$fail"
