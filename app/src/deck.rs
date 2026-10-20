//! The OPS skin's drawn devices — the shapes gpui has no property for.
//!
//! gpui styles a quad: a radius, a border, a fill. The command-deck look needs
//! three things a quad cannot say: a corner CUT rather than rounded (XCOM 2's
//! panels), a gauge made of separate slanted pips (XCOM's health bars), and a
//! selection bar that ends in an arrow tip (Fallout 4's Pip-Boy). Each is a
//! canvas painted as the FIRST child of the element that wears it, so the
//! element's own content paints over it — the same arrangement `paper` uses
//! for its rims and backings, and for the same reason.
//!
//! Every function here takes its colours from the caller, who takes them from
//! the skin, who takes them from the palette. Nothing in this file knows a hex
//! value, which is what lets one skin be cyan under Tactical and phosphor green
//! under an Omarchy theme.

use gpui::{
    canvas, div, fill, point, prelude::*, px, size, Bounds, Div, Hsla, PathBuilder, Pixels, Window,
};

/// The six corners of a plate with its top-left and bottom-right corners cut,
/// in local coordinates, clockwise from the top edge.
///
/// Two corners, not four: a panel cut on all four sides reads as an octagon, a
/// sign. Cut on one diagonal it reads as a slab that has been machined, which
/// is the XCOM panel. The cut never takes more than a third of the shorter
/// side, so a short chip stays a chip rather than becoming a parallelogram.
pub fn chamfer(w: f32, h: f32, cut: f32) -> [(f32, f32); 6] {
    let c = cut.clamp(0., w.min(h) / 3.);
    [(c, 0.), (w, 0.), (w, h - c), (w - c, h), (0., h), (0., c)]
}

fn fill_poly(bounds: Bounds<Pixels>, pts: &[(f32, f32)], ink: Hsla, window: &mut Window) {
    let Some((first, rest)) = pts.split_first() else {
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

fn stroke_poly(
    bounds: Bounds<Pixels>,
    pts: &[(f32, f32)],
    ink: Hsla,
    width: f32,
    window: &mut Window,
) {
    let Some((first, rest)) = pts.split_first() else {
        return;
    };
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(bounds.origin + point(px(first.0), px(first.1)));
    for p in rest {
        path.line_to(bounds.origin + point(px(p.0), px(p.1)));
    }
    path.close();
    if let Ok(path) = path.build() {
        window.paint_path(path, ink);
    }
}

/// A cut plate behind a region: its fill, its edge, and — for something that
/// floats over other things — a hard drop shadow cut to the same shape.
///
/// The shadow is a second plate offset down and right rather than a blurred
/// box-shadow, because a box-shadow is a rounded RECTANGLE and would show dark
/// triangles in the two corners the plate has cut away. A hard offset shadow
/// is also simply the right shadow for an instrument panel: a printed deck has
/// no soft light in it.
pub fn plate(face: Hsla, edge: Hsla, cut: f32, floating: bool) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let pts = chamfer(w, h, cut);
                if floating {
                    let drop: Vec<_> = pts.iter().map(|(x, y)| (*x + 3., *y + 4.)).collect();
                    fill_poly(bounds, &drop, gpui::black().alpha(0.42), window);
                }
                fill_poly(bounds, &pts, face, window);
                // The stroke sits half a pixel inside the plate so it is never
                // clipped by a parent's overflow edge.
                let inset: Vec<_> = chamfer(w - 1., h - 1., cut - 0.5)
                    .iter()
                    .map(|(x, y)| (*x + 0.5, *y + 0.5))
                    .collect();
                stroke_poly(bounds, &inset, edge, 1., window);
            },
        )
        .size_full(),
    )
}

/// The ink of one pip in a gauge: lit, unlit, or unknown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pip {
    Lit,
    Dark,
    /// No reading. Drawn hollow and dashed rather than dark, because a dark pip
    /// is a measured zero and an unknown is not one.
    Unknown,
}

/// How many of `n` pips a fraction lights. A reading above zero always lights
/// at least one pip, so "barely used" and "unused" never draw the same; a
/// reading short of full never lights the last, so "nearly gone" and "gone"
/// never draw the same either.
pub fn lit_count(frac: f32, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let f = frac.clamp(0., 1.);
    if f <= 0. {
        return 0;
    }
    if f >= 1. {
        return n;
    }
    ((f * n as f32).round() as usize).clamp(1, n - 1)
}

/// The states of an `n`-pip gauge for one reading, `None` being no reading.
pub fn pips_for(frac: Option<f32>, n: usize) -> Vec<Pip> {
    match frac {
        None => vec![Pip::Unknown; n],
        Some(f) => {
            let lit = lit_count(f, n);
            (0..n)
                .map(|i| if i < lit { Pip::Lit } else { Pip::Dark })
                .collect()
        }
    }
}

