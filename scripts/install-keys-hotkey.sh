#!/usr/bin/env bash
# Install (or --uninstall) the Omarchy key-sheet hotkey:
#   SUPER+ALT+T  →  td-keybindings   (every Terminal Delight key, searchable)
# beside Omarchy's own SUPER+K (its keys), SUPER+ALT+K (tmux's) and
# SUPER+CTRL+K (Herdr's). Puts scripts/td-keybindings on ~/.local/bin and the
# Keys page it reads in ~/.local/share/terminal-delight/, so the sheet works
# with no checkout; run this again after the page changes to refresh the copy.
#
# The seat was td-send's (send a tile into Terminal Delight). That hotkey is
# retired, so its block is taken out of bindings.lua here; td-send itself stays
# on PATH for anyone who runs it by hand. Idempotent; the binding lives between
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

if [[ "${1:-}" == "--uninstall" ]]; then
  if [[ -f $BINDINGS ]]; then
    sed -i "/^$OPEN\$/,/^$SHUT\$/d" "$BINDINGS"
  fi
  rm -f "$DEST/td-keybindings" "$DATA/keys.html"
  hyprctl reload >/dev/null 2>&1 || true
  echo "td-keybindings hotkey, script and page removed"
  exit 0
fi

install -Dm755 "$REPO/scripts/td-keybindings" "$DEST/td-keybindings"
install -Dm644 "$REPO/docsite/pages/keys.html" "$DATA/keys.html"
echo "installed td-keybindings -> $DEST/td-keybindings"
echo "installed the Keys page  -> $DATA/keys.html"
rows=$("$DEST/td-keybindings" --print --page "$DATA/keys.html" | wc -l)
echo "the sheet reads $rows keys"

mkdir -p "$(dirname "$BINDINGS")"
touch "$BINDINGS"
if grep -qF -- '-- >>> td-send hotkey >>>' "$BINDINGS"; then
  sed -i '/^-- >>> td-send hotkey >>>$/,/^-- <<< td-send hotkey <<<$/d' "$BINDINGS"
  echo "took SUPER+ALT+T back from the retired td-send hotkey"
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
