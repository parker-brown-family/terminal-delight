//! What a pane does with a person's hands, held by driving a real one.
//!
//! Every test here builds a [`TerminalView`](super::TerminalView) through the
//! harness ([`super::harness`]), puts something on its screen the way a
//! program would — by printing it — and then presses, moves, releases and
//! types at it through gpui, the way the window does. What is asserted is
//! what a person would see or get: a square open or gone, where it sits, what
//! landed on the clipboard, what went to the desktop.

use gpui::{point, px, TestAppContext};

use super::harness::{
    desktop_launches, Asked, FakeBriefEngine, Pane, Rect, Scratch, BRIEF_ANCHORS,
};
use crate::docopen::{Asker, DocSeat, FloatHit};
use crate::workbench::Face;

/// A picture of the repository's own, and the words printed around it.
const PICTURE: &str = "assets/img/logo-mark.png";

/// Whether two rectangles are the same to a hundredth of a pixel: the square's
/// place is the sum of float moves, and a pointer's travel added to a place
/// can land a rounding step away from the same sum written out.
fn same_rect(a: Rect, b: Rect) -> bool {
    let close = |p: f32, q: f32| (p - q).abs() < 0.01;
    close(a.0, b.0) && close(a.1, b.1) && close(a.2, b.2) && close(a.3, b.3)
}

/// A pane that has printed a picture's path, and the path.
fn pane_showing_a_picture(cx: &mut TestAppContext, tag: &str) -> (Pane, Scratch, String) {
    let dir = Scratch::new(tag);
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'saved {png}' 'ready'; exec cat"),
    );
    pane.wait_for("ready");
    (pane, dir, png)
}

/// Alt+click on a picture's path opens it in a floating square just below
/// the line, where there is room for it, and Escape closes it. Nothing goes to
/// the desktop.
#[gpui::test]
fn alt_click_on_a_printed_picture_opens_a_square_and_escape_closes_it(cx: &mut TestAppContext) {
    let dir = Scratch::new("alt-open");
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().expect("a UTF-8 temp path").to_string();
    // The path near the top of the screen, with room below it for the square.
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'saved {png}'; for i in $(seq 1 24); do echo; done; echo ready; exec cat"),
    );
    pane.wait_for("ready");
    assert_eq!(pane.float_path(), None);

    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&png)),
        "an Alt+click on the path opens that picture in a square"
    );
    let (_, y, _, _) = pane.float_rect().expect("the square is drawn");
    let (_, line_bottom) = pane.row_span(at);
    assert!(
        y >= line_bottom && y - line_bottom < 10.0,
        "the square opens just below the line it was opened from: \
         its top at {y}, the line's bottom at {line_bottom}"
    );
    assert_eq!(desktop_launches(), Vec::<String>::new());

    pane.keys("escape");
    assert_eq!(pane.float_path(), None, "Escape closes the square");
}

/// The square's strip moves it with the hand, and its edge resizes it with the
/// far side pinned. Neither leaves a selection in the grid behind it.
#[gpui::test]
fn the_square_moves_by_its_strip_and_resizes_by_its_edge(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "move-resize");
    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    let (x, y, w, h) = pane.float_rect().expect("a square to move");
    let (_, sy, _, sh) = pane.screen();

    // Up if there is room above, else down: the square opens below or above
    // its line depending on where the line is.
    let dy = if y - sy >= 40.0 {
        -30.0
    } else {
        assert!(sy + sh - (y + h) >= 40.0, "no room to move the square");
        30.0
    };
    // On the strip, clear of the corner's grip and of the buttons at its
    // right end.
    let grab = point(px(x + 40.0), px(y + 11.0));
    pane.press(grab);
    pane.drag_to(point(px(x + 40.0 - 120.0), px(y + 11.0 + dy)));
    pane.release(point(px(x + 40.0 - 120.0), px(y + 11.0 + dy)));
    let moved = pane.float_rect().expect("the square is still up");
    assert!(
        same_rect(moved, (x - 120.0, y + dy, w, h)),
        "the strip moves the square by the pointer's travel, and only moves it: \
         {moved:?} from {:?}",
        (x, y, w, h)
    );

    // The left border, inside its grip, halfway down.
    let (x, y, w, h) = moved;
    let edge = point(px(x + 3.0), px(y + h / 2.0));
    pane.press(edge);
    pane.drag_to(point(px(x + 3.0 - 50.0), px(y + h / 2.0)));
    pane.release(point(px(x + 3.0 - 50.0), px(y + h / 2.0)));
    let resized = pane.float_rect().expect("the square is still up");
    assert!(
        same_rect(resized, (x - 50.0, y, w + 50.0, h)),
        "the left edge moves and the right side stays where it was: \
         {resized:?} from {moved:?}"
    );

    assert!(
        !pane.read(|v| v.has_selection()),
        "a press on the square never starts a selection in the grid behind it"
    );
}

/// Alt+click on a command line copies the whole line, and opens nothing.
#[gpui::test]
fn alt_click_on_a_command_line_copies_it_and_opens_nothing(cx: &mut TestAppContext) {
    let command = "cargo test -p terminal-delight --lib host";
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' '{command}' 'ready'; exec cat"));
    pane.wait_for("ready");

    let at = pane.point_at("terminal-delight");
    pane.click(at, Pane::alt());
    assert_eq!(
        pane.clipboard().as_deref(),
        Some(command),
        "the command line is on the clipboard, whole"
    );
    assert_eq!(pane.float_path(), None, "a copy opens no square");
    assert_eq!(desktop_launches(), Vec::<String>::new());
    assert!(!pane.read(|v| v.has_selection()));
}

/// A pane that has printed the fixture brief's path and a picture's, with a
/// page engine that lays the brief out without a browser.
fn pane_showing_a_brief(cx: &mut TestAppContext, tag: &str) -> (Pane, Scratch, String, String) {
    let dir = Scratch::new(tag);
    let brief = dir.fixture("tests/fixtures/page/brief.html", "brief.html");
    let brief = brief.to_str().expect("a UTF-8 temp path").to_string();
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'brief {brief}' 'shot {png}' 'ready'; exec cat"),
    );
    pane.html_engine(Ok(std::sync::Arc::new(FakeBriefEngine)));
    pane.wait_for("ready");
    (pane, dir, brief, png)
}

/// Alt+click the brief open, wait for its page to be laid out, and write a
/// note into it that is not saved.
fn open_the_brief_with_an_unsaved_note(pane: &mut Pane, brief: &str) {
    let at = pane.point_at(brief);
    pane.click(at, Pane::alt());
    pane.redraw();
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(brief)),
        "the brief opens in a square"
    );
    let report = pane
        .doc_notes()
        .expect("the brief is laid out, with its notes");
    assert_eq!(report["state"], "notes", "{report}");
    let (nid, _) = BRIEF_ANCHORS[0];
    pane.view
        .update(pane.cx, |v, cx| {
            v.doc_note(
                crate::docview::NotesCommand::Add {
                    nid: nid.into(),
                    text: "Keep the tiles.".into(),
                },
                cx,
            )
        })
        .expect("a note is added");
    assert_eq!(pane.doc_notes().expect("notes")["unsaved"], 1);
}

