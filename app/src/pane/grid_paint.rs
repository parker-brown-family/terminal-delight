//! Retain only terminal text layout and paint. Pane geometry, input handlers,
//! tube registration, headers and effects still run on every parent frame.
use super::*;

#[derive(Default, PartialEq)]
pub(super) struct Snapshot {
    pub lines: styling::Lines,
    pub cell_h: f32,
    pub pad: (f32, f32),
    pub crawl: bool,
    /// The alpha of the phosphor bleed under every letter, or `None` for a
    /// clean grid — which is every design but Intrusive. See [`bleed_runs`].
    pub bleed: Option<f32>,
}

/// The bleed offsets: one pixel out in each of the four directions. Four
/// copies, not eight: the diagonals add cost and read as bold, not as glow.
const BLEED: [(f32, f32); 4] = [(-1., 0.), (1., 0.), (0., -1.), (0., 1.)];

/// A line's runs, recoloured for the bleed underneath it: each run in its OWN
/// colour at `alpha`, so green text bleeds green and a red error bleeds red,
/// with no background, underline or strike — those would print four times.
pub(super) fn bleed_runs(runs: &[TextRun], alpha: f32) -> Vec<TextRun> {
    runs.iter()
        .map(|r| TextRun {
            color: r.color.alpha(r.color.a * alpha),
            background_color: None,
            underline: None,
            strikethrough: None,
            ..r.clone()
        })
        .collect()
}

#[derive(Default)]
pub(super) struct GridPaint {
    snapshot: Snapshot,
    #[cfg(test)]
    pub renders: std::rc::Rc<std::cell::Cell<usize>>,
}

impl GridPaint {
    pub fn replace_if_changed(view: &mut gpui::Entity<Self>, snapshot: Snapshot, cx: &mut App) {
        // Final painted rows include selection, cursor, colours and inversion.
        // A generation counter would miss UI changes without new output.
        if view.read(cx).snapshot != snapshot {
            #[cfg(test)]
            let renders = view.read(cx).renders.clone();
            // GPUI collects dirty views BEFORE rendering the parent. Notifying
            // an existing child here is too late for this frame. An immutable
            // replacement has no old paint to reuse, so the change is visible
            // in this very frame, with no extra frame scheduled to catch up.
            *view = cx.new(|_| Self {
                snapshot,
                #[cfg(test)]
                renders,
            });
        }
    }
}

