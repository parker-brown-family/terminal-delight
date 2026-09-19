#!/usr/bin/env bash
# Is EVERYTHING in? The exhaustive version — every place work can hide.
#
# Answers, separately, each of: open pull requests, dirty worktrees, detached heads,
# unpushed commits, local branches with no remote, remote branches holding work main
# does not have (patch-equivalence, not commit count), untracked files, stashes,
# installed binaries nobody is running, and whether the live windows are current.
set -uo pipefail
R=/home/parker/Work/terminal-delight
GH=/home/parker/bin/gh
cd "$R" || exit 1
git fetch origin -q --prune

echo "════ 1. OPEN PULL REQUESTS ════"
n=$($GH pr list --limit 50 --json number --jq 'length')
if [ "$n" = "0" ]; then echo "  none"; else
  $GH pr list --limit 50 --json number,headRefName,mergeStateStatus \
    --jq '.[] | "  #\(.number) \(.headRefName) [\(.mergeStateStatus)]"'
fi

echo
echo "════ 2. WORKTREES — dirty, detached, or ahead ════"
printf "  %-22s %-44s %5s %6s %5s %s\n" TREE BRANCH AHEAD BEHIND DIRTY UNTRACKED
git worktree list --porcelain | awk '/^worktree /{w=$2} /^branch /{print w, $2} /^detached/{print w, "DETACHED"}' |
while read -r w b; do
  br=${b#refs/heads/}
  if [ "$br" = "DETACHED" ]; then a="-"; bh="-"; else
    a=$(git rev-list --count origin/main.."$br" 2>/dev/null || echo "?")
    bh=$(git rev-list --count "$br"..origin/main 2>/dev/null || echo "?")
  fi
  d=$(git -C "$w" status --porcelain -uno 2>/dev/null | wc -l)
  u=$(git -C "$w" status --porcelain 2>/dev/null | grep -c '^??')
  flag=""
  [ "$d" != "0" ] && flag="$flag DIRTY"
  [ "$br" = "DETACHED" ] && flag="$flag DETACHED"
  [ "${a:-0}" != "0" ] && [ "$a" != "-" ] && flag="$flag AHEAD"
  printf "  %-22s %-44s %5s %6s %5s %-4s%s\n" "$(basename "$w")" "$br" "$a" "$bh" "$d" "$u" "$flag"
done

echo
echo "════ 3. LOCAL BRANCHES WITH UNPUSHED COMMITS ════"
out=$(git for-each-ref --format='%(refname:short) %(upstream:short) %(upstream:track)' refs/heads/ | awk '$3 ~ /ahead/')
[ -z "$out" ] && echo "  none" || echo "$out" | while read -r l; do
  br=$(echo "$l" | awk '{print $1}')
  printf "  %-46s %s  (in main already: %s)\n" "$br" "$(echo "$l" | cut -d' ' -f3-)" \
    "$(git merge-base --is-ancestor "$br" origin/main && echo YES || echo NO)"
done

echo
echo "════ 4. LOCAL BRANCHES WITH NO REMOTE AT ALL ════"
out=$(git for-each-ref --format='%(refname:short) %(upstream:short)' refs/heads/ | awk 'NF==1 {print $1}')
[ -z "$out" ] && echo "  none" || echo "$out" | while read -r br; do
  a=$(git rev-list --count origin/main.."$br" 2>/dev/null)
  printf "  %-46s +%s  %s\n" "$br" "${a:-?}" "$(git log -1 --format=%cr "$br")"
done

echo
echo "════ 5. REMOTE BRANCHES HOLDING WORK MAIN DOES NOT HAVE ════"
echo "       (git cherry — NEW counts only commits main lacks by patch, not by sha)"
found=0
for r in $(git for-each-ref --format='%(refname:short)' refs/remotes/origin/ | grep -v 'origin/HEAD\|origin/main'); do
  cnt=$(git rev-list --count origin/main.."$r" 2>/dev/null); [ "${cnt:-0}" -eq 0 ] && continue
  new=$(git cherry origin/main "$r" 2>/dev/null | grep -c '^+')
  [ "$new" -eq 0 ] && continue
  found=1
  printf "  %-48s NEW=%-3s %s\n" "${r#origin/}" "$new" "$(git log -1 --format=%cr "$r")"
done
[ "$found" = "0" ] && echo "  none — every remote branch is patch-equivalent to main"

echo
echo "════ 6. STASHES ════"
s=$(git stash list | wc -l); [ "$s" = "0" ] && echo "  none" || git stash list | sed 's/^/  /'

echo
echo "════ 7. INSTALLED BINARIES ════"
live=$(basename "$(readlink -f /home/parker/.local/bin/terminal-delight)")
echo "  launcher points at: $live"
echo "  main is:            td-$(git rev-parse --short origin/main)-*"
ls -1t /home/parker/.local/lib/terminal-delight/ 2>/dev/null | head -12 | while read -r f; do
  [ "$f" = "$live" ] && echo "    $f   <- LIVE" || echo "    $f"
done
tot=$(ls -1 /home/parker/.local/lib/terminal-delight/ 2>/dev/null | wc -l)
sz=$(du -sh /home/parker/.local/lib/terminal-delight/ 2>/dev/null | cut -f1)
echo "  $tot binaries, $sz total"

echo
echo "════ 8. RUNNING WINDOWS ════"
for p in $(pgrep -f 'terminal-delight' 2>/dev/null); do
  exe=$(readlink -f "/proc/$p/exe" 2>/dev/null) || continue
  case "$exe" in *terminal-delight*) ;; *) continue ;; esac
  args=$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null)
  case "$args" in *" mcp "*|*" serve "*|*" ctl "*) continue ;; esac
  printf "  pid %-8s %s\n" "$p" "$(basename "$exe")"
done

echo
echo "════ 9. UNTRACKED FILES IN THE MAIN WORKTREE ════"
u=$(git status --porcelain | grep '^??' | wc -l)
[ "$u" = "0" ] && echo "  none" || git status --porcelain | grep '^??' | sed 's/^/  /'
