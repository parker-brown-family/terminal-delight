#!/usr/bin/env bash
# Install (or --uninstall) Terminal Delight's agent-channel hook adapter into
# Claude Code: puts scripts/td-agent-hooks on ~/.local/bin and wires it into
# ~/.claude/settings.json on five events (jq merge — appends to existing hook
# arrays, never clobbers them; a private backup is kept under XDG_STATE_HOME).
# Idempotent. The ledger hook (install-recovery-hook.sh) is separate.
#
#   UserPromptSubmit                -> the person's exact words
#   PreToolUse   matcher AskUserQuestion, timeout 600 -> the whole round, and the pre-answer
#   PostToolUse  matcher AskUserQuestion -> the harness's own record of the answer
#   Stop                            -> the agent's reply
#   Notification                    -> the harness's notifications
#
# Usage: scripts/install-agent-hooks.sh [--uninstall]
# Env:   CLAUDE_SETTINGS=path (default ~/.claude/settings.json)
#        DEST=bin dir         (default ~/.local/bin)
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${DEST:-$HOME/.local/bin}"
SETTINGS="${CLAUDE_SETTINGS:-$HOME/.claude/settings.json}"
CMD="$DEST/td-agent-hooks"

command -v jq >/dev/null 2>&1 || { echo "jq is required" >&2; exit 1; }

source "$REPO/scripts/lib/uninstall.sh"
[[ $# == 0 || ( $# == 1 && $1 == --uninstall ) ]] || { echo "Usage: $0 [--uninstall]" >&2; exit 2; }
td_receipt_init agent-hooks
if [[ "${1:-}" == "--uninstall" ]]; then
  td_unhook "$SETTINGS" "$CMD"
  td_remove_helper "$CMD" "$REPO/scripts/td-agent-hooks"
  td_receipt_remove
  echo "td-agent-hooks unhooked and removed"
  exit 0
fi
mkdir -p "$(dirname "$SETTINGS")"
[[ ! -L $SETTINGS ]] || { echo "Refusing symlink settings: $SETTINGS" >&2; exit 1; }
[[ -f $SETTINGS ]] || echo '{}' >"$SETTINGS"
td_edit_begin "$SETTINGS"
jq -es 'length == 1 and (.[0] | type == "object")' "$TD_EDIT_SNAPSHOT" >/dev/null
if grep -q td-agent-hooks "$SETTINGS"; then
  echo "hooks already wired in $SETTINGS"
else
jq --arg cmd "$CMD" '
  .hooks //= {} |
  .hooks.UserPromptSubmit = ((.hooks.UserPromptSubmit // []) + [{"hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.PreToolUse = ((.hooks.PreToolUse // []) + [{"matcher": "AskUserQuestion", "hooks": [{"type": "command", "command": $cmd, "timeout": 600}]}]) |
  .hooks.PostToolUse = ((.hooks.PostToolUse // []) + [{"matcher": "AskUserQuestion", "hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.Stop = ((.hooks.Stop // []) + [{"hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.Notification = ((.hooks.Notification // []) + [{"hooks": [{"type": "command", "command": $cmd}]}])
' "$TD_EDIT_SNAPSHOT" > "$TD_EDIT_STAGE"
td_edit_commit
echo "wired td-agent-hooks into $SETTINGS (five events; backup kept)"
fi
install -Dm755 "$REPO/scripts/td-agent-hooks" "$CMD"
td_receipt_begin
td_receipt_file "$CMD"
td_receipt_record hook "$SETTINGS" "$CMD"
td_receipt_commit
echo "installed $CMD"
echo "new agent sessions in Terminal Delight panes now talk to the bench through the channel"
