//! Paper furniture: a stable deckled silhouette and a shared grain texture.
//! Nothing here transforms glyphs, changes layout or requests animation frames.

use std::sync::{Arc, OnceLock};

use gpui::{
    canvas, div, fill, point, px, size, Bounds, ContentMask, Div, Hsla, ParentElement, PathBuilder,
    Pixels, RenderImage, Styled, Window,
};

const TILE: u32 = 192;
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
                image::Rgba([c, c, c, (n % 13 + pigment) as u8])
            });
            Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
        })
        .clone()
}

/// One atlas image, reused across every surface. A content mask clips the final
/// tile instead of resizing it: grain has the same physical size in every panel.
pub fn paint_grain(bounds: Bounds<Pixels>, window: &mut Window) {
    let image = texture();
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let w = f32::from(bounds.size.width);
        let h = f32::from(bounds.size.height);
        for y in 0..(h / TILE as f32).ceil() as u32 {
            for x in 0..(w / TILE as f32).ceil() as u32 {
                let tile = Bounds::new(
                    bounds.origin + point(px((x * TILE) as f32), px((y * TILE) as f32)),
                    size(px(TILE as f32), px(TILE as f32)),
                );
                if let Err(e) =
                    window.paint_image(tile, Default::default(), image.clone(), 0, false)
                {
                    // Do not turn an unavailable atlas entry into a substitute
                    // texture or a panic; the paper's base colour still draws.
                    eprintln!("terminal-delight: paper grain unavailable: {e}");
                    return;
                }
            }
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
                    window.paint_quad(fill(
                        Bounds::new(
                            bounds.origin + point(px(5.), px((h - 4.).max(0.))),
                            size(px((w * 0.24).clamp(3., 18.)), px(1.8)),
                        ),
                        accent,
                    ));
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
                    window.paint_quad(fill(
                        Bounds::new(
                            bounds.origin + point(px(5.), px((h - 3.).max(0.))),
                            size(px((w * 0.2).clamp(3., 18.)), px(2.)),
                        ),
                        accent,
                    ));
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
