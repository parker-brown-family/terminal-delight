#!/usr/bin/env bash
# Install (or --uninstall) Terminal Delight's agent-channel hook adapter into
# Claude Code: puts scripts/td-agent-hooks on ~/.local/bin and wires it into
# ~/.claude/settings.json on five events (jq merge — appends to existing hook
# arrays, never clobbers them; a timestamped backup is kept beside the file).
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

if [[ "${1:-}" == "--uninstall" ]]; then
  if [[ -f $SETTINGS ]] && grep -q td-agent-hooks "$SETTINGS"; then
    cp "$SETTINGS" "$SETTINGS.td-backup.$(date +%s)"
    jq '
      def strip: map(select((.hooks // []) | any(.command? // "" | contains("td-agent-hooks")) | not));
      if .hooks then
        .hooks.UserPromptSubmit = ((.hooks.UserPromptSubmit // []) | strip) |
        .hooks.PreToolUse = ((.hooks.PreToolUse // []) | strip) |
        .hooks.PostToolUse = ((.hooks.PostToolUse // []) | strip) |
        .hooks.Stop = ((.hooks.Stop // []) | strip) |
        .hooks.Notification = ((.hooks.Notification // []) | strip)
      else . end
    ' "$SETTINGS" >"$SETTINGS.tmp" && mv "$SETTINGS.tmp" "$SETTINGS"
  fi
  rm -f "$CMD"
  echo "td-agent-hooks unhooked and removed"
  exit 0
fi

install -Dm755 "$REPO/scripts/td-agent-hooks" "$CMD"
echo "installed $CMD"

mkdir -p "$(dirname "$SETTINGS")"
[[ -f $SETTINGS ]] || echo '{}' >"$SETTINGS"
if grep -q td-agent-hooks "$SETTINGS"; then
  echo "hooks already wired in $SETTINGS"
  exit 0
fi
cp "$SETTINGS" "$SETTINGS.td-backup.$(date +%s)"
jq --arg cmd "$CMD" '
  .hooks //= {} |
  .hooks.UserPromptSubmit = ((.hooks.UserPromptSubmit // []) + [{"hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.PreToolUse = ((.hooks.PreToolUse // []) + [{"matcher": "AskUserQuestion", "hooks": [{"type": "command", "command": $cmd, "timeout": 600}]}]) |
  .hooks.PostToolUse = ((.hooks.PostToolUse // []) + [{"matcher": "AskUserQuestion", "hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.Stop = ((.hooks.Stop // []) + [{"hooks": [{"type": "command", "command": $cmd}]}]) |
  .hooks.Notification = ((.hooks.Notification // []) + [{"hooks": [{"type": "command", "command": $cmd}]}])
' "$SETTINGS" >"$SETTINGS.tmp" && mv "$SETTINGS.tmp" "$SETTINGS"
echo "wired td-agent-hooks into $SETTINGS (five events; backup kept)"
echo "new agent sessions in Terminal Delight panes now talk to the bench through the channel"