impl Render for GridPaint {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders.set(self.renders.get() + 1);
        }
        let s = &self.snapshot;
        div()
            .px(px(s.pad.0))
            .py(px(s.pad.1))
            .flex()
            .flex_col()
            .children(s.lines.iter().cloned().map(|(text, runs)| {
                if s.crawl {
                    return match crawl_centered_runs(text, runs) {
                        Some((t, cut)) => div()
                            .h(px(s.cell_h))
                            .flex()
                            .justify_center()
                            .whitespace_nowrap()
                            .child(StyledText::new(t).with_runs(cut)),
                        None => div().h(px(s.cell_h)).whitespace_nowrap(),
                    };
                }
                let line = div().h(px(s.cell_h)).whitespace_nowrap();
                if text.is_empty() {
                    line
                } else if let Some(alpha) = s.bleed {
                    // The copies first, absolutely placed, so the real line
                    // paints last and sharp over its own glow.
                    let glow = bleed_runs(&runs, alpha);
                    BLEED
                        .iter()
                        .fold(line.relative(), |l, (dx, dy)| {
                            l.child(
                                div()
                                    .absolute()
                                    .left(px(*dx))
                                    .top(px(*dy))
                                    .whitespace_nowrap()
                                    .child(StyledText::new(text.clone()).with_runs(glow.clone())),
                            )
                        })
                        .child(StyledText::new(text).with_runs(runs))
                } else {
                    line.child(StyledText::new(text).with_runs(runs))
                }
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::harness::Pane;
    use gpui::TestAppContext;

    /// The bleed is each run's own colour, fainter, with nothing that would
    /// print four times: no background, no underline, no strike.
    #[test]
    fn a_bleed_copy_keeps_its_colour_and_drops_its_ground() {
        let red = gpui::hsla(0., 0.8, 0.5, 1.);
        let run = TextRun {
            len: 3,
            font: gpui::font("JetBrains Mono"),
            color: red,
            background_color: Some(gpui::black()),
            underline: Some(Default::default()),
            strikethrough: Some(Default::default()),
        };
        let glow = super::bleed_runs(&[run], 0.3);
        assert_eq!(glow[0].len, 3);
        assert_eq!((glow[0].color.h, glow[0].color.s, glow[0].color.l), (red.h, red.s, red.l));
        assert!((glow[0].color.a - 0.3).abs() < 1e-6);
        assert!(glow[0].background_color.is_none());
        assert!(glow[0].underline.is_none() && glow[0].strikethrough.is_none());
    }

    fn count(pane: &mut Pane) -> usize {
        pane.view
            .read_with(pane.cx, |v, cx| v.grid_paint.read(cx).renders.get())
    }

    #[gpui::test]
    fn quiet_grid_skips_layout_but_refresh_and_output_repaint(cx: &mut TestAppContext) {
        let mut pane = Pane::running(cx, "printf 'ready\\n'; exec cat");
        pane.wait_for("ready");
        pane.settle();
        pane.redraw();
        let before = count(&mut pane);
        assert!(before > 0, "counter must witness an actual grid render");
        for _ in 0..8 {
            pane.redraw();
        }
        assert_eq!(
            count(&mut pane),
            before,
            "quiet grid rebuilt on parent frames"
        );
        pane.cx.update(|window, _| window.refresh());
        pane.cx.run_until_parked();
        assert!(
            count(&mut pane) > before,
            "forced refresh must bypass reuse"
        );
        let before = count(&mut pane);
        pane.read(|v| v.session.term.lock().advance(b"\x1b[1;1HFRESH"));
        pane.redraw();
        assert!(
            count(&mut pane) > before,
            "output must repaint without relying on generation"
        );
        pane.view.read_with(pane.cx, |v, cx| {
            assert!(v
                .grid_paint
                .read(cx)
                .snapshot
                .lines
                .iter()
                .any(|(s, _)| s.contains("FRESH")));
        });
    }

    #[gpui::test]
    fn grid_cursor_selection_scroll_theme_and_geometry_invalidate(cx: &mut TestAppContext) {
        let mut pane = Pane::running(cx, "printf 'ready text\\n'; exec cat");
        pane.wait_for("ready");
        pane.settle();
        pane.redraw();
        let mutations: &[fn(&mut crate::vt::Term)] = &[
            |t| t.advance(b"\x1b[1;4H"),
            |t| t.advance(b"\x1b[?25l"),
            |t| t.advance(b"\x1b[?25h"),
            |t| {
                t.start_selection(
                    SelectionType::Simple,
                    TermPoint::new(Line(0), Column(0)),
                    Side::Left,
                );
                t.update_selection(TermPoint::new(Line(0), Column(4)), Side::Right);
            },
            |t| t.clear_selection(),
            |t| {
                for i in 0..80 {
                    t.advance(format!("row {i}\r\n").as_bytes());
                }
            },
            |t| t.scroll_display(Scroll::Delta(3)),
            |t| t.scroll_display(Scroll::Bottom),
            |t| t.advance(b"\x1b[?1049hALTERNATE"),
            |t| t.advance(b"\x1b[?1049l"),
        ];
        for (i, change) in mutations.iter().enumerate() {
            let before = count(&mut pane);
            pane.read(|v| change(&mut v.session.term.lock()));
            pane.redraw();
            assert!(
                count(&mut pane) > before,
                "mutation {i} left stale grid pixels"
            );
        }
        let before = count(&mut pane);
        pane.wear_own_theme();
        assert!(
            count(&mut pane) > before,
            "palette/font changes must redraw"
        );
        let before = count(&mut pane);
        pane.cx.simulate_resize(gpui::size(px(800.), px(540.)));
        pane.cx.run_until_parked();
        assert!(count(&mut pane) > before, "changed bounds must redraw");
    }
}
