#!/usr/bin/env bash
# Wait for a pull request's required checks to settle, then say what happened.
#
# Exits 0 when everything concluded SUCCESS, 1 when anything failed, 2 on timeout.
# It reports the conclusion rather than merging: a merge is a decision, and this
# is an instrument.
set -uo pipefail
cd /home/parker/Work/terminal-delight || exit 1
GH=/home/parker/bin/gh
PR="$1"
DEADLINE=$(( $(date +%s) + ${2:-1500} ))

while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  # A check with an empty conclusion has not finished. SKIPPED is a conclusion.
  pending=$($GH pr view "$PR" --json statusCheckRollup \
            --jq '[.statusCheckRollup[] | select((.conclusion // "") == "")] | length' 2>/dev/null)
  bad=$($GH pr view "$PR" --json statusCheckRollup \
        --jq '[.statusCheckRollup[] | select(.conclusion == "FAILURE" or .conclusion == "TIMED_OUT" or .conclusion == "CANCELLED")] | length' 2>/dev/null)

  if [ "${bad:-0}" -gt 0 ]; then
    echo "FAILED"
    $GH pr view "$PR" --json statusCheckRollup \
      --jq '.statusCheckRollup[] | "  \(.name)=\(.conclusion // "pending")"'
    exit 1
  fi
  if [ "${pending:-1}" -eq 0 ]; then
    echo "ALL GREEN"
    $GH pr view "$PR" --json mergeable,mergeStateStatus,statusCheckRollup \
      --jq '"  mergeable=\(.mergeable) state=\(.mergeStateStatus)", (.statusCheckRollup[] | "  \(.name)=\(.conclusion)")'
    exit 0
  fi
  sleep 30
done

echo "TIMED OUT with checks still pending"
$GH pr view "$PR" --json statusCheckRollup --jq '.statusCheckRollup[] | "  \(.name)=\(.conclusion // "pending")"'
exit 2
