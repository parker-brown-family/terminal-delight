#!/usr/bin/env bash
# Link each verify/*/SKILL.md into .claude/skills/ (gitignored) so Claude Code
# finds them as project skills. Safe to re-run.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$root/.claude/skills"
for skill in "$root"/verify/*/SKILL.md; do
  dir=$(dirname "$skill")
  name=$(sed -n 's/^name: //p' "$skill" | head -1)
  [ -n "$name" ] || continue
  ln -sfn "$dir" "$root/.claude/skills/$name"
  echo "linked $name"
done
