#!/usr/bin/env bash
# Prepare the patched gpui checkout that app/Cargo.toml's path deps point at:
# a sibling `zed-upstream/` of the repo root, pinned to the rev recorded in
# [package.metadata.terminal-delight], with the following patches applied:
#   0001-td-crt-pass        — per-pane CRT barrel-warp renderer pass
#   0002-focus-blur         — frosted-glass backdrop blur for the FOCUS modal
#                             (delta on 0001; adds gpui_wgpu::set_focus_blur,
#                             which app/src/warp.rs calls)
#   0003-text-crawl         — Star-Wars text-crawl perspective pre-map in the CRT
#                             pass + per-rect crawl uniform (delta on 0001+0002;
#                             extends set_crt_rects_tubes, which warp.rs calls)
#   0005-glyph-transform    — an ambient transform for painted glyphs
#                             (Window::with_text_transformation); the sprite
#                             structs already carried the matrix and the shaders
#                             already consumed it, only paint_glyph pinned it to
#                             the unit matrix. Independent of 0001-0004 (it
#                             touches gpui, not gpui_wgpu); app/src/sticky.rs
#                             needs it to write a note at an angle.
#   0002-sever-gpl-crates   — drops the GPL crates (ztracing/zlog) that the
#                             gpui -> sum_tree edge would otherwise link into
#                             the binary, keeping a *distributed* build MIT-clean
#   0006-lazy-path-targets  — creates the path renderer's two surface-sized
#                             targets (one of them 4x MSAA) on the first frame
#                             that draws a path instead of the first frame of
#                             all: ~96 MiB of GPU memory an idle window that
#                             draws no path never needs (#953). Delta on the
#                             renderer patches; applies after them.
#   0007-path-targets-fit-the-paths — sizes those two targets to the largest
#                             batch of paths a frame draws, rasterised at a
#                             whole-pixel offset, instead of the surface: a
#                             sticky note costs ~2 MiB, not ~96 (#962). Delta
#                             on 0006.
#   0008-glass-rim          — seats each bent tube's glass in a bezel: rounded
#                             corners in source space, so the warp bends them
#                             with everything else, and the lip's soft shadow
#                             on the glass, deepest where two edges meet; and
#                             a faint moulding round it, coloured per tube and
#                             lit from above. Adds set_crt_rects_tubes_bezel
#                             beside set_crt_rects_tubes, which is unchanged.
#                             Delta on 0001-0004 and 0007.
#   0009-image-glow         — pane-bounded terminal-image bloom, independent
#                             of whether CRT glass is enabled.
#   0010-image-glow-taps    — evaluates the same 16 weighted samples explicitly,
#                             avoiding dynamic per-fragment array indexing.
#   0012-tube-blur          — a per-tube blur radius in the spare image_glow.y
#                             lane: the tube's own picture softened inside its
#                             rect, flat or bent (set_crt_rects_tubes_blur).
#                             TD blurs an inactive pane's terminal with it.
# Idempotent; safe to re-run.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
zed_dir="${ZED_UPSTREAM_DIR:-$repo_root/../zed-upstream}"
zed_rev="$(sed -n 's/^zed_rev = "\(.*\)"/\1/p' "$repo_root/app/Cargo.toml")"
patch_crt="$repo_root/docs/patches/0001-td-crt-pass.patch"
patch_blur="$repo_root/docs/patches/0002-focus-blur.patch"
patch_crawl="$repo_root/docs/patches/0003-text-crawl.patch"
patch_tubes="$repo_root/docs/patches/0004-warp-tube-cap-32.patch"
patch_glyph="$repo_root/docs/patches/0005-glyph-transform.patch"
patch_gpl="$repo_root/docs/patches/0002-sever-gpl-crates.patch"
patch_lazy="$repo_root/docs/patches/0006-lazy-path-targets.patch"
patch_fit="$repo_root/docs/patches/0007-path-targets-fit-the-paths.patch"
patch_rim="$repo_root/docs/patches/0008-glass-rim.patch"
patch_image_glow="$repo_root/docs/patches/0009-image-glow.patch"
patch_glow_taps="$repo_root/docs/patches/0010-image-glow-taps.patch"
patch_cutout="$repo_root/docs/patches/0011-desktop-cutout.patch"
patch_tube_blur="$repo_root/docs/patches/0012-tube-blur.patch"

[ -n "$zed_rev" ] || { echo "zed_rev not found in app/Cargo.toml" >&2; exit 1; }
for p in "$patch_crt" "$patch_blur" "$patch_crawl" "$patch_tubes" "$patch_glyph" "$patch_gpl" "$patch_lazy" "$patch_fit" "$patch_rim" "$patch_image_glow" "$patch_glow_taps" "$patch_cutout" "$patch_tube_blur"; do
    [ -f "$p" ] || { echo "missing $p" >&2; exit 1; }
done

# `-e`, not `-d`: a checkout made with `git worktree add` has a .git FILE, and
# taking that for "no checkout" re-initialises it.
if [ ! -e "$zed_dir/.git" ]; then
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
# 0005-glyph-transform touches crates/gpui only, so it is independent of the
# renderer patches above and could apply anywhere in this list.
# sentinel: the ambient text transform present means the patch is already in.
apply_patch "$patch_glyph" "glyph-transform" \
    'grep -rqF "with_text_transformation" crates/gpui/src'
# sentinel: ztracing/zlog gone from sum_tree means the sever is already in the tree
apply_patch "$patch_gpl" "sever-gpl-crates" \
    '! grep -rqF "ztracing" crates/sum_tree'
# 0006-lazy-path-targets moves one call in the renderer's draw, which 0001-0004
# also touch, so it applies after them.
# sentinel: its marker comment present means the deferral is already in.
apply_patch "$patch_lazy" "lazy-path-targets" \
    'grep -rqF "td-lazy-path-targets" crates/gpui_wgpu/src'
# 0007-path-targets-fit-the-paths rewrites the call 0006 guards, so it applies
# after 0006.
# sentinel: its marker comment present means the fitted targets are already in.
apply_patch "$patch_fit" "path-targets-fit-the-paths" \
    'grep -rqF "td-path-targets-fit" crates/gpui_wgpu/src'
# 0008-glass-rim edits the tube branch of crt_pass.wgsl, which 0001 creates and
# 0003 rewrites, so it applies after them.
# sentinel: its marker comment present means the rim is already in.
apply_patch "$patch_rim" "glass-rim" \
    'grep -rqF "td-glass-rim" crates/gpui_wgpu/src'

# Terminal-image bloom is a delta after the bezel/32-tube patch stack.
apply_patch "$patch_image_glow" "image-glow" \
    'grep -rqF "td-image-glow" crates/gpui_wgpu/src/crt_pass.wgsl'

# Keep this separate from 0009 so an existing experimental checkout upgrades
# the shader instead of satisfying only the older image-glow API sentinel.
apply_patch "$patch_glow_taps" "image-glow-taps" \
    'grep -qF "Explicit taps preserve the kernel" crates/gpui_wgpu/src/crt_pass.wgsl'

# Replace the pixels under desktop-facing glass before normal foreground draws.
apply_patch "$patch_cutout" "desktop-cutout" \
    'grep -qF "is_desktop_cutout" crates/gpui/src/color.rs'

apply_patch "$patch_tube_blur" "tube-blur" \
    'grep -qF "td-tube-blur" crates/gpui_wgpu/src/crt_pass.wgsl'
