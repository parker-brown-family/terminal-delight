#!/usr/bin/env bash
# Every OPS scene, one hidden window at a time, from one frozen binary.
#   run-all.sh <frozen-td-binary> <out-root>
set -uo pipefail
TD=$1
ROOT=$2
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$ROOT"
sha256sum "$TD" > "$ROOT/binary.sha256"
run() {
  local scene=$1 surface=$2 out="$ROOT/$1-$2"
  rm -rf "$out"; mkdir -p "$out"
  echo "== $scene $surface"
  if timeout 240 bash "$HERE/capture.sh" "$TD" "$out" "$scene" "$surface" > "$out/run.log" 2>&1; then
    echo "   ok $(cat "$out/skin.txt" 2>/dev/null)"
  else
    echo "   FAILED ($?) — tail:"; tail -5 "$out/run.log" | sed 's/^/     /'
  fi
}
for scene in before tactical gruvbox tokyo current warped intrusive chalkwet; do run "$scene" workspace; done
for surface in tray usage gauges rail paint; do run tactical "$surface"; done
run current usage
