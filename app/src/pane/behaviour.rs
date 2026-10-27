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
    desktop_launches, Asked, FakeBriefEngine, FakeFilmEngine, Pane, Rect, Scratch, BRIEF_ANCHORS,
};
use crate::docopen::{Asker, DocSeat, FloatHit};
use crate::workbench::Face;

#[gpui::test]
fn queued_input_waits_while_the_pane_is_off_limits(cx: &mut TestAppContext) {
    use std::io::Read;
    let mut attached = Pane::attached(cx);
    attached.pane.running_as(super::PaneMode::Codex);
    attached.pane.view.update(attached.pane.cx, |v, cx| {
        v.set_on_screen(false, cx);
        v.bench_deliver(b"queued-canary".to_vec(), cx);
        assert_eq!(v.bench_queued(), 1);
        v.agent_access = crate::threshing_floor::AgentAccess::OffLimits;
        v.set_on_screen(true, cx);
        assert_eq!(
            v.bench_queued(),
            1,
            "showing a hidden pane must not drain the instruction"
        );
    });
    attached
        .host
        .set_read_timeout(Some(std::time::Duration::from_millis(30)))
        .unwrap();
    let mut bytes = [0; 128];
    assert!(
        attached.host.read(&mut bytes).is_err(),
        "queued input reached the host"
    );
    attached.pane.view.update(attached.pane.cx, |v, cx| {
        v.agent_access = crate::threshing_floor::AgentAccess::Open;
        v.bench_drain(cx);
        assert_eq!(v.bench_queued(), 0);
    });
    attached
        .host
        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
        .unwrap();
    let mut received = [0; 13];
    attached.host.read_exact(&mut received).unwrap();
    assert_eq!(&received, b"queued-canary");
}

#[gpui::test]
fn synthetic_bench_pointer_refuses_off_limits_but_reaches_an_allowed_pane(cx: &mut TestAppContext) {
    let pane = Pane::running(cx, "exec cat");
    let view = pane.view.clone();
    pane.view.update(pane.cx, |v, _| {
        v.agent_access = crate::threshing_floor::AgentAccess::OffLimits
    });
    for click in [false, true] {
        let request = crate::ctl::PointerReq {
            click,
            x: 20.0,
            y: 30.0,
            alt: false,
            control: false,
        };
        let said = pane
            .cx
            .update(|window, cx| view.update(cx, |v, cx| v.synthetic_pointer(request, window, cx)));
        assert!(
            said.starts_with("err") && said.contains("off-limits"),
            "{said}"
        );
    }
    pane.view.update(pane.cx, |v, _| {
        v.agent_access = crate::threshing_floor::AgentAccess::Open
    });
    let said = pane.cx.update(|window, cx| {
        view.update(cx, |v, cx| {
            v.synthetic_pointer(
                crate::ctl::PointerReq {
                    click: false,
                    x: 20.0,
                    y: 30.0,
                    alt: false,
                    control: false,
                },
                window,
                cx,
            )
        })
    });
    assert!(!said.starts_with("err"), "allowed positive control: {said}");
}

/// Observe bytes at the host socket, after real GPUI key dispatch. A final
/// letter fences the claimed chords: if they leaked, it would not arrive first.
#[gpui::test]
fn human_turn_chords_keep_shell_bytes_and_never_leak_from_agents(cx: &mut TestAppContext) {
    use std::io::Read;
    for mode in [
        super::PaneMode::Shell,
        super::PaneMode::Claude,
        super::PaneMode::Codex,
    ] {
        let mut attached = Pane::attached(cx);
        attached
            .host
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        attached.pane.running_as(mode.clone());
        // The startup input guard is 150 ms of wall time.
        attached.pane.view.update(attached.pane.cx, |v, _| {
            v.spawned = std::time::Instant::now() - std::time::Duration::from_secs(1);
        });
        attached
            .pane
            .keys("alt-up alt-down ctrl-up ctrl-down ctrl-left ctrl-right x");
        let expected: &[u8] = if mode.is_agent() {
            b"\x1b[1;5D\x1b[1;5Cx"
        } else {
            b"\x1b[1;5A\x1b[1;5B\x1b[1;5D\x1b[1;5Cx"
        };
        let mut got = vec![0; expected.len()];
        attached.host.read_exact(&mut got).unwrap();
        assert_eq!(got, expected, "{mode:?}");
    }
}

/// Both CLI carets must lead to the human's turns, skipping replies and the
/// empty composer. The window's key dispatch is held in main.rs beside its
/// other chord wiring tests.
#[gpui::test]
fn human_prompt_navigation_walks_claude_and_codex_turns(cx: &mut TestAppContext) {
    for (mode, caret) in [
        (super::PaneMode::Claude, "❯"),
        (super::PaneMode::Codex, "›"),
    ] {
        let mut pane = Pane::running(cx, &format!(
            "for n in 1 2 3; do printf '%s\\n' '{caret} human turn' 'agent reply'; for i in $(seq 1 40); do echo output; done; done; printf '%s\\n' '{caret} ' 'ready'; exec cat"
        ));
        pane.wait_for("ready");
        pane.running_as(mode);
        let turns = pane.read(|v| v.human_line_indices());
        assert_eq!(turns.len(), 3, "{caret}: only submitted human turns");
        assert!(turns.iter().all(|line| *line < 0));
        let mut jump = |next, expected| {
            pane.view
                .update(pane.cx, |v, cx| v.scroll_to_human(next, cx));
            assert_eq!(
                pane.read(|v| v.session.term.lock().display_offset()),
                expected
            );
        };
        jump(false, -turns[2] as usize);
        jump(false, -turns[1] as usize);
        jump(false, -turns[0] as usize);
        jump(false, -turns[0] as usize); // oldest holds
        jump(true, -turns[1] as usize);
        jump(true, -turns[2] as usize);
        jump(true, 0); // beyond newest returns to live
    }
}

/// The full-screen CLI's scroll walk stops on a submitted turn, never on
/// its blank input box, for both current prompt glyphs.
#[gpui::test]
fn human_prompt_navigation_recognises_alternate_screen_turns(cx: &mut TestAppContext) {
    for caret in ["❯", "›"] {
        for text in ["", "human turn"] {
            let mut pane = Pane::running(
                cx,
                &format!("printf '\\033[?1049h\\033[H{caret} {text}\\033[5;1Hready'; exec cat"),
            );
            pane.wait_for("ready");
            assert!(pane.read(|v| v
                .session
                .term
                .lock()
                .mode()
                .contains(crate::vt::TermMode::ALT_SCREEN)));
            for coarse in [false, true] {
                assert_eq!(
                    pane.read(|v| v.top_is_human(coarse)),
                    !text.is_empty(),
                    "{caret} {text}"
                );
            }
        }
    }
}

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

/// Ctrl+W puts the square away and leaves the tab alone; with no square up it
/// is the shell's werase again. Closing the tab is Ctrl+Shift+W. Parker,
/// 2026-10-21: *"Alt+w closes the ENTIRE pane, ctrl+w closes the tab ...
/// Ctrl-W closes one glow pane at a time."*
#[gpui::test]
fn ctrl_w_closes_the_square_not_the_tab(cx: &mut TestAppContext) {
    let (mut pane, _dir, png) = pane_showing_a_picture(cx, "ctrl-w");
    pane.settle();
    let tab_closes = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = tab_closes.clone();
    let view = pane.view.clone();
    pane.cx.update(|_, cx| {
        cx.subscribe(&view, move |_, _: &super::RequestCloseTab, _| {
            seen.set(seen.get() + 1)
        })
        .detach();
    });

    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    assert!(pane.float_path().is_some(), "the square is up");
    pane.keys("ctrl-w");
    assert_eq!(pane.float_path(), None, "Ctrl+W closes the square");
    assert_eq!(tab_closes.get(), 0, "and not the tab");

    // No square: the chord is ^W, and the word before it goes.
    pane.keys("z z z q ctrl-w d o n e enter");
    pane.wait_for_row("done");
    assert!(
        !pane.rows().iter().any(|r| r.contains("zzzqdone")),
        "^W reached the shell and erased the word: {:?}",
        pane.rows()
    );
    assert_eq!(tab_closes.get(), 0);

    pane.keys("ctrl-shift-w");
    assert_eq!(tab_closes.get(), 1, "Ctrl+Shift+W asks to close the tab");
}

/// Codex's file reference shape: a parenthesized path split across indented
/// rows, with room left at the right edge. Both rows must hit the same file.
#[gpui::test]
fn codex_wrapped_file_reference_answers_alt_and_ctrl_click(cx: &mut TestAppContext) {
    let dir = Scratch::new("codex-link");
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().unwrap().to_string();
    let split = png.rfind('/').unwrap() + 1;
    let (head, tail) = png.split_at(split);
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' '  Deliverable: Picture ({head}' '  {tail})' 'ready'; exec cat"),
    );
    pane.wait_for("ready");
    let first = pane.point_at(head);
    let second = pane.point_at(tail);
    for at in [first, second] {
        pane.click(at, Pane::alt());
        assert_eq!(
            pane.float_path().as_deref(),
            Some(std::path::Path::new(&png))
        );
        pane.keys("escape");
        let got = launched_by(&mut pane, at, held(false, true, false, false));
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with(&format!("xdg-open {png}")), "{got:?}");
    }
}

