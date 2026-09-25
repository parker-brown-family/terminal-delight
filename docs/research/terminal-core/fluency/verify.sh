#!/usr/bin/env bash
# Re-run one agent's own probe and check it, without trusting its report.
#   bash verify.sh <workspace>
# Keeps the agent's output.json as output.agent.json, runs the probe binary the agent built
# (from inside the workspace, where icat.bin and text.bin sit), and checks the fresh output
# against ground-truth.json with check.py. No rebuild: a libghostty-vt rebuild would clone
# Ghostty again, and the binary on disk is the agent's own program.
set -uo pipefail
ws="$1"
here="$(cd "$(dirname "$0")" && pwd)"
cd "$ws" || exit 1
[ -f output.agent.json ] || cp output.json output.agent.json
bin=$(find "$ws" -path '*/release/probe' -type f -perm -u+x | head -1)
echo "binary: ${bin:-none found}"
[ -n "$bin" ] || exit 1
tmp=$(mktemp -d)
( cd "$ws" && "$bin" ) > "$tmp/stdout.json" 2> "$tmp/stderr.txt"
echo "exit $?"
# The probe may print its JSON, write output.json itself, or both.
if python3 "$here/is_json.py" "$tmp/stdout.json"; then
  cp "$tmp/stdout.json" "$tmp/output.json"
else
  cp "$ws/output.json" "$tmp/output.json"
fi
cp "$tmp/output.json" "$ws/output.rerun.json"
cmp -s "$ws/output.rerun.json" "$ws/output.agent.json" && echo "re-run identical to the agent's output" || echo "re-run differs from the agent's output"
python3 "$here/check.py" "$tmp"
