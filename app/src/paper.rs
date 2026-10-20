//! Paper furniture: a stable deckled silhouette and a shared grain texture.
//! Nothing here transforms glyphs, changes layout or requests animation frames.

use std::sync::{Arc, OnceLock};

use gpui::{
    canvas, div, fill, point, px, size, Bounds, BoxShadow, ContentMask, Div, Hsla, ParentElement,
    PathBuilder, Pixels, RenderImage, Styled, Window,
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
                let pigment = hash((y / 8) * 67 + x / 8) % 7;
                let fibre = if hash((y / 2) * 89 + x / 6).is_multiple_of(29) {
                    8
                } else {
                    0
                };
                let c = if n & 1 == 0 { 0 } else { 255 };
                image::Rgba([c, c, c, (n % 6 + pigment + fibre) as u8])
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

/// One translucent highlighter stroke through the writing.
/// The overhang is visual only; hit targets and text layout remain unchanged.
fn stroke(bounds: Bounds<Pixels>, ink: Hsla, window: &mut Window) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    let mid = h * 0.5;
    let half = (h * 0.60).clamp(3., 14.);
    polygon(
        bounds,
        &[
            (-2., mid - half + 1.4),
            (w * 0.30, mid - half - 0.8),
            (w + 1., mid - half + 0.6),
            (w + 3., mid + half - 0.9),
            (w * 0.64, mid + half + 0.8),
            (-3., mid + half - 0.6),
        ],
        ink.alpha(0.22),
        window,
    );
}

/// Soft contact shadow plus a compressed cotton edge, never a luminous frame.
pub fn shadows() -> Vec<BoxShadow> {
    vec![
        BoxShadow {
            color: gpui::black().alpha(0.34),
            offset: point(px(2.), px(5.)),
            blur_radius: px(9.),
            spread_radius: px(-1.),
            inset: false,
        },
        BoxShadow {
            color: gpui::black().alpha(0.28),
            offset: point(px(1.), px(2.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        },
    ]
}

fn cast_shadow(bounds: Bounds<Pixels>, window: &mut Window) {
    window.paint_drop_shadows(bounds, Default::default(), &shadows());
}

fn edge_path(
    bounds: Bounds<Pixels>,
    points: &[(f32, f32)],
    ink: Hsla,
    width: f32,
    window: &mut Window,
) {
    let Some((first, rest)) = points.split_first() else {
        return;
    };
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(bounds.origin + point(px(first.0), px(first.1)));
    for p in rest {
        path.line_to(bounds.origin + point(px(p.0), px(p.1)));
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, ink);
    }
}

fn cotton_edge(bounds: Bounds<Pixels>, points: &[(f32, f32)], window: &mut Window) {
    // Each edge has its own sample count: long rails and square tiles must
    // receive light on the same physical sides, not at a quarter of the list.
    let horizontal = (f32::from(bounds.size.width) / 6.).ceil().clamp(2., 24.) as usize;
    let vertical = (f32::from(bounds.size.height) / 6.).ceil().clamp(2., 24.) as usize;
    let lower: Vec<_> = points[horizontal..2 * horizontal + vertical + 1]
        .iter()
        .map(|(x, y)| (*x + 0.8, *y + 1.6))
        .collect();
    edge_path(bounds, &lower, gpui::black().alpha(0.38), 3.2, window);
    edge_path(
        bounds,
        &points[..horizontal + 1],
        gpui::white().alpha(0.28),
        1.3,
        window,
    );
    let left: Vec<_> = points[2 * horizontal + vertical..]
        .iter()
        .copied()
        .chain(points.first().copied())
        .collect();
    edge_path(bounds, &left, gpui::white().alpha(0.14), 1.1, window);
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
/// the paper fill is painted over text. The marker crosses the control centre.
pub fn backing(face: Hsla, accent: Hsla, active: bool) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let pts = outline(w, h);
                cast_shadow(bounds, window);
                let edge: Vec<_> = pts.iter().map(|(x, y)| (*x + 0.6, *y + 2.4)).collect();
                polygon(bounds, &edge, crate::darken(face, 0.65), window);
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
                cotton_edge(bounds, &pts, window);
                if active {
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
                cotton_edge(bounds, &pts, window);
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