/// Codex prints bare relative references, including before-edge line breaks.
/// Resolve them in the child pane's cwd, not the window or test runner's cwd.
#[gpui::test]
fn codex_relative_html_reference_opens_the_panes_file(cx: &mut TestAppContext) {
    let dir = Scratch::new("codex-relative-html");
    std::fs::create_dir_all(dir.join("reports")).unwrap();
    let relative = "reports/2026-10-14-omarchy-two-leads.html";
    let html = dir.join(relative);
    std::fs::write(
        &html,
        "<!doctype html><html><body>Research brief</body></html>",
    )
    .unwrap();
    let mut pane = Pane::running(
        cx,
        &format!(
            "cd '{}' && printf '%s\\n' '  Deliverable: Commentable research brief ({relative})' '  Wrapped brief (reports/' '  2026-10-14-omarchy-two-leads.html)' '  Missing (reports/missing.html)' 'ready'; exec cat",
            dir.join("").display()
        ),
    );
    pane.html_engine(Ok(std::sync::Arc::new(FakeBriefEngine)));
    pane.wait_for("ready");
    let first = pane.point_at(relative);
    let head = pane.points_at_all("reports/")[1];
    let tail = pane.points_at_all("2026-10-14-omarchy-two-leads.html)")[1];
    for at in [first, head, tail] {
        pane.click(at, Pane::alt());
        assert_eq!(pane.float_path().as_deref(), Some(html.as_path()));
        pane.keys("escape");
    }
    let missing = pane.point_at("reports/missing.html");
    pane.click(missing, Pane::alt());
    assert_eq!(pane.float_path(), None, "a missing path opens nothing");
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// A shell command that prints `name` with `uri` behind it, the way eza and
/// `ls --hyperlink` do: OSC 8 and the address, ST, the name as shown, then the
/// link closed. The address goes in as an argument, so a `%20` in it is never
/// read as a printf directive.
fn hyperlinked(uri: &str, name: &str) -> String {
    format!(r#"printf '\033]8;;%s\033\\%s\033]8;;\033\\\n' "{uri}" "{name}""#)
}

/// A folder holding a `README.md` of its own and a `docs/` with another: the
/// shape that makes a bare name from a listing ambiguous.
fn a_folder_with_two_readmes(tag: &str) -> Scratch {
    let dir = Scratch::new(tag);
    std::fs::create_dir_all(dir.join("docs")).expect("docs/");
    std::fs::write(dir.join("README.md"), "# The folder's own\n").expect("README.md");
    std::fs::write(dir.join("docs/README.md"), "# The one listed\n").expect("docs/README.md");
    dir
}

/// A shell script that runs `then` from inside `dir`, as a person's shell sits
/// in a folder, then says it is ready.
fn in_folder(dir: &Scratch, then: &str) -> String {
    format!(
        "cd '{}' && {then}; echo ready; exec cat",
        dir.join("").display()
    )
}

/// GNU `ls --hyperlink` puts each name's absolute path behind it, naming this
/// machine (`file://<hostname>/…`). An Alt+click on a name it listed opens the
/// file that was listed, even with a file of the same name in the pane's own
/// folder.
///
/// Parker, 2026-10-15: *"We would love to get an LS from the raw terminal to
/// be alt clickable and open a glow pane in Terminal Delight."* Asked what
/// that pane should be, he chose Terminal Delight's own view over the glow
/// program.
#[gpui::test]
fn alt_click_on_a_name_ls_hyperlinked_opens_the_file_it_listed(cx: &mut TestAppContext) {
    let dir = a_folder_with_two_readmes("ls-hyperlink");
    let mut pane = Pane::running(cx, &in_folder(&dir, "ls --hyperlink=always docs"));
    pane.wait_for("ready");
    let at = pane.point_at("README.md");
    pane.click(at, Pane::alt());
    let listed = std::fs::canonicalize(dir.join("docs/README.md")).expect("docs/README.md");
    assert_eq!(
        pane.float_path().as_deref(),
        Some(listed.as_path()),
        "the listed file opens, never the folder's own README.md"
    );
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// eza quotes a name with a space and puts the path behind it with no machine
/// named (`file:///…/my%20notes.md`). Either word of the name opens the one
/// file, and a name the pane's edge breaks over two rows opens from either
/// row.
#[gpui::test]
fn alt_click_on_a_name_eza_hyperlinked_opens_it_whole(cx: &mut TestAppContext) {
    let dir = Scratch::new("eza-hyperlink");
    let spaced = dir.join("my notes.md");
    let long = dir.join("a-name-long-enough-to-wrap.md");
    for file in [&spaced, &long] {
        std::fs::write(file, "# A note\n").expect("a note");
    }
    let uri = |p: &std::path::Path| super::path_to_file_uri(p.to_str().expect("UTF-8"));
    // Eighty columns of a hundred are taken before the long name starts, so
    // its last nine characters wrap onto the next row.
    let listing = format!(
        "{} && printf '%80s' '' && {}",
        hyperlinked(&uri(&spaced), "'my notes.md'"),
        hyperlinked(&uri(&long), "a-name-long-enough-to-wrap.md"),
    );
    let mut pane = Pane::running(cx, &in_folder(&dir, &listing));
    pane.wait_for("ready");
    for (word, file) in [
        ("'my", &spaced),
        ("notes.md'", &spaced),
        ("a-name-long", &long),
        ("o-wrap.md", &long),
    ] {
        let at = pane.point_at(word);
        pane.click(at, Pane::alt());
        assert_eq!(pane.float_path().as_deref(), Some(file.as_path()), "{word}");
        pane.keys("escape");
    }
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// A name printed with no hyperlink, as plain `ls` and an agent's tool output
/// print them, is tried in the pane's own folder. There it opens, and a name
/// the folder does not hold opens nothing.
#[gpui::test]
fn alt_click_on_a_bare_name_opens_it_from_the_panes_folder(cx: &mut TestAppContext) {
    let dir = a_folder_with_two_readmes("ls-bare");
    let mut pane = Pane::running(
        cx,
        &in_folder(&dir, "printf '%s\\n' 'docs  README.md  missing.md'"),
    );
    pane.wait_for("ready");
    let at = pane.point_at("README.md");
    pane.click(at, Pane::alt());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(dir.join("README.md").as_path())
    );
    pane.keys("escape");
    let at = pane.point_at("missing.md");
    pane.click(at, Pane::alt());
    assert_eq!(
        pane.float_path(),
        None,
        "a name the folder does not hold opens nothing"
    );
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// A hyperlink the click does not follow is never swapped for a guess from the
/// name it sits on. A link to a file that has gone, a web address behind a
/// file's name, and another machine's file each sit on `README.md`, in a
/// folder whose own `README.md` is right there. None of them opens it, by Alt
/// or by Ctrl.
#[gpui::test]
fn a_hyperlinked_name_is_never_replaced_by_a_guess(cx: &mut TestAppContext) {
    let dir = a_folder_with_two_readmes("ls-no-guess");
    let gone = super::path_to_file_uri(dir.join("gone/README.md").to_str().expect("UTF-8"));
    let listing = [
        hyperlinked(&gone, "README.md"),
        hyperlinked("https://example.com/README.md", "README.md"),
        hyperlinked("file://elsewhere.invalid/README.md", "README.md"),
    ]
    .join(" && ");
    let mut pane = Pane::running(cx, &in_folder(&dir, &listing));
    pane.wait_for("ready");
    let names = pane.points_at_all("README.md");
    assert_eq!(names.len(), 3, "three names, one to a row");
    for at in names {
        pane.click(at, Pane::alt());
        assert_eq!(pane.float_path(), None, "nothing opens in the square");
        let got = launched_by(&mut pane, at, held(false, true, false, false));
        assert_eq!(got, Vec::<String>::new(), "nothing goes to the desktop");
    }
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

/// `text` laid out the way Claude Code lays out a reply in a pane `cols` wide:
/// two columns of margin on every row, broken between words.
fn claude_rows(text: &str, cols: usize) -> Vec<String> {
    let width = cols - 2;
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in text.split(' ') {
        if !row.is_empty() && row.len() + 1 + word.len() > width {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(word);
        assert!(row.len() <= width, "a word wider than the row: {word}");
    }
    rows.push(row);
    rows.into_iter().map(|r| format!("  {r}")).collect()
}

/// A pane that has printed `rows` as they are, then `ready`.
fn pane_printing(cx: &mut TestAppContext, rows: &[String]) -> Pane {
    let quoted: Vec<String> = rows.iter().map(|r| format!("'{r}'")).collect();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' {} 'ready'; exec cat", quoted.join(" ")),
    );
    pane.wait_for("ready");
    pane
}

/// A command Claude Code wrapped at the pane's width, behind its margin, is
/// copied whole by an Alt+click on either row: one line, one space where the
/// break was.
#[gpui::test]
fn alt_click_on_a_command_claude_wrapped_copies_it_whole(cx: &mut TestAppContext) {
    let command =
        "cargo test --locked --bin terminal-delight -- a_link_claude_broke_behind_its_margin \
                   a_soft_wrapped_url the_disk_decides two_links_that_meet wears_the_link_colour";
    let rows = claude_rows(command, 100);
    assert_eq!(rows.len(), 2, "the fixture must wrap once: {rows:?}");
    let mut pane = pane_printing(cx, &rows);

    for word in ["--locked", "two_links_that_meet"] {
        let at = pane.point_at(word);
        pane.click(at, Pane::alt());
        assert_eq!(
            pane.clipboard().as_deref(),
            Some(command),
            "alt+click on {word:?} copies the whole command"
        );
    }
    assert_eq!(pane.float_path(), None, "a copy opens no square");
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// A command wrapped behind Claude's margin, printed on the alternate screen,
/// the way Claude Code draws with `"tui": "fullscreen"`.
const FULLSCREEN_COMMAND: &str =
    "cargo test --locked --bin terminal-delight -- alt_click_on_a_command fullscreen_claude \
     a_soft_wrapped_url the_disk_decides two_links_that_meet wears_the_link_colour";

fn pane_on_the_alternate_screen(cx: &mut TestAppContext) -> Pane {
    let rows = claude_rows(FULLSCREEN_COMMAND, 100);
    assert_eq!(rows.len(), 2, "the fixture must wrap once: {rows:?}");
    let quoted: Vec<String> = rows.iter().map(|r| format!("'{r}'")).collect();
    let mut pane = Pane::running(
        cx,
        &format!(
            "printf '\\033[?1049h'; printf '%s\\n' {} 'ready'; exec cat",
            quoted.join(" ")
        ),
    );
    pane.wait_for("ready");
    pane
}

/// Claude Code drawing fullscreen is on the alternate screen, and a command it
/// wrapped there copies whole, as it does on the normal screen. The chip was
/// off in every Claude pane from the day that setting went on, because the
/// alternate screen was read as a canvas.
#[gpui::test]
fn alt_click_on_a_command_in_a_fullscreen_claude_pane_copies_it_whole(cx: &mut TestAppContext) {
    let mut pane = pane_on_the_alternate_screen(cx);
    pane.running_as(crate::pane::PaneMode::Claude);
    for word in ["--locked", "two_links_that_meet"] {
        let at = pane.point_at(word);
        pane.click(at, Pane::alt());
        assert_eq!(
            pane.clipboard().as_deref(),
            Some(FULLSCREEN_COMMAND),
            "alt+click on {word:?} copies the whole command"
        );
    }
    assert_eq!(pane.float_path(), None, "a copy opens no square");
}

/// The same rows on the alternate screen of a program that is not an agent —
/// vim, htop — are a canvas, and nothing is offered for a copy.
#[gpui::test]
fn alt_click_on_a_canvas_programs_alternate_screen_copies_nothing(cx: &mut TestAppContext) {
    let mut pane = pane_on_the_alternate_screen(cx);
    let at = pane.point_at("--locked");
    pane.click(at, Pane::alt());
    assert_eq!(pane.clipboard(), None);
}

/// A command whose last word Claude cut at the pane's edge is copied with the
/// word whole: the two halves meet with no space between them.
#[gpui::test]
fn alt_click_on_a_command_cut_mid_word_copies_the_word_whole(cx: &mut TestAppContext) {
    let path = format!(
        "/home/parker/Work/terminal-delight/reports/{}.html",
        "a-report-name".repeat(6)
    );
    let command = format!("ls -la {path}");
    let (head, tail) = command.split_at(98);
    let rows = [format!("  {head}"), format!("  {tail}")];
    let mut pane = pane_printing(cx, &rows);

    let at = pane.point_at("ls -la");
    pane.click(at, Pane::alt());
    assert_eq!(pane.clipboard().as_deref(), Some(command.as_str()));
}

/// A one-row command Claude indented copies without its margin.
#[gpui::test]
fn alt_click_on_an_indented_command_copies_it_without_the_margin(cx: &mut TestAppContext) {
    let command = "cargo clippy --locked -- -D warnings";
    let mut pane = pane_printing(
        cx,
        &[format!("  {command}"), "  and a sentence after it.".into()],
    );
    let at = pane.point_at("clippy");
    pane.click(at, Pane::alt());
    assert_eq!(pane.clipboard().as_deref(), Some(command));
}

/// A `file://` link Claude Code cut at the pane's edge and carried on behind
/// its margin opens whole from its second half: Alt+click floats the picture,
/// Ctrl+click hands the whole link to the desktop.
#[gpui::test]
fn a_link_claude_wrapped_opens_whole_from_its_second_half(cx: &mut TestAppContext) {
    let dir =
        Scratch::new("claude-wrapped-link-long-enough-that-its-path-crosses-the-edge-of-the-pane");
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().expect("a UTF-8 temp path").to_string();
    let url = format!("file://{png}");
    let label = "Target: ";
    let cut = 98 - label.len();
    assert!(url.len() > cut, "the link must cross the edge: {url}");
    let rows = [
        format!("  {label}{}", &url[..cut]),
        format!("  {}", &url[cut..]),
        "  What it is: a picture".to_string(),
    ];
    let mut pane = pane_printing(cx, &rows);
    let second_half = &url[cut..];

    let at = pane.point_at(second_half);
    let got = launched_by(&mut pane, at, held(false, true, false, false));
    assert!(
        got.len() == 1 && got[0].ends_with(&format!("xdg-open {url}")),
        "ctrl+click on the second half opens the whole link: {got:?}"
    );
    let at = pane.point_at(second_half);
    pane.click(at, Pane::alt());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&png)),
        "alt+click on the second half floats the picture"
    );
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
/// note into it. A forced body change exercises a failed automatic save.
fn open_the_brief_with_a_note(pane: &mut Pane, brief: &str, should_save: bool) {
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
    let original = std::fs::read(brief).expect("original brief");
    if !should_save {
        let mut changed = original.clone();
        changed.extend_from_slice(b"<!-- changed while open -->");
        std::fs::write(brief, changed).unwrap();
    }
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
    assert_eq!(
        pane.doc_notes().expect("notes")["unsaved"],
        usize::from(!should_save)
    );
    if !should_save {
        std::fs::write(brief, original).unwrap();
    }
}

/// Submit commits synchronously: even immediate close before browser read-back
/// cannot cancel persistence. Reopening the same path needs no browser storage.
#[gpui::test]
fn a_submitted_comment_survives_immediate_close_and_reopen(cx: &mut TestAppContext) {
    let (mut pane, _dir, brief, _png) = pane_showing_a_brief(cx, "autosave-reopen");
    open_the_brief_with_a_note(&mut pane, &brief, true);
    let written = std::fs::read(&brief).unwrap();
    assert!(String::from_utf8_lossy(&written).contains("Keep the tiles."));
    pane.keys("escape");
    assert_eq!(
        pane.float_path(),
        None,
        "saved comments need no close warning"
    );
    let at = pane.point_at(&brief);
    pane.click(at, Pane::alt());
    pane.redraw();
    let report = pane.doc_notes().unwrap();
    assert_eq!(report["unsaved"], 0);
    assert!(report["map"].as_str().unwrap().contains("Keep the tiles."));
    assert_eq!(
        std::fs::read(&brief).unwrap(),
        written,
        "reopening never rewrites it"
    );
    assert_eq!(
        std::fs::read_dir(std::path::Path::new(&brief).parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "html"))
            .count(),
        1,
        "only the original HTML is present"
    );
}

/// A prior successful write's delayed confirmation must not hide a newer
/// write failure. Both submissions happen before yielding to the executor.
#[gpui::test]
fn a_failed_submission_keeps_its_error_after_an_older_confirmation(cx: &mut TestAppContext) {
    let (mut pane, _dir, brief, _png) = pane_showing_a_brief(cx, "autosave-confirmation");
    let at = pane.point_at(&brief);
    pane.click(at, Pane::alt());
    pane.redraw();
    let square = pane.float_view().unwrap();
    let (nid, _) = BRIEF_ANCHORS[0];
    square.update(pane.cx, |v, cx| {
        v.notes_command(
            crate::docview::NotesCommand::Add {
                nid: nid.into(),
                text: "Already committed.".into(),
            },
            cx,
        )
        .unwrap();
        let mut bytes = std::fs::read(&brief).unwrap();
        bytes.extend_from_slice(b"<!-- an external body change -->");
        std::fs::write(&brief, &bytes).unwrap();
        v.notes_command(
            crate::docview::NotesCommand::Add {
                nid: nid.into(),
                text: "This write must fail.".into(),
            },
            cx,
        )
        .unwrap();
    });
    let before = pane.doc_notes().unwrap();
    assert_eq!(before["unsaved"], 1);
    assert!(!before["said"].as_str().unwrap().contains('✓'));
    pane.redraw();
    let after = pane.doc_notes().unwrap();
    assert_eq!(after["unsaved"], 1);
    assert_eq!(
        after["said"], before["said"],
        "stale confirmation must not hide the write error"
    );
    let bytes = std::fs::read_to_string(&brief).unwrap();
    assert!(bytes.contains("Already committed."));
    assert!(!bytes.contains("This write must fail."));
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
            .is_some_and(|s| s.contains("not kept"))
    };

    open_the_brief_with_a_note(&mut pane, &brief, false);
    pane.keys("escape");
    assert_eq!(
        pane.float_path(),
        Some(brief_path.clone()),
        "Escape keeps it once"
    );
    assert!(said_unsaved(&mut pane), "and the bar says why");
    pane.keys("escape");
    assert_eq!(pane.float_path(), None, "the second Escape closes it");

    open_the_brief_with_a_note(&mut pane, &brief, false);
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

    open_the_brief_with_a_note(&mut pane, &brief, false);
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

/// ctrl+↓ steps the document in a floating square and ctrl+shift+↓ pages
/// it; ctrl+↑ and ctrl+shift+↑ go back. The square stays open through all of
/// it: a key the square claims and its document declines closes the square,
/// which is what Escape is, and these must not.
///
/// Parker, 2026-10-21: *"ctrl seems like it will apply to glow pane, alt applies
/// to the main pane"*, with paging on shift so ctrl+PageUp/PageDown keep
/// switching tabs.
#[gpui::test]
fn ctrl_arrows_turn_the_floating_square(cx: &mut TestAppContext) {
    let (mut pane, _dir) = pane_with_history_and_a_document(cx, "glow-keys");
    let view = pane.float_view().expect("a square");
    let at = |pane: &mut Pane| pane.scroll_of(&view).expect("laid out");
    pane.keys("ctrl-down");
    pane.redraw();
    let stepped = at(&mut pane);
    assert!(stepped > 0.0, "ctrl+down steps the document: {stepped}");
    assert!(pane.float_view().is_some(), "and the square stays open");
    pane.keys("ctrl-shift-down");
    pane.redraw();
    let paged = at(&mut pane);
    assert!(
        paged > stepped * 2.0,
        "ctrl+shift+down pages it: {stepped} then {paged}"
    );
    pane.keys("ctrl-shift-up");
    pane.keys("ctrl-up");
    pane.redraw();
    assert_eq!(at(&mut pane), 0.0, "and the two go back to the top");
    assert!(pane.float_view().is_some());
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

/// A video opens as a clip, in a box of its own over the square already up,
/// and the square under it is kept: the same view, not re-opened. Escape
/// closes the clip and the square is on top again; a clip that plays out does
/// the same by itself. A press on the square around the clip is taken and
/// does nothing. The agent's road (`open_document`, here) opens the same clip
/// box as Alt+click does, and a second clip replaces the first and keeps the
/// square. Parker, 2026-10-13: "we need to make sure we don't LOSE the
/// current open overlay html, so the video SHOULD OPEN IN IT'S OWN video
/// overlay glowbox, then auto close when finished."
#[gpui::test]
fn a_video_opens_as_a_clip_over_the_square_and_its_end_puts_the_square_back(
    cx: &mut TestAppContext,
) {
    let dir = Scratch::new("clip-over-square");
    let clip = dir.join("clip.mp4");
    // An MP4's first box: a video by its name and its bytes. Whether mpv can
    // play it does not matter here; the boxes around it are what is held.
    std::fs::write(&clip, b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41").expect("write");
    let clip = clip.to_str().expect("UTF-8").to_string();
    let other = dir.join("other.mp4");
    std::fs::write(&other, b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41").expect("write");
    let other = other.to_str().expect("UTF-8").to_string();
    let png = dir.fixture(PICTURE, "shot.png");
    let png = png.to_str().expect("UTF-8").to_string();
    // The clips' lines above the picture's, so the picture's square, which
    // opens below its own line, never covers a clip's path.
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'clip {clip}' 'other {other}' 'shot {png}' 'ready'; exec cat"),
    );
    pane.video_ready(Ok(()));
    pane.wait_for("ready");
    let below = |pane: &mut Pane| {
        pane.read(|v| {
            v.float
                .as_ref()
                .and_then(|f| f.below.as_ref())
                .map(|b| b.view.clone())
        })
    };

    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    pane.redraw();
    let square = pane.float_view().expect("the picture's square");
    let square_rect = pane.float_rect().expect("drawn");

    let at = pane.point_at(&clip);
    pane.click(at, Pane::alt());
    pane.redraw();
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&clip)),
        "the clip is the box on top"
    );
    assert_eq!(
        below(&mut pane),
        Some(square.clone()),
        "the square under it is the same view"
    );
    let clip_rect = pane.float_rect().expect("the clip is drawn");
    assert!(
        clip_rect.0 > square_rect.0
            && clip_rect.1 > square_rect.1
            && clip_rect.0 + clip_rect.2 < square_rect.0 + square_rect.2
            && clip_rect.1 + clip_rect.3 < square_rect.1 + square_rect.3,
        "the clip's box sits inside the square, its rim showing: {clip_rect:?} in {square_rect:?}"
    );
    // The square's own controls are not live under the clip: the only ✕ is
    // the clip's, and the rest of the square is one covered zone.
    let closes = pane.read(|v| {
        v.float_zones
            .borrow()
            .iter()
            .filter(|z| z.hit == FloatHit::Close)
            .map(|z| (z.x, z.y, z.w, z.h))
            .collect::<Vec<_>>()
    });
    assert!(
        closes.len() == 1 && closes[0].1 >= clip_rect.1,
        "one ✕ is live, and it is the clip's: {closes:?}"
    );
    let covered = pane
        .float_zone(FloatHit::Covered)
        .expect("the square is covered");
    let rim = point(px(covered.0 + 6.0), px(covered.1 + covered.3 - 6.0));
    pane.click(rim, Default::default());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&clip)),
        "a press on the covered square changes nothing"
    );

    pane.keys("escape");
    assert_eq!(
        pane.float_view(),
        Some(square.clone()),
        "Escape closes the clip, and the same square is on top again"
    );
    assert_eq!(below(&mut pane), None);

    // The agent's road: open_document on a video opens the same clip box.
    let target = crate::docopen::drawable_document(std::path::Path::new(&clip)).expect("a video");
    let said = pane
        .view
        .update(pane.cx, |v, cx| v.open_here(target.clone(), cx));
    assert!(said.starts_with("ok float"), "{said}");
    assert_eq!(below(&mut pane), Some(square.clone()));
    // Another clip replaces this one, and the square under it stays. (The
    // same clip's path Alt+clicked again would ask for a split instead.)
    let first = pane.float_view().expect("the clip");
    let at = pane.point_at(&other);
    pane.click(at, Pane::alt());
    pane.redraw();
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&other))
    );
    let second = pane.float_view().expect("a clip");
    assert_ne!(second, first, "the second clip replaced the first");
    assert_ne!(second, square);
    assert_eq!(
        below(&mut pane),
        Some(square.clone()),
        "a clip asked for again keeps the square under it"
    );

    // It plays out: the clip closes by itself and the square is back.
    let playing = pane.float_view().expect("the clip");
    playing.update(pane.cx, |_, cx| cx.emit(crate::docview::Finished));
    pane.redraw();
    assert_eq!(
        pane.float_view(),
        Some(square),
        "a clip that plays out puts the square back on top"
    );
    pane.keys("escape");
    assert_eq!(
        pane.float_path(),
        None,
        "and Escape closes the square, as before"
    );
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// The page road: a press on a `<video>` in a page open in the square opens
/// the file it plays as a clip over the page. The page is not replaced, as a
/// followed link replaces it; it is under the clip, the same view, and back
/// on top when the clip goes.
#[gpui::test]
fn a_press_on_a_video_in_a_page_opens_it_as_a_clip_over_the_page(cx: &mut TestAppContext) {
    let dir = Scratch::new("clip-from-page");
    let film = dir.join("film.mp4");
    std::fs::write(&film, b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41").expect("write");
    let film = film.to_str().expect("UTF-8").to_string();
    let page = dir.fixture("tests/fixtures/page/brief.html", "welcome.html");
    let page = page.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'page {page}' 'ready'; exec cat"),
    );
    pane.html_engine(Ok(std::sync::Arc::new(FakeFilmEngine {
        src: Some(format!("file://{film}")),
        sound: false,
    })));
    pane.video_ready(Ok(()));
    pane.wait_for("ready");

    let at = pane.point_at(&page);
    pane.click(at, Pane::alt());
    pane.redraw();
    pane.redraw();
    let square = pane.float_view().expect("the page's square");
    let body = pane.float_zone(FloatHit::Body).expect("the page's body");
    pane.click(Pane::middle(body), Default::default());
    pane.redraw();
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&film)),
        "the film opens as the box on top"
    );
    let under = pane.read(|v| {
        v.float
            .as_ref()
            .and_then(|f| f.below.as_ref())
            .map(|b| b.view.clone())
    });
    assert_eq!(
        under,
        Some(square.clone()),
        "the page is under it, not replaced"
    );
    pane.keys("escape");
    assert_eq!(
        pane.float_view(),
        Some(square),
        "and back on top when it goes"
    );
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// The same road from a page that carries its film inside itself, as a
/// `data:` address — a page made to survive being sent as one file. The
/// film is kept as a file in the page cache, and that file opens as the clip
/// over the page. Before, a press there did nothing at all.
#[gpui::test]
fn a_press_on_a_video_the_page_carries_inside_itself_opens_it_as_a_clip(cx: &mut TestAppContext) {
    use base64::Engine;
    let film = b"\0\0\0\x20ftypisom\0\0\x02\0isomiso2avc1mp41carried";
    let address = format!(
        "data:video/mp4;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(film)
    );
    let dir = Scratch::new("clip-carried");
    let page = dir.fixture("tests/fixtures/page/brief.html", "welcome.html");
    let page = page.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'page {page}' 'ready'; exec cat"),
    );
    pane.html_engine(Ok(std::sync::Arc::new(FakeFilmEngine {
        src: Some(address),
        sound: false,
    })));
    pane.video_ready(Ok(()));
    pane.wait_for("ready");

    let at = pane.point_at(&page);
    pane.click(at, Pane::alt());
    pane.redraw();
    pane.redraw();
    let square = pane.float_view().expect("the page's square");
    let body = pane.float_zone(FloatHit::Body).expect("the page's body");
    pane.click(Pane::middle(body), Default::default());
    pane.redraw();
    let opened = pane.float_path().expect("the film opens as the box on top");
    assert!(
        opened.starts_with(crate::docview::cache::root()) && opened.ends_with("clip.mp4"),
        "from the file the page cache kept: {}",
        opened.display()
    );
    assert_eq!(
        std::fs::read(&opened).expect("kept"),
        film,
        "the film itself"
    );
    let under = pane.read(|v| {
        v.float
            .as_ref()
            .and_then(|f| f.below.as_ref())
            .map(|b| b.view.clone())
    });
    assert_eq!(under, Some(square), "the page is under it, not replaced");
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// A sound plays the way a film does. A press on a page's `<audio>`, whose
/// sound the page carries inside itself, opens it as a clip over the page,
/// from the file the page cache kept, named so it opens as a sound. Its box
/// is a band only as tall as its waveform, so most of the page stays in
/// sight, and Escape puts the page back on top. An Alt+click on a sound's
/// path opens the same box. Parker, 2026-10-13, of the takes page: "let's
/// get the same treatment for audio files."
#[gpui::test]
fn a_press_on_a_sound_in_a_page_opens_it_as_a_band_over_the_page(cx: &mut TestAppContext) {
    use base64::Engine;
    let take = b"OggS\0\x02\0\0\0\0\0\0\0\0\0\0\x9b\xc7\xfa\xbfOpusHeadcarried";
    let address = format!(
        "data:audio/ogg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(take)
    );
    let dir = Scratch::new("sound-carried");
    let song = dir.join("song.mp3");
    std::fs::write(&song, b"ID3\x04\0\0\0\0\0\x23TSSE").expect("write");
    let song = song.to_str().expect("UTF-8").to_string();
    let page = dir.fixture("tests/fixtures/page/brief.html", "takes.html");
    let page = page.to_str().expect("UTF-8").to_string();
    // The song's line above the page's, so the page's square, which opens
    // below its own line, never covers the song's path.
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'song {song}' 'page {page}' 'ready'; exec cat"),
    );
    pane.html_engine(Ok(std::sync::Arc::new(FakeFilmEngine {
        src: Some(address),
        sound: true,
    })));
    pane.video_ready(Ok(()));
    pane.wait_for("ready");

    let at = pane.point_at(&page);
    pane.click(at, Pane::alt());
    pane.redraw();
    pane.redraw();
    let square = pane.float_view().expect("the page's square");
    let page_rect = pane.float_rect().expect("drawn");
    let body = pane.float_zone(FloatHit::Body).expect("the page's body");
    pane.click(Pane::middle(body), Default::default());
    pane.redraw();
    let opened = pane
        .float_path()
        .expect("the sound opens as the box on top");
    assert!(
        opened.starts_with(crate::docview::cache::root()) && opened.ends_with("clip.ogg"),
        "from the file the page cache kept, as a sound: {}",
        opened.display()
    );
    assert_eq!(
        std::fs::read(&opened).expect("kept"),
        take,
        "the sound itself"
    );
    let under = pane.read(|v| {
        v.float
            .as_ref()
            .and_then(|f| f.below.as_ref())
            .map(|b| b.view.clone())
    });
    assert_eq!(under, Some(square.clone()), "the page is under it");
    let band = pane.float_rect().expect("the sound is drawn");
    let wave = band.3 - crate::docopen::FLOAT_STRIP_H - crate::docview::video::BAR_H;
    assert!(
        (band.2 / wave - crate::docopen::SOUND_SHAPE).abs() < 0.05,
        "a band as tall as its waveform: {band:?}"
    );
    assert!(
        band.1 > page_rect.1 && band.1 + band.3 < page_rect.1 + page_rect.3 * 0.75,
        "inside the page, which stays mostly in sight: {band:?} in {page_rect:?}"
    );
    pane.keys("escape");
    assert_eq!(pane.float_view(), Some(square), "the page is back on top");

    let at = pane.point_at(&song);
    pane.click(at, Pane::alt());
    pane.redraw();
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&song)),
        "Alt+click on a sound's path opens the same box"
    );
    let band = pane.float_rect().expect("drawn");
    assert!(band.3 < page_rect.3 * 0.75, "a band again: {band:?}");
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// A press on a page's video that names no file opens nothing, and the bar
/// says why. Before, it did nothing at all, which reads as a press that
/// never landed. Answers what the bar said. `tag` names the scratch
/// directory, which the tests calling this must not share.
fn press_a_video_with_no_file(cx: &mut TestAppContext, tag: &str, src: Option<String>) -> String {
    let dir = Scratch::new(tag);
    let page = dir.fixture("tests/fixtures/page/brief.html", "welcome.html");
    let page = page.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'page {page}' 'ready'; exec cat"),
    );
    pane.html_engine(Ok(std::sync::Arc::new(FakeFilmEngine {
        src,
        sound: false,
    })));
    pane.video_ready(Ok(()));
    pane.wait_for("ready");

    let at = pane.point_at(&page);
    pane.click(at, Pane::alt());
    pane.redraw();
    pane.redraw();
    let square = pane.float_view().expect("the page's square");
    let body = pane.float_zone(FloatHit::Body).expect("the page's body");
    pane.click(Pane::middle(body), Default::default());
    pane.redraw();
    assert_eq!(pane.float_view(), Some(square), "nothing opened over it");
    assert_eq!(desktop_launches(), Vec::<String>::new());
    let said = pane.doc_notes().expect("the page's bar")["said"].clone();
    said.as_str().unwrap_or_default().to_string()
}

