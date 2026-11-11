#!/usr/bin/env bash
# Check every model under verify/, every config each one has, against its
# findings list. The gate CI runs, and the one command an agent runs before
# landing a change to code a model covers.
#
#   verify/check-all.sh
#
# For each folder, each SPEC.tla is checked with every SPEC.cfg and
# SPEC-*.cfg beside it (a -before config keeps the code before a fix, whose
# findings must still break). Exit 1 if any rule reads BREAKS (a new break),
# FIXED? (a finding that now holds: drop it from the list) or ERROR.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
bad=0 runs=0
for spec in "$here"/*/*.tla; do
  dir=$(dirname "$spec"); name=$(basename "$spec" .tla)
  for cfg in "$dir/$name.cfg" "$dir/$name"-*.cfg; do
    [ -f "$cfg" ] || continue
    runs=$((runs + 1))
    "$here/check.sh" "$spec" "$cfg" || bad=1
  done
done
[ "$runs" -gt 0 ] || { echo "no models found under $here" >&2; exit 2; }
echo "checked $runs model configurations"
exit $bad
