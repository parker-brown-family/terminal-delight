//! One retained styled viewport. The key contains actual cells and view state:
//! UI-side scrolling and selections need not emit a terminal content event.
use super::*;

pub(super) type Lines = Vec<(String, Vec<TextRun>)>;

#[derive(PartialEq)]
pub(super) struct Key {
    pub cells: Vec<crate::vt::Indexed>,
    pub selection: Option<crate::vt::SelectionRange>,
    pub cursor: crate::vt::Cursor,
    pub display_offset: usize,
    pub show_cursor: bool,
    pub cols: usize,
    pub rows: usize,
    pub agent: bool,
    pub sel_visual: bool,
    pub fallbacks_armed: bool,
    pub theme: Theme,
}

pub(super) struct Memo {
    pub key: Key,
    pub lines: std::rc::Rc<Lines>,
    pub glyphs: std::rc::Rc<Lines>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::harness::{Pane, Scratch};
    use gpui::TestAppContext;

    #[gpui::test]
    fn afterglow_keeps_text_colours_before_selection_and_cursor_overlays(cx: &mut TestAppContext) {
        let mut pane = Pane::running(
            cx,
            "printf '\\033[31mWORD\\033[0m\\033[7mI\\033[0m'; exec cat",
        );
        pane.wait_for("WORDI");
        let th = pane.view.read_with(pane.cx, |v, cx| v.resolved_theme(cx));
        styled(&mut pane, &th, false);
        let (live, clean) = pane.read(|v| {
            let m = v.styled_memo.borrow();
            let m = m.as_ref().unwrap();
            (m.lines.clone(), m.glyphs.clone())
        });
        pane.read(|v| {
            let mut t = v.session.term.lock();
            t.start_selection(
                SelectionType::Simple,
                TermPoint::new(Line(0), Column(0)),
                Side::Left,
            );
            t.update_selection(TermPoint::new(Line(0), Column(3)), Side::Right);
            t.advance(b"\x1b[1;2H");
        });
        styled(&mut pane, &th, false);
        pane.read(|v| {
            let m = v.styled_memo.borrow();
            let m = m.as_ref().unwrap();
            assert_ne!(
                m.lines, live,
                "the selection and cursor still paint normally"
            );
            assert_eq!(
                m.glyphs, clean,
                "retained glyph colours exclude TD overlays"
            );
            assert!(m
                .glyphs
                .iter()
                .flat_map(|(_, runs)| runs)
                .all(|r| r.background_color.is_none()));
        });
    }

    fn styled(pane: &mut Pane, th: &Theme, visual: bool) -> Lines {
        pane.read(|v| v.styled_lines(th, visual))
    }

    fn memo(pane: &mut Pane) -> std::rc::Rc<Lines> {
        pane.read(|v| {
            v.styled_memo
                .borrow()
                .as_ref()
                .expect("styled viewport")
                .lines
                .clone()
        })
    }

    fn clear(pane: &mut Pane) {
        pane.read(|v| *v.styled_memo.borrow_mut() = None);
    }

    fn assert_fresh(pane: &mut Pane, th: &Theme, visual: bool) {
        let actual = styled(pane, th, visual);
        clear(pane);
        assert_eq!(
            actual,
            styled(pane, th, visual),
            "cached output must match a fresh render"
        );
    }