/// One a script feeds.
#[gpui::test]
fn a_press_on_a_video_a_script_feeds_says_why_in_the_bar(cx: &mut TestAppContext) {
    let said = press_a_video_with_no_file(cx, "clip-fed", None);
    assert!(
        said.contains("the page's own script"),
        "the bar says why: {said:?}"
    );
}

/// A film the page carries that could not be kept, with the cache's reason.
#[gpui::test]
fn a_press_on_a_film_that_could_not_be_kept_says_why_in_the_bar(cx: &mut TestAppContext) {
    use base64::Engine;
    let words = base64::engine::general_purpose::STANDARD.encode(b"just some words");
    let address = format!("data:text/plain;base64,{words}");
    let said = press_a_video_with_no_file(cx, "clip-unkept", Some(address));
    assert!(
        said.contains("could not be kept") && said.contains("text/plain"),
        "the bar says why: {said:?}"
    );
}

/// A PDF with no poppler to draw it goes to the desktop before any square or
/// pane is made, and the pane says why on the row that was clicked, as a video
/// with no libmpv does. A split asked for is refused the same way.
#[gpui::test]
fn a_pdf_nothing_can_draw_goes_to_the_desktop_and_says_why(cx: &mut TestAppContext) {
    let dir = Scratch::new("no-poppler");
    let pdf = dir.join("paper.pdf");
    std::fs::write(&pdf, crate::docview::poppler::two_page_pdf()).expect("write");
    let pdf = pdf.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'pdf {pdf}' 'ready'; exec cat"));
    pane.pdf_tools(Err(crate::docview::poppler::Missing::NotFound("pdfinfo")));
    pane.wait_for("ready");
    let asked = pane.asks_beside();

    let at = pane.point_at(&pdf);
    let row = pane.painted_row(at);
    let got = launched_by(&mut pane, at, Pane::alt());
    assert!(
        got.len() == 1 && got[0].ends_with(&format!("xdg-open {pdf}")),
        "the PDF goes to the desktop: {got:?}"
    );
    assert_eq!(pane.float_path(), None, "and no square is made for it");
    let (text, on) = pane
        .read(|v| v.said.as_ref().map(|s| (s.text.clone(), s.row)))
        .expect("the pane says why");
    assert!(text.contains("no poppler"), "{text}");
    assert_eq!(on, Some(row), "on the row that was clicked");

    let got = launched_by(&mut pane, at, held(true, true, false, false));
    assert_eq!(got.len(), 1, "a split is refused the same way: {got:?}");
    assert_eq!(*asked.borrow(), Vec::<Asked>::new(), "no pane is asked for");
}

