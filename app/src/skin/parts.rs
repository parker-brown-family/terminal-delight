//! Stateless native controls. Input handlers remain with their owners.
use crate::{
    darken, skin, sticky, theme, white, TRAY_BTN_H, TRAY_BTN_W, TRAY_MODE_BTN_H, TRAY_MODE_BTN_W,
    TRAY_NOTE_BTN_W, TRAY_NOTE_CAPTION,
};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// Immediate two-state switch; the caller owns the write.
pub(crate) fn slider_switch(on: bool, th: &theme::Theme, sk: &skin::Skin) -> gpui::Div {
    if sk.is_paper() {
        return sk.paper_control(div().w(px(36.)).h(px(18.)), on).child(
            div()
                .absolute()
                .left(px(if on { 22. } else { 3. }))
                .top(px(3.))
                .w(px(11.))
                .h(px(11.))
                .child(crate::paper::backing_with(
                    sk.ink.ink_dim,
                    sk.ink.select,
                    false,
                    sk.stock(),
                )),
        );
    }
    const TRACK_W: f32 = 36.;
    const TRACK_H: f32 = 18.;
    const KNOB: f32 = 13.;
    let pad = (TRACK_H - KNOB) / 2.;
    div()
        .w(px(TRACK_W))
        .h(px(TRACK_H))
        .flex_none()
        // A pill under a round skin, a slot under a square or cut one.
        .rounded(sk.radius_pill())
        .border_1()
        .border_color(if on { th.accent } else { th.text.alpha(0.25) })
        .bg(if on {
            th.accent.alpha(0.35)
        } else {
            th.text.alpha(0.08)
        })
        .flex()
        .flex_row()
        .items_center()
        .when(on, |d| d.justify_end())
        .when(!on, |d| d.justify_start())
        .px(px(pad))
        .child(
            div()
                .w(px(KNOB))
                .h(px(KNOB))
                .rounded(sk.radius_pill())
                .bg(if on { th.accent } else { th.text.alpha(0.45) })
                .when(on, |d| {
                    d.shadow(
                        theme::phosphor(
                            th.grade.phosphor,
                            BoxShadow {
                                color: th.accent.alpha(0.8),
                                offset: point(px(0.), px(0.)),
                                blur_radius: px(6.),
                                spread_radius: px(0.),
                                inset: false,
                            },
                        )
                        .into_iter()
                        .collect(),
                    )
                }),
        )
}

/// Palette slot glyph and caption share one choice target.
pub(crate) fn theme_icon_btn(
    th: &theme::Theme,
    sk: &skin::Skin,
    icon: &str,
    label: &str,
    active: bool,
) -> gpui::Div {
    let inner = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_0()
        .when(sk.is_paper(), |d| {
            d.justify_center().line_height(gpui::relative(1.15))
        })
        .child(div().text_size(sk.text_px(14.)).child(icon.to_string()))
        .child(
            div()
                .text_size(sk.text_px(if sk.is_paper() { 11. } else { 8. }))
                .text_color(th.text.alpha(if active { 0.85 } else { 0.6 }))
                .child(label.to_string()),
        );
    let b = div()
        .w(px(if sk.is_paper() { 56. } else { TRAY_BTN_W }))
        .h(px(if sk.is_paper() { 54. } else { TRAY_BTN_H }))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        // A corner cap (Clean's nearly square trim) holds this under it too.
        .when(sk.m.corner_cap < 6., |d| d.rounded(sk.capped(6.)))
        .border_1()
        .cursor_pointer();
    if sk.is_paper() {
        sk.paper_option(b, false).child(
            div()
                .size_full()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .absolute()
                        .top(px(3.))
                        .left(px(4.))
                        .text_size(sk.px(11.))
                        .line_height(gpui::relative(1.0))
                        .child(icon.to_string()),
                )
                .child(sk.paper_caption(label, 11., active)),
        )
    } else if active {
        b.bg(linear_gradient(
            135.,
            linear_color_stop(th.accent.alpha(0.45), 0.),
            linear_color_stop(th.accent.alpha(0.15), 1.),
        ))
        .border_color(th.accent)
        .text_color(white().alpha(0.95))
        .child(inner)
    } else {
        b.bg(darken(th.surface, 0.8).alpha(0.5))
            .border_color(th.accent.alpha(0.35))
            .text_color(th.text)
            .child(inner)
    }
}

