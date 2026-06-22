#!/usr/bin/env bash
# Prepare the patched gpui checkout that app/Cargo.toml's path deps point at:
# a sibling `zed-upstream/` of the repo root, pinned to the rev recorded in
# [package.metadata.terminal-delight], with three patches applied:
#   0001-td-crt-pass        — per-pane CRT barrel-warp renderer pass
#   0002-focus-blur         — frosted-glass backdrop blur for the FOCUS modal
#                             (delta on 0001; adds gpui_wgpu::set_focus_blur,
#                             which app/src/warp.rs calls)
#   0003-text-crawl         — Star-Wars text-crawl perspective pre-map in the CRT
#                             pass + per-rect crawl uniform (delta on 0001+0002;
#                             extends set_crt_rects_tubes, which warp.rs calls)
#   0002-sever-gpl-crates   — drops the GPL crates (ztracing/zlog) that the
#                             gpui -> sum_tree edge would otherwise link into
#                             the binary, keeping a *distributed* build MIT-clean
# Idempotent; safe to re-run.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
zed_dir="${ZED_UPSTREAM_DIR:-$repo_root/../zed-upstream}"
zed_rev="$(sed -n 's/^zed_rev = "\(.*\)"/\1/p' "$repo_root/app/Cargo.toml")"
patch_crt="$repo_root/docs/patches/0001-td-crt-pass.patch"
patch_blur="$repo_root/docs/patches/0002-focus-blur.patch"
patch_crawl="$repo_root/docs/patches/0003-text-crawl.patch"
patch_tubes="$repo_root/docs/patches/0004-warp-tube-cap-32.patch"
patch_gpl="$repo_root/docs/patches/0002-sever-gpl-crates.patch"

[ -n "$zed_rev" ] || { echo "zed_rev not found in app/Cargo.toml" >&2; exit 1; }
for p in "$patch_crt" "$patch_blur" "$patch_crawl" "$patch_tubes" "$patch_gpl"; do
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
    'grep -rqF "set_crt_rects" crates/gpui_wgpu/src'
# 0002-focus-blur is a delta on top of 0001-td-crt-pass; must apply after it.
# sentinel: set_focus_blur present in gpui_wgpu means the blur is already in the tree
apply_patch "$patch_blur" "focus-blur" \
    'grep -rqF "set_focus_blur" crates/gpui_wgpu/src'
# 0003-text-crawl is a delta on top of 0001+0002 (it patches the v3 shader and the
# tube renderer); must apply after focus-blur.
# sentinel: the per-rect crawl uniform present means the crawl is already in the tree
apply_patch "$patch_crawl" "text-crawl" \
    'grep -rqF "crawl: array" crates/gpui_wgpu/src'
# 0004-warp-tube-cap-32 bumps the per-rect tube cap 8 -> 32 (shader arrays +
# uniform packing + buffer size) so the agent wall can warp each card's logo
# square. Delta on 0001+0003; must apply after them.
# sentinel: the 32-wide rects array present means the cap bump is already in the tree
apply_patch "$patch_tubes" "warp-tube-cap-32" \
    'grep -rqF "array<vec4<f32>, 32>" crates/gpui_wgpu/src'
# sentinel: ztracing/zlog gone from sum_tree means the sever is already in the tree
apply_patch "$patch_gpl" "sever-gpl-crates" \
    '! grep -rqF "ztracing" crates/sum_tree'
