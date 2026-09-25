#!/usr/bin/env bash
# Open every brief in the decision-brief skill's shared notes fixtures in a
# real window, read what TD's notes layer shows through `ctl doc notes`, and
# hold it to what a browser showed from the same bytes — then prove that not
# one of those files was written.
#
# The fixtures are vendored at app/tests/fixtures/decision-brief/notes-format.
# For every case, its brief.html and (where the skill writes one) its
# expected.html are copied into this run's own directory, each as brief.html
# so the page's default NOTES_FILE is the one the skill recorded. The window
# opens each copy as a floating square (`ctl doc here`, what Alt+click does),
# waits for the page to be laid out, and asks the notes layer what it shows:
#
#   - the state: notes, or why the page is read-only (no island, an island
#     no browser can read, an island after the notes script);
#   - the note count and the concur count the bar shows, against the counts
#     a fresh browser's notebar showed (expect.json, shown_before/after);
#   - for every written brief, the map copy map would put on the clipboard,
#     against expected-map.txt: the text the brief's own exportNotes() gave.
#
# Afterwards every copy's sha256 and modification time must be what they were.
#
#   scripts/doc-notes-check.sh [--bin PATH] [--out DIR]
#
# THE WINDOW IS HIDDEN: launched onto a special workspace with
# `render_unfocused` and `no_initial_focus`, so nothing appears on screen and
# focus never moves; a window that lands anywhere else is killed at once and
# the run exits 4. It never touches a brief outside this run's directory, and
# its page cache is this run's too. Everything it made goes when it exits: the
# window, its session host, its session files, the browser it started.
#
# Exit 0: every case held. 1: one did not. 2: bad arguments. 4: not hidden.
set -uo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
TD="$ROOT/app/target/release/terminal-delight"
OUT="/tmp/doc-notes-check-$(date +%H%M%S)"
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) TD=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    *) echo "unknown argument $1"; exit 2 ;;
  esac
done
[ -x "$TD" ] || { echo "no binary at $TD — cargo build --release first"; exit 2; }
FIX="$ROOT/app/tests/fixtures/decision-brief/notes-format"
[ -d "$FIX/cases" ] || { echo "no fixtures at $FIX"; exit 2; }
command -v jq >/dev/null || { echo "jq is how this reads the answers; not found"; exit 2; }
mkdir -p "$OUT/bin" "$OUT/cache" "$OUT/state" "$OUT/copies"

# Nothing may reach the desktop from here: a page TD cannot draw would be
# handed to xdg-open, so these windows get a stub that writes it down.
for opener in xdg-open uwsm-app; do
  printf '#!/bin/sh\necho "%s $*" >> "%s/opened.log"\n' "$opener" "$OUT" > "$OUT/bin/$opener"
  chmod +x "$OUT/bin/$opener"
done

WINDOWS=""
SESSIONS=""
cleanup() {
  local pids s
  pids="$WINDOWS"
  for s in $SESSIONS; do pids="$pids $(pgrep -f "serve --session $s" 2>/dev/null)"; done
  for p in $pids; do kill "$p" 2>/dev/null; done
  for _ in $(seq 1 50); do
    alive=0
    for p in $pids; do kill -0 "$p" 2>/dev/null && alive=1; done
    [ "$alive" -eq 0 ] && break
    sleep 0.1
  done
  for s in $SESSIONS; do
    rm -f "$HOME/.config/terminal-delight/sessions/$s".*
    rm -rf "$HOME/.config/terminal-delight/sessions/backups/$s"
  done
  for w in $WINDOWS; do rm -rf "${XDG_RUNTIME_DIR:-/nonexistent}/terminal-delight/chromium-$w-"*; done
}
trap cleanup EXIT

launch() {
  local name=$1
  local session="docnotes$$$name" ws="special:tdnotes$$"
  local log="$OUT/$name.log" script="$OUT/launch-$name.sh"
  {
    echo "export TD_SESSION=$session TD_DOCDEBUG=1 PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state"
    echo "exec $TD > $log 2>&1"
  } > "$script"
  SESSIONS="$SESSIONS $session"
  local said
  said=$(hyprctl dispatch "hl.dsp.exec_cmd(\"sh $script\", { workspace = \"$ws silent\", render_unfocused = true, no_initial_focus = true })" 2>&1)
  case "$said" in ok|"") ;; *) echo "hyprctl refused the launch: $said"; exit 1 ;; esac
  WIN=""
  for _ in $(seq 1 60); do
    WIN=$(hyprctl clients -j 2>/dev/null | jq -r --arg t "terminal-delight — $session" '.[] | select(.title==$t) | .pid' | head -1)
    [ -n "$WIN" ] && break
    sleep 0.5
  done
  [ -n "$WIN" ] || { echo "no window appeared — see $log"; exit 1; }
  WINDOWS="$WINDOWS $WIN"
  local landed
  landed=$(hyprctl clients -j | jq -r --argjson p "$WIN" '.[] | select(.pid==$p) | .workspace.name' | head -1)
  if [ "$landed" != "$ws" ]; then
    echo "the window landed on '$landed', not $ws — killed, nothing ran"
    kill "$WIN" 2>/dev/null
    exit 4
  fi
  LOG=$log
  for _ in $(seq 1 40); do [ "$(ctl ping)" = "pong" ] && break; sleep 0.5; done
  echo "   window $WIN, hidden on $landed"
}

