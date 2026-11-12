#!/usr/bin/env bash
# Print the numbers the docs page /verification states, so each one can be
# measured again instead of trusted.
#
#   scripts/verification-numbers.sh            # the tree, then the last 30 days
#   scripts/verification-numbers.sh --days 90
#
# The tree numbers come from this checkout. The activity numbers come from
# GitHub through `gh`, and are left out, with a line saying so, when `gh` is
# missing or not signed in: an absent number is never printed as zero.
set -euo pipefail
days=30
[ "${1:-}" = "--days" ] && days=${2:?--days needs a number}
root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
repo=parker-brown-family/terminal-delight

echo "tree at $(git rev-parse --short HEAD)"
echo "  rust tests:            $(git grep -cE '#\[(tokio::)?test\]' -- '*.rs' | awk -F: '{s+=$NF} END {print s}')"
echo "  folders with findings: $(find verify -mindepth 2 -maxdepth 2 -name findings.txt | wc -l)"
echo "  models:                $(find verify -name '*.tla' | wc -l)"
echo "  check scripts:         $(find scripts -maxdepth 2 -type f \( -name '*check*' -o -name '*smoke*' -o -name '*test*' \) | wc -l)"

if ! command -v gh >/dev/null 2>&1 || ! gh auth status >/dev/null 2>&1; then
  echo "last $days days: not measured (gh is missing or not signed in)"
  exit 0
fi
since=$(date -u -d "$days days ago" +%Y-%m-%d)
echo "last $days days, since $since"
gh pr list -R "$repo" --state merged --search "merged:>=$since" --limit 1000 \
  --json baseRefName,createdAt,mergedAt --jq '
  def hours: ((.mergedAt | fromdate) - (.createdAt | fromdate)) / 3600;
  "  merged changes:        \(length) (\([.[] | select(.baseRefName == "main")] | length) into main, \([.[] | select(.baseRefName | startswith("dev/"))] | length) into a release branch)",
  "  median hours open:     \([.[] | hours] | sort | .[(length / 2 | floor)] * 10 | round / 10)"'
gh run list -R "$repo" --branch main --workflow CI --limit 500 --json conclusion,createdAt --jq "
  [.[] | select(.createdAt >= \"$since\") | select(.conclusion != null and .conclusion != \"\")] as \$r
  | \"  CI runs on main:       \(\$r | length), \([\$r[] | select(.conclusion == \"success\")] | length) green\""
echo "  revert commits on main: $(git log origin/main --since="$since" --oneline -i --grep='^Revert' 2>/dev/null | wc -l)"
gh release list -R "$repo" --limit 100 --json publishedAt,isPrerelease --jq "
  [.[] | select(.publishedAt >= \"$since\")] as \$r
  | \"  releases:              \(\$r | length), \([\$r[] | select(.isPrerelease | not)] | length) stable\""
