#!/usr/bin/env bash
# Install the bundled TD plugins' MCP-server executables onto the INSTALLED
# binary's resolver path so a deployed terminal-delight can discover them.
#
# Why this is needed (issue #92): the plugin resolver in `plugins.rs` finds a
# bundled plugin by walking UP from the running exe for `plugins/<name>/<bin>`.
# That works for the dev binary (under the checkout) but NOT for the installed
# binary at ~/.local/bin — its ancestors are ~/.local, ~, / — none of which hold
# the checkout. The resolver's other hop is `~/.local/bin/<bin>` (and PATH), so
# copying each plugin executable there makes the installed binary resolve it.
#
# Idempotent. Usage: scripts/install-plugins.sh [DEST]   (DEST default ~/.local/bin)
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="${1:-$HOME/.local/bin}"
mkdir -p "$DEST"

installed=0
shopt -s nullglob
for exe in "$REPO"/plugins/*/*; do
  # only the executable server file (skip plugin.json, READMEs, etc.)
  [ -f "$exe" ] && [ -x "$exe" ] || continue
  name="$(basename "$exe")"
  # write-then-rename so a running plugin process is never clobbered mid-read
  cp "$exe" "$DEST/$name.new"
  chmod +x "$DEST/$name.new"
  mv -f "$DEST/$name.new" "$DEST/$name"
  echo "installed plugin: $name -> $DEST/$name"
  installed=$((installed + 1))
done

if [ "$installed" -eq 0 ]; then
  echo "no bundled plugin executables found under $REPO/plugins/*/" >&2
fi
echo "done — $installed plugin executable(s) on $DEST"