/// Alt+click on a PDF's path floats it over the pane, read by the real
/// poppler: both its pages known, and the page in view drawn. Closing the
/// square drops the view and every tile with it; nothing went to the desktop.
///
/// Runs wherever poppler is installed, and under `CI`, which installs it,
/// fails rather than passing unseen when it is missing.
#[gpui::test]
fn a_pdf_alt_clicked_floats_as_its_pages(cx: &mut TestAppContext) {
    let tools = match crate::docview::poppler::locate(std::env::var_os("PATH").as_deref()) {
        Ok(tools) => tools,
        Err(why) if std::env::var_os("CI").is_none() => {
            eprintln!("skipped: {}", why.reason());
            return;
        }
        Err(why) => panic!("{}", why.reason()),
    };
    let dir = Scratch::new("pdf-square");
    let pdf = dir.join("paper.pdf");
    std::fs::write(&pdf, crate::docview::poppler::two_page_pdf()).expect("write");
    let pdf = pdf.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'pdf {pdf}' 'ready'; exec cat"));
    pane.pdf_tools(Ok(tools));
    pane.wait_for("ready");
    let before = desktop_launches().len();

    let at = pane.point_at(&pdf);
    pane.click(at, Pane::alt());
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&pdf))
    );
    let view = pane.float_view().expect("a square");
    let (pages, tiles) = pane.pdf_once(&view, |pages, tiles| pages == Some(2) && tiles >= 1);
    assert_eq!(pages, Some(2), "both pages read");
    assert!(tiles >= 1, "the page in view is drawn");
    assert_eq!(
        pane.scroll_of(&view),
        Some(0.0),
        "laid out at the top, so a restart can keep the place"
    );

    // A document's link to `#page=2` of it opens on the second page.
    view.update(pane.cx, |_, cx| {
        cx.emit(crate::docview::FollowLink {
            target: pdf.clone(),
            fragment: Some("page=2".into()),
        })
    });
    pane.redraw();
    let view = pane.float_view().expect("a square");
    pane.pdf_once(&view, |pages, tiles| pages == Some(2) && tiles >= 1);
    let top = pane.scroll_of(&view).expect("laid out");
    assert!(top > 0.3, "on the second page: {top}");

    let weak = view.downgrade();
    drop(view);
    pane.keys("escape");
    pane.redraw();
    assert_eq!(pane.float_path(), None, "Escape closes the square");
    assert!(
        weak.upgrade().is_none(),
        "and the view, with its tiles, is gone"
    );
    assert_eq!(
        desktop_launches().len(),
        before,
        "nothing went to the desktop"
    );
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

/// One of the four size chords as the desktop hands it to a pane: ctrl held,
/// the shifted pair named by what it types with its shift dropped, and the
/// character each would type carried along, as Linux carries it. `^_` is a
/// control character, so ctrl+shift+- carries none.
fn size_chord_as_typed(key: &str) -> gpui::Keystroke {
    gpui::Keystroke {
        modifiers: gpui::Modifiers {
            control: true,
            ..Default::default()
        },
        key: key.into(),
        key_char: (key != "_").then(|| key.to_string()),
    }
}

/// The pane's own dial, wherever the pane keeps it: `TextSize` for the grid,
/// `BenchSize` for the workbench's type.
fn dial(pane: &mut Pane, key: crate::theme::GradeKey) -> f32 {
    pane.view.read_with(pane.cx, |v, cx| {
        v.appearance
            .effective(&crate::theme::outer_choice(cx))
            .grade
            .get(key)
            .expect("a size dial always has a position")
    })
}

/// Parker, 2026-10-16: *"highlighted window will now use ctrl + = and ctrl + -
/// to zoom in increase and decrease text size ... ctrl+shift+= and
/// ctrl+shift+- to increase decrease OUTTER scale (tracks to ctrl + scroll
/// wheel up or down)"*.
///
/// On the terminal face the unshifted pair steps the highlighted pane's text
/// one ctrl+wheel notch a press. The shifted pair is the outer scale, the
/// window's to act on, so this pane's text stays where it is. And none of the
/// four types into the shell, which is where each went before: ctrl+= typed a
/// bare `=`.
#[gpui::test]
fn the_size_chords_step_the_text_and_never_type(cx: &mut TestAppContext) {
    use crate::theme::GradeKey;
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    // Settled first: a terminal still being born refuses every key, and
    // "nothing typed" would then pass on a pane that took nothing at all.
    pane.settle();
    let notch = GradeKey::WHEEL_STEP;
    let was = dial(&mut pane, GradeKey::TextSize);

    pane.keystroke(size_chord_as_typed("="));
    let up = dial(&mut pane, GradeKey::TextSize);
    assert!(
        (up - (was + notch)).abs() < 1e-4,
        "ctrl+= is a notch up: {was} → {up}"
    );
    pane.keystroke(size_chord_as_typed("-"));
    pane.keystroke(size_chord_as_typed("-"));
    let down = dial(&mut pane, GradeKey::TextSize);
    assert!(
        (down - (was - notch)).abs() < 1e-4,
        "ctrl+- is a notch down a press: {was} → {down}"
    );

    pane.keystroke(size_chord_as_typed("+"));
    pane.keystroke(size_chord_as_typed("_"));
    pane.keys("ctrl-shift-= ctrl-shift--");
    assert_eq!(
        dial(&mut pane, GradeKey::TextSize),
        down,
        "the shifted pair sizes the outer chrome, never this pane's text"
    );

    pane.keys("o k enter");
    pane.wait_for("ok");
    let rows = pane.rows();
    assert!(
        !rows.iter().any(|r| r.contains(['=', '-', '+', '_', '^'])),
        "no size chord typed into the shell:\n{}",
        rows.join("\n")
    );
}

/// On the workbench the unshifted pair turns the bench's own type, the dial
/// ctrl+wheel over the bench turns, and leaves the grid's text alone.
#[gpui::test]
fn the_size_chords_on_the_workbench_size_its_type(cx: &mut TestAppContext) {
    use crate::theme::GradeKey;
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    pane.view
        .update(pane.cx, |v, cx| v.set_face(Face::Workbench, cx));
    pane.redraw();
    let text = dial(&mut pane, GradeKey::TextSize);
    let bench = dial(&mut pane, GradeKey::BenchSize);

    pane.keystroke(size_chord_as_typed("="));
    let now = dial(&mut pane, GradeKey::BenchSize);
    assert!(
        (now - (bench + GradeKey::WHEEL_STEP)).abs() < 1e-4,
        "ctrl+= is a notch up on the bench: {bench} → {now}"
    );
    assert_eq!(
        dial(&mut pane, GradeKey::TextSize),
        text,
        "and the grid's text stays"
    );
}

/// On the Document face the unshifted pair zooms the document a step each
/// way, as ctrl+wheel there does, and leaves the pane's text dial alone.
#[gpui::test]
fn the_size_chords_on_the_document_face_zoom_the_document(cx: &mut TestAppContext) {
    use crate::theme::GradeKey;
    let (mut pane, _dir, _md) = pane_on_its_document_face(cx, "face-size-chords");
    let view = pane.face_view().expect("the face's view");
    let zoom = |pane: &mut Pane| view.read_with(pane.cx, |v, _| v.zoom_now());
    let text = dial(&mut pane, GradeKey::TextSize);
    assert_eq!(
        zoom(&mut pane),
        Some(crate::docview::ImageZoom::Scale(1.0)),
        "a Markdown document opens at 100%"
    );

    pane.keystroke(size_chord_as_typed("="));
    pane.redraw();
    assert_eq!(
        zoom(&mut pane),
        Some(crate::docview::ImageZoom::Scale(1.1)),
        "ctrl+= zooms the document in a step"
    );
    pane.keystroke(size_chord_as_typed("-"));
    pane.keystroke(size_chord_as_typed("-"));
    pane.redraw();
    assert_eq!(
        zoom(&mut pane),
        Some(crate::docview::ImageZoom::Scale(0.9)),
        "ctrl+- zooms it out a step a press"
    );
    assert_eq!(
        dial(&mut pane, GradeKey::TextSize),
        text,
        "and the text dial stays"
    );
}

