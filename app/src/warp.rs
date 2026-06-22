//! Per-pane CRT warp registry. Each visible pane registers its content rect
//! (physical px) during prepaint; the renderer's td-crt-pass warps exactly
//! those rects, leaving chrome flat so hit-testing stays honest.
//! The workspace clears the set at the start of every frame.
//!
//! When an overlay panel is open (a theme breakout or a confirm dialog) the
//! workspace suppresses the pass for that frame: the barrel warp is a pixel
//! post-process, so a panel floating over a tube would bow with the glass while
//! gpui keeps hit-testing its flat layout box — visibly off-target. Suppressing
//! flattens the whole screen behind the panel so what you see is what you click.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// One registered tube: (rect[x,y,w,h] physical px, glass glare, k1, k2, crawl
/// `[enabled, a, depth]`). Each tube carries the barrel curvature *and* the
/// crawl perspective of *its own* pane theme, so a bent/crawling pane stays
/// independent of its neighbours. `crawl[0]` is `1.0` when crawl is on (else
/// `0.0`), `crawl[1]` the top-edge width ratio `a`, `crawl[2]` the depth ratio
/// — see `theme::crawl_coeffs` and `fs_crt` in `crt_pass.wgsl`.
type Tube = ([f32; 4], f32, f32, f32, [f32; 3]);

/// Max simultaneous warp tubes the renderer reads. MUST match the `array<…, N>`
/// sizes in `crt_pass.wgsl` and the `[_; N]` packing in `gpui_wgpu::wgpu_renderer`.
/// Raised from 8 → 32 so the agent-wall can warp each card's logo square.
pub const MAX_TUBES: usize = 32;

static RECTS: Mutex<Vec<Tube>> = Mutex::new(Vec::new());
static SUPPRESSED: AtomicBool = AtomicBool::new(false);

/// Suppress the warp pass for the current frame (set in the workspace render
/// before panes paint). While suppressed no tube registers, so the renderer's
/// `rect_count` is zero and the pass is a no-op — the glass reads flat.
pub fn set_suppressed(suppressed: bool) {
    SUPPRESSED.store(suppressed, Ordering::Relaxed);
}

pub fn begin_frame() {
    let mut rects = RECTS.lock().unwrap();
    rects.clear();
    push(&rects);
    // The FOCUS backdrop blur is re-armed each frame by the panel's canvas while
    // the reading modal is open; clear it here so it switches off the moment the
    // modal closes (no canvas → no re-arm → blur gone next frame).
    clear_focus_blur();
}

/// Arm the FOCUS reading-modal backdrop blur for this frame. `rect` is the panel
/// (physical px) that stays sharp; everything outside it is frosted. Called from
/// the panel's measurement canvas during prepaint so the rect is pixel-exact.
pub fn set_focus_blur(rect: [f32; 4], radius: f32, feather: f32, tint: f32, corner: f32) {
    #[cfg(target_os = "linux")]
    gpui_wgpu::set_focus_blur(rect, radius, feather, tint, corner);
    #[cfg(not(target_os = "linux"))]
    let _ = (rect, radius, feather, tint, corner);
}

/// Disable the FOCUS backdrop blur (radius 0 = inert + gates the pass off).
pub fn clear_focus_blur() {
    set_focus_blur([0.0; 4], 0.0, 0.0, 0.0, 0.0);
}

/// Register the FOCUS reader panel itself as the ONE warp tube for this frame —
/// the "Inherit theme" path, so the reading modal bends + glares like the pane
/// it mirrors instead of reading flat. Unlike [`register_tube`] this ignores the
/// modal suppression (a reader open normally flattens the glass) and *replaces*
/// the tube set with exactly this rect: the panes behind are dimmed + frosted,
/// so only the panel's own curvature is visible and there's no rect-ordering
/// ambiguity over the panel pixels. `crawl` is normally identity — the modal
/// already centres crawl rows for readable mirroring, so we inherit curve + glare
/// only. Called from the panel's measurement canvas right beside `set_focus_blur`.
pub fn register_focus_tube(rect: [f32; 4], glare: f32, k1: f32, k2: f32, crawl: [f32; 3]) {
    let mut rects = RECTS.lock().unwrap();
    rects.clear();
    rects.push((rect, glare.clamp(0.0, 1.0), k1, k2, crawl));
    push(&rects);
}

