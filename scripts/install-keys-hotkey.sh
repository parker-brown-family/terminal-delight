#!/usr/bin/env bash
# Install (or --uninstall) the Omarchy key-sheet hotkey:
#   SUPER+ALT+T  →  td-keybindings   (every Terminal Delight key, searchable)
# beside Omarchy's own SUPER+K (its keys), SUPER+ALT+K (tmux's) and
# SUPER+CTRL+K (Herdr's). Puts scripts/td-keybindings on ~/.local/bin and the
# Keys page it reads in ~/.local/share/terminal-delight/, so the sheet works
# with no checkout; run this again after the page changes to refresh the copy.
#
# The seat was td-send's (send a tile into Terminal Delight). That hotkey is
# retired; an existing legacy block is retained for manual review before this
# installer proceeds. td-send itself stays on PATH. The binding lives between
# marker comments in bindings.lua.
#
# Usage: scripts/install-keys-hotkey.sh [--uninstall]
# Env:   HYPR_BINDINGS=path (default ~/.config/hypr/bindings.lua)
#        DEST=bin dir       (default ~/.local/bin)
#        DATA=data dir      (default ${XDG_DATA_HOME:-~/.local/share}/terminal-delight)
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BINDINGS="${HYPR_BINDINGS:-$HOME/.config/hypr/bindings.lua}"
DEST="${DEST:-$HOME/.local/bin}"
DATA="${DATA:-${XDG_DATA_HOME:-$HOME/.local/share}/terminal-delight}"
OPEN='-- >>> td-keybindings hotkey >>>'
SHUT='-- <<< td-keybindings hotkey <<<'

source "$REPO/scripts/lib/uninstall.sh"
if [[ "${1:-}" == "--uninstall" ]]; then
  td_strip_key_block "$BINDINGS" "$DEST"
  td_remove_helper "$DEST/td-keybindings" "$REPO/scripts/td-keybindings"
  td_remove_helper "$DATA/keys.html" "$REPO/docsite/pages/keys.html"
  hyprctl reload >/dev/null 2>&1 || { echo "Files cleaned; Hyprland reload failed or unavailable." >&2; exit 1; }
  echo "td-keybindings hotkey, script and page removed"
  exit 0
fi
[[ $# == 0 ]] || { echo "Usage: $0 [--uninstall]" >&2; exit 2; }

install -Dm755 "$REPO/scripts/td-keybindings" "$DEST/td-keybindings"
install -Dm644 "$REPO/docsite/pages/keys.html" "$DATA/keys.html"
echo "installed td-keybindings -> $DEST/td-keybindings"
echo "installed the Keys page  -> $DATA/keys.html"
rows=$("$DEST/td-keybindings" --print --page "$DATA/keys.html" | wc -l)
echo "the sheet reads $rows keys"

mkdir -p "$(dirname "$BINDINGS")"
touch "$BINDINGS"
if grep -qF -- '-- >>> td-send hotkey >>>' "$BINDINGS"; then
  echo "Legacy td-send block retained; review it before installing the replacement hotkey." >&2
  exit 1
fi
if grep -qF -- "$OPEN" "$BINDINGS"; then
  echo "hotkey block already present in $BINDINGS"
else
  cat >>"$BINDINGS" <<EOF

$OPEN
-- SUPER+ALT+T shows every Terminal Delight key in Omarchy's searchable key
-- menu, the way SUPER+K shows Omarchy's own and SUPER+CTRL+K shows Herdr's.
-- The list is the docs site's Keys page, copied beside the script by
-- scripts/install-keys-hotkey.sh in the terminal-delight repo. The seat was
-- td-send's, whose hotkey is retired; T reads for Terminal Delight, and the K
-- family on SUPER, SUPER+ALT and SUPER+CTRL is all Omarchy's already.
o.bind("SUPER + ALT + T", "Terminal Delight keybindings", "$DEST/td-keybindings")
$SHUT
EOF
  echo "bound SUPER+ALT+T -> td-keybindings in $BINDINGS"
fi
hyprctl reload >/dev/null 2>&1 || true
echo "done — press SUPER+ALT+T"