/// A square holding a note not yet saved into its brief is kept once when it
/// is closed, and says why in the brief's bar; asked a second time, it goes.
/// Escape, the ✕ and an Alt+click that would put another document in its
/// place all ask the same way.
#[gpui::test]
fn a_square_with_unsaved_notes_is_kept_once_then_closes(cx: &mut TestAppContext) {
    let (mut pane, _dir, brief, png) = pane_showing_a_brief(cx, "unsaved");
    let brief_path = std::path::PathBuf::from(&brief);
    let said_unsaved = |pane: &mut Pane| {
        pane.doc_notes().expect("notes")["said"]
            .as_str()
            .is_some_and(|s| s.contains("unsaved"))
    };

    open_the_brief_with_an_unsaved_note(&mut pane, &brief);
    pane.keys("escape");
    assert_eq!(
        pane.float_path(),
        Some(brief_path.clone()),
        "Escape keeps it once"
    );
    assert!(said_unsaved(&mut pane), "and the bar says why");
    pane.keys("escape");
    assert_eq!(pane.float_path(), None, "the second Escape closes it");

    open_the_brief_with_an_unsaved_note(&mut pane, &brief);
    let close = pane.float_zone(FloatHit::Close).expect("the square's ✕");
    pane.click(Pane::middle(close), Default::default());
    assert_eq!(
        pane.float_path(),
        Some(brief_path.clone()),
        "the ✕ keeps it once"
    );
    assert!(said_unsaved(&mut pane));
    let close = pane.float_zone(FloatHit::Close).expect("the square's ✕");
    pane.click(Pane::middle(close), Default::default());
    assert_eq!(pane.float_path(), None, "the second ✕ closes it");

    open_the_brief_with_an_unsaved_note(&mut pane, &brief);
    let other = pane.point_at(&png);
    pane.click(other, Pane::alt());
    assert_eq!(
        pane.float_path(),
        Some(brief_path),
        "another document does not take its place while it holds unsaved notes"
    );
    assert!(said_unsaved(&mut pane));
    pane.click(other, Pane::alt());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&png)),
        "asked again, the other document takes its place"
    );
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// Whether `p` lies inside `r` and at least 20 pixels from each of its edges,
/// clear of any grip a border has.
fn well_inside(r: Rect, p: gpui::Point<gpui::Pixels>) -> bool {
    let (x, y) = (f32::from(p.x), f32::from(p.y));
    x > r.0 + 20.0 && x < r.0 + r.2 - 20.0 && y > r.1 + 20.0 && y < r.1 + r.3 - 20.0
}

/// Held modifiers, by name.
fn held(alt: bool, control: bool, shift: bool, platform: bool) -> gpui::Modifiers {
    gpui::Modifiers {
        alt,
        control,
        shift,
        platform,
        ..Default::default()
    }
}

/// What a click at `at` with `mods` held sent to the desktop.
fn launched_by(
    pane: &mut Pane,
    at: gpui::Point<gpui::Pixels>,
    mods: gpui::Modifiers,
) -> Vec<String> {
    let before = desktop_launches().len();
    pane.click(at, mods);
    desktop_launches()[before..].to_vec()
}

/// Every modified click on a path or link does what the click table
/// (`docopen::click_intent`) says: Shift on a file reveals it, and so does
/// Super+Ctrl; Ctrl on a file or a link hands it to the desktop, and so does
/// Shift on a web link, which has nothing on disk to reveal; Alt on a line
/// that is neither a document nor a command does nothing.
#[gpui::test]
fn every_modified_click_on_a_path_does_what_the_click_table_says(cx: &mut TestAppContext) {
    let dir = Scratch::new("click-table");
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().expect("a UTF-8 temp path").to_string();
    let url = "https://example.com/td-harness";
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'file {png}' 'link {url}' 'ready'; exec cat"),
    );
    pane.wait_for("ready");
    let on_file = pane.point_at(&png);
    let on_link = pane.point_at(url);
    let reveals = |got: &[String]| {
        got.len() == 1
            && got[0].starts_with("sh -c dbus-send")
            && got[0].contains("FileManager1.ShowItems")
            && got[0].contains(&format!("file://{png}"))
    };
    let opens = |got: &[String], what: &str| {
        got.len() == 1 && got[0].ends_with(&format!("xdg-open {what}"))
    };

    let got = launched_by(&mut pane, on_file, held(false, false, true, false));
    assert!(reveals(&got), "shift+click on a file reveals it: {got:?}");
    let got = launched_by(&mut pane, on_file, held(false, true, false, true));
    assert!(
        reveals(&got),
        "super+ctrl+click on a file reveals it: {got:?}"
    );
    let got = launched_by(&mut pane, on_file, held(false, true, false, false));
    assert!(opens(&got, &png), "ctrl+click on a file opens it: {got:?}");
    let got = launched_by(&mut pane, on_link, held(false, true, false, false));
    assert!(opens(&got, url), "ctrl+click on a link opens it: {got:?}");
    let got = launched_by(&mut pane, on_link, held(false, false, true, false));
    assert!(
        opens(&got, url),
        "shift+click on a web link opens it: {got:?}"
    );
    assert_eq!(pane.float_path(), None, "none of those opens a square");

    let at = pane.point_at("ready");
    let got = launched_by(&mut pane, at, Pane::alt());
    assert_eq!(got, Vec::<String>::new(), "alt+click on plain words");
    assert_eq!(pane.float_path(), None);
    assert_eq!(pane.clipboard(), None);
}

/// The square lies over the grid and over the note, so a press on it is its
/// own: an Alt+click there does not open the other picture printed under it,
/// and a click there does not pick up the note underneath.
#[gpui::test]
fn a_press_on_the_square_is_the_squares_even_over_a_path_or_a_note(cx: &mut TestAppContext) {
    let dir = Scratch::new("square-first");
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().expect("a UTF-8 temp path").to_string();
    let under = dir.fixture(PICTURE, "under.png");
    let under = under.to_str().expect("a UTF-8 temp path").to_string();
    // The other picture, right-aligned on each of many rows so its name ends
    // at the same column under the square, however long the temp path is.
    let mut pane = Pane::running(
        cx,
        &format!(
            "for i in $(seq 1 24); do printf 'row %02d %90s\\n' \"$i\" '{under}'; done; \
             printf '%s\\n' 'open {png}' 'ready'; exec cat"
        ),
    );
    pane.wait_for("ready");
    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    let body = pane.float_zone(FloatHit::Body).expect("the square's body");
    // Aimed at the file's name, which ends the path: a click anywhere on a
    // path is a click on the whole of it.
    let beneath = pane
        .points_at_all("under.png")
        .into_iter()
        .find(|p| well_inside(body, *p))
        .expect("the other picture's path lies under the square somewhere");

    let got = launched_by(&mut pane, beneath, Pane::alt());
    assert_eq!(got, Vec::<String>::new());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&png)),
        "an Alt+click on the square does not open what is printed under it"
    );
    let got = launched_by(&mut pane, beneath, held(false, true, false, false));
    assert_eq!(got, Vec::<String>::new(), "nor does a Ctrl+click");

    // The note, stuck to the pane; the square is moved over it.
    pane.view.update(pane.cx, |v, cx| {
        v.note_post(None, "a note under the square".into(), false, cx)
    });
    pane.redraw();
    let note = pane
        .read(|v| v.note_layout().map(|l| l.center))
        .expect("the note is laid out");
    let (x, y, w, h) = pane.float_rect().expect("the square");
    let grab = gpui::point(px(x + 40.0), px(y + 11.0));
    let (dx, dy) = (
        f32::from(note.x) - (x + w / 2.0),
        f32::from(note.y) - (y + h / 2.0),
    );
    pane.press(grab);
    pane.drag_to(point(grab.x + px(dx), grab.y + px(dy)));
    pane.release(point(grab.x + px(dx), grab.y + px(dy)));
    let body = pane.float_zone(FloatHit::Body).expect("the square's body");
    assert!(
        well_inside(body, note),
        "the square now covers the note's middle: {body:?} against {note:?}"
    );
    pane.click(note, Default::default());
    assert!(
        !pane.read(|v| v.sticky_composing()),
        "a click on the square over the note does not pick the note up"
    );
}

/// A square dragged and let go outside the pane is let go: gpui hands the pane
/// no mouse-up there, and without one the square would stay stuck to the hand
/// and move with the next drag, wherever it started.
#[gpui::test]
fn a_square_let_go_outside_the_pane_does_not_follow_the_next_drag(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "let-go-outside");
    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    let (x, y, _, _) = pane.float_rect().expect("a square");
    let grab = point(px(x + 40.0), px(y + 11.0));
    pane.press(grab);
    pane.drag_to(point(grab.x - px(60.0), grab.y));
    pane.release(point(px(-40.0), grab.y));
    let dropped = pane.float_rect().expect("the square is still up");

    // A drag in the grid, well away from the square.
    let from = pane.point_at("ready");
    pane.press(from);
    pane.drag_to(point(from.x + px(100.0), from.y));
    pane.release(point(from.x + px(100.0), from.y));
    assert!(
        same_rect(pane.float_rect().expect("the square"), dropped),
        "the square stays where it was let go"
    );
    assert!(
        pane.read(|v| v.has_selection()),
        "the drag in the grid selects, as any drag there does"
    );
}

