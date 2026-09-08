#!/usr/bin/env bash
# build-shot-board — inline review copies of the clips into docs/media/shot-list.html.
#
# The board has to be watchable from anywhere — a phone on the sofa, a machine with
# no checkout — so each filmed shot carries its own clip as a data URI. Masters stay
# in docs/media/clips/ at capture resolution; what goes in the page is a 760px h264
# review copy, small enough that forty rows of them would still open quickly.
#
#   scripts/build-shot-board.sh
#
# Idempotent: it rewrites everything between the CLIPS_START / CLIPS_END markers in
# the page, so run it again whenever a clip is reshot. Add a new clip by giving its
# shot a `clip:` key in the page and a line in CLIPS below, keyed the same.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
page="$here/docs/media/shot-list.html"
clips="$here/docs/media/clips"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT

# key=file — the key matches the `clip:` field on a shot in the page.
CLIPS=(
  "sticky-note=sticky-note-agent-20260907-153214.mp4"
  "agent-wall=agent-wall-20260907-203925.mp4"
  "mcp-policy=mcp-policy-20260907-204531.mp4"
)

{
  echo "/*CLIPS_START*/"
  echo "const CLIPS = {"
  for entry in "${CLIPS[@]}"; do
    key="${entry%%=*}"; file="${entry#*=}"
    src="$clips/$file"
    if [ ! -f "$src" ]; then
      echo "build-shot-board: missing $src — skipping $key" >&2
      continue
    fi
    # -an: the clips are silent anyway, and an empty audio track costs bytes.
    ffmpeg -v error -y -i "$src" -vf "scale=760:-2" -c:v libx264 -preset slow \
           -crf 30 -movflags +faststart -an "$work/$key.mp4"
    printf '  "%s": "data:video/mp4;base64,%s",\n' "$key" "$(base64 -w0 "$work/$key.mp4")"
    printf 'build-shot-board: %-14s %s → %s\n' "$key" \
      "$(du -h "$src" | cut -f1)" "$(du -h "$work/$key.mp4" | cut -f1)" >&2
  done
  echo "};"
  echo "/*CLIPS_END*/"
} > "$work/block.js"

awk -v blockfile="$work/block.js" '
  /\/\*CLIPS_START\*\// { while ((getline line < blockfile) > 0) print line; skip = 1; next }
  /\/\*CLIPS_END\*\//   { skip = 0; next }
  !skip
' "$page" > "$work/page.html"

mv "$work/page.html" "$page"
printf 'build-shot-board: page is now %s\n' "$(du -h "$page" | cut -f1)" >&2