/// While the pane is open in the FOCUS reader the unshifted pair is not the
/// pane's, as ctrl+wheel over the reader is not: the pane declines it, so its
/// letters stay and nothing reaches its shell. The window takes it from there
/// and steps the reader's own zoom (`Workspace::on_key`).
#[gpui::test]
fn the_size_chords_leave_a_pane_being_read_to_the_reader(cx: &mut TestAppContext) {
    use crate::theme::GradeKey;
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    pane.settle();
    let text = dial(&mut pane, GradeKey::TextSize);
    let reader = pane.open_reader((900.0, 700.0));

    // Read after each press: a step up and a step down would cancel, and
    // the pane would look untouched after both even if each had sized it.
    for key in ["=", "-"] {
        pane.keystroke(size_chord_as_typed(key));
        assert_eq!(
            dial(&mut pane, GradeKey::TextSize),
            text,
            "ctrl+{key} leaves the pane behind the reader its letters"
        );
    }
    reader.close(&mut pane);

    pane.keys("o k enter");
    pane.wait_for("ok");
    assert!(
        !pane.rows().iter().any(|r| r.contains(['=', '-'])),
        "and nothing reached its shell"
    );
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

/// A pane launched with a program — `-e` through xdg-terminal-exec — runs that
/// program as its process and types nothing into it, and wears its launch mark.
///
/// The resume line is the discriminator: were it typed, `cat` would echo it
/// back onto the screen. The arithmetic is the other one: only a shell that
/// was handed these exact arguments prints 42, where the person's own shell
/// would print a prompt.
///
/// Mutation-tested: typing the resume line regardless of a program, and
/// dropping `options.shell = spec.program`, each fail this.
#[gpui::test]
fn a_launched_pane_runs_its_program_and_types_nothing(cx: &mut TestAppContext) {
    let restore = crate::session::PaneRestore {
        program: Some((
            "/bin/sh".into(),
            vec!["-c".into(), "echo launched-$((6*7)); exec cat".into()],
        )),
        resume: Some("echo typed-into-a-shell".into()),
        mark: Some("LOCAL MODEL"),
        ..Default::default()
    };
    let mut pane = Pane::launched(cx, restore);
    pane.wait_for("launched-42");
    std::thread::sleep(std::time::Duration::from_millis(300));
    pane.redraw();
    let rows = pane.rows();
    assert!(
        !rows.iter().any(|r| r.contains("typed-into-a-shell")),
        "a line was typed into the program:\n{}",
        rows.join("\n")
    );
    assert_eq!(pane.read(|v| v.launch_mark), Some("LOCAL MODEL"));
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

/// The words being written in the document's open note box, as the control
/// socket reports them.
fn draft_text(pane: &mut Pane) -> Option<String> {
    pane.doc_notes().expect("notes")["draft_text"]
        .as_str()
        .map(str::to_string)
}

/// Paste each of `pastes` with its chord, the clipboard holding its words.
fn paste_each(pane: &mut Pane, pastes: &[(&str, &str)]) {
    for (chord, words) in pastes {
        pane.cx
            .write_to_clipboard(gpui::ClipboardItem::new_string((*words).into()));
        pane.keys(chord);
        pane.redraw();
    }
}

/// A paste while a note is being written lands in the note, by each of the
/// three chords — ctrl+v, ctrl+shift+v, and shift+insert, which is what
/// Omarchy's clipboard manager types — and never in the terminal under the
/// square. Ctrl+Shift+V used to be the pane's own chord even then, and the
/// pane pastes into its terminal: writing a note on a document floating over
/// an agent, it went into the agent's prompt, hidden behind the square.
#[gpui::test]
fn a_paste_while_writing_a_note_lands_in_the_note_and_never_the_terminal(cx: &mut TestAppContext) {
    use crate::docview::markdown::{NOTE_GUTTER, PAD};
    use crate::docview::notes_ui::BUTTON_CSS;
    let dir = Scratch::new("md-note-paste");
    let md = dir.join("plan.md");
    std::fs::write(&md, "A paragraph that takes a note.\n").expect("the document");
    let md = md.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'doc {md}' 'ready'; exec cat"));
    pane.wait_for("ready");
    pane.settle();
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
    assert_eq!(
        draft_text(&mut pane).as_deref(),
        Some(""),
        "the box is open on an empty draft"
    );

    paste_each(
        &mut pane,
        &[
            ("ctrl-shift-v", "pastedalpha"),
            ("ctrl-v", " pastedbeta"),
            ("shift-insert", "\npastedgamma"),
        ],
    );
    assert_eq!(
        draft_text(&mut pane).as_deref(),
        Some("pastedalpha pastedbeta\npastedgamma"),
        "all three chords paste into the note, in order, line break kept"
    );

    pane.keys("escape escape");
    pane.redraw();
    assert!(pane.float_path().is_none(), "the note box, then the square");
    pane.keys("e c h o e d enter");
    pane.wait_for("echoed");
    let rows = pane.rows();
    assert!(
        !rows.iter().any(|r| r.contains("pasted")),
        "nothing pasted reached the terminal under the square:\n{}",
        rows.join("\n")
    );
}

/// The same on the Document face, where ctrl+shift+v over a note being
/// written did nothing at all: the pane claimed the chord, then refused to
/// paste into a shell the page hides.
#[gpui::test]
fn a_paste_while_writing_a_note_on_the_document_face_lands_in_the_note(cx: &mut TestAppContext) {
    use crate::docview::markdown::{NOTE_GUTTER, PAD};
    use crate::docview::notes_ui::BUTTON_CSS;
    let dir = Scratch::new("face-note-paste");
    let md = dir.join("plan.md");
    std::fs::write(&md, "A paragraph that takes a note.\n").expect("the document");
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    pane.settle();
    pane.show_document(&md);
    assert_eq!(pane.face(), Face::Document);
    pane.redraw();
    let (x, y, w, _) = pane
        .read(|v| v.doc_rect.get())
        .expect("the face's document was laid out");
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
    assert_eq!(
        draft_text(&mut pane).as_deref(),
        Some(""),
        "the box is open on the face"
    );

    paste_each(
        &mut pane,
        &[
            ("ctrl-shift-v", "pastedalpha"),
            ("ctrl-v", " pastedbeta"),
            ("shift-insert", " pastedgamma"),
        ],
    );
    assert_eq!(
        draft_text(&mut pane).as_deref(),
        Some("pastedalpha pastedbeta pastedgamma")
    );
}

/// The words in a sticky note that still has the pen.
fn sticky_draft(pane: &mut Pane) -> Option<String> {
    pane.read(|v| {
        v.note
            .as_ref()
            .and_then(|n| n.edit.as_ref())
            .map(|e| e.buf.text())
    })
}

/// A paste while a sticky note is being written lands on the paper, by each
/// of the three chords, and never in the terminal under it.
///
/// The square and the Document face already do this. The sticky note is the
/// third caret drawn over the same terminal, and it was left out of that fix:
/// ctrl+shift+v is still the pane's paste, so the clipboard is typed into
/// whatever is running behind the paper, and ctrl+v and shift+insert reach
/// the note and then vanish because its editor treats them as chords. The
/// paper draws one line, so the line break in the last paste lands as a space:
/// kept, it would hide every word after it.
#[gpui::test]
fn a_sticky_note_being_written_takes_a_paste_instead_of_the_shell(cx: &mut TestAppContext) {
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    pane.settle();
    pane.keys("alt-s");
    assert!(
        pane.read(|v| v.sticky_composing()),
        "alt+s gives the note the pen"
    );
    assert_eq!(sticky_draft(&mut pane).as_deref(), Some(""));

    paste_each(
        &mut pane,
        &[
            ("ctrl-shift-v", "pastedalpha"),
            ("ctrl-v", " pastedbeta"),
            ("shift-insert", "\npastedgamma"),
        ],
    );
    let draft = sticky_draft(&mut pane);
    pane.keys("alt-s");
    assert!(
        !pane.read(|v| v.sticky_composing()),
        "alt+s again posts the note and hands the keyboard back"
    );
    pane.keys("e c h o e d enter");
    pane.wait_for("echoed");
    let rows = pane.rows();
    assert_eq!(
        draft.as_deref(),
        Some("pastedalpha pastedbeta pastedgamma"),
        "all three chords paste onto the paper, in order, the line break a space; shell saw:\n{}",
        rows.join("\n")
    );
    assert!(
        !rows.iter().any(|r| r.contains("pasted")),
        "nothing pasted reached the shell under the note:\n{}",
        rows.join("\n")
    );
}

/// A picture pasted while a note is being written on a Markdown file is
/// attached to that block: kept as a file in TD's notes folder, listed in the
/// box as `[doc-image #1]` and never by its path, nothing written into the
/// note, and the map giving its full path. The same picture pasted again is
/// the one already there. Everything is kept under the test's own folder.
#[gpui::test]
fn a_picture_pasted_into_a_note_is_attached_to_its_block(cx: &mut TestAppContext) {
    use crate::docview::markdown::{NOTE_GUTTER, PAD};
    use crate::docview::notes_ui::BUTTON_CSS;
    let dir = Scratch::new("md-note-picture");
    let root = dir.join("state");
    let shot = std::fs::read(dir.fixture(PICTURE, "shot.png")).expect("a picture");
    let md = dir.join("plan.md");
    std::fs::write(&md, "A paragraph that takes a note.\n").expect("the document");
    let md = md.to_str().expect("a UTF-8 temp path").to_string();
    let mut pane = Pane::running(cx, &format!("printf '%s\\n' 'doc {md}' 'ready'; exec cat"));
    pane.notes_root(root.clone());
    pane.wait_for("ready");
    pane.settle();
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
    let report = pane.doc_notes().expect("notes");
    assert_eq!(report["pictures"], 0, "read, and none yet: {report}");

    let picture = gpui::Image::from_bytes(gpui::ImageFormat::Png, shot.clone());
    pane.cx
        .write_to_clipboard(gpui::ClipboardItem::new_image(&picture));
    pane.keys("ctrl-v");
    pane.redraw();
    let report = pane.doc_notes().expect("notes");
    assert_eq!(
        report["pictures_open"],
        serde_json::json!(["[doc-image #1]"]),
        "listed on the open block by its label: {report}"
    );
    assert_eq!(report["draft_text"], "", "nothing written into the note");
    assert_eq!(report["said"], "attached as [doc-image #1]");
    let file = crate::docview::doc_images::folder(&root, std::path::Path::new(&md))
        .join("doc-image-1.png");
    assert_eq!(std::fs::read(&file).expect("kept as a file"), shot);
    let map = report["map"].as_str().expect("a map");
    assert!(
        map.contains(&format!(
            "[L1] A paragraph that takes a note.\n  · [doc-image #1] {}\n",
            file.display()
        )),
        "the map gives the block its picture's full path:\n{map}"
    );
    std::fs::read_to_string(&md)
        .map(|text| {
            assert_eq!(
                text, "A paragraph that takes a note.\n",
                "the file is untouched"
            )
        })
        .expect("the document");

    pane.keys("ctrl-v");
    pane.redraw();
    let report = pane.doc_notes().expect("notes");
    assert_eq!(report["pictures"], 1, "the same picture is one picture");
    assert_eq!(report["said"], "already attached as [doc-image #1]");
}

/// On a brief the same: the picture is attached to the element whose note
/// box is open, listed there and mapped with its full path, and the brief's
/// own file does not change by a byte. Its notes island is a format its own
/// notes.js shares, and a picture has no place in it.
#[gpui::test]
fn a_picture_pasted_into_a_note_on_a_brief_leaves_the_brief_untouched(cx: &mut TestAppContext) {
    let (mut pane, dir, brief, png) = pane_showing_a_brief(cx, "brief-picture");
    let root = dir.join("state");
    pane.notes_root(root.clone());
    let before = std::fs::read(&brief).expect("the brief");
    let at = pane.point_at(&brief);
    pane.click(at, Pane::alt());
    pane.redraw();
    assert_eq!(
        pane.doc_notes().expect("notes")["pictures"],
        0,
        "read, and none yet"
    );
    // Well inside the first element, which the engine lays out at 28–548
    // across and 80–200 down, at any scale the square draws it.
    let (x, y, _, _) = pane.float_zone(FloatHit::Body).expect("the square's body");
    let inside = point(px(x + 60.), px(y + 90.));
    pane.hover(inside);
    pane.modifiers(Pane::alt());
    pane.redraw();
    pane.click(inside, Pane::alt());
    pane.modifiers(Default::default());
    pane.redraw();
    let (nid, title) = BRIEF_ANCHORS[0];
    assert_eq!(
        pane.doc_notes().expect("notes")["open"],
        nid,
        "Alt+press opens the first element's note box"
    );

    let shot = std::fs::read(&png).expect("a picture");
    let picture = gpui::Image::from_bytes(gpui::ImageFormat::Png, shot.clone());
    pane.cx
        .write_to_clipboard(gpui::ClipboardItem::new_image(&picture));
    pane.keys("ctrl-v");
    pane.redraw();
    let report = pane.doc_notes().expect("notes");
    assert_eq!(
        report["pictures_open"],
        serde_json::json!(["[doc-image #1]"]),
        "{report}"
    );
    assert_eq!(report["unsaved"], 0, "a picture is no edit to save");
    let file = crate::docview::doc_images::folder(&root, std::path::Path::new(&brief))
        .join("doc-image-1.png");
    assert_eq!(std::fs::read(&file).expect("kept as a file"), shot);
    let map = report["map"].as_str().expect("a map");
    assert!(
        map.contains(&format!(
            "[{nid}] {title}\n  · [doc-image #1] {}\n",
            file.display()
        )),
        "{map}"
    );
    assert_eq!(
        std::fs::read(&brief).expect("the brief"),
        before,
        "the brief is byte for byte what it was"
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
    open_the_brief_with_a_note(&mut pane, &brief, true);
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

#[gpui::test]
fn brief_launch_local_notes_reach_composer_and_prompt(cx: &mut TestAppContext) {
    let mut pane = Pane::running(cx, "printf '\\033[?2004hready\\n'; exec cat");
    pane.wait_for("ready");
    let map = "NOTES — local-brief.html\n[fig-01] Draw the missing case.";
    pane.view.update(pane.cx, |v, cx| {
        v.mode = super::PaneMode::Other(crate::launcher::LOCAL_AGENT_MODE.into());
        v.set_face(Face::Workbench, cx);
    });
    let landed = pane.view.update(pane.cx, |v, cx| v.send_notes(map, cx));
    assert_eq!(
        landed,
        Ok(super::NotesLanded::Composer(map.chars().count()))
    );
    let draft = pane.read(|v| v.wb_compose.as_ref().map(|l| l.text().to_string()));
    assert_eq!(draft.as_deref(), Some(map));
    pane.view.update(pane.cx, |v, cx| {
        v.wb_compose = None;
        v.set_face(Face::Terminal, cx);
    });
    let landed = pane.view.update(pane.cx, |v, cx| v.send_notes(map, cx));
    assert_eq!(landed, Ok(super::NotesLanded::Prompt(map.chars().count())));
    pane.wait_for("missing case");
}

/// A shell's lines reach the FOCUS reader exactly as the program printed them:
/// joined where the terminal soft-wrapped them, and nowhere else.
///
/// Both cases here are ones the old width guess got wrong on this very pane.
/// A line that wraps into a row starting with spaces was refused, because an
/// indented row looked like a code block; and a half-full row followed by a
/// long first word was glued to it, because the word "could not have fit".
/// The terminal knew both answers all along, in its soft-wrap flag.
#[gpui::test]
fn the_reader_joins_a_shells_lines_where_the_terminal_wrapped_them(cx: &mut TestAppContext) {
    let wide = format!("{}    tail", "x".repeat(100));
    let half = "h".repeat(60);
    let word = "L".repeat(50);
    let script = format!("printf '%s\\n%s\\n%s\\n' '{wide}' '{half}' '{word}'; exec cat");
    let mut pane = Pane::running(cx, &script);
    pane.wait_for(&word);
    let doc = pane
        .view
        .read_with(pane.cx, |v, cx| v.mirror_snapshot(cx).doc);
    let lines: Vec<&str> = doc.lines.iter().map(|l| l.text.as_str()).collect();
    assert!(
        lines.contains(&wide.as_str()),
        "the 108-character line wraps into an indented row on a 100-column pane, \
         and still reads whole: {lines:?}"
    );
    assert!(
        lines.contains(&half.as_str()) && lines.contains(&word.as_str()),
        "two lines the program ended stay two lines: {lines:?}"
    );
}

/// The person's toggle marks the pane off-limits and lifts the mark again, and
/// says so each time — the event the workspace saves the layout on, which is
/// how the mark outlives the window.
///
/// Mutation-tested: a toggle that forgets to emit fails this.
#[gpui::test]
fn the_off_limits_toggle_marks_and_unmarks_the_pane_and_says_so(cx: &mut TestAppContext) {
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    let said = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = said.clone();
    let view = pane.view.clone();
    pane.cx.update(|_, cx| {
        cx.subscribe(&view, move |_, _: &super::OffLimitsChanged, _| {
            seen.set(seen.get() + 1)
        })
        .detach();
    });
    assert!(
        !pane.read(|v| v.agent_access.is_off_limits()),
        "a pane starts visible to agents"
    );
    pane.view.update(pane.cx, |v, cx| {
        v.set_agent_access(crate::threshing_floor::AgentAccess::OffLimits, cx)
    });
    pane.redraw();
    assert!(pane.read(|v| v.agent_access.is_off_limits()));
    pane.view.update(pane.cx, |v, cx| {
        v.set_agent_access(crate::threshing_floor::AgentAccess::Open, cx)
    });
    pane.redraw();
    assert!(!pane.read(|v| v.agent_access.is_off_limits()));
    assert_eq!(said.get(), 2, "each change is announced, for the save");
}

/// Every [`super::CloseFocusRead`] the pane raises from now on, counted: the
/// pane asking the workspace to close the reader, which is what Escape in the
/// reader does once nothing in the document wants it.
fn reader_closes(pane: &mut Pane) -> std::rc::Rc<std::cell::Cell<usize>> {
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = count.clone();
    let view = pane.view.clone();
    pane.cx.update(|_, cx| {
        cx.subscribe(&view, move |_, _: &super::CloseFocusRead, _| {
            seen.set(seen.get() + 1)
        })
        .detach();
    });
    count
}

/// A pane opened to show a document lends the reader that very view: the
/// reader draws it at the reader's size, and the pane stops drawing it. It is
/// read there as on the pane — Ctrl+wheel zooms it and PageDown turns a page
/// the height of the reader's glass — and when the pane flips to its shell
/// mid-read, the reader reads the shell and the document goes home, at the
/// zoom it had before the reader zoomed it. Closing the reader gives it back
/// for good, at the place it was read to.
///
/// Before this, Alt+R on a document pane showed the shell hidden behind the
/// document. Parker, 2026-09-29: *"opening the reader for video, PDF, HTML,
/// images, and markdown files should work, and it doesn't."*
#[gpui::test]
fn a_document_face_lends_its_view_to_the_reader_and_gets_it_back(cx: &mut TestAppContext) {
    use crate::docview::ImageZoom;
    let dir = Scratch::new("reader-doc-face");
    let md = dir.join("long.md");
    // Paragraphs long enough to wrap onto more lines in the pane than in the
    // reader's wider glass, so the same pixel is a different block in each.
    // About 600 characters: each paragraph runs to several lines at either
    // width, so the 1,200-pixel glass's fewer lines per paragraph are certain,
    // whatever the font. Half this length wrapped alike in both, and a reader
    // that forgot to keep its place by block still passed.
    let words = "a document long enough to page through, whose paragraphs run on \
                 long enough to wrap onto more lines in a pane than in the reader";
    let text: String = (1..=120)
        .map(|i| format!("Paragraph {i}: {words}; {words}; {words}; and {words}.\n\n"))
        .collect();
    std::fs::write(&md, text).expect("the document");
    let mut pane = Pane::running(cx, "printf 'ready\\n'; exec cat");
    pane.wait_for("ready");
    pane.show_document(&md);
    let view = pane.face_view().expect("the document is on the face");
    assert_eq!(pane.face(), Face::Document);
    let zoom = |pane: &mut Pane| view.read_with(pane.cx, |v, _| v.zoom_now());
    let reading = |pane: &mut Pane| {
        view.read_with(pane.cx, |v, _| v.reading_now())
            .expect("the document is laid out and measured")
    };
    let block = |pane: &mut Pane| {
        view.update(pane.cx, |v, _| v.block_at_top())
            .expect("the document is laid out")
            .0
    };
    let closes = reader_closes(&mut pane);

    pane.keys("pagedown");
    pane.keys("pagedown");
    pane.redraw();
    let on_the_pane = block(&mut pane);
    assert!(on_the_pane > 0, "paged into the document on the pane");

    let mut reader = pane.open_reader((1200.0, 900.0));
    assert_eq!(
        pane.lent().map(|v| v.entity_id()),
        Some(view.entity_id()),
        "the reader reads the face's own view, not a second copy"
    );
    assert!(
        pane.read(|v| v.lends(&view)),
        "and the face no longer draws it"
    );
    reader.redraw(&mut pane);
    assert_eq!(
        view.read_with(pane.cx, |v, _| v.drawn_size()),
        Some(gpui::size(px(1200.), px(900.))),
        "it is drawn at the reader's size, and the pane draws it nowhere"
    );
    assert_eq!(
        block(&mut pane),
        on_the_pane,
        "the reader's wider glass re-flows the column, and the reader is still \
         on the block they were on in the pane"
    );

    let before = reading(&mut pane);
    pane.keys("pagedown");
    reader.redraw(&mut pane);
    let after = reading(&mut pane);
    let page = 0.9 * (before.bottom - before.top);
    assert!(
        (after.top - before.top - page).abs() < 1e-3,
        "PageDown turns nine tenths of the reader's glass: {before:?} → {after:?}"
    );
    assert_eq!(closes.get(), 0, "and leaves the reader open");

    // Ctrl+wheel over the reader, as the workspace hands it to the view.
    view.update(pane.cx, |v, cx| v.zoom_by_wheel(2.0, cx));
    reader.redraw(&mut pane);
    assert_eq!(
        zoom(&mut pane),
        Some(ImageZoom::Scale(1.25)),
        "two steps in"
    );

    pane.keys("alt-k");
    reader.redraw(&mut pane);
    assert_eq!(
        pane.face(),
        Face::Terminal,
        "alt+k flips the pane to its shell"
    );
    assert!(
        pane.lent().is_none(),
        "the reader reads the terminal now, and nothing is lent"
    );
    assert_eq!(
        zoom(&mut pane),
        Some(ImageZoom::Scale(1.0)),
        "the document went home at the pane's own zoom, not the reader's"
    );

    pane.keys("alt-k");
    reader.redraw(&mut pane);
    reader.redraw(&mut pane);
    assert_eq!(
        pane.lent().map(|v| v.entity_id()),
        Some(view.entity_id()),
        "back on the document face, the reader reads it again"
    );
    let read_to = block(&mut pane);
    assert!(read_to > on_the_pane, "a page further on, in the reader");

    reader.close(&mut pane);
    pane.redraw();
    assert!(pane.lent().is_none());
    assert!(
        !pane.read(|v| v.lends(&view)),
        "closed, the face draws its document again"
    );
    assert_eq!(block(&mut pane), read_to, "at the place it was read to");
}

/// A brief read through Alt+R takes notes in the reader exactly as it does on
/// its pane. With Alt held over it, a press inside an element opens that
/// element's note box; the keys write the note, and Ctrl+Enter adds it; Escape
/// puts the box away and leaves the reader open, and only a second Escape asks
/// to close the reader. A press on a decision's space stamps a CONCUR, and ↪
/// sends everything to the agent beside the brief, from the reader, as the
/// button on the pane does, saving it into the file on the way.
///
/// The reader's glass is stood in for ([`super::harness::GlassStandIn`]),
/// because it belongs to the workspace this harness does not build; what it
/// hands the view, a point relative to the view, is what the workspace hands.
///
/// Parker, approving the reader's design, 2026-09-29: *"Keying in on how a doc
/// allows comments.... Full concur"*.
#[gpui::test]
fn a_brief_in_the_reader_takes_notes_and_stamps_and_sends_them(cx: &mut TestAppContext) {
    use super::harness::{FakeDecisionEngine, CONCUR_SPACE};
    let (mut pane, _dir, brief, _png) = pane_showing_a_brief(cx, "reader-notes");
    pane.html_engine(Ok(std::sync::Arc::new(FakeDecisionEngine)));
    let at = pane.point_at(&brief);
    pane.click(at, Pane::alt());
    pane.redraw();
    let square = pane.float_view().expect("the brief opens in a square");
    assert_eq!(pane.doc_notes().expect("notes")["state"], "notes");
    let closes = reader_closes(&mut pane);
    let sent = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let log = sent.clone();
    let view = pane.view.clone();
    pane.cx.update(|_, cx| {
        cx.subscribe(&view, move |_, ev: &super::SendNotesBeside, _| {
            log.borrow_mut().push(ev.map.clone());
        })
        .detach();
    });

    let mut reader = pane.open_reader((1200.0, 800.0));
    assert_eq!(
        pane.lent().map(|v| v.entity_id()),
        Some(square.entity_id()),
        "a square over the terminal lends the reader its own view"
    );
    assert!(
        pane.read(|v| v.lends(&square)),
        "which the square no longer draws"
    );
    reader.wait(&mut pane, 300);
    assert_eq!(
        square.read_with(pane.cx, |v, _| v.drawn_size()),
        Some(gpui::size(px(1200.), px(800.))),
        "it is drawn at the reader's size, and the square draws it nowhere"
    );
    let laid = square
        .update(pane.cx, |v, _| v.page_laid_out())
        .expect("the brief is laid out in the reader");
    assert_eq!(laid.css_width, 1200, "at the reader's width");

    // The first element, at (28, 80) and 520 × 120 CSS pixels, is where the
    // reader draws it: the page starts at the view's top-left, at 100%.
    let inside = point(px(288.), px(140.));
    reader.hover(&mut pane, inside);
    pane.modifiers(Pane::alt());
    reader.redraw(&mut pane);
    assert!(
        pane.doc_notes().expect("notes")["boxes"]
            .as_u64()
            .is_some_and(|n| n >= 1),
        "Alt outlines what takes a note, in the reader"
    );
    reader.click(&mut pane, inside);
    pane.modifiers(Default::default());
    reader.redraw(&mut pane);
    let (nid, _) = BRIEF_ANCHORS[0];
    assert_eq!(
        pane.doc_notes().expect("notes")["open"],
        nid,
        "a press inside it opens its note box"
    );

    let place = square.read_with(pane.cx, |v, _| v.reading_now());
    assert!(place.is_some(), "the brief is laid out and scrolls");
    pane.keys("pagedown");
    reader.redraw(&mut pane);
    assert_eq!(
        square.read_with(pane.cx, |v, _| v.reading_now()),
        place,
        "while a note is being written a page key is the note's, and the page stays"
    );
    assert_eq!(pane.doc_notes().expect("notes")["open"], nid);

    pane.keys("k e e p space t h e space t i l e s");
    pane.keys("ctrl-enter");
    reader.redraw(&mut pane);
    let report = pane.doc_notes().expect("notes");
    assert_eq!(report["unsaved"], 0, "submission saved the note: {report}");
    assert!(
        std::fs::read_to_string(&brief)
            .unwrap()
            .contains("keep the tiles"),
        "the original HTML contains the comment before any send"
    );
    assert!(
        report["map"]
            .as_str()
            .is_some_and(|m| m.contains("keep the tiles")),
        "as typed: {report}"
    );

    pane.keys("escape");
    reader.redraw(&mut pane);
    assert_eq!(
        pane.doc_notes().expect("notes")["open"],
        serde_json::Value::Null,
        "Escape puts the note box away"
    );
    assert_eq!(closes.get(), 0, "and leaves the reader open");
    assert!(pane.lent().is_some());

    let space = point(
        px(CONCUR_SPACE.x + CONCUR_SPACE.w / 2.0),
        px(CONCUR_SPACE.y + CONCUR_SPACE.h / 2.0),
    );
    reader.click(&mut pane, space);
    let report = pane.doc_notes().expect("notes");
    assert_eq!(
        report["concurs"], 1,
        "a press on the space stamps it: {report}"
    );
    assert_eq!(report["unsaved"], 0, "the concurrence is saved immediately");

    // With nothing open, Escape asks for the reader to close. The square it
    // borrowed from is not the thing closing, and keeps every note, so it
    // does not stop to warn about notes it would lose. (No workspace here
    // acts on the ask, so the reader stays open for what follows.)
    pane.keys("escape");
    assert_eq!(closes.get(), 1, "Escape asks to close the reader");
    let report = pane.doc_notes().expect("notes");
    assert_eq!(report["unsaved"], 0, "both annotations are already on disk");

    // Who is beside the brief, as the workspace works it out every frame.
    pane.view.update(pane.cx, |v, cx| {
        v.set_notes_beside(Some("agent".into()), None, cx)
    });
    reader.redraw(&mut pane);
    let before_send = std::fs::read(&brief).unwrap();
    let before_mtime = std::fs::metadata(&brief).unwrap().modified().unwrap();
    pane.keys("ctrl-shift-enter");
    reader.redraw(&mut pane);
    assert_eq!(
        std::fs::read(&brief).unwrap(),
        before_send,
        "Send never rewrites the file"
    );
    assert_eq!(
        std::fs::metadata(&brief).unwrap().modified().unwrap(),
        before_mtime
    );
    {
        let sent = sent.borrow();
        assert_eq!(sent.len(), 1, "↪ from the reader sends once: {sent:?}");
        assert!(
            sent[0].contains("keep the tiles"),
            "the map carries the note: {}",
            sent[0]
        );
    }
    let file = std::fs::read_to_string(&brief).expect("the brief");
    let island = |id: &str| {
        file.split(&format!("id=\"{id}\""))
            .nth(1)
            .and_then(|rest| rest.split("</script>").next())
            .unwrap_or_default()
            .to_string()
    };
    assert!(
        island("report-notes").contains("keep the tiles"),
        "submission saved the note into the brief's notes island"
    );
    assert!(
        island("report-concurs").contains(nid),
        "and the stamp into its concurs island"
    );
    assert_eq!(pane.doc_notes().expect("notes")["unsaved"], 0);

    // Ctrl+wheel over the reader zooms the brief there; closed, the square
    // gets it back at its own zoom.
    let zoom = |pane: &mut Pane| square.read_with(pane.cx, |v, _| v.zoom_now());
    square.update(pane.cx, |v, cx| v.zoom_by_wheel(1.0, cx));
    assert_eq!(zoom(&mut pane), Some(crate::docview::ImageZoom::Scale(1.1)));
    pane.keys("escape");
    assert_eq!(closes.get(), 2, "Escape asks to close the reader again");
    reader.close(&mut pane);
    assert!(
        !pane.read(|v| v.lends(&square)),
        "closed, the square draws its brief again"
    );
    assert_eq!(
        zoom(&mut pane),
        Some(crate::docview::ImageZoom::Scale(1.0)),
        "at the square's own zoom"
    );
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&brief))
    );
}

/// What an agent handed over is one chord away, on a page over the pane, and
/// a plain click on its name there opens it into the pane in the page's place.
/// Opening it is what clears its mark, and the same chord on the page puts it
/// away again.
#[gpui::test]
fn a_handover_is_on_its_page_and_opens_into_the_pane_from_there(cx: &mut TestAppContext) {
    let dir = Scratch::new("handover");
    let report = dir.fixture("../README.md", "report.md");
    let report = report.to_str().expect("UTF-8").to_string();
    let mut pane = Pane::running(cx, "printf '%s\\n' 'ready'; exec cat");
    pane.wait_for("ready");
    assert!(!pane.read(|v| v.has_handovers()), "nothing handed over yet");

    // The verb, as the MCP path calls it.
    pane.view.update(pane.cx, |v, cx| {
        v.hand_over(
            Some("The report".into()),
            report.clone(),
            crate::handover::Source::Declared,
            cx,
        );
    });
    pane.redraw();
    assert!(pane.read(|v| v.has_handovers()));

    pane.keys("ctrl-shift-m");
    let page = pane.float_path().expect("the handover page, over the pane");
    assert!(page.ends_with(crate::handover::PAGE), "{page:?}");
    let text = std::fs::read_to_string(&page).expect("the page");
    assert!(text.contains("[The report]"), "{text}");
    assert!(text.contains("**1 new**"), "{text}");

    // A plain click on the name is the document's own link press.
    let view = pane.float_view().expect("the page's view");
    pane.follow(&view, &report);
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&report)),
        "the report takes the page's place"
    );

    // Opened now, so the page no longer marks it.
    pane.keys("ctrl-shift-m");
    assert_eq!(pane.float_path().as_deref(), Some(page.as_path()));
    let text = std::fs::read_to_string(&page).expect("the page");
    assert!(text.contains("all opened"), "{text}");

    // And the chord on the page puts it away.
    pane.keys("ctrl-shift-m");
    assert_eq!(pane.float_path(), None, "the page is toggled off");
}

