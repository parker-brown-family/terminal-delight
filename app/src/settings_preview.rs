//! Small pictures beside Settings' rows: what a setting changes, drawn from
//! the numbers the window itself draws with, so moving a gauge moves its
//! picture in the same frame.
//!
//! Two kinds. A GAUGE sample is a little terminal (or a pair of them, or a
//! document over one) graded by the scope's own grade through
//! `pane::graded_palette`, the function every pane's colours go through, so
//! brightness, contrast, colour, gamma and the text and background levels
//! are the window's arithmetic and not an imitation of it. What a sample
//! cannot draw without the window's post-pass (an inactive pane's blur, a
//! letter's glow) it leaves out rather than fakes; the sliders still carry
//! those numbers.
//!
//! An INTERFACE sample is a map of the window with one part lit: where the
//! part a switch shows or hides sits, solid while it is shown and a dashed
//! outline while it is hidden.

use super::*;
use crate::theme::{Grade, Theme};

// ---- where an Interface switch's part sits --------------------------------

/// The part of the window an Interface row's switch shows or hides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Spot {
    AppMark,
    GitNotice,
    AgentCounter,
    Spine,
    LeftBar,
    BarHeader,
    Allowance,
    Splits,
    Counts,
    Scale,
    MenuGlyphs,
    OptionRows,
    PaneLogo,
    PaneMode,
    PaneFace,
    PaneAgent,
    PaneHandover,
    PaneSurface,
    PaneSliders,
    PaneClose,
    PaneNotes,
}

impl Spot {
    /// The spot a row's keys draw. A row that flips several keys (the menu
    /// glyphs) is the spot of its first.
    pub(crate) fn of(keys: &[uiconfig::Key]) -> Option<Spot> {
        use uiconfig::Key as K;
        Some(match keys.first()? {
            K::AppMark => Spot::AppMark,
            K::GitNotice => Spot::GitNotice,
            K::AgentCounter => Spot::AgentCounter,
            K::LeftBar => Spot::LeftBar,
            K::LeftBarHeader => Spot::BarHeader,
            K::Allowance => Spot::Allowance,
            K::Spine => Spot::Spine,
            K::SplitButtons => Spot::Splits,
            K::Counts => Spot::Counts,
            K::Scale => Spot::Scale,
            K::Artifacts
            | K::Surfaces
            | K::Sliders
            | K::Agents
            | K::Graveyard
            | K::Notifications
            | K::BugReport
            | K::Plugins => Spot::MenuGlyphs,
            K::OptionRows => Spot::OptionRows,
            K::PaneLogo => Spot::PaneLogo,
            K::PaneMode => Spot::PaneMode,
            K::PaneFace => Spot::PaneFace,
            K::PaneAgent => Spot::PaneAgent,
            K::PaneHandover => Spot::PaneHandover,
            K::PaneSurface => Spot::PaneSurface,
            K::PaneSliders => Spot::PaneSliders,
            K::PaneClose => Spot::PaneClose,
            K::PaneNotes => Spot::PaneNotes,
        })
    }

    fn on_pane(self) -> bool {
        matches!(
            self,
            Spot::PaneLogo
                | Spot::PaneMode
                | Spot::PaneFace
                | Spot::PaneAgent
                | Spot::PaneHandover
                | Spot::PaneSurface
                | Spot::PaneSliders
                | Spot::PaneClose
                | Spot::PaneNotes
        )
    }
}

/// The window a map is drawn in, in its own units; `K` scales it to the
/// row. Every Interface sample is the same size, so a column of rows lines
/// their pictures up.
const DW: f32 = 132.;
const DH: f32 = 78.;
const K: f32 = 0.8;
pub(crate) const MAP_W: f32 = DW * K;
pub(crate) const MAP_H: f32 = DH * K;

/// A rectangle on a map, by its offset and size in the map's units.
fn at(x: f32, y: f32, w: f32, h: f32) -> gpui::Div {
    div()
        .absolute()
        .left(px(x * K))
        .top(px(y * K))
        .w(px(w * K))
        .h(px(h * K))
}

