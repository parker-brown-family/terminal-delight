#!/usr/bin/env bash
# Print the numbers the docs page /verification states, so each one can be
# measured again instead of trusted.
#
#   scripts/verification-numbers.sh            # the tree, then the last 30 days
#   scripts/verification-numbers.sh --days 90
#
# The tree numbers come from this checkout. The activity numbers come from
# GitHub through `gh` and from origin/main. A source that cannot be read, or a
# list that came back cut off at its cap, prints `not measured` or
# `incomplete` on that line and the script carries on: an absent number is
# never printed as zero, and a partial one is never printed as a total.
set -uo pipefail
days=30
[ "${1:-}" = "--days" ] && days=${2:?--days needs a number}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root" || exit 1
repo=parker-brown-family/terminal-delight
PR_CAP=1000 RUN_CAP=1000 RELEASE_CAP=200

count() { git grep -cE "$1" -- '*.rs' 2>/dev/null | awk -F: '{s+=$NF} END {print s+0}'; }

echo "tree at $(git rev-parse --short HEAD)"
# Declarations in the source, not tests run: a normal `cargo test` skips the
# ignored ones, and CI runs only the Restricted suite's.
echo "  test declarations:     $(count '#\[(tokio::)?test\]') ($(count '#\[ignore') marked ignored)"
echo "  folders with findings: $(find verify -mindepth 2 -maxdepth 2 -name findings.txt | wc -l)"
echo "  models:                $(find verify -name '*.tla' | wc -l)"

since=$(date -u -d "$days days ago" +%Y-%m-%d)
echo "last $days days, since $since"

if git rev-parse --verify --quiet origin/main >/dev/null; then
  echo "  revert commits on main: $(git log origin/main --since="$since" --oneline -i --grep='^Revert' | wc -l) (as of the last fetch)"
else
  echo "  revert commits on main: not measured (no origin/main in this checkout)"
fi

if ! command -v gh >/dev/null 2>&1 || ! gh auth status >/dev/null 2>&1; then
  echo "  merged changes:        not measured (gh is missing or not signed in)"
  echo "  CI runs on main:       not measured (gh is missing or not signed in)"
  echo "  releases:              not measured (gh is missing or not signed in)"
  exit 0
fi

# label, cap, the command that fetches a JSON list, the jq that formats it.
measure() {
  local label=$1 cap=$2 format=$4 json n
  if ! json=$(eval "$3" 2>/dev/null) || ! n=$(jq 'length' <<<"$json" 2>/dev/null); then
    printf '  %-22s not measured (the query failed)\n' "$label"; return
  fi
  if [ "$n" -ge "$cap" ]; then
    printf '  %-22s incomplete (the list reached its cap of %s; use fewer days)\n' "$label" "$cap"; return
  fi
  printf '  %-22s %s\n' "$label" "$(jq -r "$format" <<<"$json")"
}

# The jq programs below are meant literally, so their $ and \( stay unexpanded.
# shellcheck disable=SC2016
measure "merged changes:" "$PR_CAP" \
  "gh pr list -R $repo --state merged --search 'merged:>=$since' --limit $PR_CAP --json baseRefName,createdAt,mergedAt" \
  '"\(length) (\([.[] | select(.baseRefName == "main")] | length) into main, \([.[] | select(.baseRefName | startswith("dev/"))] | length) into a development branch); half open under \(([.[] | ((.mergedAt | fromdate) - (.createdAt | fromdate)) / 60] | sort | .[(length / 2 | floor)] // 0) | round) minutes"'
# shellcheck disable=SC2016
measure "CI runs on main:" "$RUN_CAP" \
  "gh run list -R $repo --branch main --workflow CI --created '>=$since' --limit $RUN_CAP --json conclusion" \
  '[.[] | select(.conclusion != null and .conclusion != "")] as $r | "\($r | length) finished, \([$r[] | select(.conclusion == "success")] | length) green"'
measure "releases:" "$RELEASE_CAP" \
  "gh release list -R $repo --limit $RELEASE_CAP --json publishedAt,isPrerelease" \
  "[.[] | select(.publishedAt >= \"$since\")] as \$r | \"\(\$r | length), \([\$r[] | select(.isPrerelease | not)] | length) stable\""