/// Leaving the pane puts out what the pointer lit in it: the square's ✕ is lit
/// while the pointer is on it, and goes out when the pointer moves off the
/// pane, and when it leaves the window, which gpui reports with no move.
#[gpui::test]
fn leaving_the_pane_puts_out_the_control_the_pointer_lit(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "leave-lights");
    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    let close = Pane::middle(pane.float_zone(FloatHit::Close).expect("the ✕"));
    let lit = |pane: &mut Pane| pane.read(|v| v.float.as_ref().and_then(|f| f.hover));

    pane.hover(close);
    assert_eq!(lit(&mut pane), Some(FloatHit::Close), "the ✕ lights");
    pane.hover(point(px(-30.0), px(300.0)));
    assert_eq!(lit(&mut pane), None, "a move off the pane puts it out");

    pane.hover(close);
    assert_eq!(lit(&mut pane), Some(FloatHit::Close));
    pane.leave_window();
    assert_eq!(lit(&mut pane), None, "leaving the window puts it out");
}

/// A pane that has printed two hundred lines of history, then a Markdown
/// document's path, with the document open in a square.
fn pane_with_history_and_a_document(cx: &mut TestAppContext, tag: &str) -> (Pane, Scratch) {
    let dir = Scratch::new(tag);
    let md = dir.fixture("../README.md", "readme.md");
    let md = md.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(
        cx,
        &format!("seq 1 200; printf '%s\\n' 'doc {md}' 'ready'; exec cat"),
    );
    pane.wait_for("ready");
    let at = pane.point_at(&md);
    pane.click(at, Pane::alt());
    pane.redraw();
    let view = pane.float_view().expect("the document opens in a square");
    assert_eq!(
        pane.scroll_of(&view),
        Some(0.0),
        "the document is laid out, at its top"
    );
    (pane, dir)
}

/// The wheel over the square moves the document in it, and never the
/// scrollback behind it; off the square it moves the scrollback, and never
/// the document. Ctrl+wheel over the square zooms the document and leaves the
/// pane's text dial alone; off the square it is the text dial again. And the
/// FOCUS reader, which scrolls the pane with the pointer over its own modal,
/// never reaches a square hidden underneath.
///
/// Parker, 2026-09-25: *"When I am hovering over a floating doc to read the
/// ctl+mouse wheel should resize THE DOC! not the pane underneath!"*
#[gpui::test]
fn the_wheel_over_the_square_moves_its_document_and_nothing_else(cx: &mut TestAppContext) {
    let (mut pane, _dir) = pane_with_history_and_a_document(cx, "wheel-square");
    let view = pane.float_view().expect("a square");
    let body = Pane::middle(pane.float_zone(FloatHit::Body).expect("its body"));

    // Ctrl first, while the document is at its top: a turn that scrolled the
    // document would move it off the top, and a zoom must keep it there.
    let size = |pane: &mut Pane| {
        pane.view
            .read_with(pane.cx, |v, cx| v.resolved_theme(cx).font_size)
    };
    let zoom = |pane: &mut Pane, view: &gpui::Entity<crate::docview::DocumentView>| {
        view.read_with(pane.cx, |v, _| v.zoom_now())
    };
    let was = size(&mut pane);
    assert_eq!(
        zoom(&mut pane, &view),
        Some(crate::docview::ImageZoom::Scale(1.0)),
        "a Markdown document opens at 100%"
    );
    pane.wheel(body, 1.0, held(false, true, false, false));
    pane.redraw();
    assert_eq!(
        zoom(&mut pane, &view),
        Some(crate::docview::ImageZoom::Scale(1.1)),
        "ctrl+wheel up over the square zooms the document in a step"
    );
    assert_eq!(
        size(&mut pane),
        was,
        "and the text dial of the pane under it does not move"
    );
    assert_eq!(
        pane.scroll_of(&view),
        Some(0.0),
        "the document stays at its top"
    );
    pane.wheel(body, -2.0, held(false, true, false, false));
    assert_eq!(
        zoom(&mut pane, &view),
        Some(crate::docview::ImageZoom::Scale(0.9)),
        "two notches down step out twice"
    );

    let grid = pane.point_at("ready");
    pane.wheel(grid, -1.0, held(false, true, false, false));
    assert_ne!(
        size(&mut pane),
        was,
        "off the square, ctrl+wheel is the pane's text dial"
    );
    assert_eq!(
        zoom(&mut pane, &view),
        Some(crate::docview::ImageZoom::Scale(0.9)),
        "and the document's zoom stays"
    );
    pane.redraw();

    pane.wheel(body, -3.0, Default::default());
    let read_to = pane.scroll_of(&view).expect("laid out");
    assert!(read_to > 0.0, "the document scrolls down");
    assert_eq!(pane.scrolled_back(), 0, "and the scrollback stays live");

    let grid = pane.point_at("ready");
    pane.wheel(grid, 3.0, Default::default());
    let back = pane.scrolled_back();
    assert!(back > 0, "off the square the wheel walks the history");
    assert_eq!(
        pane.scroll_of(&view),
        Some(read_to),
        "and the document stays"
    );

    let focus_turn = gpui::ScrollWheelEvent {
        position: body,
        delta: gpui::ScrollDelta::Lines(point(0.0, 3.0)),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    };
    pane.view
        .update(pane.cx, |v, cx| v.scroll_by_wheel(&focus_turn, cx));
    pane.redraw();
    assert!(pane.scrolled_back() > back, "the reader walks the history");
    assert_eq!(
        pane.scroll_of(&view),
        Some(read_to),
        "not the square under it"
    );
}

/// A square floating over the workbench takes the wheel over it, both ways:
/// ctrl zooms the document and a plain turn scrolls it. The bench paints a
/// capture-phase hook that runs before the pane root and halts every turn on
/// the bench, so the square has to be asked from inside that hook — until it
/// was, ctrl+wheel over the square sized the bench and a plain turn scrolled
/// whatever of the bench lay under it.
#[gpui::test]
fn the_wheel_over_a_square_on_the_bench_is_the_documents(cx: &mut TestAppContext) {
    let (mut pane, _dir) = pane_with_history_and_a_document(cx, "wheel-bench");
    pane.view
        .update(pane.cx, |v, cx| v.set_face(Face::Workbench, cx));
    pane.redraw();
    assert_eq!(pane.face(), Face::Workbench);
    let view = pane
        .float_view()
        .expect("the square stays up over the bench");
    let body = Pane::middle(pane.float_zone(FloatHit::Body).expect("its body"));
    let looks = pane.read(|v| v.appearance.clone());

    pane.wheel(body, 1.0, held(false, true, false, false));
    pane.redraw();
    assert_eq!(
        view.read_with(pane.cx, |v, _| v.zoom_now()),
        Some(crate::docview::ImageZoom::Scale(1.1)),
        "ctrl+wheel over the square zooms the document"
    );
    assert!(
        pane.read(|v| v.appearance == looks),
        "and sizes nothing of the pane's own, the bench's dial included"
    );

    pane.wheel(body, -3.0, Default::default());
    pane.redraw();
    assert!(
        pane.scroll_of(&view).expect("laid out") > 0.0,
        "a plain turn over the square scrolls the document"
    );
}

