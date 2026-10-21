//! Retain only terminal text layout and paint. Pane geometry, input handlers,
//! tube registration, headers and effects still run on every parent frame.
use super::*;

#[derive(Clone, PartialEq)]
pub(super) struct HistoryContext {
    pub theme: Theme,
    pub offset: usize,
    pub alternate: bool,
    pub cols: usize,
    pub rows: usize,
    pub inverted: bool,
}

#[derive(Default, PartialEq)]
pub(super) struct Snapshot {
    pub lines: styling::Lines,
    pub cell_h: f32,
    pub cell_w: f32,
    pub afterglow_ms: f32,
    pub context: Option<HistoryContext>,
    pub pad: (f32, f32),
    pub crawl: bool,
    /// The alpha of the phosphor bleed under every letter, or `None` for a
    /// clean grid — which is every design but Intrusive. See [`bleed_runs`].
    pub bleed: Option<f32>,
}

/// The bleed: two rings of copies, and the alpha each ring takes of the bleed's
/// strength. One ring a pixel out reads as BOLD, not as glow — measured on the
/// first native capture — so a fainter ring two pixels out softens the edge
/// into light. Eight copies, painted only when the text changes.
const BLEED: [(f32, f32, f32); 8] = [
    (-2., 0., 0.45),
    (2., 0., 0.45),
    (0., -2., 0.45),
    (0., 2., 0.45),
    (-1., 0., 0.8),
    (1., 0., 0.8),
    (0., -1., 0.8),
    (0., 1., 0.8),
];

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
    trail: afterglow::Trail,
    #[cfg(test)]
    pub renders: std::rc::Rc<std::cell::Cell<usize>>,
}

impl GridPaint {
    pub fn clear_afterglow(&mut self) {
        self.trail.clear();
        self.snapshot.context = None;
    }

    pub fn replace_if_changed(view: &mut gpui::Entity<Self>, snapshot: Snapshot, cx: &mut App) {
        // Final painted rows include selection, cursor, colours and inversion.
        // A generation counter would miss UI changes without new output.
        if view.read(cx).snapshot != snapshot {
            let previous = view.read(cx);
            let mut trail = previous.trail.clone();
            let compatible = previous.snapshot.context.is_some()
                && previous.snapshot.context == snapshot.context
                && previous.snapshot.cell_h == snapshot.cell_h
                && previous.snapshot.cell_w == snapshot.cell_w
                && previous.snapshot.pad == snapshot.pad
                && previous.snapshot.crawl == snapshot.crawl
                && previous.snapshot.afterglow_ms == snapshot.afterglow_ms;
            if compatible {
                trail.observe(
                    &previous.snapshot.lines,
                    &snapshot.lines,
                    cx.background_executor().now(),
                    snapshot.afterglow_ms,
                    snapshot
                        .context
                        .as_ref()
                        .map_or(afterglow::MIN_GLYPH_BUDGET, |c| {
                            c.cols
                                .saturating_mul(c.rows)
                                .max(afterglow::MIN_GLYPH_BUDGET)
                        }),
                );
            } else {
                trail.clear();
            }
            #[cfg(test)]
            let renders = view.read(cx).renders.clone();
            // GPUI collects dirty views BEFORE rendering the parent. Notifying
            // an existing child here is too late for this frame. An immutable
            // replacement has no old paint to reuse, so the change is visible
            // in this very frame, with no extra frame scheduled to catch up.
            *view = cx.new(|_| Self {
                snapshot,
                trail,
                #[cfg(test)]
                renders,
            });
        }
    }
}

