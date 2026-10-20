//! Paper furniture: a stable deckled silhouette and a shared grain texture.
//! Nothing here transforms glyphs, changes layout or requests animation frames.

use std::sync::{Arc, OnceLock};

use gpui::{
    canvas, div, fill, point, px, size, Bounds, ContentMask, Div, Hsla, ParentElement, PathBuilder,
    Pixels, RenderImage, Styled, Window,
};

const TILE: u32 = 512;
static GRAIN: OnceLock<Arc<RenderImage>> = OnceLock::new();

fn hash(mut n: u32) -> u32 {
    n ^= n >> 16;
    n = n.wrapping_mul(0x7feb_352d);
    n ^= n >> 15;
    n = n.wrapping_mul(0x846c_a68b);
    n ^ (n >> 16)
}

fn texture() -> Arc<RenderImage> {
    GRAIN
        .get_or_init(|| {
            let pixels = image::RgbaImage::from_fn(TILE, TILE, |x, y| {
                let n = hash(y * TILE + x);
                let pigment = hash((y / 8) * 24 + x / 8) % 5;
                let c = if n & 1 == 0 { 0 } else { 255 };
                image::Rgba([c, c, c, (n % 5 + pigment / 2) as u8])
            });
            Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
        })
        .clone()
}

/// One shared atlas image per region. Sampling a continuous sheet avoids the
/// visible tile joins that fractional display scaling exposed on pale stock.
pub fn paint_grain(bounds: Bounds<Pixels>, window: &mut Window) {
    let image = texture();
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        if let Err(e) = window.paint_image(bounds, Default::default(), image, 0, false) {
            eprintln!("terminal-delight: paper grain unavailable: {e}");
        }
    });
}

pub fn grain() -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            |bounds, _, window, _| paint_grain(bounds, window),
        )
        .size_full(),
    )
}

/// Four edges with bounded detail, always inside the rectangular hit target.
/// Dimensions supply the seed, so rebuilding a view never makes a fibre crawl.
pub fn outline(w: f32, h: f32) -> Vec<(f32, f32)> {
    let mut points = Vec::with_capacity(100);
    let inset = 1.6_f32.min(w / 5.).min(h / 5.);
    let seed = w.to_bits() ^ h.to_bits().rotate_left(13);
    for edge in 0..4_u32 {
        let length = if edge % 2 == 0 { w } else { h };
        let steps = (length / 6.).ceil().clamp(2., 24.) as u32;
        for i in 0..steps {
            let t = i as f32 / steps as f32;
            let jitter = (hash(seed ^ (edge * 73 + i)) % 1024) as f32 / 1024. - 0.5;
            let d = inset + jitter * inset * 0.9;
            points.push(match edge {
                0 => (inset + t * (w - 2. * inset), d),
                1 => (w - d, inset + t * (h - 2. * inset)),
                2 => (w - inset - t * (w - 2. * inset), h - d),
                _ => (d, h - inset - t * (h - 2. * inset)),
            });
        }
    }
    points
}

fn polygon(bounds: Bounds<Pixels>, points: &[(f32, f32)], ink: Hsla, window: &mut Window) {
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    let mut path = PathBuilder::fill();
    path.move_to(bounds.origin + point(px(first.0), px(first.1)));
    for p in rest {
        path.line_to(bounds.origin + point(px(p.0), px(p.1)));
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, ink);
    }
}

/// A hand-rubbed pigment stroke. Stable flecks break the edge without animation.
fn stroke(bounds: Bounds<Pixels>, ink: Hsla, window: &mut Window) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let y = (h - 6.).max(0.);
    let points = [
        (3., y + 1.4),
        (w * 0.29, y),
        (w - 4., y + 0.8),
        (w - 2., y + 3.4),
        (w * 0.64, y + 4.6),
        (2., y + 3.8),
    ];
    polygon(bounds, &points, ink.alpha(0.55), window);
    for i in 0..14 {
        let x = 3. + (w - 7.).max(0.) * i as f32 / 14.;
        let d = (hash(i) % 100) as f32 / 100.;
        window.paint_quad(fill(
            Bounds::new(
                bounds.origin + point(px(x), px(y + d * 4.)),
                size(px(1.7), px(0.6)),
            ),
            ink.alpha(0.75),
        ));
    }
}