/// Only a person replaces a square. An agent's `open_document` over a square
/// that is already up — the person's page, here a picture they Alt+clicked —
/// is held: the square still shows their page, the agent is answered `held`,
/// and the file is kept as the pane's handover, not yet opened, for the
/// artifact rail. With no square up, the same verb opens it. And the rail's
/// road, which is the person's, opens it over whatever is there.
///
/// Parker, 2026-10-22, after an agent's finished turn replaced the page he was
/// writing a note in: *"definitely will not override the currently open glow
/// pane"*.
#[gpui::test]
fn an_agents_open_never_replaces_a_square_the_person_has_up(cx: &mut TestAppContext) {
    let (mut pane, dir, png) = pane_showing_a_picture(cx, "held-open");
    let report = dir.fixture("../README.md", "report.md");
    let report = report.to_str().expect("UTF-8").to_string();
    let target =
        || crate::docopen::drawable_document(std::path::Path::new(&report)).expect("markdown");
    let mine = std::path::PathBuf::from(&png);

    // The person's page is up.
    let at = pane.point_at(&png);
    pane.click(at, Pane::alt());
    assert_eq!(pane.float_path(), Some(mine.clone()));

    // The agent's open is held.
    let said = pane.view.update(pane.cx, |v, cx| v.open_here(target(), cx));
    assert!(said.starts_with("held "), "{said}");
    assert_eq!(
        pane.float_path(),
        Some(mine.clone()),
        "the person's square is still the person's"
    );
    let key = crate::handover::key_of(&report);
    assert!(
        pane.read(|v| v.handovers().contains(&key)),
        "kept as a handover"
    );
    assert!(
        !pane.read(|v| v.handovers().is_opened(&key)),
        "and not opened: nobody has seen it"
    );
    // Asked again, it is held again and still kept once.
    let again = pane.view.update(pane.cx, |v, cx| v.open_here(target(), cx));
    assert!(again.starts_with("held "), "{again}");
    assert_eq!(pane.read(|v| v.handovers().len()), 1);

    // The rail's road is a person's: the newest handover opens over the square.
    pane.view
        .update(pane.cx, |v, cx| v.open_newest_handover(cx));
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&report))
    );
    assert!(
        pane.read(|v| v.handovers().is_opened(&key)),
        "and now it is opened"
    );

    // With no square up, the agent's open opens.
    pane.keys("escape");
    assert_eq!(pane.float_path(), None);
    let said = pane.view.update(pane.cx, |v, cx| v.open_here(target(), cx));
    assert!(said.starts_with("ok float"), "{said}");
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&report))
    );
    assert_eq!(desktop_launches(), Vec::<String>::new());
}