/// The picture for an Interface row: where `spot` sits, solid while `on`,
/// a dashed outline while it is hidden, everything else quiet.
pub(crate) fn interface(th: &Theme, sk: &skin::Skin, spot: Spot, on: bool) -> AnyElement {
    // The accent, made loud enough to find on a picture this small: a pale
    // accent (chalk's grey-green) otherwise lights nothing.
    let acc = Hsla {
        s: th.accent.s.max(0.55),
        l: th.accent.l.clamp(0.58, 0.72),
        ..th.accent
    };
    let quiet = th.text.alpha(0.11);
    let halo = move |a: f32| {
        vec![BoxShadow {
            color: acc.alpha(a),
            offset: point(px(0.), px(0.)),
            blur_radius: px(5.),
            spread_radius: px(1.),
            inset: false,
        }]
    };
    // One part: lit, filled and haloed when it is the spot and shown; a
    // dashed outline where it would be when it is the spot and hidden;
    // quiet otherwise.
    // The spot is ringed, so even a part three units wide is found at a
    // glance: a solid ring round a lit part while it is shown, a dashed ring
    // round where it would be while it is hidden.
    let part = move |x: f32, y: f32, w: f32, h: f32, me: bool| {
        if !me {
            return at(x, y, w, h).rounded(px(1.5)).bg(quiet);
        }
        const R: f32 = 3.;
        let ring = at(x - R, y - R, w + 2. * R, h + 2. * R)
            .rounded(px(3.))
            .p(px(R * K - 1.))
            .border_1()
            .border_color(acc);
        if on {
            ring.shadow(halo(0.55))
                .child(div().size_full().rounded(px(1.)).bg(acc))
        } else {
            ring.border_dashed()
        }
    };
    let frame = div()
        .relative()
        .flex_shrink_0()
        .w(px(MAP_W))
        .h(px(MAP_H))
        .rounded(px(4.))
        .border_1()
        .border_color(th.text.alpha(0.22))
        .bg(darken(th.bg, 0.2))
        .overflow_hidden();
    let (w, h) = (DW, DH);
    let frame = match spot {
        Spot::OptionRows => return option_rows(th, sk, on),
        s if s.on_pane() => {
            // A pane up close: its header and the glass under it.
            let me = |x: Spot| x == s;
            frame
                .child(
                    at(4., 4., w - 8., h - 8.)
                        .rounded(px(2.))
                        .bg(th.text.alpha(0.05)),
                )
                .child(at(4., 4., w - 8., 11.).bg(th.text.alpha(0.07)))
                .child(part(7., 6., 7., 7., me(Spot::PaneLogo)))
                .child(part(17., 8., 16., 3., me(Spot::PaneMode)))
                .child(part(37., 6., 20., 7., me(Spot::PaneFace)))
                .child(part(60., 6., 14., 7., me(Spot::PaneAgent)))
                .child(part(w - 47., 6., 7., 7., me(Spot::PaneHandover)))
                .child(part(w - 37., 6., 7., 7., me(Spot::PaneSurface)))
                .child(part(w - 27., 6., 7., 7., me(Spot::PaneSliders)))
                .child(part(w - 15., 6., 7., 7., me(Spot::PaneClose)))
                .child(part(w - 46., 24., 34., 26., me(Spot::PaneNotes)))
                // a few lines of terminal, for scale
                .children((0..4).map(|i| {
                    at(10., 22. + i as f32 * 9., 40. + (i * 13 % 30) as f32, 3.)
                        .rounded(px(1.))
                        .bg(th.text.alpha(0.12))
                }))
        }
        s => {
            let me = |x: Spot| x == s;
            let bar_lit = me(Spot::LeftBar);
            frame
                // top bar: the mark and the notice at the left, the agent
                // counter at the right
                .child(part(4., 3., 6., 6., me(Spot::AppMark)))
                .child(part(13., 4., 30., 4., me(Spot::GitNotice)))
                .child(part(w - 38., 4., 26., 4., me(Spot::AgentCounter)))
                // the left bar, its header and the allowance at its foot
                .child(
                    at(4., 12., 28., h - 23.)
                        .rounded(px(2.))
                        .map(|d| match (bar_lit, on) {
                            (true, true) => d.bg(acc.alpha(0.45)).shadow(halo(0.5)),
                            (true, false) => {
                                d.border_1().border_dashed().border_color(acc.alpha(0.85))
                            }
                            _ => d.bg(th.text.alpha(0.06)),
                        }),
                )
                .child(part(6., 14., 24., 4., me(Spot::BarHeader)))
                .child(part(6., h - 25., 24., 9., me(Spot::Allowance)))
                // the pane
                .child(
                    at(35., 12., w - 47., h - 23.)
                        .rounded(px(2.))
                        .bg(th.text.alpha(0.05)),
                )
                .child(at(35., 12., w - 47., 5.).bg(th.text.alpha(0.08)))
                // the right column: the pill at its head, a split at its foot
                .child(part(w - 9., 13., 5., 10., me(Spot::Spine)))
                .child(part(w - 9., h - 20., 5., 5., me(Spot::Splits)))
                // the footer's right end
                .child(part(w - 74., h - 8., 16., 4., me(Spot::Counts)))
                .children((0..5).map(|i| {
                    part(
                        w - 55. + i as f32 * 5.,
                        h - 8.,
                        3.,
                        4.,
                        me(Spot::MenuGlyphs),
                    )
                }))
                .child(part(w - 28., h - 7., 12., 2., me(Spot::Scale)))
                .child(part(w - 12., h - 8., 6., 4., me(Spot::Splits)))
        }
    };
    frame.into_any_element()
}

/// Every option in a list, or only what each is set to.
fn option_rows(th: &Theme, sk: &skin::Skin, on: bool) -> AnyElement {
    let acc = th.accent;
    div()
        .flex_shrink_0()
        .w(px(MAP_W))
        .h(px(MAP_H))
        .rounded(px(4.))
        .border_1()
        .border_color(th.text.alpha(0.22))
        .bg(darken(th.bg, 0.2))
        .p(px(5.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .children((0..3).map(|row| {
            let pick = (row * 2 + 1) % 4;
            div()
                .flex()
                .flex_row()
                .gap(px(3.))
                .child(
                    div()
                        .w(px(14.))
                        .h(px(8.))
                        .rounded(px(1.))
                        .bg(th.text.alpha(0.12)),
                )
                .children((0..4).filter(|i| on || *i == pick).map(move |i| {
                    div()
                        .w(px(14.))
                        .h(px(8.))
                        .rounded(px(1.5))
                        .when(i == pick, |d| d.bg(acc.alpha(0.9)))
                        .when(i != pick, |d| d.bg(th.text.alpha(0.2)))
                }))
        }))
        .child(
            div()
                .text_size(sk.text_px(7.))
                .text_color(th.text.alpha(0.45))
                .child(if on { "all shown" } else { "the choice only" }),
        )
        .into_any_element()
}

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

    /// Every Interface row has a spot, and the menu glyphs' several keys
    /// draw one spot.
    #[test]
    fn every_interface_key_has_a_spot() {
        for k in uiconfig::Key::ALL.iter() {
            assert!(Spot::of(&[*k]).is_some(), "{k:?} has no spot");
        }
        assert_eq!(Spot::of(uiconfig::Key::MENU_GLYPHS), Some(Spot::MenuGlyphs));
        assert_eq!(Spot::of(&[]), None);
    }

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
