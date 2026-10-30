#!/usr/bin/env bash
# Install (or --uninstall) Terminal Delight's agent-session ledger hook into
# Claude Code: puts scripts/td-agent-ledger on ~/.local/bin and wires it as a
# SessionStart + SessionEnd hook in ~/.claude/settings.json (jq merge —
# appends to existing hook arrays, never clobbers them; a timestamped backup
# is kept under XDG_STATE_HOME). Idempotent.
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
[[ $# == 0 || ( $# == 1 && $1 == --uninstall ) ]] || { echo "Usage: $0 [--uninstall]" >&2; exit 2; }
td_receipt_init recovery-hook
if [[ "${1:-}" == "--uninstall" ]]; then
  td_unhook "$SETTINGS" "$CMD"
  td_remove_helper "$CMD" "$REPO/scripts/td-agent-ledger"
  td_receipt_remove
  echo "td-agent-ledger unhooked and removed"
  exit 0
fi
mkdir -p "$(dirname "$SETTINGS")"
[[ ! -L $SETTINGS ]] || { echo "Refusing symlink settings: $SETTINGS" >&2; exit 1; }
[[ -f $SETTINGS ]] || echo '{}' >"$SETTINGS"
td_edit_begin "$SETTINGS"
jq -es 'length == 1 and (.[0] | type == "object")' "$TD_EDIT_SNAPSHOT" >/dev/null
if grep -q td-agent-ledger "$SETTINGS"; then
  echo "hook already wired in $SETTINGS"
else
jq --arg cmd "$CMD" '
  .hooks //= {} |
  .hooks.SessionStart = ((.hooks.SessionStart // []) + [{"hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.SessionEnd = ((.hooks.SessionEnd // []) + [{"hooks": [{"type": "command", "command": $cmd}]}])
' "$TD_EDIT_SNAPSHOT" > "$TD_EDIT_STAGE"
td_edit_commit
echo "wired td-agent-ledger into $SETTINGS (SessionStart + SessionEnd; backup kept)"
fi
install -Dm755 "$REPO/scripts/td-agent-ledger" "$CMD"
td_receipt_begin
td_receipt_file "$CMD"
td_receipt_record hook "$SETTINGS" "$CMD"
td_receipt_commit
echo "installed $CMD"
echo "new agent sessions will now push their ids to the TD ledger"