/// A square holding notes not yet saved is held like any other, and the agent
/// hears `held`, not the old `err` that read as a failure to retry.
#[gpui::test]
fn an_agents_open_over_unsaved_notes_is_held_and_keeps_the_notes(cx: &mut TestAppContext) {
    let (mut pane, _dir, brief, png) = pane_showing_a_brief(cx, "held-unsaved");
    open_the_brief_with_a_note(&mut pane, &brief, false);
    let target = crate::docopen::drawable_document(std::path::Path::new(&png)).expect("a picture");
    let said = pane.view.update(pane.cx, |v, cx| v.open_here(target, cx));
    assert!(said.starts_with("held "), "{said}");
    assert_eq!(
        pane.float_path().as_deref(),
        Some(std::path::Path::new(&brief))
    );
    assert_eq!(
        pane.doc_notes().expect("notes")["unsaved"],
        1,
        "the note is still there"
    );
}

/// A `Deliverable:` line an agent printed is read off the pane's real grid,
/// including the link Claude Code wraps at the pane's edge and carries on two
/// spaces in. These are the rows of the HC Video pane on 2026-10-06, at that
/// pane's own width of 100 columns, which is also this harness's.
#[gpui::test]
fn a_printed_handover_is_read_off_the_screen_and_becomes_the_needs_me_link(
    cx: &mut TestAppContext,
) {
    let first = "  Deliverable: Club HC VIDEO spec — file:///home/parker/BROWN-FAMILY-SPORTS/hc-video/software/hc-vid";
    let rest = "  eo-web/docs/plans/club-hc-video/2026-09-29-club-hc-video.html";
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' '{first}' '{rest}' 'ready'; exec cat"),
    );
    pane.wait_for("ready");
    let href = "file:///home/parker/BROWN-FAMILY-SPORTS/hc-video/software/hc-video-web/docs/plans/club-hc-video/2026-09-29-club-hc-video.html";

    // A shell pane is not read: only an agent hands things over.
    pane.view.update(pane.cx, |v, cx| v.scan_handovers(cx));
    assert!(!pane.read(|v| v.has_handovers()));

    pane.view.update(pane.cx, |v, cx| {
        v.mode = super::PaneMode::Claude;
        v.scan_handovers(cx);
    });
    let newest = pane.read(|v| v.handovers.newest().cloned());
    let newest = newest.expect("the line was read");
    assert_eq!(newest.label, "Club HC VIDEO spec");
    assert_eq!(newest.href, href, "the link, joined across the wrap");
    assert_eq!(newest.source, crate::handover::Source::Said);
    let link = pane.read(|v| v.deliverable()).expect("the needs-me link");
    assert_eq!(link.href, href);

    // Read again a second later, it is the same handover, not a second one.
    pane.view.update(pane.cx, |v, cx| v.scan_handovers(cx));
    assert_eq!(pane.read(|v| v.handovers.newest_first().len()), 1);
}

/// A link longer than the pane, printed as one line, is wrapped by the
/// terminal itself and carried on at the left edge with the row flagged. An
/// agent that does not draw fullscreen hands things over in this shape, and
/// the link is read whole off the real grid rather than cut at the edge.
#[gpui::test]
fn a_link_the_terminal_itself_wrapped_is_read_whole(cx: &mut TestAppContext) {
    let href = "file:///home/parker/Work/cinema-delight/campaigns/2026-10-05-terminal-delight-movements/sound/README.md";
    assert!(
        href.len() + 2 > 100,
        "longer than the harness's 100 columns"
    );
    let mut pane = Pane::running(
        cx,
        &format!(
            "printf '%s\\n' '  Deliverable: the four voicings —' '  {href}' 'ready'; exec cat"
        ),
    );
    pane.wait_for("ready");
    pane.view.update(pane.cx, |v, cx| {
        v.mode = super::PaneMode::Claude;
        v.scan_handovers(cx);
    });
    let newest = pane.read(|v| v.handovers.newest().cloned());
    assert_eq!(newest.expect("read off the screen").href, href);
}

/// An agent that declares a file and then presents its own artifact for it —
/// the order the house rules ask for at the end of a turn — leaves one card for
/// that file on its shelf, its own, and one row in the list.
#[gpui::test]
fn the_agents_own_card_for_a_file_replaces_the_one_filed_for_its_declaration(
    cx: &mut TestAppContext,
) {
    let mut pane = Pane::running(cx, "printf '%s\\n' 'ready'; exec cat");
    pane.wait_for("ready");
    let href = "/tmp/td-handover-test/plan.html";
    pane.view.update(pane.cx, |v, cx| {
        v.hand_over(
            Some("Plan".into()),
            href.into(),
            crate::handover::Source::Declared,
            cx,
        );
    });
    let cards = |pane: &mut Pane| {
        pane.read(|v| {
            v.bench
                .artifacts()
                .map(|(id, _)| id.0.clone())
                .collect::<Vec<_>>()
        })
    };
    let filed = cards(&mut pane);
    assert_eq!(filed.len(), 1);
    assert!(filed[0].starts_with("handover-"), "{filed:?}");

    let theirs = serde_json::json!({
        "td": "0.4", "kind": "artifact", "id": "plan", "title": "Plan, with a summary",
        "model": { "href": format!("file://{href}"), "summary": "four decisions" }
    });
    pane.view.update(pane.cx, |v, cx| {
        v.present(crate::surface::parse_lenient(&theirs, 1, "plan"), cx);
    });
    assert_eq!(
        cards(&mut pane),
        vec!["plan".to_string()],
        "only the agent's own card"
    );
    let rows = pane.read(|v| v.handovers.newest_first().len());
    assert_eq!(rows, 1, "one file, one row");
    let kept = pane
        .read(|v| v.handovers.newest().cloned())
        .expect("the row");
    assert_eq!(
        kept.source,
        crate::handover::Source::Declared,
        "the stronger word stays"
    );
}

