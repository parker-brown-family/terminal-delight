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
# Then it writes, into fresh copies only:
#
#   - write: every case's edits.json made through the note box's own commands
#     (`ctl doc note add|delete`, `ctl doc concur`), saved with `ctl doc save`,
#     and the written copy held to the skill's expected.html byte for byte —
#     once the times TD stamps (each note's minute, the island's revision) are
#     set aside, since TD writes now and the fixtures a fixed moment. The map
#     afterwards is expected-map.txt, the save's read-back says ✓, and the
#     backup ring holds the copy's bytes from before. The four cases the skill
#     refuses are refused, in words, and their copies are not touched.
#   - on disk: a brief changed under an open window. Its notes alone changed:
#     they are shown, and the page is not drawn again. Its body changed: the
#     page is drawn again and a note waiting to be saved survives it and saves.
#     The file removed: saving is off, and says why.
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
. "$ROOT/scripts/lib/hidden-window.sh"
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

# The window, its session host, its session files and the browser profile it
# left go when the script exits (scripts/lib/hidden-window.sh).
trap hidden_cleanup EXIT

launch() {
  local name=$1
  local session="docnotes$$$name" ws="special:tdnotes$$"
  local log="$OUT/$name.log" script="$OUT/launch-$name.sh"
  {
    echo "export TD_SESSION=$session TD_DOCDEBUG=1 PATH=$OUT/bin:\$PATH XDG_CACHE_HOME=$OUT/cache XDG_STATE_HOME=$OUT/state"
    echo "exec $TD > $log 2>&1"
  } > "$script"
  hidden_launch "$session" "$ws" "$log" "sh $script"
  LOG=$log
  echo "   window $WIN, hidden on $ws"
}

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

# ── write ────────────────────────────────────────────────────────────────────
echo "== write: each case's edits made through the note box's commands, saved, and held to expected.html"
# TD stamps the time it writes; the fixtures a fixed moment. Set both aside.
norm() { sed -E -e 's/data-rev="[^"]*"/data-rev="R"/g' -e 's/"[0-9]{4}-[0-9]{2}-[0-9]{2} [0-9]{2}:[0-9]{2}"/"T"/g' "$1"; }
wait_saved() {
  local j
  for _ in $(seq 1 300); do
    j=$(notes_now) || { echo "$j"; return 1; }
    if [ "$(jq -r .saving <<< "$j")" = false ] && ! jq -r '.said // ""' <<< "$j" | grep -q -- '…'; then
      echo "$j"; return 0
    fi
    sleep 0.1
  done
  echo "timed out waiting for the save"; return 1
}
written=0
for case_dir in "$FIX"/cases/*/; do
  name=$(basename "$case_dir")
  dir="$OUT/writes/$name"
  mkdir -p "$dir"
  cp "$case_dir/brief.html" "$dir/brief.html"
  sum_before=$(sha256sum < "$dir/brief.html")
  refuse=$(jq -r '.refuse // empty' "$case_dir/expect.json")
  r=$(ctl doc here "$dir/brief.html")
  case "$r" in ok*) ;; *) echo "FAIL write $name: doc here said: $r"; fail=1; continue ;; esac
  if ! json=$(notes_now); then echo "FAIL write $name: $json"; fail=1; ctl doc close > /dev/null; continue; fi
  problems=""
  while read -r e; do
    op=$(jq -r .op <<< "$e"); nid=$(jq -r .nid <<< "$e"); text=$(jq -c '.text // empty' <<< "$e")
    case "$op" in
      add) r=$(ctl doc note add "$nid" "$text") ;;
      delete) r=$(ctl doc note delete "$nid" "$text") ;;
      concur|unconcur) r=$(ctl doc concur "$nid") ;;
    esac
    if [ -n "$refuse" ]; then
      case "$r" in err\ *.) ;; *) problems="$problems $op was not refused in words: $r" ;; esac
    else
      case "$r" in ok*) ;; *) problems="$problems $op said: $r" ;; esac
    fi
  done < <(jq -c '.edits[]' "$case_dir/edits.json")
  r=$(ctl doc save)
  if [ -n "$refuse" ]; then
    case "$r" in err\ *) ;; *) problems="$problems save was not refused: $r" ;; esac
    [ "$(sha256sum < "$dir/brief.html")" = "$sum_before" ] || problems="$problems the refused copy changed"
    said="refused: ${r#err }"
  else
    case "$r" in ok*) ;; *) problems="$problems doc save said: $r" ;; esac
    if json=$(wait_saved); then
      said=$(jq -r '.said // ""' <<< "$json")
      [ "$said" = "saved into brief.html ✓" ] || problems="$problems said '$said'"
      [ "$(jq -r .unsaved <<< "$json")" = 0 ] || problems="$problems $(jq -r .unsaved <<< "$json") still unsaved"
      jq -j .map <<< "$json" > "$dir/map.txt"
      cmp -s "$dir/map.txt" "$case_dir/expected-map.txt" || problems="$problems map differs"
      norm "$dir/brief.html" > "$dir/written.norm"
      norm "$case_dir/expected.html" > "$dir/expected.norm"
      cmp -s "$dir/written.norm" "$dir/expected.norm" || problems="$problems bytes differ from expected.html (diff $dir/written.norm $dir/expected.norm)"
      backed=0
      for b in "$OUT"/state/terminal-delight/brief-backups/*/*; do
        [ "$(sha256sum < "$b")" = "$sum_before" ] && backed=1
      done
      [ "$backed" = 1 ] || problems="$problems no backup holds the bytes from before"
      written=$((written + 1))
    else
      problems="$problems $json"
    fi
  fi
  ctl doc close > /dev/null
  if [ -n "$problems" ]; then echo "FAIL write $name:$problems"; fail=1; else echo "   ok $name: $said"; fi
