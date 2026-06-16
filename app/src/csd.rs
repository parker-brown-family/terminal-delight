//! Client-side window decorations — the app draws its own frame so it can run
//! with NO system titlebar (`WindowDecorations::Client`). gpui hands us the
//! tiling state + the move/resize primitives; this module wraps the workspace
//! in the transparent shadow margin, rounds + clips the body, paints the drop
//! shadow, and turns the margin into live resize edges. Ported from gpui/Zed's
//! reference `client_side_decorations`, trimmed to this app's needs.

use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, div, point, px, transparent_black, Bounds, CursorStyle, Decorations, Div, Global,
    HitboxBehavior, Hsla, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Point, ResizeEdge, Size, Stateful, Styled, Tiling, Window,
};

/// Window corner radius + shadow/resize-grab margin (px). The rounding matches
/// the cabinet's own outer radius so the clip and the body agree.
const ROUNDING: Pixels = px(14.0);
const SHADOW: Pixels = px(10.0);

/// Round only the corners whose two adjacent edges are both untiled (a tiled
/// edge means the window is snapped flush there, so that corner stays square).
fn round_client_corners<E: Styled>(mut el: E, tiling: Tiling) -> E {
    if !tiling.top && !tiling.left {
        el = el.rounded_tl(ROUNDING);
    }
    if !tiling.top && !tiling.right {
        el = el.rounded_tr(ROUNDING);
    }
    if !tiling.bottom && !tiling.left {
        el = el.rounded_bl(ROUNDING);
    }
    if !tiling.bottom && !tiling.right {
        el = el.rounded_br(ROUNDING);
    }
    el
}

/// Which resize edge (if any) the pointer at `pos` is over, given the shadow
/// margin. Returns `None` for the interior so ordinary clicks pass through.
fn resize_edge(pos: Point<Pixels>, size: Size<Pixels>, tiling: Tiling) -> Option<ResizeEdge> {
    let interior = Bounds::new(Point::default(), size).inset(SHADOW * 1.5);
    if interior.contains(&pos) {
        return None;
    }
    let corner = gpui::size(SHADOW * 1.5, SHADOW * 1.5);
    let tl = Bounds::new(point(px(0.), px(0.)), corner);
    if !tiling.top && tl.contains(&pos) {
        return Some(ResizeEdge::TopLeft);
    }
    let tr = Bounds::new(point(size.width - corner.width, px(0.)), corner);
    if !tiling.top && tr.contains(&pos) {
        return Some(ResizeEdge::TopRight);
    }
    let bl = Bounds::new(point(px(0.), size.height - corner.height), corner);
    if !tiling.bottom && bl.contains(&pos) {
        return Some(ResizeEdge::BottomLeft);
    }
    let br = Bounds::new(
        point(size.width - corner.width, size.height - corner.height),
        corner,
    );
    if !tiling.bottom && br.contains(&pos) {
        return Some(ResizeEdge::BottomRight);
    }
    if !tiling.top && pos.y < SHADOW {
        Some(ResizeEdge::Top)
    } else if !tiling.bottom && pos.y > size.height - SHADOW {
        Some(ResizeEdge::Bottom)
    } else if !tiling.left && pos.x < SHADOW {
        Some(ResizeEdge::Left)
    } else if !tiling.right && pos.x > size.width - SHADOW {
        Some(ResizeEdge::Right)
    } else {
        None
    }
}

/// The most recent resize edge under the cursor, stashed globally so the
/// cursor-styling canvas and the mouse-down handler agree on the same edge.
struct GlobalResizeEdge(ResizeEdge);
impl Global for GlobalResizeEdge {}

