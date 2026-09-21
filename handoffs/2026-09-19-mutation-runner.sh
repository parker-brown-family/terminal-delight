#!/usr/bin/env bash
# Round four. The first mutation is the shipped bug itself, put back.
set -u
W=/home/parker/Work/td-pane-textsize
cd "$W/app" || exit 1

pass=0; fail=0
try() {
  local label="$1" filter="$2" file="$3" mut="$4"
  cp "$W/$file" /tmp/gate-backup4.rs
  python3 -c "$mut" "$W/$file" || { echo "  DID-NOT-APPLY $label"; cp /tmp/gate-backup4.rs "$W/$file"; fail=$((fail+1)); return; }
  local out
  out=$(cargo test --bins "$filter" 2>&1)
  if grep -q "test result: FAILED" <<<"$out"; then
    echo "  caught        $label"; pass=$((pass+1))
  elif grep -qE "^error(\[|:)" <<<"$out"; then
    echo "  COMPILE-ERR   $label"; fail=$((fail+1))
  else
    echo "  MISSED        $label  <-- the gate does not hold"; fail=$((fail+1))
  fi
  cp /tmp/gate-backup4.rs "$W/$file"
}

sub() {
  python3 - "$1" "$2" <<'PY'
import sys
old, new = sys.argv[1], sys.argv[2]
print(f'''import sys
p = sys.argv[1]
s = open(p).read()
old = {old!r}
new = {new!r}
assert s.count(old) == 1, f"expected 1 occurrence, found {{s.count(old)}}"
open(p, "w").write(s.replace(old, new))''')
PY
}

BENCH_CHORD='        if self.size_by_wheel(ev, cx) {
            cx.stop_propagation();
            return;
        }
'

echo "== THE SHIPPED BUG, put back =="
try "the bench capture hook stops asking the chord (the bug Parker hit)" every_wheel_handler app/src/pane/bench.rs \
  "$(sub "$BENCH_CHORD" '')"
try "…and the same removal against the ordering gate" the_bench_takes_the_size_chord app/src/pane/bench.rs \
  "$(sub "$BENCH_CHORD" '')"

echo "== the coverage gate =="
try "a fifth wheel handler appears unreviewed" every_wheel_handler app/src/pane.rs \
  "$(sub '    pub fn size_by_wheel(&mut self, ev: &ScrollWheelEvent, cx: &mut Context<Self>) -> bool {' '    pub fn rogue_wheel(&mut self, ev: &ScrollWheelEvent, _cx: &mut Context<Self>) {
        let _ = ev;
    }

    pub fn size_by_wheel(&mut self, ev: &ScrollWheelEvent, cx: &mut Context<Self>) -> bool {')"
try "the pane root stops asking the chord" every_wheel_handler app/src/pane.rs \
  "$(sub '        if self.size_by_wheel(ev, cx) {
            cx.stop_propagation();
            return;
        }' '        if ev.modifiers.control {
            cx.stop_propagation();
            return;
        }')"

echo "== the ordering gate =="
try "the bench asks the chord after its bail" the_bench_takes_the_size_chord app/src/pane/bench.rs \
  "$(sub "$BENCH_CHORD"'        let Some((hit, _)) = self.bench_flat(ev.position) else {
            return;
        };' '        let Some((hit, _)) = self.bench_flat(ev.position) else {
            return;
        };
'"$BENCH_CHORD")"
try "the bench sizes but does not halt" the_bench_takes_the_size_chord app/src/pane/bench.rs \
  "$(sub "$BENCH_CHORD" '        if self.size_by_wheel(ev, cx) {
            return;
        }
')"

echo "== the chord itself =="
try "an unmeasured pane leaks the flick to the cabinet" ctrl_wheel_over_a_pane app/src/pane.rs \
  "$(sub '        if let Some(key) = self.size_dial_under(ev.position) {
            self.nudge_size(key, theme::wheel_notches(ev.delta), cx);
        }
        true' '        if let Some(key) = self.size_dial_under(ev.position) {
            self.nudge_size(key, theme::wheel_notches(ev.delta), cx);
            return true;
        }
        false')"
try "the chord stops resolving a region" ctrl_wheel_over_a_pane app/src/pane.rs \
  "$(sub '        if let Some(key) = self.size_dial_under(ev.position) {
            self.nudge_size(key, theme::wheel_notches(ev.delta), cx);
        }' '        self.nudge_size(theme::GradeKey::TextSize, theme::wheel_notches(ev.delta), cx);')"
try "the pane root halt survives only as a comment" ctrl_wheel_over_a_pane app/src/pane.rs \
  "$(sub '        if self.size_by_wheel(ev, cx) {
            cx.stop_propagation();
            return;
        }' '        if self.size_by_wheel(ev, cx) {
            // cx.stop_propagation();
            return;
        }')"

echo
echo "caught $pass, missed $fail"
git -C "$W" diff --stat | tail -2