/// Ctrl+wheel over a brief zooms it as a browser does. The notches are
/// counted as they come and the page waits for them to stop; then it is laid
/// out again in a column narrower by the zoom, at the window's scale times the
/// zoom, and the strip carries the zoom's controls. The pane's own sizes never
/// move.
#[gpui::test]
fn ctrl_wheel_over_a_brief_lays_it_out_again_at_the_zoom(cx: &mut TestAppContext) {
    let (mut pane, _dir, brief, _png) = pane_showing_a_brief(cx, "brief-zoom");
    let at = pane.point_at(&brief);
    pane.click(at, Pane::alt());
    pane.redraw();
    let view = pane.float_view().expect("the brief opens in a square");
    let laid = |pane: &mut Pane| view.update(pane.cx, |v, _| v.page_laid_out());
    let before = laid(&mut pane).expect("the brief is laid out");
    let looks = pane.read(|v| v.appearance.clone());
    let body = Pane::middle(pane.float_zone(FloatHit::Body).expect("its body"));

    pane.wheel(body, 1.0, held(false, true, false, false));
    pane.wheel(body, 1.0, held(false, true, false, false));
    pane.redraw();
    assert_eq!(
        view.read_with(pane.cx, |v, _| v.zoom_now()),
        Some(crate::docview::ImageZoom::Scale(1.25)),
        "two notches up, two steps in"
    );
    assert!(
        pane.float_zone(FloatHit::ZoomFit).is_some(),
        "and the strip shows the zoom"
    );
    assert_eq!(
        laid(&mut pane),
        Some(before),
        "nothing is laid out while the notches may still be coming"
    );

    pane.cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(300));
    pane.redraw();
    let after = laid(&mut pane).expect("still laid out");
    assert!(
        (after.scale - before.scale * 1.25).abs() < 1e-4,
        "drawn at the window's scale times the zoom: {} from {}",
        after.scale,
        before.scale
    );
    let narrower = before.css_width as f32 / 1.25;
    assert!(
        (after.css_width as f32 - narrower).abs() <= 1.0,
        "laid out in a column narrower by the zoom: {} from {}",
        after.css_width,
        before.css_width
    );
    assert!(
        pane.read(|v| v.appearance == looks),
        "and nothing of the pane's own was sized"
    );
}

/// Ctrl+Alt+click on a document asks the workspace for a pane beside this one,
/// with the row it was clicked on; a second Alt+click on the path the square
/// already shows, and the square's own split button, ask for the square to be
/// promoted, carrying its view so nothing is opened twice.
#[gpui::test]
fn the_split_gestures_ask_the_workspace(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "split");
    let asked = pane.asks_beside();
    let at = pane.point_at(&png);
    let row = pane.painted_row(at);
    let path = std::path::PathBuf::from(&png);

    pane.click(at, held(true, true, false, false));
    assert_eq!(
        *asked.borrow(),
        vec![Asked {
            path: path.clone(),
            by: Asker::Click,
            carry: None,
            row: Some(row),
        }],
        "ctrl+alt+click asks for a pane beside, from that row"
    );
    assert_eq!(pane.float_path(), None, "and opens no square itself");

    pane.click(at, Pane::alt());
    let square = pane.float_view().expect("a square").entity_id();
    pane.click(at, Pane::alt());
    let split = pane
        .float_zone(FloatHit::Split)
        .expect("the square's split button");
    pane.click(Pane::middle(split), Default::default());
    let promoted = Asked {
        path: path.clone(),
        by: Asker::Float,
        carry: Some(square),
        row: None,
    };
    assert_eq!(
        asked.borrow()[1..].to_vec(),
        vec![promoted.clone(), promoted],
        "the second Alt+click and the split button both promote the square"
    );
    assert_eq!(
        pane.float_path(),
        Some(path),
        "the square stays until the workspace answers"
    );
}

/// A link pressed in a document is routed by the pane: a file TD can draw
/// takes the document's place — the same square, in the same spot, or the
/// same Document face — and a web link, or a file TD does not draw, goes to
/// the desktop. Any other scheme goes nowhere.
#[gpui::test]
fn a_link_out_of_a_document_is_routed_by_the_pane(cx: &mut TestAppContext) {
    let dir = Scratch::new("links");
    let md = dir.fixture("../README.md", "readme.md");
    let png = dir.fixture(PICTURE, "shot.png");
    let txt = dir.fixture("Cargo.toml", "notes.txt");
    let (md, png, txt) = (
        md.to_str().expect("UTF-8").to_string(),
        png.to_str().expect("UTF-8").to_string(),
        txt.to_str().expect("UTF-8").to_string(),
    );
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'doc {md}' 'ready'; exec cat"));
    pane.wait_for("ready");
    let at = pane.point_at(&md);
    pane.click(at, Pane::alt());
    let place = pane.float_rect().expect("the document's square");

    let view = pane.float_view().expect("a square");
    pane.follow(&view, &png);
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&png))
    );
    assert!(
        same_rect(pane.float_rect().expect("a square"), place),
        "the linked picture takes the document's place, in the same spot"
    );

    let view = pane.float_view().expect("a square");
    let before = desktop_launches().len();
    pane.follow(&view, "https://example.com/linked");
    pane.follow(&view, &txt);
    pane.follow(&view, "ftp://example.com/refused");
    let got = desktop_launches()[before..].to_vec();
    assert_eq!(
        got.len(),
        2,
        "the web link and the text file, and nothing else: {got:?}"
    );
    assert!(
        got[0].ends_with("xdg-open https://example.com/linked"),
        "{got:?}"
    );
    assert!(got[1].ends_with(&format!("xdg-open {txt}")), "{got:?}");
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&png))
    );

    // The Document face routes its links the same way, onto itself.
    pane.show_document(std::path::Path::new(&md));
    let face = pane.face_view().expect("the Document face");
    pane.follow(&face, &png);
    assert_eq!(
        pane.read(|v| v.document_path().map(|p| p.to_path_buf())),
        Some(std::path::PathBuf::from(&png)),
        "the face shows the linked picture"
    );
}

/// A video with no libmpv to play it goes to the desktop before any square or
/// pane is made, and the pane says why on the row that was clicked — the HTML
/// engine's refusal, for mpv. A split asked for is refused the same way.
#[gpui::test]
fn a_video_nothing_can_play_goes_to_the_desktop_and_says_why(cx: &mut TestAppContext) {
    let dir = Scratch::new("no-libmpv");
    let clip = dir.join("clip.mp4");
    // An MP4's first box. Nothing plays it here: the refusal comes first.
    std::fs::write(&clip, b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41").expect("write");
    let clip = clip.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'clip {clip}' 'ready'; exec cat"),
    );
    pane.video_ready(Err(crate::docview::mpv::Missing::NotFound));
    pane.wait_for("ready");
    let asked = pane.asks_beside();

    let at = pane.point_at(&clip);
    let row = pane.painted_row(at);
    let got = launched_by(&mut pane, at, Pane::alt());
    assert!(
        got.len() == 1 && got[0].ends_with(&format!("xdg-open {clip}")),
        "the clip goes to the desktop: {got:?}"
    );
    assert_eq!(pane.float_path(), None, "and no square is made for it");
    let (text, on) = pane
        .read(|v| v.said.as_ref().map(|s| (s.text.clone(), s.row)))
        .expect("the pane says why");
    assert!(text.contains("no libmpv"), "{text}");
    assert_eq!(on, Some(row), "on the row that was clicked");

    let got = launched_by(&mut pane, at, held(true, true, false, false));
    assert_eq!(got.len(), 1, "a split is refused the same way: {got:?}");
    assert_eq!(*asked.borrow(), Vec::<Asked>::new(), "no pane is asked for");
}

