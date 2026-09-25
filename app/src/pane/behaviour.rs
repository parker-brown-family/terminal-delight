//! What a pane does with a person's hands, held by driving a real one.
//!
//! Every test here builds a [`TerminalView`](super::TerminalView) through the
//! harness ([`super::harness`]), puts something on its screen the way a
//! program would — by printing it — and then presses, moves, releases and
//! types at it through gpui, the way the window does. What is asserted is
//! what a person would see or get: a square open or gone, where it sits, what
//! landed on the clipboard, what went to the desktop.

use gpui::{point, px, TestAppContext};

use super::harness::{desktop_launches, FakeBriefEngine, Pane, Rect, Scratch, BRIEF_ANCHORS};
use crate::docopen::FloatHit;

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

/// Alt+click on a picture's path opens it in a floating square beside the
/// line, and Escape closes it. Nothing goes to the desktop.
#[gpui::test]
fn alt_click_on_a_printed_picture_opens_a_square_and_escape_closes_it(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "alt-open");
    assert_eq!(pane.float_path(), None);

    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&png)),
        "an Alt+click on the path opens that picture in a square"
    );
    let (_, y, _, h) = pane.float_rect().expect("the square is drawn");
    let (line_top, line_bottom) = pane.row_span(at);
    assert!(
        y >= line_bottom || y + h <= line_top,
        "the square opens beside the line it was opened from, not over it: \
         square {y}..{} against line {line_top}..{line_bottom}",
        y + h
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