/// Register one pane's tube for this frame: its content rect (physical px),
/// glass glare, its own barrel curvature (k1, k2), and its crawl perspective
/// (`crawl = [enabled, a, depth]`, all `0`/identity when crawl is off), each
/// from its resolved theme.
pub fn register_tube(rect: [f32; 4], glare: f32, k1: f32, k2: f32, crawl: [f32; 3]) {
    if SUPPRESSED.load(Ordering::Relaxed) {
        return;
    }
    let mut rects = RECTS.lock().unwrap();
    if rects.len() < MAX_TUBES {
        rects.push((rect, glare.clamp(0.0, 1.0), k1, k2, crawl));
    }
    push(&rects);
}

/// Register one OVERLAY tube (an agent-wall card's logo square) — like
/// [`register_tube`] but it IGNORES suppression. The dashboard is a suppressed
/// overlay (so the panes behind read flat), yet we still want each card's art to
/// bend by the theme's own curvature. The whole card stays a flat click target;
/// only these logo-square pixels are post-warped. Appends (capped at `MAX_TUBES`)
/// so many cards coexist — unlike `register_focus_tube`, which replaces the set.
pub fn register_overlay_tube(rect: [f32; 4], glare: f32, k1: f32, k2: f32, crawl: [f32; 3]) {
    let mut rects = RECTS.lock().unwrap();
    if rects.len() < MAX_TUBES {
        rects.push((rect, glare.clamp(0.0, 1.0), k1, k2, crawl));
    }
    push(&rects);
}

#[allow(unused_variables)]
fn push(rects: &[Tube]) {
    #[cfg(target_os = "linux")]
    gpui_wgpu::set_crt_rects_tubes(rects);
}

#[cfg(test)]
fn rect_count() -> usize {
    RECTS.lock().unwrap().len()
}

#[cfg(test)]
fn rect_curvature(i: usize) -> (f32, f32) {
    let rects = RECTS.lock().unwrap();
    let (_, _, k1, k2, _) = rects[i];
    (k1, k2)
}

