//! Per-pane CRT warp registry. Each visible pane registers its content rect
//! (physical px) during prepaint; the renderer's td-crt-pass warps exactly
//! those rects, leaving chrome flat so hit-testing stays honest.
//! The workspace clears the set at the start of every frame.

use std::sync::Mutex;

static RECTS: Mutex<Vec<[f32; 4]>> = Mutex::new(Vec::new());

pub fn begin_frame() {
    let mut rects = RECTS.lock().unwrap();
    rects.clear();
    push(&rects);
}

pub fn register(rect: [f32; 4]) {
    let mut rects = RECTS.lock().unwrap();
    if rects.len() < 8 {
        rects.push(rect);
    }
    push(&rects);
}

#[allow(unused_variables)]
fn push(rects: &[[f32; 4]]) {
    #[cfg(target_os = "linux")]
    gpui_wgpu::set_crt_rects(rects);
}
