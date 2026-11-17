//! Gauge pictures beside Settings' rows: what a setting changes, drawn from
//! the numbers the window itself draws with, so moving a gauge moves its
//! picture in the same frame.
//!
//! A GAUGE sample is a little terminal (or a pair of them, or a
//! document over one) graded by the scope's own grade through
//! `pane::graded_palette`, the function every pane's colours go through, so
//! brightness, contrast, colour, gamma and the text and background levels
//! are the window's arithmetic and not an imitation of it. What a sample
//! cannot draw without the window's post-pass (an inactive pane's blur, a
//! letter's glow) it leaves out rather than fakes; the sliders still carry
//! those numbers.
//!

use super::*;
use crate::theme::{Grade, Theme};

// The gauge rows use the same small sample size as the former Interface maps.
pub(crate) const MAP_W: f32 = 105.6;
pub(crate) const MAP_H: f32 = 62.4;

// ---- gauge samples --------------------------------------------------------

/// Which gauge group a sample draws.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Gauge {
    Type,
    Picture,
    Document,
    Active,
    Inactive,
    Crt,
}

/// `th` with `g` spent into its colours, exactly as a pane spends it.
fn graded(th: &Theme, g: &Grade) -> Theme {
    let mut t = th.clone();
    t.grade = *g;
    pane::graded_palette(&t)
}

/// A colour's lightness multiplied by `gain`, as the window's post-pass
/// multiplies a pane it brightens or dims.
fn lit(c: Hsla, gain: f32) -> Hsla {
    Hsla {
        l: (c.l * gain).clamp(0., 1.),
        ..c
    }
}

/// The document gauges' arithmetic on one colour: lightness times the
/// brightness gain, then pushed from mid-grey by the contrast gain.
fn doc_tone(c: Hsla, gains: (f32, f32)) -> Hsla {
    let l = ((c.l * gains.0) - 0.5) * gains.1 + 0.5;
    Hsla {
        l: l.clamp(0., 1.),
        ..c
    }
}

/// Five lines of a terminal in `p`'s colours, each ink run through `ink`.
fn lines(p: &Theme, size: Pixels, ink: impl Fn(Hsla) -> Hsla) -> gpui::Div {
    let line = |runs: Vec<(Hsla, &'static str)>| {
        div()
            .flex()
            .flex_row()
            .whitespace_nowrap()
            .children(runs.into_iter().map(|(c, s)| div().text_color(c).child(s)))
    };
    let (text, acc) = (ink(p.text), ink(p.accent));
    div()
        .flex()
        .flex_col()
        .gap(px(1.))
        .font(pane::grid_font(p, gpui::FontWeight::NORMAL))
        .text_size(size)
        .line_height(size * 1.25)
        .child(line(vec![(acc, "\u{276f} "), (text, "cargo build")]))
        .child(line(vec![
            (ink(p.ansi[2]), "  Compiling"),
            (text, " td v0.5"),
        ]))
        .child(line(vec![(ink(p.ansi[3]), "warning"), (text, ": unused")]))
        .child(line(vec![(ink(p.ansi[1]), "error"), (text, "[E0425]")]))
        .child(line(vec![
            (ink(p.ansi[4]), "~/work"),
            (ink(p.ansi[5]), " main"),
            (acc, " \u{276f} "),
            (ink(p.cursor), "\u{2588}"),
        ]))
}

/// What stands behind a see-through pane: a wallpaper-ish sweep, so a
/// field that lets it through shows it.
fn desktop() -> gpui::Background {
    linear_gradient(
        120.,
        linear_color_stop(hsla(0.58, 0.55, 0.42, 1.), 0.),
        linear_color_stop(hsla(0.05, 0.65, 0.48, 1.), 1.),
    )
}

