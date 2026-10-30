#!/usr/bin/env bash
# Install (or --uninstall) Terminal Delight's agent-session ledger hook into
# Claude Code: puts scripts/td-agent-ledger on ~/.local/bin and wires it as a
# SessionStart + SessionEnd hook in ~/.claude/settings.json (jq merge —
# appends to existing hook arrays, never clobbers them; a timestamped backup
# is kept beside the file). Idempotent.
#
# Usage: scripts/install-recovery-hook.sh [--uninstall]
# Env:   CLAUDE_SETTINGS=path (default ~/.claude/settings.json)
#        DEST=bin dir         (default ~/.local/bin)
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${DEST:-$HOME/.local/bin}"
SETTINGS="${CLAUDE_SETTINGS:-$HOME/.claude/settings.json}"
CMD="$DEST/td-agent-ledger"

command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 1; }

source "$REPO/scripts/lib/uninstall.sh"
if [[ "${1:-}" == "--uninstall" ]]; then
  td_unhook "$SETTINGS" "$CMD"
  td_remove_helper "$CMD" "$REPO/scripts/td-agent-ledger"
  echo "td-agent-ledger unhooked and removed"
  exit 0
fi
[[ $# == 0 ]] || { echo "Usage: $0 [--uninstall]" >&2; exit 2; }

install -Dm755 "$REPO/scripts/td-agent-ledger" "$CMD"
echo "installed $CMD"

mkdir -p "$(dirname "$SETTINGS")"
[[ -f $SETTINGS ]] || echo '{}' >"$SETTINGS"
if grep -q td-agent-ledger "$SETTINGS"; then
  echo "hook already wired in $SETTINGS"
  exit 0
fi
cp "$SETTINGS" "$SETTINGS.td-backup.$(date +%s)"
jq --arg cmd "$CMD" '
  .hooks //= {} |
  .hooks.SessionStart = ((.hooks.SessionStart // []) + [{"hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.SessionEnd = ((.hooks.SessionEnd // []) + [{"hooks": [{"type": "command", "command": $cmd}]}])
' "$SETTINGS" >"$SETTINGS.tmp" && mv "$SETTINGS.tmp" "$SETTINGS"
echo "wired td-agent-ledger into $SETTINGS (SessionStart + SessionEnd; backup kept)"
echo "new agent sessions will now push their ids to the TD ledger"
