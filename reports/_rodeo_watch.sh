#!/usr/bin/env bash
# Wait until the board changes, then rebuild the rodeo checklist and stop.
#
# "The board" is the set of open pull requests plus the tip of main. Five panes are
# finishing at once, so the useful signal is any of: a new pull request, one merging,
# or main moving. Exits 0 on a change (having rebuilt), 2 on timeout with nothing seen.
set -uo pipefail
cd /home/parker/Work/terminal-delight || exit 1

GH=/home/parker/bin/gh
DEADLINE=$(( $(date +%s) + ${1:-1500} ))      # default 25 minutes

snapshot() {
  git fetch origin -q --prune 2>/dev/null
  printf '%s|%s\n' \
    "$(git rev-parse origin/main)" \
    "$($GH pr list --limit 60 --json number --jq '[.[].number]|sort|@csv' 2>/dev/null)"
}

BEFORE=$(snapshot)
echo "watching from: $BEFORE"

while [ "$(date +%s)" -lt "$DEADLINE" ]; do
  sleep 45
  NOW=$(snapshot)
  if [ "$NOW" != "$BEFORE" ]; then
    echo "BOARD CHANGED"
    echo "  was: $BEFORE"
    echo "  now: $NOW"
    python3 reports/_assemble_rodeo.py
    exit 0
  fi
done

echo "no change in the window; board still: $BEFORE"
exit 2