/// One little pane: `p`'s field at `opacity` over the desktop, its ink
/// through `ink`, a veil of `veil` black over it, and a border lit by
/// `glow` (the phosphor gain, 1 the house look).
#[allow(clippy::too_many_arguments)]
fn mini_pane(
    p: &Theme,
    size: Pixels,
    opacity: f32,
    veil: f32,
    glow: f32,
    focused: bool,
    caption: Option<&'static str>,
    sk: &skin::Skin,
    ink: impl Fn(Hsla) -> Hsla,
) -> gpui::Div {
    let acc = p.accent;
    let border = if focused {
        acc.alpha(0.85)
    } else {
        p.text.alpha(0.25)
    };
    let glow = glow.clamp(0., theme::PHOSPHOR_MAX);
    div()
        .relative()
        .flex_1()
        .min_w(px(0.))
        .h_full()
        .rounded(px(4.))
        .border_1()
        .border_color(border)
        .when(focused && glow > 0.01, |d| {
            d.shadow(vec![BoxShadow {
                color: acc.alpha((0.22 * glow).min(0.8)),
                offset: point(px(0.), px(0.)),
                blur_radius: px(4. + 6. * glow),
                spread_radius: px(0.),
                inset: false,
            }])
        })
        .bg(desktop())
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .inset_0()
                .bg(p.bg.alpha(p.bg.a * opacity.clamp(0., 1.))),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .p(px(6.))
                .overflow_hidden()
                .child(lines(p, size, ink)),
        )
        .when(veil > 0.001, |d| {
            d.child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(gpui::black().alpha(veil.clamp(0., 1.))),
            )
        })
        .when_some(caption, |d, c| {
            d.child(
                div()
                    .absolute()
                    .right(px(4.))
                    .bottom(px(2.))
                    .text_size(sk.text_px(7.5))
                    .text_color(p.text.alpha(0.6))
                    .child(c),
            )
        })
}

