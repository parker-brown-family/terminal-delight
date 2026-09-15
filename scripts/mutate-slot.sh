#!/usr/bin/env bash
# Prove the slot's tests can fail — with a rig that can tell a compile error
# apart from a green suite.
#
# Two rig bugs found the hard way, and both are the same bug this widget is
# about. First: `\|` is alternation in sed's BRE, so a pattern containing a
# literal `||` silently matched nothing and the run reported SURVIVED for a
# mutation it never made. Second, worse: the pass/fail verdict was read from a
# `N failed` line, and a mutation that does not COMPILE prints no such line —
# so absence of a failure count was scored as zero failures. Unknown is not
# zero, in a test harness as much as in a gauge.
set -u
cd "$(dirname "$0")/../app" || exit 1
SRC=src/slot.rs
LOG=${LOG:-/tmp/mutate-slot.out}
BAK=$(mktemp)
cp "$SRC" "$BAK"
trap 'cp "$BAK" "$SRC"; rm -f "$BAK"' EXIT
: > "$LOG"

mutate() {
  local name="$1" find="$2" repl="$3"
  cp "$BAK" "$SRC"
  if ! grep -qF -- "$find" "$SRC"; then
    echo "RIG FAIL   pattern absent — mutation never applied: $name"
    return
  fi
  FIND="$find" REPL="$repl" SRC="$SRC" python3 -c '
import os
p = os.environ["SRC"]
s = open(p).read()
f, r = os.environ["FIND"], os.environ["REPL"]
assert f in s
open(p, "w").write(s.replace(f, r, 1))
'
  if cmp -s "$BAK" "$SRC"; then
    echo "RIG FAIL   file unchanged after edit: $name"
    return
  fi

  local out rc
  out=$(cargo test --quiet slot:: 2>&1); rc=$?
  { echo "########## $name (exit $rc)"; echo "$out"; echo; } >> "$LOG"

  if ! printf '%s' "$out" | grep -q 'test result:'; then
    # No suite ran at all. That is a broken mutation, not a surviving one.
    echo "NO BUILD   mutation does not compile, says nothing about the tests: $name"
    printf '%s' "$out" | grep -E '^error' | head -2 | sed 's/^/             /'
    cp "$BAK" "$SRC"
    return
  fi

  local n
  n=$(printf '%s' "$out" | grep -oE '[0-9]+ failed' | head -1 | grep -oE '[0-9]+')
  if [ "${n:-0}" -gt 0 ]; then
    echo "CAUGHT     ($n failed)  $name"
    printf '%s' "$out" | grep -E '^ +slot::' | sed 's/^/             /' | head -4
  else
    echo "SURVIVED   suite green — this rule is untested: $name"
  fi
  cp "$BAK" "$SRC"
}

echo "=== baseline ==="
cargo test --quiet slot:: 2>&1 | grep -E '^test result' | head -1
echo
echo "=== mutations ==="

mutate "the collector's own verdict is ignored (expired sign-in reads as current)" \
  'let mute = !rec.ready || !rec.status_text.is_empty();' \
  'let mute = false;'

mutate "nothing is ever stale (a rolled-over window reads as current)" \
  '(Some(age), Some(h)) if age > h => Reading::Stale {' \
  '(Some(age), Some(h)) if age > h * 1000.0 => Reading::Stale {'

mutate "an absent updatedAt is assumed to be current" \
  'Reading::Stale {
                    spent: l.percent,
                    age: "?".into(),
                }' \
  'Reading::Fresh(l.percent)'

mutate "the week borrows the session's number when it has none" \
  'rec.limits.get(1)' \
  'rec.limits.first()'

echo
echo "=== restored ==="
cargo test --quiet slot:: 2>&1 | grep -E '^test result' | head -1
echo "full output: $LOG"
