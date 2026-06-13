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

/// One registered tube: (rect[x,y,w,h] physical px, glass glare, k1, k2). Each
/// tube carries the barrel curvature of *its own* pane theme, so a bent pane
/// bows even when the window theme is flat (and a flat pane stays flat beside a
/// bent one).
type Tube = ([f32; 4], f32, f32, f32);

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
}

/// Register one pane's tube for this frame: its content rect (physical px),
/// glass glare, and its own barrel curvature (k1, k2) from its resolved theme.
pub fn register_tube(rect: [f32; 4], glare: f32, k1: f32, k2: f32) {
    if SUPPRESSED.load(Ordering::Relaxed) {
        return;
    }
    let mut rects = RECTS.lock().unwrap();
    if rects.len() < 8 {
        rects.push((rect, glare.clamp(0.0, 1.0), k1, k2));
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
    let (_, _, k1, k2) = rects[i];
    (k1, k2)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The statics are process-global; this is the only test that touches them,
    // so it owns the sequence start-to-finish and restores the default at the end.
    #[test]
    fn suppression_stops_tubes_from_registering() {
        let r = [0.0, 0.0, 100.0, 100.0];
        begin_frame();
        set_suppressed(false);
        register_tube(r, 0.5, 0.14, 0.06);
        register_tube(r, 0.5, 0.14, 0.06);
        assert_eq!(rect_count(), 2, "tubes register while the glass is live");

        // an open menu suppresses: begin_frame clears, and nothing re-registers
        begin_frame();
        set_suppressed(true);
        register_tube(r, 0.5, 0.14, 0.06);
        register_tube(r, 0.5, 0.14, 0.06);
        assert_eq!(rect_count(), 0, "an open overlay flattens the whole screen");

        // closing the menu restores warping on the next frame
        begin_frame();
        set_suppressed(false);
        register_tube(r, 0.5, 0.14, 0.06);
        assert_eq!(rect_count(), 1, "warp resumes once the overlay closes");

        // never bank more than the 8 the renderer reads
        begin_frame();
        for _ in 0..12 {
            register_tube(r, 0.5, 0.14, 0.06);
        }
        assert_eq!(rect_count(), 8, "the tube set is capped at the shader's 8");

        // per-pane override: a flat (tactical) tube and a bent (hacker) tube
        // each keep their OWN curvature — the window theme doesn't flatten or
        // bend its neighbours. This is what makes the sub-tab theme an override.
        begin_frame();
        set_suppressed(false);
        register_tube(r, 0.4, 0.0, 0.0); // a no-bend pane
        register_tube(r, 0.4, 0.14, 0.06); // a bent pane
        assert_eq!(rect_count(), 2);
        assert_eq!(rect_curvature(0), (0.0, 0.0), "flat pane stays flat");
        assert_eq!(rect_curvature(1), (0.14, 0.06), "bent pane keeps its bend");

        set_suppressed(false);
    }
}
