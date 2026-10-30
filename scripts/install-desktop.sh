#!/usr/bin/env bash
# Install the terminal-delight launcher + icon for the current user.
# GNOME matches the window's WM_CLASS / app_id ("terminal-delight") to
# terminal-delight.desktop, which points at the icon — that is what turns
# the dock cogwheel into our CRT icon.
set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icons="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
[[ $# == 0 || ( $# == 1 && $1 == --uninstall ) ]] || { echo "Usage: $0 [--uninstall]" >&2; exit 2; }
source "$REPO/scripts/lib/uninstall.sh"
td_receipt_init desktop

refresh_caches() {
  local failed=0
  if command -v update-desktop-database >/dev/null && [[ -d $apps ]]; then
    update-desktop-database "$apps" || failed=1
  fi
  if command -v gtk-update-icon-cache >/dev/null && [[ -d $icons ]]; then
    gtk-update-icon-cache -f -t "$icons" || failed=1
  fi
  return "$failed"
}

if [[ ${1:-} == --uninstall ]]; then
  failed=0
  td_remove_helper "$apps/terminal-delight.desktop" "$REPO/packaging/terminal-delight.desktop" || failed=1
  td_remove_helper "$icons/scalable/apps/terminal-delight.svg" "$REPO/packaging/terminal-delight.svg" || failed=1
  for s in 48 64 128 256; do
    # Rasterized output depends on the converter version; only its receipt
    # establishes ownership. Never infer ownership from the PNG filename.
    td_remove_helper "$icons/${s}x${s}/apps/terminal-delight.png" || failed=1
  done
  refresh_caches || failed=1
  [[ $failed == 0 ]] || exit 1
  td_receipt_remove
  echo "Terminal Delight desktop entry and owned icons removed"
  exit 0
fi
td_receipt_begin

install -Dm644 "$REPO/packaging/terminal-delight.svg" \
  "$icons/scalable/apps/terminal-delight.svg"
td_receipt_file "$icons/scalable/apps/terminal-delight.svg"

# pre-render fixed sizes when rsvg-convert is available (crisper in docks)
if command -v rsvg-convert >/dev/null; then
  for s in 48 64 128 256; do
    install -d "$icons/${s}x${s}/apps"
    rsvg-convert -w "$s" -h "$s" "$REPO/packaging/terminal-delight.svg" \
      -o "$icons/${s}x${s}/apps/terminal-delight.png"
    td_receipt_file "$icons/${s}x${s}/apps/terminal-delight.png"
  done
fi

install -Dm644 "$REPO/packaging/terminal-delight.desktop" \
  "$apps/terminal-delight.desktop"
td_receipt_file "$apps/terminal-delight.desktop"

refresh_caches
td_receipt_commit

echo "installed: $apps/terminal-delight.desktop"
echo "installed: $icons/scalable/apps/terminal-delight.svg"