/// Program colour choice: glyph and caption.
pub(crate) fn color_mode_btn(
    th: &theme::Theme,
    sk: &skin::Skin,
    icon: &str,
    caption: &str,
    active: bool,
) -> gpui::Div {
    let inner = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_0()
        .when(sk.is_paper(), |d| {
            d.justify_center().line_height(gpui::relative(1.15))
        })
        .child(div().text_size(sk.text_px(15.)).child(icon.to_string()))
        .child(div().text_size(sk.text_px(8.)).child(caption.to_string()));
    let b = div()
        .w(px(TRAY_MODE_BTN_W))
        .h(px(if sk.is_paper() { 54. } else { TRAY_MODE_BTN_H }))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        // A corner cap (Clean's nearly square trim) holds this under it too.
        .when(sk.m.corner_cap < 6., |d| d.rounded(sk.capped(6.)))
        .border_1()
        .cursor_pointer();
    if sk.is_paper() {
        sk.paper_option(b, false).child(
            div()
                .size_full()
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .absolute()
                        .top(px(3.))
                        .left(px(4.))
                        .text_size(sk.px(11.))
                        .line_height(gpui::relative(1.0))
                        .child(icon.to_string()),
                )
                .child(sk.paper_caption(caption, 11., active)),
        )
    } else if active {
        b.bg(linear_gradient(
            135.,
            linear_color_stop(th.accent.alpha(0.45), 0.),
            linear_color_stop(th.accent.alpha(0.15), 1.),
        ))
        .border_color(th.accent)
        .text_color(white().alpha(0.95))
        .child(inner)
    } else {
        b.bg(darken(th.surface, 0.8).alpha(0.5))
            .border_color(th.accent.alpha(0.35))
            .text_color(th.text)
            .child(inner)
    }
}

/// A note colour choice shows its actual paper beside its caption.
pub(crate) fn note_paper_btn(
    th: &theme::Theme,
    paper: &sticky::Paper,
    sk: &skin::Skin,
    caption: &str,
    active: bool,
) -> gpui::Div {
    let ink = |w: f32| div().w(px(w)).h(px(1.5)).bg(paper.ink.alpha(0.7));
    let sheet = div()
        .relative()
        .w(px(26.))
        .h(px(18.))
        .bg(paper.base)
        .border_b_1()
        .border_color(darken(paper.base, 0.7))
        .shadow(vec![BoxShadow {
            color: hsla(0., 0., 0., 0.55),
            offset: point(px(1.), px(1.5)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        }])
        .flex()
        .flex_col()
        .justify_end()
        .gap(px(2.5))
        .px(px(4.))
        .pb(px(3.))
        .child(ink(15.))
        .child(ink(10.))
        .child(
            div()
                .absolute()
                .top(px(1.5))
                .left(px(11.))
                .w(px(4.))
                .h(px(4.))
                .rounded_full()
                .bg(paper.pin),
        );
    let inner = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(3.))
        .child(sheet)
        .child(sk.caption(caption, TRAY_NOTE_CAPTION, active));
    let b = div()
        .w(px(if sk.is_paper() { 60. } else { TRAY_NOTE_BTN_W }))
        .h(px(if sk.is_paper() { 50. } else { TRAY_MODE_BTN_H }))
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded_md()
        // A corner cap (Clean's nearly square trim) holds this under it too.
        .when(sk.m.corner_cap < 6., |d| d.rounded(sk.capped(6.)))
        .border_1()
        .cursor_pointer();
    if sk.is_paper() {
        sk.paper_option(b, false)
            .child(inner.justify_center().line_height(gpui::relative(1.15)))
    } else if active {
        b.bg(linear_gradient(
            135.,
            linear_color_stop(th.accent.alpha(0.45), 0.),
            linear_color_stop(th.accent.alpha(0.15), 1.),
        ))
        .border_color(th.accent)
        .text_color(white().alpha(0.95))
        .child(inner)
    } else {
        b.bg(darken(th.surface, 0.8).alpha(0.5))
            .border_color(th.accent.alpha(0.35))
            .text_color(th.text)
            .child(inner)
    }
}
