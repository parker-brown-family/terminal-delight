#!/usr/bin/env bash
# Capture Ctrl+Alt+T for Terminal Delight on GNOME.
#
# Ctrl+Alt+T is the GNOME desktop's built-in "open terminal" shortcut; an app
# cannot grab a global hotkey itself, so we register it at the desktop level:
#   1. free the built-in `terminal` media key (so it stops launching gnome-terminal)
#   2. add a custom keybinding on <Primary><Alt>t that runs the terminal-delight binary
#
# Each launch opens a fresh window (terminal-delight is not single-instance), and a
# GNOME global shortcut fires even while TD is focused — so this one binding covers
# both "from anywhere" and "from inside TD".
#
# Undo uses the ownership receipt written by this version. Older installations
# without a receipt are retained for review; later user choices are preserved.
set -euo pipefail

MEDIA=org.gnome.settings-daemon.plugins.media-keys
KEYPATH=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/terminal-delight/
SCHEMA="${MEDIA}.custom-keybinding:${KEYPATH}"
BACKUP="${XDG_CONFIG_HOME:-$HOME/.config}/terminal-delight/hotkey-terminal-key.bak"
RECEIPT="${BACKUP%.bak}.owned"
ACCEL='<Primary><Alt>t'

command -v gsettings >/dev/null || { echo "gsettings not found — this installer targets GNOME."; exit 1; }
case "${XDG_CURRENT_DESKTOP:-}" in
  *GNOME*) : ;;
  *) echo "Warning: XDG_CURRENT_DESKTOP='${XDG_CURRENT_DESKTOP:-?}' is not GNOME; continuing anyway." ;;
esac

# Add/remove our relocatable path in the custom-keybindings list, preserving others.
list_has() { gsettings get "$MEDIA" custom-keybindings | grep -qF "$KEYPATH"; }
list_add() {
  local cur; cur=$(gsettings get "$MEDIA" custom-keybindings)
  if [[ "$cur" == "@as []" || "$cur" == "[]" ]]; then
    gsettings set "$MEDIA" custom-keybindings "['$KEYPATH']"
  else
    gsettings set "$MEDIA" custom-keybindings "${cur%]}, '$KEYPATH']"
  fi
}
list_del() {
  local cur; cur=$(gsettings get "$MEDIA" custom-keybindings)
  # drop our entry in either position, then tidy stray commas/spaces
  cur=${cur//\'$KEYPATH\', /}
  cur=${cur//, \'$KEYPATH\'/}
  cur=${cur//\'$KEYPATH\'/}
  gsettings set "$MEDIA" custom-keybindings "$cur"
}

if [[ "${1:-}" == "--uninstall" ]]; then
  # Older installations have no record of the values TD wrote. A matching
  # display name alone cannot prove ownership of a shortcut somebody edited.
  if [[ ! -f $RECEIPT || -L $RECEIPT ]]; then
    echo "No trusted hotkey receipt; retained GNOME settings and backup for review." >&2
    exit 1
  fi
  mapfile -t written < "$RECEIPT"
  [[ ${#written[@]} == 3 ]] || { echo "Invalid hotkey receipt; retained settings." >&2; exit 1; }
  keys=(name command binding)
  for i in 0 1 2; do
    current=$(gsettings get "$SCHEMA" "${keys[i]}")
    [[ $current == "${written[i]}" ]] || {
      echo "Shortcut changed since installation; retained settings and receipt." >&2; exit 1;
    }
  done
  list_del
  if [[ -f "$BACKUP" ]]; then
    current=$(gsettings get "$MEDIA" terminal)
    if [[ $current == '[]' || $current == '@as []' ]]; then
      gsettings set "$MEDIA" terminal "$(cat "$BACKUP")"
      rm -- "$BACKUP"
      echo "Restored the recorded built-in terminal binding."
    else
      echo "Kept the later built-in terminal choice; backup retained at $BACKUP."
    fi
  else
    echo "Prior built-in binding unknown; kept current value."
  fi
  for key in name command binding; do gsettings reset "$SCHEMA" "$key"; done
  rm -- "$RECEIPT"
  echo "Terminal Delight hotkey removed."
  exit 0
fi
[[ $# == 0 ]] || { echo "Usage: $0 [--uninstall]" >&2; exit 2; }

if list_has || [[ -e $RECEIPT || -L $RECEIPT ]]; then
  echo "Existing shortcut or receipt retained; review it before reinstalling." >&2
  exit 1
fi

# Resolve a stable launcher (absolute path — gnome-settings-daemon runs with a minimal PATH).
LAUNCHER=$(command -v terminal-delight || true)
[[ -n "$LAUNCHER" ]] || { echo "terminal-delight not on PATH — run scripts/install-desktop.sh first."; exit 1; }

# Free the built-in terminal media key so it can't also launch gnome-terminal.
mkdir -p "$(dirname "$BACKUP")"
[[ -f "$BACKUP" ]] || gsettings get "$MEDIA" terminal > "$BACKUP"
gsettings set "$MEDIA" terminal "@as []"

list_has || list_add
gsettings set "$SCHEMA" name 'Terminal Delight'
gsettings set "$SCHEMA" command "$LAUNCHER"
gsettings set "$SCHEMA" binding "$ACCEL"

# Persist the exact GVariant values for compare-before-restore. Keep this after
# successful installation: a failed install is explicitly unresolved.
receipt_tmp=$(mktemp "${RECEIPT}.XXXXXXXX")
trap 'rm -f -- "$receipt_tmp"' EXIT
for key in name command binding; do gsettings get "$SCHEMA" "$key" >> "$receipt_tmp"; done
mv -T -- "$receipt_tmp" "$RECEIPT"

echo "Bound $ACCEL -> $LAUNCHER"
echo "Press Ctrl+Alt+T to open a new Terminal Delight window."
echo "Undo with: scripts/install-hotkey.sh --uninstall"