done
echo "   $written briefs written, each byte-identical to the skill's expected.html but for the times"

# ── on disk ──────────────────────────────────────────────────────────────────
echo "== on disk: a brief changed under an open window"
dir="$OUT/ondisk"; mkdir -p "$dir"
case_dir="$FIX/cases/current-pristine"
cp "$case_dir/brief.html" "$dir/brief.html"
laid() { grep -c "\[doc\] page laid out $dir/brief.html" "$LOG" 2>/dev/null || true; }
ctl doc here "$dir/brief.html" > /dev/null
notes_now > /dev/null
drawn=$(laid)
# Another writer changes only the notes: the skill's written copy.
cp "$case_dir/expected.html" "$dir/brief.html"
ok=0
for _ in $(seq 1 50); do
  j=$(notes_now); [ "$(jq -r .notes <<< "$j")" = 2 ] && [ "$(jq -r .concurs <<< "$j")" = 2 ] && { ok=1; break; }
  sleep 0.1
done
if [ "$ok" = 1 ] && [ "$(laid)" = "$drawn" ]; then
  echo "   ok notes changed on disk: 2 notes and 2 concurs shown, the page not drawn again"
else
  echo "FAIL on disk: notes-only change: $(jq -c '{notes,concurs}' <<< "$j"), drawn $drawn -> $(laid)"; fail=1
fi
# A note waits; then the body changes: the page is drawn again, the note survives.
ctl doc note add finding-the-island-is-the "Written before the body changed." > /dev/null
python3 - "$dir/brief.html" <<'PY'
import sys
p = sys.argv[1]
b = open(p, 'rb').read()
i = b.index(b'<body')
j = b.index(b'>', i) + 1
open(p, 'wb').write(b[:j] + b'<p>A paragraph an agent added.</p>' + b[j:])
PY
ok=0
for _ in $(seq 1 100); do
  [ "$(laid)" -gt "$drawn" ] && { ok=1; break; }
  sleep 0.1
done
j=$(notes_now)
if [ "$ok" = 1 ] && [ "$(jq -r .unsaved <<< "$j")" = 1 ]; then
  echo "   ok the body changed: drawn again, and the note waiting to be saved is still waiting"
else
  echo "FAIL on disk: body change: drawn $drawn -> $(laid), unsaved $(jq -r .unsaved <<< "$j")"; fail=1
fi
ctl doc save > /dev/null
j=$(wait_saved)
if [ "$(jq -r .said <<< "$j")" = "saved into brief.html ✓" ] && grep -q "Written before the body changed." "$dir/brief.html" && grep -q "A paragraph an agent added." "$dir/brief.html"; then
  echo "   ok saved over the other writer's change, keeping it"
else
  echo "FAIL on disk: save after the change said $(jq -r .said <<< "$j")"; fail=1
fi
# The file goes: saving is off, and says so.
mv "$dir/brief.html" "$dir/away.html"
ok=0
for _ in $(seq 1 30); do
  j=$(notes_now); [ "$(jq -r .gone <<< "$j")" = true ] && { ok=1; break; }
  sleep 0.1
done
r=$(ctl doc note add finding-the-island-is-the "Into a file that is gone.")
if [ "$ok" = 1 ] && case "$r" in "err The file is no longer on disk"*) true ;; *) false ;; esac; then
  echo "   ok the file removed: saving is off — \"${r#err }\""
else
  echo "FAIL on disk: gone=$(jq -r .gone <<< "$j"), add said: $r"; fail=1
fi
mv "$dir/away.html" "$dir/brief.html"
ctl doc close > /dev/null

[ "$fail" -eq 0 ] && echo "ALL PASS ($checked briefs read, $written written)" || echo "SOMETHING FAILED — logs in $OUT"
exit "$fail"