/// Wrap the workspace so it can run frameless: a transparent backdrop holds the
/// shadow margin + resize edges, the inner container rounds/clips the body and
/// casts the desktop drop shadow. On server-side decorations (the fallback)
/// this is a near-passthrough — no margin, no rounding.
pub fn decorate(content: impl IntoElement, window: &mut Window) -> Stateful<Div> {
    let decorations = window.window_decorations();
    let tiling = match decorations {
        Decorations::Server => Tiling::default(),
        Decorations::Client { tiling } => tiling,
    };
    match decorations {
        Decorations::Client { .. } => window.set_client_inset(SHADOW),
        Decorations::Server => window.set_client_inset(px(0.)),
    }

    div()
        .id("td-window-backdrop")
        .bg(transparent_black())
        .size_full()
        .map(|d| match decorations {
            Decorations::Server => d,
            Decorations::Client { .. } => round_client_corners(d, tiling)
                .when(!tiling.top, |d| d.pt(SHADOW))
                .when(!tiling.bottom, |d| d.pb(SHADOW))
                .when(!tiling.left, |d| d.pl(SHADOW))
                .when(!tiling.right, |d| d.pr(SHADOW))
                .on_mouse_move(move |e, window, cx| {
                    // a fresh edge under the cursor → repaint so the cursor canvas updates
                    let sz = window.window_bounds().get_bounds().size;
                    let new_edge = resize_edge(e.position, sz, tiling);
                    let cur = cx.try_global::<GlobalResizeEdge>().map(|g| g.0);
                    if new_edge != cur {
                        window.refresh();
                    }
                })
                .on_mouse_down(MouseButton::Left, move |e, window, _| {
                    let sz = window.window_bounds().get_bounds().size;
                    if let Some(edge) = resize_edge(e.position, sz, tiling) {
                        window.start_window_resize(edge);
                    }
                }),
        })
        .child(
            div()
                .cursor(CursorStyle::Arrow)
                .size_full()
                .map(|d| match decorations {
                    Decorations::Server => d,
                    Decorations::Client { .. } => round_client_corners(d, tiling)
                        .overflow_hidden()
                        .when(!tiling.is_tiled(), |d| {
                            d.shadow(vec![gpui::BoxShadow {
                                color: Hsla {
                                    h: 0.,
                                    s: 0.,
                                    l: 0.,
                                    a: 0.45,
                                },
                                offset: point(px(0.), px(0.)),
                                blur_radius: SHADOW / 2.,
                                spread_radius: px(0.),
                                inset: false,
                            }])
                        }),
                })
                // inside the body, swallow moves so they never reach the
                // backdrop's resize probe (keeps the interior cursor an arrow)
                .on_mouse_move(|_e, _w, cx| cx.stop_propagation())
                .child(content),
        )
        .map(|d| match decorations {
            Decorations::Server => d,
            Decorations::Client { .. } => d.child(
                // a full-window hitbox whose only job is to set the resize
                // cursor while the pointer sits in the margin
                canvas(
                    |_bounds, window, _| {
                        window.insert_hitbox(
                            Bounds::new(
                                point(px(0.), px(0.)),
                                window.window_bounds().get_bounds().size,
                            ),
                            HitboxBehavior::Normal,
                        )
                    },
                    move |_bounds, hitbox, window, cx| {
                        let sz = window.window_bounds().get_bounds().size;
                        let Some(edge) = resize_edge(window.mouse_position(), sz, tiling) else {
                            return;
                        };
                        cx.set_global(GlobalResizeEdge(edge));
                        window.set_cursor_style(
                            match edge {
                                ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
                                ResizeEdge::Left | ResizeEdge::Right => {
                                    CursorStyle::ResizeLeftRight
                                }
                                ResizeEdge::TopLeft | ResizeEdge::BottomRight => {
                                    CursorStyle::ResizeUpLeftDownRight
                                }
                                ResizeEdge::TopRight | ResizeEdge::BottomLeft => {
                                    CursorStyle::ResizeUpRightDownLeft
                                }
                            },
                            &hitbox,
                        );
                    },
                )
                .size_full()
                .absolute(),
            ),
        })
}
