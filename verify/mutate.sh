#!/usr/bin/env bash
# Prove a rule can fail: break the model the way a careless change to the code
# would, and check that the rule now breaks.
#
#   verify/mutate.sh SPEC.tla CONFIG.cfg RULE 'sed expression'
#
# The model is copied; the original is never touched. Exit 0 when RULE breaks
# on the mutant (the check can fail), 1 when it still holds (it cannot).
set -uo pipefail
spec=$1 cfg=$2 rule=$3 expr=$4
dir=$(cd "$(dirname "$spec")" && pwd)
work=$(mktemp -d "${TMPDIR:-/tmp}/mutant.XXXXXX")
trap 'rm -rf "$work"' EXIT
cp "$dir"/* "$work"/
sed -i "$expr" "$work/$(basename "$spec")"
if cmp -s "$dir/$(basename "$spec")" "$work/$(basename "$spec")"; then
  echo "mutation did not apply: $expr" >&2; exit 2
fi
: > "$work/findings.txt"; rm -f "$work"/findings-*.txt
out=$("$(dirname "$0")/check.sh" "$work/$(basename "$spec")" "$work/$(basename "$cfg")")
line=$(grep -E "^  $rule " <<<"$out")
echo "$line"
grep -q BREAKS <<<"$line"
