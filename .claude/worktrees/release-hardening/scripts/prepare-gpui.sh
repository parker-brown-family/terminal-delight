#!/usr/bin/env bash
# Prepare the patched gpui checkout that app/Cargo.toml's path deps point at:
# a sibling `zed-upstream/` of the repo root, pinned to the rev recorded in
# [package.metadata.terminal-delight], with two patches applied:
#   0001-td-crt-pass        — per-pane CRT barrel-warp renderer pass
#   0002-sever-gpl-crates   — drops the GPL crates (ztracing/zlog) that the
#                             gpui -> sum_tree edge would otherwise link into
#                             the binary, keeping a *distributed* build MIT-clean
# Idempotent; safe to re-run.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
zed_dir="${ZED_UPSTREAM_DIR:-$repo_root/../zed-upstream}"
zed_rev="$(sed -n 's/^zed_rev = "\(.*\)"/\1/p' "$repo_root/app/Cargo.toml")"
patch_crt="$repo_root/docs/patches/0001-td-crt-pass.patch"
patch_gpl="$repo_root/docs/patches/0002-sever-gpl-crates.patch"

[ -n "$zed_rev" ] || { echo "zed_rev not found in app/Cargo.toml" >&2; exit 1; }
for p in "$patch_crt" "$patch_gpl"; do
    [ -f "$p" ] || { echo "missing $p" >&2; exit 1; }
done

if [ ! -d "$zed_dir/.git" ]; then
    echo "Cloning zed at $zed_rev into $zed_dir ..."
    git init -q "$zed_dir"
    git -C "$zed_dir" remote add origin https://github.com/zed-industries/zed.git
    git -C "$zed_dir" fetch -q --depth 1 origin "$zed_rev"
    git -C "$zed_dir" checkout -q FETCH_HEAD
fi

cd "$zed_dir"

# Apply a patch idempotently. $1=patch file, $2=label, $3=sentinel command that
# returns 0 when the change is already present in the tree (covers local dev
# checkouts that carry the patch as commits rather than a working-tree diff).
apply_patch() {
    local pf="$1" label="$2" sentinel="$3"
    if git apply --reverse --check "$pf" 2>/dev/null; then
        echo "$label already applied in $zed_dir"
    elif git apply --check "$pf" 2>/dev/null; then
        git apply "$pf"
        echo "$label applied in $zed_dir"
    elif eval "$sentinel" 2>/dev/null; then
        echo "$label present in $zed_dir (carried as commits)"
    else
        echo "ERROR: $zed_dir exists but $label does not apply cleanly." >&2
        echo "Regenerate $pf or fix the checkout." >&2
        exit 1
    fi
}

apply_patch "$patch_crt" "td-crt-pass" \
    'git grep -q "set_crt_rects" -- crates/gpui_wgpu/src'
# sentinel: ztracing/zlog gone from sum_tree means the sever is already in the tree
apply_patch "$patch_gpl" "sever-gpl-crates" \
    '! git grep -q "ztracing" -- crates/sum_tree'