ctl() { "$TD" ctl --pid "$WIN" "$@" 2>&1 | head -1 | sed 's/^[0-9]*\t//'; }
# The JSON after the pane's answer, once the page has been laid out.
notes_now() {
  local r
  for _ in $(seq 1 200); do
    r=$(ctl doc notes)
    case "$r" in
      ok*"{"*) echo "{${r#*\{}"; return 0 ;;
      "err the document is not a laid-out HTML page yet") sleep 0.1 ;;
      *) echo "$r"; return 1 ;;
    esac
  done
  echo "timed out waiting for the page"; return 1
}
fingerprint() { echo "$(sha256sum "$1" | cut -d" " -f1) $(stat -c %y "$1")"; }

# ── copies ───────────────────────────────────────────────────────────────────
echo "== copies of the shared fixtures in $OUT/copies"
for case in "$FIX"/cases/*/; do
  name=$(basename "$case")
  for file in brief expected; do
    [ -f "$case/$file.html" ] || continue
    mkdir -p "$OUT/copies/$name-$file"
    cp "$case/$file.html" "$OUT/copies/$name-$file/brief.html"
    touch -d '1 minute ago' "$OUT/copies/$name-$file/brief.html"
    fingerprint "$OUT/copies/$name-$file/brief.html" > "$OUT/copies/$name-$file/before"
  done
done
echo "   $(ls "$OUT/copies" | wc -l) briefs"

# ── read ─────────────────────────────────────────────────────────────────────
echo "== read: what the notes layer shows, against what a browser showed"
launch read
fail=0
checked=0
for dir in "$OUT"/copies/*/; do
  id=$(basename "$dir")
  name=${id%-*}
  file=${id##*-}
  case_dir="$FIX/cases/$name"
  shown=$([ "$file" = brief ] && echo shown_before || echo shown_after)
  r=$(ctl doc here "$dir/brief.html")
  case "$r" in ok*) ;; *) echo "FAIL $id: doc here said: $r"; fail=1; continue ;; esac
  if ! json=$(notes_now); then
    echo "FAIL $id: $json"; fail=1; ctl doc close > /dev/null; continue
  fi
  echo "$json" > "$dir/notes.json"
  state=$(jq -r .state <<< "$json")
  want_state=notes
  case "$name" in
    no-island) want_state=no-island ;;
    unreadable-island) want_state=unreadable ;;
    island-after-script) want_state=after-script ;;
  esac
  problems=""
  [ "$state" = "$want_state" ] || problems="$problems state=$state (want $want_state)"
  if [ "$state" = notes ]; then
    got_n=$(jq -r .notes <<< "$json")
    want_n=$(jq -r ".$shown.note_count" "$case_dir/expect.json")
    [ "$got_n" = "$want_n" ] || problems="$problems notes=$got_n (browser $want_n)"
    got_c=$(jq -r .concurs <<< "$json")
    want_c=$(jq -r ".$shown.concur_count" "$case_dir/expect.json")
    [ "$got_c" = "$want_c" ] || problems="$problems concurs=$got_c (browser $want_c)"
  else
    [ "$(jq -r .read_only <<< "$json")" != null ] || problems="$problems read-only without saying why"
    [ "$(jq -r .map <<< "$json")" = null ] || problems="$problems a map from a page that shows no notes"
  fi
  if [ "$file" = expected ]; then
    jq -j .map <<< "$json" > "$dir/map.txt"
    cmp -s "$dir/map.txt" "$case_dir/expected-map.txt" || problems="$problems map differs (see $dir/map.txt)"
  fi
  r=$(ctl doc close)
  case "$r" in ok*) ;; *) problems="$problems doc close said: $r" ;; esac
  if [ -n "$problems" ]; then
    echo "FAIL $id:$problems"; fail=1
  else
    echo "   ok $id: $state$([ "$state" = notes ] && echo " · $(jq -r .notes <<< "$json") notes · $(jq -r .concurs <<< "$json") concurs")$([ "$file" = expected ] && echo ' · map identical')"
  fi
  checked=$((checked + 1))
done

# ── bytes ────────────────────────────────────────────────────────────────────
echo "== bytes: every copy as it was before the window opened it"
moved=0
for dir in "$OUT"/copies/*/; do
  now=$(fingerprint "$dir/brief.html")
  if [ "$now" != "$(cat "$dir/before")" ]; then
    echo "FAIL $(basename "$dir"): the file changed: $(cat "$dir/before") -> $now"
    moved=$((moved + 1)); fail=1
  fi
done
[ "$moved" -eq 0 ] && echo "   not one of $checked files changed: same sha256, same modification time"
[ -s "$OUT/opened.log" ] && { echo "FAIL something went to the desktop:"; cat "$OUT/opened.log"; fail=1; }

[ "$fail" -eq 0 ] && echo "ALL PASS ($checked briefs)" || echo "SOMETHING FAILED — logs in $OUT"
exit "$fail"
