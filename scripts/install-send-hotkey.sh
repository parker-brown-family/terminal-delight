#!/usr/bin/env bash
# Install (or --uninstall) the Omarchy/Hyprland send-tile hotkey:
#   SUPER+ALT+T  →  td-send   (migrate the focused terminal tile into TD)
# Also puts scripts/td-send on ~/.local/bin so the binding has something to
# call. Idempotent; the binding lives between marker comments in bindings.lua.
#
# Usage: scripts/install-send-hotkey.sh [--uninstall]
# Env:   HYPR_BINDINGS=path (default ~/.config/hypr/bindings.lua)
#        DEST=bin dir       (default ~/.local/bin)
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINDINGS="${HYPR_BINDINGS:-$HOME/.config/hypr/bindings.lua}"
DEST="${DEST:-$HOME/.local/bin}"

if [[ "${1:-}" == "--uninstall" ]]; then
  if [[ -f $BINDINGS ]]; then
    sed -i '/^-- >>> td-send hotkey >>>$/,/^-- <<< td-send hotkey <<<$/d' "$BINDINGS"
  fi
  rm -f "$DEST/td-send"
  hyprctl reload >/dev/null 2>&1 || true
  echo "td-send hotkey and script removed"
  exit 0
fi

install -Dm755 "$REPO/scripts/td-send" "$DEST/td-send"
echo "installed td-send -> $DEST/td-send"

mkdir -p "$(dirname "$BINDINGS")"
touch "$BINDINGS"
if grep -qF -- '-- >>> td-send hotkey >>>' "$BINDINGS"; then
  echo "hotkey block already present in $BINDINGS"
else
  cat >>"$BINDINGS" <<'EOF'

-- >>> td-send hotkey >>>
-- Send the focused terminal tile into Terminal Delight (idle shells, agents,
-- and tmux attaches migrate; anything else is refused). T on SUPER+ALT was
-- the open seat: SUPER, SUPER+CTRL, SUPER+CTRL+ALT and CTRL+ALT own T already.
o.bind("SUPER + ALT + T", "Send tile to Terminal Delight", "td-send")
-- <<< td-send hotkey <<<
EOF
  echo "bound SUPER+ALT+T -> td-send in $BINDINGS"
fi
hyprctl reload >/dev/null 2>&1 || true
echo "done — press SUPER+ALT+T on a terminal tile"
