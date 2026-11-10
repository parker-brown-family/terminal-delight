#!/usr/bin/env bash
# Check one TLA+ model, every rule on its own, against what is expected.
#
#   verify/check.sh verify/rest-sleep/RestSleep.tla [CONFIG.cfg]
#
# Each INVARIANT in the config is checked by itself with TLC 1.7.4 (pinned:
# mise http:tla2tools@1.7.4, or TLA2TOOLS=/path/to/tla2tools.jar), one worker.
# The model's folder may hold `findings.txt`: one rule per line that the code
# as it stands is KNOWN to break. The result per rule is one of
#   holds             as promised
#   breaks (finding)  a known finding, listed in findings.txt
#   BREAKS            a new break: a bug in the code, or the model drifted
#   FIXED?            listed as a finding but now holds: drop it from the list
# Exit status 1 when anything reads BREAKS or FIXED?, 0 otherwise.
set -uo pipefail
spec=${1:?usage: verify/check.sh SPEC.tla [CONFIG.cfg]}
dir=$(cd "$(dirname "$spec")" && pwd)
name=$(basename "$spec" .tla)
cfg=${2:-$dir/$name.cfg}
jar=${TLA2TOOLS:-}
if [ -z "$jar" ]; then
  jar="$(mise where http:tla2tools@1.7.4 2>/dev/null)/tla2tools.jar"
fi
[ -f "$jar" ] || { echo "no tla2tools.jar: mise install 'http:tla2tools[url=https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar]@1.7.4', or set TLA2TOOLS" >&2; exit 2; }
java=${JAVA:-java}
# A config of its own may keep its own list: findings-<config>.txt.
findings="$dir/findings-$(basename "$cfg" .cfg).txt"
[ -f "$findings" ] || findings="$dir/findings.txt"
[ -f "$findings" ] || findings=/dev/null
work=$(mktemp -d "${TMPDIR:-/tmp}/verify.XXXXXX")
trap 'rm -rf "$work"' EXIT
cp "$dir"/*.tla "$work"/
base=$(grep -vE '^\s*INVARIANT' "$cfg")
rules=$(grep -E '^\s*INVARIANT' "$cfg" | awk '{print $2}')
bad=0
printf '%s (%s)\n' "$name" "$(basename "$cfg")"
for rule in $rules; do
  printf '%s\nINVARIANT %s\n' "$base" "$rule" > "$work/one.cfg"
  out=$(cd "$work" && "$java" -XX:+UseParallelGC -cp "$jar" tlc2.TLC -workers 1 -deadlock \
        -config one.cfg "$name.tla" 2>&1)
  known=$(grep -qx "$rule" "$findings" && echo yes || echo no)
  if grep -q "Invariant $rule is violated" <<<"$out"; then
    if [ "$known" = yes ]; then res="breaks (finding)"; else res="BREAKS"; bad=1; fi
  elif grep -q "Model checking completed. No error has been found" <<<"$out"; then
    if [ "$known" = yes ]; then res="FIXED?"; bad=1; else res="holds"; fi
  else
    res="ERROR"; bad=1
    printf '%s\n' "$out" | tail -5 >&2
  fi
  printf '  %-28s %s\n' "$rule" "$res"
done
exit $bad