/// The sample for one gauge group, `w` wide and `h` tall, drawn from the
/// scope's resolved theme `th` (its colours as authored, its grade on it).
pub(crate) fn gauge(th: &Theme, sk: &skin::Skin, which: Gauge, w: f32, h: f32) -> AnyElement {
    let g = th.grade;
    let p = graded(th, &g);
    // The terminal's type in proportion to the picture: five lines fill a
    // little more than half of it.
    let small = px((h / 12.).clamp(6., 9.));
    let preset = th.terminal_light;
    let frame = div()
        .flex_shrink_0()
        .w(px(w))
        .h(px(h))
        .flex()
        .flex_row()
        .gap(px(6.));
    match which {
        Gauge::Type => {
            let row = |name: &'static str, k: f32, base: f32, mono: bool| {
                div()
                    .flex()
                    .flex_row()
                    .items_baseline()
                    .gap(px(8.))
                    .child(
                        div()
                            .w(px(58.))
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .text_size(sk.text_px(8.))
                            .text_color(th.text.alpha(0.5))
                            .child(name),
                    )
                    .child(
                        div()
                            .map(|d| {
                                // The grid's own font at its own size; the
                                // bench and the bar in the skin's type, sized
                                // as the skin sizes it.
                                if mono {
                                    d.font(pane::grid_font(&p, gpui::FontWeight::NORMAL))
                                        .text_size(px(base * k))
                                        .line_height(px(base * k * 1.2))
                                } else {
                                    d.text_size(sk.text_px(base * k))
                                        .line_height(sk.text_px(base * k * 1.2))
                                }
                            })
                            .text_color(p.text)
                            .whitespace_nowrap()
                            .child("Aa \u{276f} ls"),
                    )
            };
            frame
                .flex_col()
                .justify_center()
                .gap(px(4.))
                .px(px(8.))
                .rounded(px(4.))
                .border_1()
                .border_color(th.text.alpha(0.22))
                .bg(p.bg)
                .overflow_hidden()
                .child(row("terminal", g.text_size, 13., true))
                .child(row("bench", g.bench_gauge(), 13., false))
                .child(row("menu bar", g.scale, 11., false))
                .into_any_element()
        }
        Gauge::Picture => frame
            .child(mini_pane(
                &p,
                small,
                1.,
                0.,
                g.phosphor,
                true,
                None,
                sk,
                |c| c,
            ))
            .into_any_element(),
        Gauge::Active | Gauge::Inactive => {
            // Both panes, the one this group sets lit by its border, so the
            // relation between them is part of the picture.
            let gain = g.active_brightness_at_edge();
            let active = mini_pane(
                &p,
                small,
                g.opacity_for_focus(true),
                0.,
                if which == Gauge::Active {
                    g.phosphor
                } else {
                    0.
                },
                which == Gauge::Active,
                Some("active"),
                sk,
                move |c| lit(c, gain),
            );
            // An inactive pane: its own text level, its veil over it.
            let mut gi = g;
            gi.text = g.text_for_focus(false, preset);
            let pi = graded(th, &gi);
            let inactive = mini_pane(
                &pi,
                small,
                g.opacity_for_focus(false),
                1. - g.inactive_brightness_at_edge(),
                if which == Gauge::Inactive {
                    g.phosphor
                } else {
                    0.
                },
                which == Gauge::Inactive,
                Some("inactive"),
                sk,
                |c| c,
            );
            frame.child(active).child(inactive).into_any_element()
        }
        Gauge::Document => {
            let gains = (
                g.doc_brightness_at_edge() / 0.5,
                g.doc_contrast_at_edge() / 0.5,
            );
            let clear = g.doc_clear_at_edge();
            let glow = g.doc_glow_at_edge();
            let acc = p.accent;
            let card_bg = doc_tone(darken(p.surface, 0.0), gains);
            let ink = doc_tone(p.text, gains);
            let card = div()
                .absolute()
                .top(px(h * 0.18))
                .left(px(w * 0.34))
                .w(px(w * 0.58))
                .h(px(h * 0.7))
                .rounded(px(3.))
                .border_1()
                .border_color(acc.alpha(0.8))
                .when(glow > 0.01, |d| {
                    d.shadow(vec![BoxShadow {
                        color: acc.alpha((0.25 * glow).min(0.85)),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(4. + 8. * glow),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                })
                .bg(card_bg.alpha(1. - clear))
                .p(px(6.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(div().w(px(w * 0.3)).h(px(4.)).rounded(px(1.)).bg(ink))
                .children((0..4).map(|i| {
                    div()
                        .w(px(w * (0.46 - 0.05 * (i % 3) as f32)))
                        .h(px(2.5))
                        .rounded(px(1.))
                        .bg(ink.alpha(0.7))
                }))
                .child(
                    div()
                        .w(px(w * 0.2))
                        .h(px(2.5))
                        .rounded(px(1.))
                        .bg(doc_tone(acc, gains)),
                );
            div()
                .relative()
                .flex_shrink_0()
                .w(px(w))
                .h(px(h))
                .flex()
                .child(mini_pane(&p, small, 1., 0., 0., false, None, sk, |c| c))
                .child(card)
                .into_any_element()
        }
        Gauge::Crt => {
            let on = g.crt;
            let warp = if on {
                (g.warp / theme::WARP_MAX).clamp(0., 1.)
            } else {
                0.
            };
            let glare = if on {
                g.glare_over(th.screen_glare).clamp(0., 1.)
            } else {
                0.
            };
            frame
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .h_full()
                        // the bulge of the glass, as the corner it rounds
                        .rounded(px(3. + 22. * warp))
                        .border_1()
                        .border_color(th.text.alpha(0.3))
                        .bg(p.bg)
                        .overflow_hidden()
                        .p(px(8.))
                        .child(lines(&p, small, |c| c))
                        .when(glare > 0.01, |d| {
                            d.child(div().absolute().inset_0().bg(linear_gradient(
                                160.,
                                linear_color_stop(white().alpha(0.5 * glare), 0.),
                                linear_color_stop(white().alpha(0.), 0.55),
                            )))
                        }),
                )
                .into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The document gauges at neutral leave a colour where it was, and a
    /// brighter gauge lifts it.
    #[test]
    fn the_document_tone_is_the_identity_at_neutral() {
        let c = hsla(0.3, 0.5, 0.4, 1.);
        let same = doc_tone(c, (1., 1.));
        assert!((same.l - c.l).abs() < 1e-6);
        assert!(doc_tone(c, (1.4, 1.)).l > c.l);
        assert!(
            doc_tone(c, (1., 2.)).l < c.l,
            "below mid-grey, contrast darkens"
        );
    }

    /// The sample's colours are the pane's: a dimmed grade dims the
    /// sample's field exactly as `graded_palette` dims a pane's.
    #[test]
    fn a_gauge_sample_grades_through_the_panes_own_function() {
        let mut th = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).unwrap();
        th.bg = hsla(0.6, 0.3, 0.3, 1.);
        let mut g = Grade::neutral();
        g.brightness = 0.25;
        let p = graded(&th, &g);
        let mut t = th.clone();
        t.grade = g;
        assert_eq!(p.bg, pane::graded_palette(&t).bg);
        assert!(p.bg.l < th.bg.l);
    }
}
