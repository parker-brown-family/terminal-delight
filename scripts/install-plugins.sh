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
# Idempotent. Usage: scripts/install-plugins.sh [--uninstall] [DEST]
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
uninstall=false
if [[ ${1:-} == --uninstall ]]; then uninstall=true; shift; fi
[[ $# -le 1 && ${1:-} != -* ]] || { echo "Usage: $0 [--uninstall] [DEST]" >&2; exit 2; }
DEST="${1:-${DEST:-$HOME/.local/bin}}"
source "$REPO/scripts/lib/uninstall.sh"
td_receipt_init plugins
shopt -s nullglob
if $uninstall; then
  failed=0
  for exe in "$REPO"/plugins/*/*; do
    [[ -f $exe && -x $exe ]] || continue
    td_remove_helper "$DEST/${exe##*/}" "$exe" || failed=1
  done
  [[ $failed == 0 ]] || exit 1
  td_receipt_remove
  echo "Owned Terminal Delight plugin executables removed"
  exit 0
fi
mkdir -p "$DEST"
td_receipt_begin

installed=0
for exe in "$REPO"/plugins/*/*; do
  # only the executable server file (skip plugin.json, READMEs, etc.)
  [ -f "$exe" ] && [ -x "$exe" ] || continue
  name="$(basename "$exe")"
  # write-then-rename so a running plugin process is never clobbered mid-read
  cp "$exe" "$DEST/$name.new"
  chmod +x "$DEST/$name.new"
  mv -f "$DEST/$name.new" "$DEST/$name"
  td_receipt_file "$DEST/$name"
  echo "installed plugin: $name -> $DEST/$name"
  installed=$((installed + 1))
done

if [ "$installed" -eq 0 ]; then
  echo "no bundled plugin executables found under $REPO/plugins/*/" >&2
fi
td_receipt_commit
echo "done — $installed plugin executable(s) on $DEST"
