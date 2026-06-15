#!/usr/bin/env bash
# Pre-release smoke: the same gates CI runs, plus an AppImage package check.
# Set TD_SKIP_APPIMAGE=1 to run only the offline cargo gates (no network).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root/app"

cargo fmt -- --check
cargo check --locked
cargo clippy --locked -- -D warnings
cargo test --locked
cargo build --release --locked
cargo deny check

if [ "${TD_SKIP_APPIMAGE:-0}" = "1" ]; then
    echo "release-smoke: cargo gates passed (AppImage skipped)"
    exit 0
fi

# Build the distributable AppImage and verify it is a valid, runnable bundle.
bash "$repo_root/scripts/build-appimage.sh"
img="$repo_root/dist/terminal-delight-x86_64.AppImage"
[ -x "$img" ] || { echo "AppImage missing: $img" >&2; exit 1; }
# Validate structure without needing a display: a real AppImage self-extracts.
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
( cd "$tmp" && "$img" --appimage-extract >/dev/null )
[ -x "$tmp/squashfs-root/usr/bin/terminal-delight" ] || { echo "bundle missing binary" >&2; exit 1; }
[ -s "$tmp/squashfs-root/usr/share/licenses/terminal-delight/THIRD-PARTY-LICENSES.txt" ] \
    || { echo "bundle missing THIRD-PARTY-LICENSES" >&2; exit 1; }
# Default agent-bell sounds must ship so the AppImage seeds them on first run.
if [ -d "$repo_root/app/assets/sounds" ]; then
    n=$(find "$tmp/squashfs-root/usr/share/terminal-delight/sounds" -type f 2>/dev/null | wc -l)
    [ "$n" -gt 0 ] || { echo "bundle missing default bell sounds" >&2; exit 1; }
fi

echo "release-smoke: cargo gates + AppImage package check passed"