/// A `Deliverable:` line the derive sweep read out of the transcript before
/// the screen scan saw it is the same printed handover, and gets what the scan
/// gives one: the word `said`, the needs-me link, and a record of its own.
///
/// The record is the part that was missing. Derived cards are never filed and
/// the sweep re-reads only the newest line, so a restart kept the newest and
/// forgot the rest — the M2M packet on 2026-10-07 was on its conversation's
/// page that day and in no record (#909).
#[gpui::test]
fn a_deliverable_line_read_from_the_transcript_is_kept_like_one_read_off_the_screen(
    cx: &mut TestAppContext,
) {
    let mut pane = Pane::running(cx, "printf '%s\\n' 'ready'; exec cat");
    pane.wait_for("ready");
    let href = "/tmp/td-handover-test/packet.pdf";
    let transcript = serde_json::json!({ "type": "assistant", "message": { "content": [
        { "type": "text", "text": format!("Done.\n\nDeliverable: Application packet — file://{href}\n") }
    ] } })
    .to_string();
    let derived = || {
        crate::derive::walk(&transcript, 1)
            .posts
            .into_iter()
            .find(|p| {
                p.surface
                    .as_ref()
                    .is_some_and(crate::derive::is_deliverable_line)
            })
            .expect("derive reads the line")
    };
    let post = derived();
    pane.view.update(pane.cx, |v, cx| {
        v.mode = super::PaneMode::Claude;
        v.present(post, cx);
    });

    let kept = pane.read(|v| v.handovers.newest().cloned());
    let kept = kept.expect("the row");
    assert_eq!(kept.href, href);
    assert_eq!(
        kept.source,
        crate::handover::Source::Said,
        "printed, not presented"
    );
    let link = pane.read(|v| v.deliverable()).expect("the needs-me link");
    assert_eq!(link.href, href);

    // The restart: the list rebuilt from what the pane filed and nothing else,
    // as `handovers_from_record` rebuilds it from the conversation's record.
    // This pane has no conversation, so what it filed is held, not written.
    let rebuilt = |pane: &mut Pane| {
        pane.read(|v| {
            v.wb_unfiled
                .iter()
                .filter_map(|rec| match rec {
                    crate::benchstore::Rec::Said { at_ms, surface, .. } => {
                        crate::handover::from_surface(surface, *at_ms)
                    }
                    _ => None,
                })
                .map(|h| (h.key(), h.source))
                .collect::<Vec<_>>()
        })
    };
    let said = vec![(crate::handover::key_of(href), crate::handover::Source::Said)];
    assert_eq!(rebuilt(&mut pane), said, "filed, so a restart keeps it");

    // One card on the shelf, the derived one: filed is not drawn.
    let cards = pane.read(|v| {
        v.bench
            .artifacts()
            .map(|(id, _)| id.0.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert!(cards[0].starts_with("deliv-"), "{cards:?}");

    // The next sweep re-derives the same post. Nothing is filed twice.
    let again = derived();
    pane.view.update(pane.cx, |v, cx| v.present(again, cx));
    assert_eq!(rebuilt(&mut pane), said, "the same line, filed once");
}

/// A paste while the pane's title is being renamed lands in the name, by each
/// of the three chords, and never in the terminal under the header.
///
/// Right-click or a double-click on the title opens the box, and the box owns
/// the keyboard until it commits. Ctrl+Shift+V was still the pane's paste, so
/// the clipboard was typed into whatever was running behind the header. A
/// newline in that paste is a return. Ctrl+V and Shift+Insert reached the box
/// and vanished, because a modified key is not taken as a character. A name is
/// one line, so the line break in the last paste lands as a space.
#[gpui::test]
fn a_header_being_renamed_takes_a_paste_instead_of_the_shell(cx: &mut TestAppContext) {
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    pane.settle();
    pane.view.update(pane.cx, |v, _cx| {
        v.renaming = Some(String::new());
    });
    assert_eq!(pane.read(|v| v.renaming.clone()).as_deref(), Some(""));

    paste_each(
        &mut pane,
        &[
            ("ctrl-shift-v", "qx"),
            ("ctrl-v", " qy"),
            ("shift-insert", "\nqz"),
        ],
    );
    let draft = pane.read(|v| v.renaming.clone());
    pane.keys("enter");
    assert!(
        pane.read(|v| v.renaming.is_none()),
        "enter commits the name and hands the keyboard back"
    );
    pane.keys("e c h o e d enter");
    pane.wait_for("echoed");
    let rows = pane.rows();
    assert_eq!(
        draft.as_deref(),
        Some("qx qy qz"),
        "all three chords paste into the name, in order, the line break a space; shell saw:\n{}",
        rows.join("\n")
    );
    assert!(
        !rows
            .iter()
            .any(|r| r.contains("qx") || r.contains("qy") || r.contains("qz")),
        "nothing pasted reached the shell under the header:\n{}",
        rows.join("\n")
    );
}

/// The terminal and bench ask the workspace for the same launcher. The key
/// never reaches the running program and opening it does not flip the face.
#[gpui::test]
fn terminal_launcher_alt_a_and_header_share_the_bench_event(cx: &mut TestAppContext) {
    let mut pane = Pane::running(cx, "echo ready; exec cat");
    pane.wait_for("ready");
    pane.settle();
    let launches = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = launches.clone();
    pane.cx.update(|_, cx| {
        cx.subscribe(&pane.view, move |view, _: &super::OpenAgentLauncher, cx| {
            log.borrow_mut()
                .push((view.entity_id(), view.read(cx).bench.face()));
        })
        .detach();
    });
    let id = pane.view.entity_id();
    for face in [Face::Terminal, Face::Workbench] {
        pane.view.update(pane.cx, |v, cx| v.set_face(face, cx));
        pane.redraw();
        pane.keys("alt-a");
        assert_eq!(pane.face(), face);
        let bounds = pane
            .cx
            .debug_bounds("launch-agent")
            .expect("header button is drawn");
        pane.click(bounds.center(), gpui::Modifiers::default());
        assert_eq!(pane.face(), face);
    }
    assert_eq!(
        *launches.borrow(),
        vec![
            (id, Face::Terminal),
            (id, Face::Terminal),
            (id, Face::Workbench),
            (id, Face::Workbench)
        ],
        "each gesture emits exactly once, naming its own pane and face"
    );
    pane.view
        .update(pane.cx, |v, cx| v.set_face(Face::Terminal, cx));
    // The workspace returns focus after closing its popup. This harness only
    // records the launch event, so return it explicitly before checking PTY input.
    pane.cx.update(|window, cx| {
        let focus = pane.view.read(cx).focus_handle.clone();
        window.focus(&focus, cx);
    });
    pane.redraw();
    pane.keys("z enter");
    pane.wait_for("z");
    let rows = pane.rows().join("\n");
    assert!(!rows.contains("^[a"), "Alt+A leaked into the PTY: {rows}");
    assert!(
        !pane.read(|v| v.wb_compose.is_some()),
        "the launcher leaves no composer draft"
    );
}

/// The strip stops saying an agent is reading an answer once that agent has
/// left, and the next agent in the pane does not inherit the stamp.
///
/// The bench stamps the moment it types. For eight seconds that stamp makes
/// the strip say "Reading your answer", and that rung outranks a question, so
/// a question still on screen is not shown again. The stamp names the agent
/// that was written to. Departure already drops every other latch that names
/// that conversation. This one was left, so a pane whose agent has gone keeps
/// the words until the window ends, and an agent started inside the window is
/// born reading an answer nobody gave it.
#[gpui::test]
fn a_departed_agent_does_not_leave_the_bar_reading_an_answer(cx: &mut TestAppContext) {
    let mut pane = Pane::running(cx, "exec cat");
    pane.running_as(crate::pane::PaneMode::Claude);
    pane.view.update(pane.cx, |v, cx| {
        let now = crate::surfacefeed::now_ms();
        v.wb_delivered_ms = Some(now.saturating_sub(1_000));
        assert!(v.reading_answer(now), "the bench typed a second ago");
        assert_eq!(
            v.bench_status(),
            crate::workbench::AgentState::Reading,
            "the strip says the agent is reading that answer"
        );
        v.set_mode(crate::pane::PaneMode::Shell, cx);
        assert!(
            !v.reading_answer(now),
            "the agent that was reading has left"
        );
        assert_ne!(
            v.bench_status(),
            crate::workbench::AgentState::Reading,
            "a shell does not keep reading an answer"
        );
    });
    pane.running_as(crate::pane::PaneMode::Claude);
    let (reading, state) = pane.read(|v| {
        let now = crate::surfacefeed::now_ms();
        (v.reading_answer(now), v.bench_status())
    });
    assert!(!reading, "the next agent does not inherit the stamp");
    assert_ne!(
        state,
        crate::workbench::AgentState::Reading,
        "the next agent is not born reading the answer the last one was given"
    );
}

/// A real line editor at a prompt, on the alternate screen when `alt`:
/// readline with no rc file and no history file, so what it does with a key
/// is readline's default and nothing on this machine.
fn pane_at_a_prompt(cx: &mut TestAppContext, alt: bool) -> Pane {
    let screen = if alt { "printf '\\033[?1049h'; " } else { "" };
    let mut pane = Pane::running(
        cx,
        &format!(
            "{screen}TERM=xterm-256color INPUTRC=/dev/null HISTFILE= PS1='ready> ' exec bash --norc --noprofile -i"
        ),
    );
    pane.wait_for("ready>");
    pane.settle();
    pane
}

/// Typed at the prompt, one key per character, and echoed back.
fn type_line(pane: &mut Pane, line: &str) {
    let keys: Vec<String> = line
        .chars()
        .map(|c| {
            if c == ' ' {
                "space".to_string()
            } else {
                c.to_string()
            }
        })
        .collect();
    pane.keys(&keys.join(" "));
    pane.wait_for(&format!("ready> {line}"));
}

/// Backspace over a word selected with the mouse, in the middle of the line
/// being typed, takes the whole word — not the one character before the
/// caret, which is what it took before.
#[gpui::test]
fn backspace_over_a_mouse_selection_erases_it_from_the_line(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    type_line(&mut pane, "echo hello world foo");

    // From the left quarter of the w to the right quarter of the d.
    let cell_w = pane.read(|v| v.cell_w);
    let w = pane.point_at("w");
    let d = pane.point_at("ld");
    pane.press(point(w.x - px(cell_w / 4.0), w.y));
    pane.drag_to(point(d.x + px(cell_w / 4.0), d.y));
    pane.release(point(d.x + px(cell_w / 4.0), d.y));
    assert_eq!(
        pane.read(|v| v.selection_clipboard_text()).as_deref(),
        Some("world"),
        "the drag selects the word"
    );

    pane.keys("backspace");
    pane.wait_for("ready> echo hello  foo");
    assert!(
        !pane.read(|v| v.has_selection()),
        "the selection went with the text"
    );
    pane.keys("enter");
    pane.wait_for_row("hello foo");
}

/// Ctrl+Shift+Left at the end of the line selects the last word and the blank
/// cell the caret sits on; Delete takes the word and leaves the space before
/// it.
#[gpui::test]
fn delete_over_a_keyboard_selection_takes_the_word_and_not_the_space(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    type_line(&mut pane, "echo abc def");
    pane.keys("ctrl-shift-left");
    assert!(pane.read(|v| v.has_selection()), "the word is selected");

    pane.keys("delete");
    pane.keys("z enter");
    pane.wait_for("ready> echo abc z");
    pane.wait_for_row("abc z");
}

/// Claude Code draws its prompt on the alternate screen, so an agent pane
/// erases a selection there; a full-screen program that is not an agent —
/// `less`, `vim` — is not a line being typed, and Backspace stays one key.
#[gpui::test]
fn the_alternate_screen_erases_a_selection_only_in_an_agent_pane(cx: &mut TestAppContext) {
    let mut agent = pane_at_a_prompt(cx, true);
    agent.running_as(crate::pane::PaneMode::Claude);
    type_line(&mut agent, "echo abc def");
    agent.keys("ctrl-shift-left backspace z enter");
    agent.wait_for("ready> echo abc z");
    agent.wait_for_row("abc z");

    let mut other = pane_at_a_prompt(cx, true);
    type_line(&mut other, "echo abc def");
    other.keys("ctrl-shift-left backspace z enter");
    other.wait_for("ready> echo abc dez");
    other.wait_for_row("abc dez");
}

#[gpui::test]
fn backspace_erases_a_mouse_selection_across_a_soft_wrap(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    let cols = pane.read(|v| v.session.term.lock().columns());
    let prefix = "x".repeat(cols - 35);
    let selected = format!("BEGIN{}END", "y".repeat(30));
    let line = format!("echo {prefix} {selected} tail");
    pane.read(|v| v.paste_text(&line));
    pane.wait_for("END tail");
    pane.settle();
    let cell_w = pane.read(|v| v.cell_w);
    let begin = pane.point_at("BEGIN");
    let end = pane.point_at("END tail");
    assert_ne!(begin.y, end.y, "fixture must cross a real grid wrap");
    pane.press(point(begin.x - px(cell_w * 2.25), begin.y));
    let last = point(end.x - px(cell_w * 1.75), end.y);
    pane.drag_to(last);
    pane.release(last);
    assert_eq!(pane.read(|v| v.selection_clipboard_text()), Some(selected));
    pane.keys("backspace");
    pane.keys("enter");
    pane.wait_for_row(&format!("{prefix} tail"));
}

#[gpui::test]
fn keyboard_selection_and_backspace_cross_every_soft_wrap(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    let word = "WRAPPED".repeat(30);
    pane.read(|v| v.paste_text(&format!("keep {word}")));
    pane.wait_for("WRAPPED");
    pane.settle();
    pane.keys(&vec!["shift-left"; word.len()].join(" "));
    assert_eq!(
        pane.read(|v| v.selection_clipboard_text())
            .unwrap()
            .trim_end(),
        word
    );
    pane.keys("backspace");
    pane.wait_for_row("ready> keep");
    assert!(!pane.rows().iter().any(|r| r.contains("WRAPPED")));
}

#[gpui::test]
fn shift_backspace_deletes_the_previous_word_at_a_real_prompt(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    type_line(&mut pane, "echo first second");
    pane.keys("shift-backspace z enter");
    pane.wait_for_row("first z");
}

#[gpui::test]
fn control_shift_backspace_erases_both_sides_of_a_wrapped_line(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    let text = format!("echo {} END", "word ".repeat(30));
    pane.read(|v| v.paste_text(&text));
    pane.wait_for("END");
    pane.keys("left left ctrl-shift-backspace");
    pane.wait_for_row("ready>");
    assert!(!pane.rows().iter().any(|r| r.contains("word")));
    type_line(&mut pane, "echo kept");
    pane.keys("enter");
    pane.wait_for_row("kept");
}

#[gpui::test]
fn the_new_shortcuts_distinguish_a_line_from_the_entire_multiline_input(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    pane.read(|v| v.paste_text("first ONE\nsecond TWO\nthird THREE"));
    pane.wait_for("third THREE");
    pane.keys("ctrl-shift-backspace");
    pane.keys("z");
    pane.wait_for_row("z");
    let rows = pane.rows().join("\n");
    assert!(
        rows.contains("first ONE") && rows.contains("second TWO"),
        "{rows}"
    );
    assert!(!rows.contains("third THREE"), "{rows}");
    pane.keys("ctrl-alt-shift-backspace");
    pane.wait_for_row("ready>");
    let rows = pane.rows().join("\n");
    assert!(!rows.contains("ONE") && !rows.contains("TWO"), "{rows}");
}

#[gpui::test]
fn backspace_erases_selected_literal_newlines_and_keeps_the_suffix(cx: &mut TestAppContext) {
    let mut pane = pane_at_a_prompt(cx, false);
    pane.read(|v| v.paste_text("prefix\nSELECTONE\nSELECTTWO suffix"));
    pane.wait_for("SELECTTWO suffix");
    pane.settle();
    let cell_w = pane.read(|v| v.cell_w);
    let start = pane.point_at("SELECTONE");
    let end = pane.point_at("SELECTTWO");
    let start = point(start.x - px(cell_w * 4.25), start.y);
    let end = point(end.x + px(cell_w * 4.25), end.y);
    pane.press(start);
    pane.drag_to(end);
    pane.release(end);
    assert_eq!(
        pane.read(|v| v.selection_clipboard_text()).as_deref(),
        Some("SELECTONE\nSELECTTWO")
    );
    pane.keys("backspace");
    pane.wait_for_row(" suffix");
    let rows = pane.rows().join("\n");
    assert!(rows.contains("ready> prefix"), "{rows}");
    assert!(!rows.contains("SELECT"), "{rows}");
}

/// Exercise the same owner seam used when the workspace ends its last leaf.
#[gpui::test]
fn ending_a_window_owned_terminal_releases_its_child_without_closing_the_window(
    cx: &mut TestAppContext,
) {
    let mut pane = Pane::running(cx, "trap '' HUP; echo ready; while :; do sleep 1; done");
    pane.wait_for("ready");
    let pid = pane.read(|v| v.session.shell_pid).expect("an owned child");
    pane.read(|v| v.close_owned_terminal());
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::path::Path::new(&format!("/proc/{pid}")).exists()
        && std::time::Instant::now() < until
    {
        pane.redraw();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "the owned child was reaped"
    );
    pane.redraw();
}

/// A real note entered through the native document view carries its creation
/// evidence through a file rewrite and a fresh opening of that view.
#[gpui::test]
fn a_native_markdown_note_reopens_on_moved_words_with_its_original_revision(
    cx: &mut TestAppContext,
) {
    use crate::docview::markdown::{NOTE_GUTTER, PAD};
    use crate::docview::notes_ui::BUTTON_CSS;
    let dir = Scratch::new("md-evidence-reopen");
    let root = dir.join("notes");
    let md = dir.join("plan.md");
    let original = "The original passage needs a second look.\n\nA different closing passage.\n";
    std::fs::write(&md, original).unwrap();
    let mut pane = Pane::running(
        cx,
        &format!("printf '%s\\n' 'doc {}' 'ready'; exec cat", md.display()),
    );
    pane.notes_root(root);
    pane.wait_for("ready");
    pane.settle();
    let at = pane.point_at(md.to_str().unwrap());
    pane.click(at, Pane::alt());
    pane.redraw();
    let (x, y, w, _) = pane.float_zone(FloatHit::Body).unwrap();
    pane.hover(point(px(x + 60.), px(y + PAD + 6.)));
    pane.redraw();
    pane.click(
        point(
            px(x + w - PAD - NOTE_GUTTER / 2.),
            px(y + PAD + BUTTON_CSS / 2.),
        ),
        Default::default(),
    );
    pane.keys("r e v i s e ctrl-enter");
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        pane.redraw();
        let report = pane.doc_notes().unwrap();
        if report["notes"] == 1 && report["unsaved"] == 0 {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "note did not save: {report}"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    pane.view.update(pane.cx, |v, cx| {
        v.close_float(cx);
    });
    std::fs::write(&md, format!("A new opening.\n\n{original}")).unwrap();
    let at = pane.point_at(md.to_str().unwrap());
    pane.click(at, Pane::alt());
    pane.redraw();
    let report = pane.doc_notes().unwrap();
    let map = report["map"].as_str().unwrap();
    assert!(map.contains("[L3]"), "{map}");
    assert!(map.contains("revise"), "{map}");
    assert!(
        map.contains("The original passage needs a second look."),
        "{map}"
    );
    assert!(
        map.contains(&crate::artifact_archive::digest(original.as_bytes())),
        "{map}"
    );
    assert!(!map.contains("md-unresolved-"), "{map}");
}
