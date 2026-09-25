#!/usr/bin/env bash
# Repository facts for the build-quality comparison, the same queries for each repository.
#   bash repo_facts.sh > repo-facts.txt
set -uo pipefail
for repo in raphamorim/rio ghostty-org/ghostty Uzaaft/libghostty-rs; do
  echo "=== $repo"
  echo "-- default branch, pushed, stars, open issues"
  gh api "repos/$repo" --jq '[.default_branch, .pushed_at, .stargazers_count, .open_issues_count] | @tsv'
  echo "-- workflows"
  gh api "repos/$repo/actions/workflows" --jq '.workflows[] | [.name, .path, .state] | @tsv' 2>/dev/null | head -30
  echo "-- top-level entries"
  gh api "repos/$repo/contents" --jq '.[].name' 2>/dev/null | tr '\n' ' '; echo
  echo "-- agent and AI files at the root"
  for f in AGENTS.md CLAUDE.md AI_POLICY.md .cursorrules CONTRIBUTING.md; do
    gh api "repos/$repo/contents/$f" --jq '.name' 2>/dev/null
  done
  echo "-- last 100 commits: trailers naming an AI tool"
  gh api "repos/$repo/commits?per_page=100" --jq '.[].commit.message' 2>/dev/null \
    | grep -ioE 'co-authored-by: *(claude|codex|copilot|cursor|gpt|gemini)[^<]*|generated with \[?(claude|codex)[^]]*' | sort | uniq -c | sort -rn | head
  echo
done
