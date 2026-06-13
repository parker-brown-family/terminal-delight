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

static RECTS: Mutex<Vec<([f32; 4], f32)>> = Mutex::new(Vec::new());
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

pub fn register_with_glare(rect: [f32; 4], glare: f32) {
    if SUPPRESSED.load(Ordering::Relaxed) {
        return;
    }
    let mut rects = RECTS.lock().unwrap();
    if rects.len() < 8 {
        rects.push((rect, glare.clamp(0.0, 1.0)));
    }
    push(&rects);
}

#[allow(unused_variables)]
fn push(rects: &[([f32; 4], f32)]) {
    #[cfg(target_os = "linux")]
    gpui_wgpu::set_crt_rects_with_glare(rects);
}

#[cfg(test)]
fn rect_count() -> usize {
    RECTS.lock().unwrap().len()
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
        register_with_glare(r, 0.5);
        register_with_glare(r, 0.5);
        assert_eq!(rect_count(), 2, "tubes register while the glass is live");

        // an open menu suppresses: begin_frame clears, and nothing re-registers
        begin_frame();
        set_suppressed(true);
        register_with_glare(r, 0.5);
        register_with_glare(r, 0.5);
        assert_eq!(rect_count(), 0, "an open overlay flattens the whole screen");

        // closing the menu restores warping on the next frame
        begin_frame();
        set_suppressed(false);
        register_with_glare(r, 0.5);
        assert_eq!(rect_count(), 1, "warp resumes once the overlay closes");

        // never bank more than the 8 the renderer reads
        begin_frame();
        for _ in 0..12 {
            register_with_glare(r, 0.5);
        }
        assert_eq!(rect_count(), 8, "the tube set is capped at the shader's 8");

        set_suppressed(false);
    }
}