/// An HTML file with nothing to draw it goes to the desktop before any square
/// or pane is made, and the pane says why where it was clicked, in a chip only
/// its own timer takes down. A link to one from a document leaves the document
/// where it is. A square or face whose engine gives up after all hands its
/// file over; a face restored after a restart, which nobody clicked, does not.
#[gpui::test]
fn an_html_file_nothing_can_draw_goes_to_the_desktop_and_says_why(cx: &mut TestAppContext) {
    let dir = Scratch::new("no-engine");
    let brief = dir.fixture("tests/fixtures/page/brief.html", "brief.html");
    let md = dir.fixture("../README.md", "readme.md");
    let (brief, md) = (
        brief.to_str().expect("UTF-8").to_string(),
        md.to_str().expect("UTF-8").to_string(),
    );
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'brief {brief}' 'doc {md}' 'ready'; exec cat"),
    );
    pane.html_engine(Err(crate::docview::engine::Unavailable::Off));
    pane.wait_for("ready");
    let asked = pane.asks_beside();
    let said = |pane: &mut Pane| pane.read(|v| v.said.as_ref().map(|s| (s.text.clone(), s.row)));

    let at = pane.point_at(&brief);
    let row = pane.painted_row(at);
    let got = launched_by(&mut pane, at, Pane::alt());
    assert!(
        got.len() == 1 && got[0].ends_with(&format!("xdg-open {brief}")),
        "the brief goes to the desktop: {got:?}"
    );
    assert_eq!(pane.float_path(), None, "and no square is made for it");
    let (text, on) = said(&mut pane).expect("the pane says why");
    assert!(text.contains("HTML engine off"), "{text}");
    assert_eq!(on, Some(row), "on the row that was clicked");

    // Four seconds on, a split is asked for, and refused the same way.
    pane.cx
        .executor()
        .advance_clock(std::time::Duration::from_secs(4));
    let at = pane.point_at(&brief);
    let got = launched_by(&mut pane, at, held(true, true, false, false));
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(*asked.borrow(), Vec::<Asked>::new(), "no pane is asked for");
    // The first saying's timer runs out and must not take the second down.
    pane.cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(4500));
    assert!(
        said(&mut pane).is_some(),
        "the newer saying outlives the older timer"
    );
    pane.cx
        .executor()
        .advance_clock(std::time::Duration::from_secs(4));
    assert_eq!(said(&mut pane), None, "and goes when its own runs out");

    // A link to the brief from a document in a square.
    let at = pane.point_at(&md);
    pane.click(at, Pane::alt());
    let view = pane.float_view().expect("the document's square");
    let before = desktop_launches().len();
    pane.follow(&view, &brief);
    assert_eq!(desktop_launches()[before..].len(), 1);
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&md)),
        "the document stays"
    );

    // An engine that gives up after the view is up hands the file over, from
    // either seat.
    let give_up = |pane: &mut Pane, view: &gpui::Entity<crate::docview::DocumentView>| {
        let before = desktop_launches().len();
        view.update(pane.cx, |_, cx| {
            cx.emit(crate::docview::CannotShow {
                reason: "the browser would not start".into(),
            })
        });
        pane.cx.run_until_parked();
        desktop_launches()[before..].to_vec()
    };
    let got = give_up(&mut pane, &view);
    assert!(
        got.len() == 1 && got[0].ends_with(&format!("xdg-open {md}")),
        "{got:?}"
    );
    pane.show_document(std::path::Path::new(&md));
    let face = pane.face_view().expect("the Document face");
    let got = give_up(&mut pane, &face);
    assert!(
        got.len() == 1 && got[0].ends_with(&format!("xdg-open {md}")),
        "{got:?}"
    );

    // Restored after a restart: nobody clicked, so nothing opens by itself.
    pane.view
        .update(pane.cx, |v, cx| v.restore_document(&brief, None, cx));
    pane.redraw();
    let restored = pane.face_view().expect("the restored face");
    assert_eq!(give_up(&mut pane, &restored), Vec::<String>::new());
}

// ── the Document face ───────────────────────────────────────────────────────

/// A pane whose terminal has printed `ready`, is past its first moments, and
/// shows a copy of the repository's README on its Document face.
fn pane_on_its_document_face(cx: &mut TestAppContext, tag: &str) -> (Pane, Scratch, String) {
    let dir = Scratch::new(tag);
    let md = dir.fixture("../README.md", "readme.md");
    let md = md.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(cx, "seq 1 200; echo ready; exec cat");
    pane.wait_for("ready");
    pane.settle();
    pane.show_document(std::path::Path::new(&md));
    assert_eq!(pane.face(), Face::Document);
    (pane, dir, md)
}

/// Back on the terminal face, type a word and wait for the shell's echo of
/// it: anything sent to the terminal before it would be on screen first.
fn flip_to_the_terminal_and_type(pane: &mut Pane, word: &str) {
    pane.keys("alt-k");
    assert_eq!(pane.face(), Face::Terminal, "alt+k shows the shell again");
    let keys = word.chars().map(String::from).collect::<Vec<_>>().join(" ");
    pane.keys(&format!("{keys} enter"));
    pane.wait_for(word);
}

/// A key on the Document face moves the document or stops there: nothing
/// typed or pasted there reaches the shell hidden behind the page.
#[gpui::test]
fn the_document_face_swallows_typing_and_pasting(cx: &mut TestAppContext) {
    let (mut pane, _dir, _md) = pane_on_its_document_face(cx, "face-typing");
    pane.keys("x y z enter");
    pane.cx
        .write_to_clipboard(gpui::ClipboardItem::new_string("pasted-behind".into()));
    pane.keys("ctrl-shift-v");
    flip_to_the_terminal_and_type(&mut pane, "echoed");
    let rows = pane.rows();
    assert!(
        !rows
            .iter()
            .any(|r| r.contains("xyz") || r.contains("pasted-behind")),
        "nothing typed or pasted on the document reached the shell:\n{}",
        rows.join("\n")
    );
}

/// Every press on the Document face is the document's: a drag there selects
/// nothing in the grid behind it, a right click opens no copy/paste tray over
/// the hidden shell, and a file dropped there is not typed into it.
#[gpui::test]
fn a_press_or_a_drop_on_the_document_face_never_reaches_the_grid(cx: &mut TestAppContext) {
    let dir = Scratch::new("face-press");
    let png = dir.fixture(PICTURE, "shot.png");
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    pane.settle();
    pane.show_document(&png);
    let (sx, sy, sw, sh) = pane.screen();
    let mid = point(px(sx + sw / 2.0), px(sy + sh / 2.0));

    pane.press(mid);
    assert!(pane.read(|v| v.doc_holding), "the press is the picture's");
    pane.drag_to(point(mid.x + px(80.0), mid.y + px(40.0)));
    pane.release(point(mid.x + px(80.0), mid.y + px(40.0)));
    assert!(!pane.read(|v| v.doc_holding), "and the release lets it go");
    assert!(
        !pane.read(|v| v.has_selection()),
        "the grid selects nothing"
    );

    pane.right_click(mid);
    assert!(
        pane.read(|v| v.ctx_menu.is_none()),
        "no tray over a hidden shell"
    );

    pane.drop_files(mid, vec![png.clone()]);
    flip_to_the_terminal_and_type(&mut pane, "echoed");
    assert!(
        !pane.rows().iter().any(|r| r.contains("shot.png")),
        "a file dropped on the document is not typed into the shell"
    );
}

/// The wheel anywhere on the Document face moves the document and never the
/// hidden scrollback; Ctrl+wheel zooms the document there, as it does over
/// the square, and leaves the pane's text dial alone.
#[gpui::test]
fn the_wheel_on_the_document_face_moves_the_document(cx: &mut TestAppContext) {
    let (mut pane, _dir, _md) = pane_on_its_document_face(cx, "face-wheel");
    let view = pane.face_view().expect("the face's view");
    assert_eq!(pane.scroll_of(&view), Some(0.0), "laid out, at its top");
    let (sx, sy, _, sh) = pane.screen();
    let corner = point(px(sx + 30.0), px(sy + sh - 30.0));

    let size = |pane: &mut Pane| {
        pane.view
            .read_with(pane.cx, |v, cx| v.resolved_theme(cx).font_size)
    };
    let was = size(&mut pane);
    pane.wheel(corner, -1.0, held(false, true, false, false));
    pane.redraw();
    assert_eq!(
        view.read_with(pane.cx, |v, _| v.zoom_now()),
        Some(crate::docview::ImageZoom::Scale(0.9)),
        "ctrl+wheel down zooms the document out a step"
    );
    assert_eq!(size(&mut pane), was, "and leaves the text dial");
    assert_eq!(
        pane.scroll_of(&view),
        Some(0.0),
        "and the document at its top"
    );

    pane.wheel(corner, -3.0, Default::default());
    assert!(
        pane.scroll_of(&view).expect("laid out") > 0.0,
        "the document moves"
    );
    assert_eq!(pane.scrolled_back(), 0, "the hidden scrollback does not");
}

