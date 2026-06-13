#!/usr/bin/env bash
# Prepare the patched gpui checkout that app/Cargo.toml's path deps point at:
# a sibling `zed-upstream/` of the repo root, pinned to the rev recorded in
# [package.metadata.terminal-delight], with the td-crt-pass renderer patch
# (per-pane CRT barrel warp) applied. Idempotent; safe to re-run.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
zed_dir="${ZED_UPSTREAM_DIR:-$repo_root/../zed-upstream}"
zed_rev="$(sed -n 's/^zed_rev = "\(.*\)"/\1/p' "$repo_root/app/Cargo.toml")"
patch_file="$repo_root/docs/patches/0001-td-crt-pass.patch"

[ -n "$zed_rev" ] || { echo "zed_rev not found in app/Cargo.toml" >&2; exit 1; }
[ -f "$patch_file" ] || { echo "missing $patch_file" >&2; exit 1; }

if [ ! -d "$zed_dir/.git" ]; then
    echo "Cloning zed at $zed_rev into $zed_dir ..."
    git init -q "$zed_dir"
    git -C "$zed_dir" remote add origin https://github.com/zed-industries/zed.git
    git -C "$zed_dir" fetch -q --depth 1 origin "$zed_rev"
    git -C "$zed_dir" checkout -q FETCH_HEAD
fi

cd "$zed_dir"
if git apply --reverse --check "$patch_file" 2>/dev/null; then
    echo "td-crt-pass already applied in $zed_dir"
elif git apply --check "$patch_file" 2>/dev/null; then
    git apply "$patch_file"
    echo "td-crt-pass applied in $zed_dir"
else
    # Local dev checkouts carry the patch as commits on the td-crt-pass
    # branch, where neither apply nor reverse-apply matches cleanly only if
    # the tree has drifted from the patch artifact — surface that.
    if git grep -q "set_crt_rects" -- crates/gpui_wgpu/src 2>/dev/null; then
        echo "td-crt-pass present in $zed_dir (carried as commits)"
    else
        echo "ERROR: $zed_dir exists but td-crt-pass does not apply cleanly." >&2
        echo "Regenerate docs/patches/0001-td-crt-pass.patch or fix the checkout." >&2
        exit 1
    fi
fi
