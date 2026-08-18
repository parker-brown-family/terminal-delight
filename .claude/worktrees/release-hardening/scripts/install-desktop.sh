#!/usr/bin/env bash
# Install the terminal-delight launcher + icon for the current user.
# GNOME matches the window's WM_CLASS / app_id ("terminal-delight") to
# terminal-delight.desktop, which points at the icon — that is what turns
# the dock cogwheel into our CRT icon.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
apps="$HOME/.local/share/applications"
icons="$HOME/.local/share/icons/hicolor"

install -Dm644 "$repo/packaging/terminal-delight.svg" \
  "$icons/scalable/apps/terminal-delight.svg"

# pre-render fixed sizes when rsvg-convert is available (crisper in docks)
if command -v rsvg-convert >/dev/null; then
  for s in 48 64 128 256; do
    install -d "$icons/${s}x${s}/apps"
    rsvg-convert -w "$s" -h "$s" "$repo/packaging/terminal-delight.svg" \
      -o "$icons/${s}x${s}/apps/terminal-delight.png"
  done
fi

install -Dm644 "$repo/packaging/terminal-delight.desktop" \
  "$apps/terminal-delight.desktop"

update-desktop-database "$apps" 2>/dev/null || true
gtk-update-icon-cache -f -t "$icons" 2>/dev/null || true

echo "installed: $apps/terminal-delight.desktop"
echo "installed: $icons/scalable/apps/terminal-delight.svg"