/// A row of slanted pips filling its box — XCOM's health bar.
///
/// `n` is chosen by the caller for the width it has; the pips share the width
/// with a fixed gap between them, and every pip leans by the same amount so the
/// row reads as one instrument.
pub fn pips(states: Vec<Pip>, lit: Hsla, dark: Hsla, unknown: Hsla) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let n = states.len();
                if n == 0 {
                    return;
                }
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let gap = (h * 0.45).clamp(1.5, 3.);
                let lean = (h * 0.35).min(3.);
                let pw = ((w - lean - gap * (n as f32 - 1.)) / n as f32).max(1.);
                for (i, st) in states.iter().enumerate() {
                    let x = i as f32 * (pw + gap);
                    let quad = [(x + lean, 0.), (x + lean + pw, 0.), (x + pw, h), (x, h)];
                    match st {
                        Pip::Lit => fill_poly(bounds, &quad, lit, window),
                        Pip::Dark => fill_poly(bounds, &quad, dark, window),
                        Pip::Unknown => {
                            // Hollow: a faint outline, dashed by drawing only
                            // the two long edges. A full outline at this size
                            // fills in and reads as a dark pip.
                            let top = [(x + lean, 0.5), (x + lean + pw, 0.5)];
                            let bottom = [(x, h - 0.5), (x + pw, h - 0.5)];
                            for edge in [top, bottom] {
                                let mut path = PathBuilder::stroke(px(1.));
                                path.move_to(bounds.origin + point(px(edge[0].0), px(edge[0].1)));
                                path.line_to(bounds.origin + point(px(edge[1].0), px(edge[1].1)));
                                if let Ok(path) = path.build() {
                                    window.paint_path(path, unknown);
                                }
                            }
                        }
                    }
                }
            },
        )
        .size_full(),
    )
}

/// The Pip-Boy selection: a solid bar the row sits on, ending in an arrow tip
/// on the right. The row's own text paints over it, in the ink-on-mark colour.
pub fn bar_tip(ink: Hsla, glow: Hsla) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let tip = (h * 0.5).min(9.);
                let pts = [(0., 0.), (w - tip, 0.), (w, h * 0.5), (w - tip, h), (0., h)];
                if glow.a > 0. {
                    // One pixel of the bar's own colour around it, faint: the
                    // lit edge, without a blurred box that would ignore the tip.
                    let halo = [
                        (-1., -1.),
                        (w - tip + 1., -1.),
                        (w + 2., h * 0.5),
                        (w - tip + 1., h + 1.),
                        (-1., h + 1.),
                    ];
                    fill_poly(bounds, &halo, glow, window);
                }
                fill_poly(bounds, &pts, ink, window);
            },
        )
        .size_full(),
    )
}

/// The hatched strip along the top edge of the focused pane — the dotted rails
/// either side of XCOM's ability name. Thin slanted ticks, never under text.
pub fn hatch(ink: Hsla) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let step = (h * 1.6).max(5.);
                let mut x = 0.;
                while x < w + h {
                    let quad = [(x, 0.), (x + 2., 0.), (x + 2. - h, h), (x - h, h)];
                    fill_poly(bounds, &quad, ink, window);
                    x += step;
                }
            },
        )
        .size_full(),
    )
}

/// The OPS material: a faint square grid washed under a region's content —
/// parker-job's 44px grid, at the deck's scale. Under the text, never over it.
pub fn grid(ink: Hsla, pitch: f32) -> Div {
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let w = f32::from(bounds.size.width);
                let h = f32::from(bounds.size.height);
                let p = pitch.max(8.);
                let mut x = p;
                while x < w {
                    window.paint_quad(fill(
                        Bounds::new(bounds.origin + point(px(x), px(0.)), size(px(1.), px(h))),
                        ink,
                    ));
                    x += p;
                }
                let mut y = p;
                while y < h {
                    window.paint_quad(fill(
                        Bounds::new(bounds.origin + point(px(0.), px(y)), size(px(w), px(1.))),
                        ink,
                    ));
                    y += p;
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
    fn a_plate_is_cut_on_one_diagonal_only() {
        let p = chamfer(100., 40., 8.);
        assert_eq!(p[0], (8., 0.), "top-left cut");
        assert_eq!(p[1], (100., 0.), "top-right square");
        assert_eq!(p[3], (92., 40.), "bottom-right cut");
        assert_eq!(p[4], (0., 40.), "bottom-left square");
    }

    #[test]
    fn a_cut_never_eats_a_short_element() {
        let p = chamfer(60., 12., 10.);
        assert_eq!(p[0].0, 4., "a third of the short side, not the asked ten");
        assert_eq!(
            chamfer(10., 10., -3.)[0],
            (0., 0.),
            "a negative cut is no cut"
        );
    }

    /// Unknown is not zero: no reading draws a different gauge from a reading
    /// of zero, and the pips say so on their own.
    #[test]
    fn an_unknown_reading_is_not_an_empty_gauge() {
        assert_eq!(pips_for(None, 4), vec![Pip::Unknown; 4]);
        assert_eq!(pips_for(Some(0.), 4), vec![Pip::Dark; 4]);
        assert_ne!(pips_for(None, 4), pips_for(Some(0.), 4));
    }

    #[test]
    fn the_ends_of_a_gauge_are_never_rounded_away() {
        assert_eq!(lit_count(0.001, 12), 1, "barely used still lights one");
        assert_eq!(lit_count(0.999, 12), 11, "nearly gone is not gone");
        assert_eq!(lit_count(1., 12), 12);
        assert_eq!(lit_count(0., 12), 0);
        assert_eq!(lit_count(0.88, 12), 11);
        assert_eq!(lit_count(0.5, 0), 0);
    }
}
