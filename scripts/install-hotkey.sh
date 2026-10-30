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
INTENT="${BACKUP%.bak}.intent"
ACCEL='<Primary><Alt>t'
[[ $# == 0 || ( $# == 1 && $1 == --uninstall ) ]] || { echo "Usage: $0 [--uninstall]" >&2; exit 2; }

# These commands are printed for the person to review, not evaluated here.
# shellcheck disable=SC2016
manual_review() {
  echo 'Review these values before changing the shortcut:' >&2
  for key in name command binding; do printf 'gsettings get %q %q\n' "$SCHEMA" "$key" >&2; done
  printf 'gsettings get %q custom-keybindings\n' "$MEDIA" >&2
  printf 'gsettings get %q terminal\n' "$MEDIA" >&2
  echo 'If the custom slot still belongs to Terminal Delight, remove only its path from the list, then reset its keys:' >&2
  printf 'td_keys=$(gsettings get %q custom-keybindings)\n' "$MEDIA" >&2
  printf 'td_path=%q\n' "$KEYPATH" >&2
  cat >&2 <<'EOF'
td_keys=${td_keys//\'$td_path\', /}
td_keys=${td_keys//, \'$td_path\'/}
td_keys=${td_keys//\'$td_path\'/}
EOF
  printf 'gsettings set %q custom-keybindings "$td_keys"\n' "$MEDIA" >&2
  for key in name command binding; do printf 'gsettings reset %q %q\n' "$SCHEMA" "$key" >&2; done
  if [[ -f $BACKUP && ! -L $BACKUP ]]; then
    echo 'Restore this saved built-in key only if its current value is still empty and the backup is yours:' >&2
    printf 'cat -- %q\n' "$BACKUP" >&2
    printf 'gsettings set %q terminal "$(cat -- %q)"\n' "$MEDIA" "$BACKUP" >&2
  fi
  if [[ -f $INTENT ]]; then printf 'Incomplete install intent: %s\n' "$INTENT" >&2; fi
}

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
    manual_review
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
  rm -f -- "$INTENT"
  echo "Terminal Delight hotkey removed."
  exit 0
fi
if list_has || [[ -e $RECEIPT || -L $RECEIPT || -e $INTENT || -L $INTENT ]]; then
  echo "Existing shortcut or receipt retained; review it before reinstalling." >&2
  exit 1
fi

# Resolve a stable launcher (absolute path — gnome-settings-daemon runs with a minimal PATH).
LAUNCHER=$(command -v terminal-delight || true)
[[ -n "$LAUNCHER" ]] || { echo "terminal-delight not on PATH — run scripts/install-desktop.sh first."; exit 1; }

# Free the built-in terminal media key so it can't also launch gnome-terminal.
umask 077
mkdir -p "$(dirname "$BACKUP")"
[[ ! -L $BACKUP ]] || { echo "Refusing symlink backup: $BACKUP" >&2; exit 1; }
[[ -f "$BACKUP" ]] || gsettings get "$MEDIA" terminal > "$BACKUP"
# Record intent atomically before the first settings write. A failed install
# leaves this record and the old binding for manual resolution; it does not
# claim the three final values were successfully applied.
receipt_tmp=$(mktemp "${INTENT}.XXXXXXXX")
trap 'rm -f -- "${receipt_tmp:-}"' EXIT
printf 'name\t%s\ncommand\t%s\nbinding\t%s\n' 'Terminal Delight' "$LAUNCHER" "$ACCEL" > "$receipt_tmp"
mv -T -- "$receipt_tmp" "$INTENT"
gsettings set "$MEDIA" terminal "@as []"

list_has || list_add
gsettings set "$SCHEMA" name 'Terminal Delight'
gsettings set "$SCHEMA" command "$LAUNCHER"
gsettings set "$SCHEMA" binding "$ACCEL"

# Persist the exact GVariant values for compare-before-restore. Keep this after
# successful installation; the intent remains if any preceding write failed.
receipt_tmp=$(mktemp "${RECEIPT}.XXXXXXXX")
trap 'rm -f -- "$receipt_tmp"' EXIT
for key in name command binding; do gsettings get "$SCHEMA" "$key" >> "$receipt_tmp"; done
mv -T -- "$receipt_tmp" "$RECEIPT"
rm -- "$INTENT"

echo "Bound $ACCEL -> $LAUNCHER"
echo "Press Ctrl+Alt+T to open a new Terminal Delight window."
echo "Undo with: scripts/install-hotkey.sh --uninstall"