/// The Document face comes only with a document: asked for without one, the
/// pane stays on its terminal; `show_document` puts both up together; and a
/// square's view carried onto the face is that same view, seated on the face,
/// never the file opened a second time.
#[gpui::test]
fn the_document_face_comes_only_with_a_document(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "face-only");
    pane.view
        .update(pane.cx, |v, cx| v.set_face(Face::Document, cx));
    assert_eq!(pane.face(), Face::Terminal, "no document, no Document face");

    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    let square = pane.float_view().expect("a square");
    let target = crate::docopen::drawable_document(std::path::Path::new(&png)).expect("a picture");
    pane.view.update(pane.cx, |v, cx| {
        let carried = v.release_float(cx);
        v.show_document(target, carried, cx)
    });
    pane.redraw();
    assert_eq!(pane.face(), Face::Document);
    let face = pane.face_view().expect("the face's view");
    assert_eq!(
        face.entity_id(),
        square.entity_id(),
        "the carried view, not a new one"
    );
    let (seat, _) = face.read_with(pane.cx, |v, _| v.seat_and_theme());
    assert_eq!(seat, DocSeat::Face, "re-seated on the face");
}

/// A replica repair builds a new pane and hands it the old one's
/// presentation: the document behind its terminal and the square over it
/// cross as the same views — nothing opened again — the square in its place
/// and with its note, and the new pane routes their links itself.
#[gpui::test]
fn a_repaired_replica_keeps_its_document_and_its_square(cx: &mut TestAppContext) {
    let dir = Scratch::new("replica");
    let md = dir.fixture("../README.md", "readme.md");
    let png = dir.fixture(PICTURE, "shot.png");
    let other = dir.fixture(PICTURE, "other.png");
    let png = png.to_str().expect("UTF-8").to_string();
    let mut old = Pane::running(
        cx,
        &format!("printf '%s\\n' 'shot {png}' 'ready'; exec cat"),
    );
    old.wait_for("ready");
    old.show_document(&md);
    old.keys("alt-k");
    assert_eq!(old.face(), Face::Terminal);
    let at = old.point_at(&png);
    old.click(at, Pane::alt());
    old.view.update(old.cx, |v, cx| {
        v.note_float(crate::docopen::FloatNote::FourPanes, cx)
    });
    let (doc, square) = (
        old.face_view().expect("a document"),
        old.float_view().expect("a square"),
    );
    let rect = old.read(|v| v.float.as_ref().map(|f| f.rect));

    let mut new = Pane::running(cx, "echo repaired; exec cat");
    new.wait_for("repaired");
    let carried = old.read(|v| v.presentation());
    new.view
        .update(new.cx, |v, cx| v.adopt_presentation(carried, cx));
    new.redraw();
    assert_eq!(
        new.face_view().map(|v| v.entity_id()),
        Some(doc.entity_id())
    );
    assert_eq!(
        new.float_view().map(|v| v.entity_id()),
        Some(square.entity_id())
    );
    assert_eq!(
        new.face(),
        Face::Terminal,
        "the document was behind the terminal"
    );
    assert_eq!(new.read(|v| v.float.as_ref().map(|f| f.rect)), rect);
    assert_eq!(
        new.read(|v| v.float.as_ref().and_then(|f| f.note)),
        Some(crate::docopen::FloatNote::FourPanes)
    );
    new.follow(&square, other.to_str().expect("UTF-8"));
    assert_eq!(
        new.float_path(),
        Some(other),
        "the new pane routes the square's links"
    );
}

/// A saved document comes back by its name alone: a file that has gone keeps
/// its pane and its face, and its place in the page, never measured, is
/// carried as unknown rather than as the top. A name TD does not draw leaves
/// the pane a terminal.
#[gpui::test]
fn a_missing_document_keeps_its_pane_on_restore(cx: &mut TestAppContext) {
    let dir = Scratch::new("restore");
    let gone = dir.join("gone.md");
    let gone = gone.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");

    pane.view
        .update(pane.cx, |v, cx| v.restore_document(&gone, Some(0.4), cx));
    pane.redraw();
    assert_eq!(pane.face(), Face::Document, "the pane keeps its face");
    assert_eq!(
        pane.view.read_with(pane.cx, |v, cx| v.saved_document(cx)),
        Some((gone.clone(), None)),
        "an unmeasured place is saved as unknown, not as the top"
    );

    let mut other = Pane::running(cx, "echo ready; exec cat");
    other.wait_for("ready");
    let odd = dir.join("notes.xyz");
    other.view.update(other.cx, |v, cx| {
        v.restore_document(odd.to_str().expect("UTF-8"), None, cx)
    });
    assert_eq!(
        other.face(),
        Face::Terminal,
        "a name TD does not draw stays a terminal"
    );
    assert!(!other.read(|v| v.has_document()));
}

// ── the terminal ────────────────────────────────────────────────────────────

/// A file dropped on the terminal face goes to the shell as a paste of its
/// path, as every other terminal on the machine does with one.
#[gpui::test]
fn a_file_dropped_on_the_terminal_is_pasted_into_it(cx: &mut TestAppContext) {
    let dir = Scratch::new("drop");
    let png = dir.fixture(PICTURE, "dropped.png");
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    let at = pane.point_at("ready");
    pane.drop_files(at, vec![png]);
    pane.wait_for("dropped.png");
}

/// The first reply the terminal typed back that starts with `start`, from
/// where the line discipline echoed it onto the screen.
fn echoed_reply(pane: &mut Pane, start: &str) -> String {
    pane.wait_for(start);
    let row = pane
        .rows()
        .into_iter()
        .find(|r| r.contains(start))
        .expect("the reply");
    row[row.find(start).expect("the reply")..].to_string()
}

/// A program asking a window-owned pane how big its text area is (`CSI 14 t`)
/// is answered, from what the pseudoterminal was last told, and in device
/// pixels: the cell at the window's scale, not a logical size cut to an
/// integer.
#[gpui::test]
fn a_program_asking_the_text_area_size_is_told_it_in_device_pixels(cx: &mut TestAppContext) {
    let mut pane = Pane::running(cx, "echo ready; read go; printf '\\033[14t'; exec cat");
    pane.wait_for("ready");
    // Let the first layout's size reach the pseudoterminal: the pane waits for
    // it to stop changing, on both clocks.
    std::thread::sleep(std::time::Duration::from_millis(200));
    pane.cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(300));
    pane.redraw();
    let (rows, cols, told, cell) =
        pane.read(|v| (v.grid.rows, v.grid.cols, v.cell_px, (v.cell_w, v.cell_h)));
    let scale = pane.scale();
    assert_eq!(
        told,
        (
            (cell.0 * scale).round() as u16,
            (cell.1 * scale).round() as u16
        ),
        "the pseudoterminal was told its cell in device pixels"
    );
    // And the kernel was told the same: what `TIOCGWINSZ` answers any program.
    let kernel = pane.kernel_winsize_once(|ws| ws.ws_xpixel == cols as u16 * told.0);
    assert_eq!(
        (
            kernel.ws_row,
            kernel.ws_col,
            kernel.ws_xpixel,
            kernel.ws_ypixel
        ),
        (
            rows as u16,
            cols as u16,
            cols as u16 * told.0,
            rows as u16 * told.1
        ),
        "the pseudoterminal's own size, in device pixels"
    );
    pane.settle();
    pane.keys("g o enter");
    let reply = echoed_reply(&mut pane, "[4;");
    let numbers: Vec<u32> = reply["[4;".len()..]
        .split(['t', ';'])
        .take(2)
        .map(|n| n.parse().expect("a number"))
        .collect();
    assert_eq!(
        numbers,
        vec![rows as u32 * told.1 as u32, cols as u32 * told.0 as u32],
        "height and width in device pixels: {reply}"
    );
}