    #[gpui::test]
    fn styled_viewport_reuses_only_identical_cells_and_view_state(cx: &mut TestAppContext) {
        let mut pane = Pane::running(
            cx,
            "printf '\\033[31mred\\033[0m text\\n❯ human\\nreply\\n'; exec cat",
        );
        pane.wait_for("reply");
        pane.settle();
        let mut th = pane.view.read_with(pane.cx, |v, cx| v.resolved_theme(cx));
        styled(&mut pane, &th, false);
        let kept = memo(&mut pane);
        styled(&mut pane, &th, false);
        assert!(
            std::rc::Rc::ptr_eq(&kept, &memo(&mut pane)),
            "a redraw reuses its styling"
        );
        // Each mutation must retire the old result AND equal a fresh render.
        // These modify the real core, not its event generation, deliberately.
        let mutations: &[fn(&mut crate::vt::Term)] = &[
            |t| t.advance(b"\x1b[1;1Hchanged"),
            |t| {
                t.start_selection(
                    SelectionType::Simple,
                    TermPoint::new(Line(0), Column(0)),
                    Side::Left,
                );
                t.update_selection(TermPoint::new(Line(0), Column(2)), Side::Right);
            },
            |t| t.update_selection(TermPoint::new(Line(0), Column(5)), Side::Right),
            |t| t.clear_selection(),
            |t| t.advance(b"\x1b[2;4H"),
            |t| t.advance(b"\x1b[?25l"),
            |t| t.advance(b"\x1b[?1049hALT"),
            |t| t.advance(b"\x1b[?1049l"),
            |t| {
                for _ in 0..80 {
                    t.advance(b"history\r\n");
                }
            },
            |t| t.scroll_display(Scroll::Delta(3)),
            |t| t.scroll_display(Scroll::Bottom),
        ];
        for change in mutations {
            let before = memo(&mut pane);
            pane.read(|v| change(&mut v.session.term.lock()));
            let actual = styled(&mut pane, &th, false);
            assert!(
                !std::rc::Rc::ptr_eq(&before, &memo(&mut pane)),
                "changed view must retire its styling"
            );
            clear(&mut pane);
            assert_eq!(actual, styled(&mut pane, &th, false));
        }
        for change in [
            (|t: &mut Theme| t.text = gpui::rgb(0x19da51).into()) as fn(&mut Theme),
            |t| t.syntax = !t.syntax,
            |t| t.grade.brightness = 0.7,
            |t| t.ansi[1] = gpui::rgb(0xe8da19).into(),
            |t| t.cursor = gpui::rgb(0xaaff19).into(),
            |t| t.font_family = "monospace".into(),
            |t| t.crawl = !t.crawl,
        ] {
            let before = memo(&mut pane);
            change(&mut th);
            styled(&mut pane, &th, false);
            assert!(!std::rc::Rc::ptr_eq(&before, &memo(&mut pane)));
            assert_fresh(&mut pane, &th, false);
        }
        pane.running_as(PaneMode::Codex);
        assert_fresh(&mut pane, &th, false);
        assert_fresh(&mut pane, &th, true);
        pane.view.update(pane.cx, |v, _| v.grid.cols += 1);
        assert_fresh(&mut pane, &th, true);
        pane.view.update(pane.cx, |v, _| v.grid.rows += 1);
        assert_fresh(&mut pane, &th, true);
        // Post-processing a caller's rows must never alter the memo's copy.
        let mut outside = styled(&mut pane, &th, true);
        outside[0].0.push_str("caller-only");
        assert!(!styled(&mut pane, &th, true)[0].0.contains("caller-only"));
    }

    #[gpui::test]
    fn covered_faces_skip_grid_styling_and_terminal_return_catches_up(cx: &mut TestAppContext) {
        let mut pane = Pane::running(cx, "echo ready; exec cat");
        pane.wait_for("ready");
        pane.settle();
        let scratch = Scratch::new("covered-style");
        let doc = scratch.join("reading.md");
        std::fs::write(&doc, "# A document\n").unwrap();
        for face in [
            crate::workbench::Face::Workbench,
            crate::workbench::Face::Document,
        ] {
            if face == crate::workbench::Face::Document {
                pane.show_document(&doc);
            }
            pane.view.update(pane.cx, |v, cx| v.set_face(face, cx));
            clear(&mut pane);
            pane.read(|v| v.session.term.lock().advance(b"\r\narrived while covered"));
            pane.redraw();
            assert!(
                pane.read(|v| v.styled_memo.borrow().is_none()),
                "covered grid must not be styled"
            );
            pane.view.update(pane.cx, |v, cx| {
                v.set_face(crate::workbench::Face::Terminal, cx)
            });
            pane.redraw();
            let lines = memo(&mut pane);
            assert!(
                lines
                    .iter()
                    .any(|(t, _)| t.contains("arrived while covered")),
                "switching back shows current output"
            );
        }
    }