#[cfg(test)]
fn rect_crawl(i: usize) -> [f32; 3] {
    let rects = RECTS.lock().unwrap();
    rects[i].4
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crawl triad with crawl disabled (identity: off, no taper, flat depth).
    const CRAWL_OFF: [f32; 3] = [0.0, 1.0, 1.0];

    // The statics are process-global; this is the only test that touches them,
    // so it owns the sequence start-to-finish and restores the default at the end.
    #[test]
    fn suppression_stops_tubes_from_registering() {
        let r = [0.0, 0.0, 100.0, 100.0];
        begin_frame();
        set_suppressed(false);
        register_tube(r, 0.5, 0.14, 0.06, CRAWL_OFF);
        register_tube(r, 0.5, 0.14, 0.06, CRAWL_OFF);
        assert_eq!(rect_count(), 2, "tubes register while the glass is live");

        // an open menu suppresses: begin_frame clears, and nothing re-registers
        begin_frame();
        set_suppressed(true);
        register_tube(r, 0.5, 0.14, 0.06, CRAWL_OFF);
        register_tube(r, 0.5, 0.14, 0.06, CRAWL_OFF);
        assert_eq!(rect_count(), 0, "an open overlay flattens the whole screen");

        // closing the menu restores warping on the next frame
        begin_frame();
        set_suppressed(false);
        register_tube(r, 0.5, 0.14, 0.06, CRAWL_OFF);
        assert_eq!(rect_count(), 1, "warp resumes once the overlay closes");

        // never bank more than MAX_TUBES the renderer reads
        begin_frame();
        for _ in 0..(MAX_TUBES + 8) {
            register_tube(r, 0.5, 0.14, 0.06, CRAWL_OFF);
        }
        assert_eq!(
            rect_count(),
            MAX_TUBES,
            "the tube set is capped at the shader's MAX_TUBES"
        );

        // per-pane override: a flat (tactical) tube and a bent (hacker) tube
        // each keep their OWN curvature — the window theme doesn't flatten or
        // bend its neighbours. This is what makes the sub-tab theme an override.
        begin_frame();
        set_suppressed(false);
        register_tube(r, 0.4, 0.0, 0.0, CRAWL_OFF); // a no-bend pane
        register_tube(r, 0.4, 0.14, 0.06, CRAWL_OFF); // a bent pane
        assert_eq!(rect_count(), 2);
        assert_eq!(rect_curvature(0), (0.0, 0.0), "flat pane stays flat");
        assert_eq!(rect_curvature(1), (0.14, 0.06), "bent pane keeps its bend");

        // FOCUS "Inherit theme": a reader open suppresses the normal pane tubes,
        // but register_focus_tube bypasses that and REPLACES the set with exactly
        // the panel — so the reader bends by its own pane's curvature and there's
        // no rect-ordering ambiguity over the panel pixels.
        begin_frame();
        set_suppressed(true); // a reader open flattens the panes behind
        register_tube(r, 0.5, 0.14, 0.06, CRAWL_OFF); // no-op while suppressed
        assert_eq!(rect_count(), 0, "panes don't register under the reader");
        register_focus_tube(r, 0.4, 0.20, 0.08, CRAWL_OFF);
        assert_eq!(
            rect_count(),
            1,
            "the focus tube registers despite suppression"
        );
        assert_eq!(
            rect_curvature(0),
            (0.20, 0.08),
            "the reader bends by its pane's own curvature"
        );

        set_suppressed(false);
    }

    // Crawl is per-pane like curvature: each tube carries its own crawl triad,
    // so a crawling pane and a plain pane coexist without leaking into each other.
    #[test]
    fn crawl_triad_is_per_pane_and_passes_through() {
        let r = [0.0, 0.0, 100.0, 100.0];
        begin_frame();
        set_suppressed(false);
        register_tube(r, 0.4, 0.14, 0.06, CRAWL_OFF); // plain pane
        register_tube(r, 0.4, 0.14, 0.06, [1.0, 0.6, 2.5]); // a crawling pane
        assert_eq!(rect_count(), 2);
        assert_eq!(rect_crawl(0), CRAWL_OFF, "plain pane carries no crawl");
        assert_eq!(
            rect_crawl(1),
            [1.0, 0.6, 2.5],
            "crawling pane keeps its triad"
        );
        set_suppressed(false);
    }

    // Mirror of the WGSL `panel_mask` in crt_pass.wgsl (rounded-box SDF + a
    // smoothstep feather). The shader is the runtime authority; this Rust copy
    // exists so the masking spec — sharp inside the FOCUS panel, frosted
    // outside, clean rounded corners — is locked by a unit test. Keep in sync.
    fn rounded_box_mask(px: f32, py: f32, rect: [f32; 4], corner: f32, feather: f32) -> f32 {
        let (cx, cy) = (rect[0] + rect[2] * 0.5, rect[1] + rect[3] * 0.5);
        let (hx, hy) = (rect[2] * 0.5, rect[3] * 0.5);
        let qx = (px - cx).abs() - hx + corner;
        let qy = (py - cy).abs() - hy + corner;
        let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
        let sdf = qx.max(qy).min(0.0) + outside - corner;
        let f = feather.max(1e-4);
        let t = ((sdf + f * 0.5) / f).clamp(0.0, 1.0); // smoothstep(-f/2, f/2, sdf)
        1.0 - t * t * (3.0 - 2.0 * t)
    }

    #[test]
    fn focus_panel_mask_is_sharp_inside_and_frosted_outside() {
        let rect = [100.0, 100.0, 400.0, 300.0]; // x, y, w, h → right edge x=500
        let (corner, feather) = (12.0, 16.0);
        let inside = rounded_box_mask(300.0, 250.0, rect, corner, feather);
        assert!(inside > 0.99, "panel interior must be sharp, got {inside}");
        let outside = rounded_box_mask(700.0, 250.0, rect, corner, feather);
        assert!(outside < 0.01, "backdrop must be frosted, got {outside}");
        // monotonic falloff across the right edge (the feathered seam)
        let just_in = rounded_box_mask(496.0, 250.0, rect, corner, feather);
        let just_out = rounded_box_mask(508.0, 250.0, rect, corner, feather);
        assert!(just_in > just_out, "mask falls off across the panel edge");
        // the rounded corner clips OUTSIDE (a hard rect would read this as inside)
        let corner_pt = rounded_box_mask(499.0, 399.0, rect, corner, feather);
        assert!(
            corner_pt < 0.5,
            "rounded corner reads as outside, got {corner_pt}"
        );
    }
}