/// A program asking the pane its background colour (`OSC 11 ?`) is told the
/// colour the pane draws, from the pane's own theme when it wears one, not
/// the window's.
#[gpui::test]
fn a_program_asking_a_colour_is_told_the_one_the_pane_draws(cx: &mut TestAppContext) {
    let mut pane = Pane::running(
        cx,
        "echo ready; read go; printf '\\033]11;?\\007'; exec cat",
    );
    pane.wait_for("ready");
    pane.wear_own_theme();
    let (drawn, windows) = pane.view.read_with(pane.cx, |v, cx| {
        let own = v.resolved_theme(cx);
        let window = crate::theme::theme(cx);
        (
            super::rgb8(super::graded(own.bg, &own.grade, super::Channel::Bg)),
            super::rgb8(super::graded(window.bg, &window.grade, super::Channel::Bg)),
        )
    });
    assert_ne!(
        (drawn.r, drawn.g, drawn.b),
        (windows.r, windows.g, windows.b),
        "the pane's own background differs from the window's, so the answer can tell"
    );
    pane.settle();
    pane.keys("g o enter");
    let reply = echoed_reply(&mut pane, "11;rgb:");
    let hex: Vec<u8> = reply["11;rgb:".len()..]
        .split('/')
        .take(3)
        .map(|c| u8::from_str_radix(&c[..2], 16).expect("hex"))
        .collect();
    assert_eq!(hex, vec![drawn.r, drawn.g, drawn.b], "{reply}");
}

/// The document in a square paints in the pane's own theme, not the window's.
#[gpui::test]
fn the_square_paints_in_the_panes_own_theme(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "square-theme");
    pane.wear_own_theme();
    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    pane.redraw();
    let square = pane.float_view().expect("a square");
    let (_, theme) = square.read_with(pane.cx, |v, _| v.seat_and_theme());
    let theme = theme.expect("the pane handed the square a theme");
    let (own, window) = pane.view.read_with(pane.cx, |v, cx| {
        (v.resolved_theme(cx), crate::theme::theme(cx))
    });
    assert_ne!(own.bg, window.bg, "the pane wears its own");
    assert_eq!(theme.bg, own.bg, "and the square paints in it");
}

/// A Markdown file takes notes under the pointer, as a brief does: the bar
/// is on screen from the start, at 0 notes, so the file says it takes them;
/// over a block, its 💬 shows in the margin on the column's right; and a
/// press there opens the note box on that block. Opening it writes nothing.
///
/// Parker, on the first build, which hid the bar until a note existed: *"not
/// seeing it in this MD"*.
#[gpui::test]
fn a_markdown_block_opens_its_note_box_from_the_button_under_the_pointer(cx: &mut TestAppContext) {
    use crate::docview::markdown::{NOTE_GUTTER, PAD};
    use crate::docview::notes_ui::BUTTON_CSS;
    let dir = Scratch::new("md-note-button");
    let md = dir.join("plan.md");
    std::fs::write(
        &md,
        "A first paragraph that takes a note.\n\nA second one.\n",
    )
    .expect("the document");
    let md = md.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'doc {md}' 'ready'; exec cat"));
    pane.wait_for("ready");
    let at = pane.point_at(&md);
    pane.click(at, Pane::alt());
    pane.redraw();
    let report = pane.doc_notes().expect("the Markdown file shows its notes");
    assert_eq!(report["kept"], "store", "{report}");
    assert_eq!(report["open"], serde_json::Value::Null);
    assert_eq!(
        report["bar"], true,
        "the bar is drawn with nothing written: {report}"
    );

    let (x, y, w, _) = pane.float_zone(FloatHit::Body).expect("the square's body");
    assert_eq!(report["buttons"], 0, "no 💬 before the pointer comes");
    pane.hover(point(px(x + 60.), px(y + PAD + 6.)));
    pane.redraw();
    assert_eq!(
        pane.doc_notes().expect("notes")["buttons"],
        1,
        "the 💬 of the block under the pointer is DRAWN, not only pressable"
    );
    let button = point(
        px(x + w - PAD - NOTE_GUTTER / 2.0),
        px(y + PAD + BUTTON_CSS / 2.0),
    );
    pane.click(button, Default::default());
    pane.redraw();
    let report = pane.doc_notes().expect("notes");
    assert_eq!(
        report["open"], "p-a-first-paragraph-that",
        "the box opens on the block under the pointer: {report}"
    );
    assert_eq!(report["notes"], 0, "and nothing was written");
}

/// A note being written wraps at the width of its box. Forty words typed on
/// one line come out as several lines of the box's width, so the draft grows
/// taller than the four and a half lines it opens at and stays as wide as
/// the box. It used to be drawn as the text before the caret and the text
/// after it side by side, each measured at its full length, so the whole
/// note ran on as one line past the box's edge.
#[gpui::test]
fn a_note_being_written_wraps_at_the_width_of_its_box(cx: &mut TestAppContext) {
    use crate::docview::markdown::{NOTE_GUTTER, PAD};
    use crate::docview::notes_ui::BUTTON_CSS;
    let dir = Scratch::new("md-note-wraps");
    let md = dir.join("plan.md");
    std::fs::write(&md, "A paragraph that takes a note.\n").expect("the document");
    let md = md.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'doc {md}' 'ready'; exec cat"));
    pane.wait_for("ready");
    let at = pane.point_at(&md);
    pane.click(at, Pane::alt());
    pane.redraw();
    let (x, y, w, _) = pane.float_zone(FloatHit::Body).expect("the square's body");
    pane.hover(point(px(x + 60.), px(y + PAD + 6.)));
    pane.redraw();
    pane.click(
        point(
            px(x + w - PAD - NOTE_GUTTER / 2.0),
            px(y + PAD + BUTTON_CSS / 2.0),
        ),
        Default::default(),
    );
    pane.redraw();
    let size = |pane: &mut Pane| -> (f64, f64) {
        let report = pane.doc_notes().expect("notes");
        let d = &report["draft"];
        (
            d[0].as_f64().expect("a drawn draft"),
            d[1].as_f64().expect("a drawn draft"),
        )
    };
    let (empty_w, empty_h) = size(&mut pane);

    let word = "w r a p p i n g space";
    pane.keys(&vec![word; 40].join(" "));
    pane.redraw();
    let (typed_w, typed_h) = size(&mut pane);
    assert_eq!(typed_w, empty_w, "the draft stays as wide as its box");
    assert!(
        typed_h > empty_h * 1.5,
        "forty words wrap onto more lines than the box opens with: {empty_h} → {typed_h}"
    );
}

/// A program draws a picture with the Kitty graphics protocol — here two
/// pixels, red and green, over four cells by two — and the pane builds one
/// texture for it and lays it over the grid. Then the pane's tab is hidden,
/// which is the one thing that makes a picture go: the texture is dropped and
/// the core forgets the image, so coming back shows only the text.
///
/// rio-vt only: alacritty drops the picture in its parser, so on the fallback
/// there is nothing to show.
#[cfg(not(feature = "core-alacritty"))]
#[gpui::test]
fn a_picture_a_program_draws_is_shown_and_then_forgotten_when_hidden(cx: &mut TestAppContext) {
    let mut pane = Pane::running(
        cx,
        "printf '\\033_Gi=5,s=2,v=1,a=T,t=d,f=24,c=4,r=2;/wAAAP8A\\033\\\\'; echo; echo ready; exec cat",
    );
    pane.wait_for("ready");
    pane.redraw();
    assert_eq!(
        pane.read(|v| v.session.term.lock().pictures().len()),
        1,
        "the core holds the picture"
    );
    assert_eq!(
        pane.read(|v| v.pictures.len()),
        1,
        "one texture for one picture on the screen"
    );

    let view = pane.view.clone();
    view.update(pane.cx, |v, cx| v.forget_pictures(cx));
    pane.redraw();
    assert_eq!(pane.read(|v| v.pictures.len()), 0, "the texture is gone");
    assert!(
        pane.read(|v| v.session.term.lock().pictures().is_empty()),
        "and so is the image, so a redraw cannot bring it back"
    );
    assert!(
        pane.rows().iter().any(|r| r.contains("ready")),
        "the text is untouched"
    );
}