    #[gpui::test]
    fn quiet_documents_reuse_paint_and_changes_invalidate_it(cx: &mut TestAppContext) {
        let mut pane = Pane::running(cx, "echo ready; exec cat");
        pane.wait_for("ready");
        pane.settle();
        let scratch = Scratch::new("cached-document");
        let path = scratch.join("long.md");
        std::fs::write(
            &path,
            "# Reading\n\n".to_owned()
                + &"A paragraph with [a link](https://example.com).\n\n".repeat(120),
        )
        .unwrap();
        let target = crate::docopen::drawable_document(&path).unwrap();
        pane.view.update(pane.cx, |v, cx| {
            v.open_float(target, None, cx).unwrap();
        });
        pane.redraw();
        pane.settle();
        let view = pane.float_view().unwrap();
        let count = |pane: &mut Pane, view: &gpui::Entity<crate::docview::DocumentView>| {
            view.read_with(pane.cx, |v, _| v.render_count)
        };
        let before = count(&mut pane, &view);
        for _ in 0..5 {
            pane.redraw();
        }
        assert_eq!(
            count(&mut pane, &view),
            before,
            "parent redraws must not rebuild an unchanged floating document"
        );
        // GPUI's explicit refresh bypasses the cache: a positive control for
        // the render counter, and the same redraw path uncached views take.
        pane.cx.update(|window, _| window.refresh());
        pane.cx.run_until_parked();
        assert!(
            count(&mut pane, &view) > before,
            "bypassing the cache really redraws"
        );
        let before = count(&mut pane, &view);
        let body = Pane::middle(pane.float_zone(crate::docopen::FloatHit::Body).unwrap());
        pane.wheel(body, -5.0, Default::default());
        pane.redraw();
        assert!(count(&mut pane, &view) > before, "scrolling repaints");
        let before = count(&mut pane, &view);
        pane.wear_own_theme();
        assert!(count(&mut pane, &view) > before, "theme changes repaint");
        let before = count(&mut pane, &view);
        pane.view.update(pane.cx, |v, cx| {
            let square = v.float.as_mut().unwrap();
            square.rect.x += 7.0;
            square.rect.w -= 20.0;
            cx.notify();
        });
        pane.redraw();
        assert!(
            count(&mut pane, &view) > before,
            "new bounds repaint and re-record link positions"
        );
        pane.show_document(&path);
        pane.redraw();
        pane.settle();
        let face = pane.face_view().unwrap();
        let before = count(&mut pane, &face);
        for _ in 0..5 {
            pane.redraw();
        }
        assert_eq!(
            count(&mut pane, &face),
            before,
            "the Document face reuses paint too"
        );
    }

    /// Run explicitly in release mode; reports the styling path, not whole-window CPU.
    #[gpui::test]
    #[ignore]
    fn styled_viewport_benchmark(cx: &mut TestAppContext) {
        let mut pane = Pane::running(cx, "for i in $(seq 1 50); do printf '\\033[32mrow %s\\033[0m https://example.com/path code = 123;\\n' $i; done; echo ready; exec cat");
        pane.wait_for("ready");
        pane.settle();
        let mut th = pane.view.read_with(pane.cx, |v, cx| v.resolved_theme(cx));
        th.syntax = true;
        for round in 0..5 {
            for fresh in if round % 2 == 0 {
                [true, false]
            } else {
                [false, true]
            } {
                let start = std::time::Instant::now();
                for _ in 0..1000 {
                    if fresh {
                        clear(&mut pane);
                    }
                    std::hint::black_box(styled(&mut pane, &th, false));
                }
                eprintln!(
                    "styling round={round} fresh={fresh} calls=1000 elapsed_us={}",
                    start.elapsed().as_micros()
                );
            }
        }
    }
}