pub fn selection(ink: Hsla) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| stroke(bounds, ink, window),
        )
        .size_full(),
    )
}

/// A chalk score on stock, with a movable torn-paper thumb. Input and bounds
/// stay with the existing gauge; this child only paints its material.
pub fn gauge(frac: f32, face: Hsla, ink: Hsla, accent: Hsla) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let mid = h * 0.5;
                window.paint_quad(fill(
                    Bounds::new(
                        bounds.origin + point(px(0.), px(mid - 1.)),
                        size(px(w), px(2.)),
                    ),
                    ink.alpha(0.2),
                ));
                let lit = Bounds::new(
                    bounds.origin + point(px(0.), px(mid - 4.)),
                    size(px(w * frac.clamp(0., 1.)), px(9.)),
                );
                if w * frac > 6. {
                    stroke(lit, accent, window);
                }
                let x = (w * frac - 5.).clamp(0., (w - 10.).max(0.));
                let knob = Bounds::new(
                    bounds.origin + point(px(x), px(mid - 6.)),
                    size(px(10.), px(12.)),
                );
                polygon(knob, &outline(10., 12.), ink, window);
                let inner = Bounds::new(knob.origin + point(px(1.), px(1.)), size(px(8.), px(10.)));
                polygon(inner, &outline(8., 10.), face, window);
            },
        )
        .size_full(),
    )
}

/// Full backing. Add this before the control's label, so neither the grain nor
/// the paper fill is painted over text. The accent is a small lower-edge stamp.
pub fn backing(face: Hsla, accent: Hsla, active: bool) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let pts = outline(w, h);
                let shadow: Vec<_> = pts.iter().map(|(x, y)| (*x, *y + 1.4)).collect();
                polygon(bounds, &shadow, gpui::black().alpha(0.18), window);
                let mut face = face;
                if bounds.contains(&window.mouse_position()) {
                    face.l = (face.l + 0.025).min(1.0);
                }
                polygon(bounds, &pts, face, window);
                let inner = Bounds::new(
                    bounds.origin + point(px(3.), px(3.)),
                    size(px((w - 6.).max(0.)), px((h - 6.).max(0.))),
                );
                paint_grain(inner, window);
                if active {
                    let top = (h * 0.5).max(h - 19.);
                    polygon(
                        bounds,
                        &[
                            (3., top + 3.),
                            (w * 0.4, top),
                            (w - 3., top + 1.),
                            (w - 2., h - 4.),
                            (w * 0.6, h - 2.),
                            (2., h - 3.),
                        ],
                        accent.alpha(0.28),
                        window,
                    );
                    stroke(bounds, accent, window);
                }
            },
        )
        .size_full(),
    )
}

/// Edge-only treatment for containers whose labels have already been added by
/// their caller. It never paints inside the label's area.
pub fn rim(face: Hsla, accent: Hsla, active: bool) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let pts = outline(w, h);
                let mut path = PathBuilder::stroke(px(0.8));
                path.move_to(bounds.origin + point(px(pts[0].0), px(pts[0].1)));
                for p in &pts[1..] {
                    path.line_to(bounds.origin + point(px(p.0), px(p.1)));
                }
                path.close();
                if let Ok(path) = path.build() {
                    window.paint_path(path, face);
                }
                if active {
                    stroke(bounds, accent, window);
                }
            },
        )
        .size_full(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paper_edges_are_still_bounded_at_tiny_and_large_sizes() {
        for (w, h) in [(8., 8.), (26., 17.), (140., 35.), (3000., 1600.)] {
            let pts = outline(w, h);
            assert_eq!(pts, outline(w, h), "a redraw must not move the fibres");
            assert!(pts.len() <= 96, "detail has a fixed upper bound");
            assert!(pts
                .iter()
                .all(|(x, y)| *x >= 0. && *x <= w && *y >= 0. && *y <= h));
            assert!(
                pts.iter().any(|(x, _)| x.fract() != 0.),
                "not a square fallback"
            );
        }
    }

    #[test]
    fn all_surfaces_share_one_grain_image() {
        assert!(Arc::ptr_eq(&texture(), &texture()));
        assert_eq!(texture().size(0), gpui::size(TILE.into(), TILE.into()));
    }
}