/// Alt held over a document outlines every element that takes a note, and a
/// press anywhere inside one opens its note box; let go of Alt and the same
/// press opens nothing. Parker, 2026-09-25: *"I expect that If I hold alt
/// hovering over the md file display overlay --- that it will show BOXES
/// where I can click to add comments to each element"*.
///
/// Mutation-tested: dropping the pane's modifiers listener, and drawing no
/// box while Alt is held, each fail this.
#[gpui::test]
fn alt_over_a_markdown_file_outlines_every_block_and_a_press_in_one_opens_it(
    cx: &mut TestAppContext,
) {
    use crate::docview::markdown::PAD;
    let dir = Scratch::new("md-alt-boxes");
    let md = dir.join("plan.md");
    std::fs::write(
        &md,
        "A first paragraph that takes a note.\n\nA second one.\n",
    )
    .expect("the document");
    let md = md.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'doc {md}' 'ready'; exec cat"));
    pane.wait_for("ready");
    let at = pane.point_at(&md);
    pane.click(at, Pane::alt());
    pane.redraw();
    let (x, y, _, _) = pane.float_zone(FloatHit::Body).expect("the square's body");
    // Over the first paragraph's words, well left of its 💬.
    let words = point(px(x + 40.), px(y + PAD + 6.));
    pane.hover(words);
    pane.redraw();
    assert_eq!(
        pane.doc_notes().expect("notes")["boxes"],
        0,
        "no boxes without Alt"
    );
    pane.click(words, Default::default());
    pane.redraw();
    assert_eq!(
        pane.doc_notes().expect("notes")["open"],
        serde_json::Value::Null,
        "without Alt, a press on the words opens nothing"
    );

    // Alt goes down with the pointer still: the boxes are drawn at once.
    pane.modifiers(Pane::alt());
    pane.redraw();
    let report = pane.doc_notes().expect("notes");
    assert_eq!(report["boxes"], 2, "both paragraphs outlined: {report}");
    pane.click(words, Pane::alt());
    pane.redraw();
    assert_eq!(
        pane.doc_notes().expect("notes")["open"],
        "p-a-first-paragraph-that",
        "Alt+click on the words opens that paragraph's note box"
    );
    pane.keys("escape");
    pane.modifiers(Default::default());
    pane.redraw();
    assert_eq!(
        pane.doc_notes().expect("notes")["boxes"],
        0,
        "let go, and they go"
    );
}

/// Alt over a brief outlines its anchors the same way, since a brief and a
/// Markdown file share one notes layer; let go and they go.
#[gpui::test]
fn alt_over_a_brief_outlines_its_anchors(cx: &mut TestAppContext) {
    let (mut pane, _dir, brief, _png) = pane_showing_a_brief(cx, "brief-alt-boxes");
    let at = pane.point_at(&brief);
    pane.click(at, Pane::alt());
    pane.redraw();
    let body = Pane::middle(pane.float_zone(FloatHit::Body).expect("the square's body"));
    pane.hover(body);
    pane.redraw();
    assert_eq!(pane.doc_notes().expect("notes")["boxes"], 0);
    pane.modifiers(Pane::alt());
    pane.redraw();
    let boxes = pane.doc_notes().expect("notes")["boxes"]
        .as_u64()
        .unwrap_or(0);
    assert!(boxes >= 1, "every anchor in view is outlined: {boxes}");
    pane.modifiers(Default::default());
    pane.redraw();
    assert_eq!(pane.doc_notes().expect("notes")["boxes"], 0);
}

/// Ctrl+shift+enter over a brief is ↪ from the keyboard. With an agent beside
/// the square it raises the very event a press on the bar's button raises,
/// carrying the map with the note not yet saved; with nobody beside it the
/// chord is not the square's, nothing is sent and the square stays up.
#[gpui::test]
fn ctrl_shift_enter_over_a_brief_sends_its_notes_as_the_button_does(cx: &mut TestAppContext) {
    let (mut pane, _dir, brief, _png) = pane_showing_a_brief(cx, "send-chord");
    open_the_brief_with_an_unsaved_note(&mut pane, &brief);
    let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let log = sent.clone();
    let view = pane.view.clone();
    pane.cx.update(|_, cx| {
        cx.subscribe(&view, move |_, ev: &super::SendNotesBeside, _| {
            log.borrow_mut().push(ev.map.clone());
        })
        .detach();
    });

    pane.keys("ctrl-shift-enter");
    pane.redraw();
    assert!(sent.borrow().is_empty(), "nobody beside it: nothing sent");
    assert!(pane.float_path().is_some(), "and the square stays up");

    // What the workspace works out every frame from the tab, said here.
    pane.view.update(pane.cx, |v, cx| {
        v.set_notes_beside(Some("agent".into()), None, cx)
    });
    pane.redraw();
    let square = pane.float_view().expect("the square");
    assert!(
        square.read_with(pane.cx, |v, _| v.sends()),
        "the bar draws ↪ once somebody is beside it: {}",
        pane.doc_notes().map(|r| r.to_string()).unwrap_or_default()
    );
    pane.keys("ctrl-shift-enter");
    pane.redraw();
    let sent = sent.borrow();
    assert_eq!(sent.len(), 1, "one send: {sent:?}");
    assert!(
        sent[0].contains("Keep the tiles."),
        "the map carries the unsaved note: {}",
        sent[0]
    );
    assert!(pane.float_path().is_some(), "the square stays up after it");
}

/// ↪ on a brief beside an agent that is showing its BENCH puts the notes map
/// in the bench's composer, whole, and writes nothing to the terminal; the
/// same press on the terminal face pastes it into the prompt, unsent. A brief
/// opened from a bench card floats over the bench, so the first is the
/// Workbench's own loop — it used to be refused with "turn it to its prompt
/// first".
#[gpui::test]
fn notes_sent_to_an_agent_on_its_bench_land_in_the_composer(cx: &mut TestAppContext) {
    // `cat` echoes whatever reaches the terminal, and bracketed paste is
    // turned on first, as an agent does, so a paste would have somewhere to go.
    let mut pane = Pane::running(cx, "printf '\\033[?2004hready\\n'; exec cat");
    pane.wait_for("ready");
    let map = "NOTES — brief.html\n\n[fig-01-what-a-miss] 01 · What a miss costs\n  · Draw the cold start too.";
    pane.view.update(pane.cx, |v, cx| {
        v.mode = super::PaneMode::Claude;
        v.set_face(Face::Workbench, cx);
    });
    pane.redraw();

    let landed = pane.view.update(pane.cx, |v, cx| v.send_notes(map, cx));
    assert_eq!(
        landed,
        Ok(super::NotesLanded::Composer(map.chars().count())),
        "on the bench the map goes to the composer"
    );
    let draft = pane.read(|v| v.wb_compose.as_ref().map(|l| l.text().to_string()));
    assert_eq!(draft.as_deref(), Some(map), "whole, as a draft");
    std::thread::sleep(std::time::Duration::from_millis(150));
    pane.redraw();
    assert!(
        !pane.rows().iter().any(|r| r.contains("cold start")),
        "and nothing of it reached the terminal: {:?}",
        pane.rows()
    );

    pane.view.update(pane.cx, |v, cx| {
        v.wb_compose = None;
        v.set_face(Face::Terminal, cx);
    });
    let landed = pane.view.update(pane.cx, |v, cx| v.send_notes(map, cx));
    assert_eq!(landed, Ok(super::NotesLanded::Prompt(map.chars().count())));
    pane.wait_for("cold start");
    assert!(
        pane.read(|v| v.wb_compose.is_none()),
        "the terminal face pastes, and leaves the composer alone"
    );
}