impl Render for GridPaint {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(test)]
        {
            self.renders.set(self.renders.get() + 1);
        }
        let now = cx.background_executor().now();
        self.trail.prune(now);
        if !self.trail.glyphs.is_empty() {
            window.request_animation_frame();
        }
        let s = &self.snapshot;
        div()
            .relative()
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
                    let outer = bleed_runs(&runs, alpha * BLEED[0].2);
                    let inner = bleed_runs(&runs, alpha * BLEED[4].2);
                    BLEED
                        .iter()
                        .fold(line.relative(), |l, (dx, dy, k)| {
                            let glow = if *k < 0.6 { &outer } else { &inner };
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
            .children(self.trail.glyphs.iter().map(|ghost| {
                let run = TextRun {
                    color: ghost
                        .run
                        .color
                        .alpha(ghost.run.color.a * ghost.opacity(now)),
                    ..ghost.run.clone()
                };
                div()
                    .absolute()
                    .map(|d| {
                        if s.crawl {
                            d.left(gpui::relative(0.5)).ml(px((ghost.column as f32
                                - ghost.leading_columns as f32
                                - ghost.line_columns as f32 * 0.5)
                                * s.cell_w))
                        } else {
                            d.left(px(s.pad.0 + ghost.column as f32 * s.cell_w))
                        }
                    })
                    .top(px(s.pad.1 + ghost.row as f32 * s.cell_h))
                    .h(px(s.cell_h))
                    .whitespace_nowrap()
                    .child(StyledText::new(ghost.text.clone()).with_runs(vec![run]))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::harness::Pane;
    use gpui::TestAppContext;

    #[gpui::test]
    fn real_focus_changes_only_the_drawn_terminal_text(cx: &mut TestAppContext) {
        let mut pane = Pane::running(cx, "printf 'focus fixture\\n'; exec cat");
        pane.wait_for("focus fixture");
        pane.view.update(pane.cx, |v, cx| {
            let mut grade = theme::Grade::neutral();
            grade.text = 0.8;
            grade.afterglow_ms = Some(0.0);
            v.appearance.grade = Some(grade);
            v.appearance.pins = theme::GradePins::all();
            cx.notify();
        });
        pane.redraw();
        let before = pane.read(|v| v.appearance.clone());
        let active = pane.view.read_with(pane.cx, |v, cx| {
            v.grid_paint
                .read(cx)
                .snapshot
                .context
                .as_ref()
                .unwrap()
                .theme
                .grade
                .text
        });
        assert!((active - 0.8).abs() < 0.001);
        pane.cx.update(|window, _| window.blur());
        pane.redraw();
        let inactive = pane.view.read_with(pane.cx, |v, cx| {
            v.grid_paint
                .read(cx)
                .snapshot
                .context
                .as_ref()
                .unwrap()
                .theme
                .grade
                .text
        });
        assert!((inactive - 0.36).abs() < 0.001);
        pane.cx.update(|window, cx| {
            window.focus(&pane.view.read(cx).focus_handle.clone(), cx);
        });
        pane.redraw();
        assert!(
            pane.read(|v| v.appearance == before),
            "focus changed saved settings"
        );
        let active_again = pane.view.read_with(pane.cx, |v, cx| {
            v.grid_paint
                .read(cx)
                .snapshot
                .context
                .as_ref()
                .unwrap()
                .theme
                .grade
                .text
        });
        assert!((active_again - active).abs() < 0.001);
    }

    #[gpui::test]
    fn an_erased_paragraph_survives_bottom_alignment_and_cursor_motion(cx: &mut TestAppContext) {
        let mut pane = Pane::running(cx, "printf 'ready\\n'; exec cat");
        pane.wait_for("ready");
        pane.view.update(pane.cx, |v, cx| {
            let mut grade = theme::Grade::neutral();
            grade.afterglow_ms = Some(220.0);
            v.appearance.grade = Some(grade);
            v.appearance.pins = theme::GradePins::all();
            v.session
                .term
                .lock()
                .advance(b"\x1b[2J\x1b[HWHOLE WORD\r\nSECOND ROW\r\nFINAL ROW");
            v.grid_paint.update(cx, |g, _| g.clear_afterglow());
            cx.notify();
        });
        pane.redraw();
        let old_offset = pane.read(|v| v.paint_offset);
        pane.read(|v| v.session.term.lock().advance(b"\x1b[2J\x1b[H>"));
        pane.redraw();
        assert_ne!(
            old_offset,
            pane.read(|v| v.paint_offset),
            "fixture must move bottom alignment"
        );
        let erased = pane.view.read_with(pane.cx, |v, cx| {
            v.grid_paint
                .read(cx)
                .trail
                .glyphs
                .iter()
                .map(|g| (g.row, g.column, g.text.clone()))
                .collect::<Vec<_>>()
        });
        assert_eq!(
            erased
                .iter()
                .map(|(_, _, s)| s.as_str())
                .collect::<String>(),
            "WHOLEWORDSECONDROWFINALROW"
        );
        pane.cx.executor().advance_clock(Duration::from_millis(260));
        pane.read(|v| v.session.term.lock().advance(b"\x1b[1;3H"));
        pane.redraw();
        let after_cursor = pane.view.read_with(pane.cx, |v, cx| {
            v.grid_paint
                .read(cx)
                .trail
                .glyphs
                .iter()
                .map(|g| (g.row, g.column, g.text.clone()))
                .collect::<Vec<_>>()
        });
        assert_eq!(after_cursor, erased);
        pane.cx.executor().advance_clock(Duration::from_millis(190));
        pane.cx.update(|window, _| window.refresh());
        pane.cx.run_until_parked();
        assert!(pane.view.read_with(pane.cx, |v, cx| v
            .grid_paint
            .read(cx)
            .trail
            .glyphs
            .is_empty()));
    }

    #[gpui::test]
    fn live_grid_shows_new_text_while_old_glyphs_expire_and_clear_on_cover(
        cx: &mut TestAppContext,
    ) {
        let mut pane = Pane::running(cx, "printf 'ready\\n'; exec cat");
        pane.wait_for("ready");
        pane.view.update(pane.cx, |v, cx| {
            let mut grade = theme::Grade::neutral();
            grade.afterglow_ms = Some(200.0);
            v.appearance.grade = Some(grade);
            v.appearance.pins = theme::GradePins::all();
            v.session.term.lock().advance(b"\x1b[2J\x1b[HOLD");
            v.grid_paint.update(cx, |g, _| g.clear_afterglow());
            cx.notify();
        });
        pane.redraw();
        pane.read(|v| v.session.term.lock().advance(b"\x1b[HNEW"));
        pane.redraw();
        let (new_is_live, ghosts) = pane.view.read_with(pane.cx, |v, cx| {
            let grid = v.grid_paint.read(cx);
            (
                grid.snapshot
                    .lines
                    .iter()
                    .any(|(text, _)| text.contains("NEW")),
                grid.trail.glyphs.len(),
            )
        });
        assert!(new_is_live, "new text was delayed");
        assert!(ghosts > 0, "replaced letters left no afterglow");
        pane.cx.executor().advance_clock(Duration::from_millis(250));
        pane.cx.update(|window, _| window.refresh());
        pane.cx.run_until_parked();
        assert!(pane.view.read_with(pane.cx, |v, cx| v
            .grid_paint
            .read(cx)
            .trail
            .glyphs
            .is_empty()));
        pane.read(|v| v.session.term.lock().advance(b"\x1b[HOLD"));
        pane.redraw();
        pane.view.update(pane.cx, |v, cx| {
            v.set_face(crate::workbench::Face::Workbench, cx)
        });
        pane.redraw();
        assert!(pane.view.read_with(pane.cx, |v, cx| v
            .grid_paint
            .read(cx)
            .trail
            .glyphs
            .is_empty()));
    }

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
        assert_eq!(
            (glow[0].color.h, glow[0].color.s, glow[0].color.l),
            (red.h, red.s, red.l)
        );
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
