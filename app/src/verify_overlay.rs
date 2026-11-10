//! Replays of the overlay-stack model's counterexamples against the real
//! window (`verify/overlay-stack/`). Each test states the PROMISE and is
//! ignored while the current code breaks it: run them with
//! `cargo test --locked -- --ignored verify_overlay` to see the finding, and
//! drop the `#[ignore]` when a change makes the promise hold.

use super::*;

fn window(
    cx: &mut gpui::TestAppContext,
) -> (gpui::Entity<Workspace>, &mut gpui::VisualTestContext) {
    cx.executor().allow_parking();
    cx.update(|cx| {
        theme::init_embedded(cx, theme::house_outer());
        skin::init_embedded(cx);
    });
    let seed = session::PaneRestore {
        program: Some(("/bin/sh".into(), vec!["-c".into(), "exec /bin/cat".into()])),
        ..Default::default()
    };
    let window = {
        let _fork = crate::testsync::forks_and_locks();
        cx.open_window(size(px(1400.), px(900.)), |w, cx| {
            Workspace::build(true, true, Some(seed), None, w, cx)
        })
    };
    let ws = window.root(cx).unwrap();
    let cx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
    let stops: Vec<_> = ws
        .read_with(cx, |ws, _| ws.every_view())
        .into_iter()
        .map(|p| p.read_with(cx, |v, _| v.fixture_shutdown()))
        .collect();
    cx.on_quit(move || stops.into_iter().for_each(|s| s()));
    cx.update(|w, cx| ws.update(cx, |ws, cx| ws.focus_active(w, cx)));
    (ws, cx)
}

fn redraw(cx: &mut gpui::VisualTestContext) {
    for _ in 0..2 {
        cx.update(|w, _| w.refresh());
        cx.run_until_parked();
    }
}

/// RaisedIsOnTop, path: Ctrl+, then Ctrl+Shift+E. The promise: a surface a
/// chord raises is the one on top. Today the composer opens inside the
/// screen box, drawn under the Settings book.
#[gpui::test]
#[ignore = "finding: the composer opens behind Settings (verify/overlay-stack)"]
fn verify_overlay_a_chord_never_raises_a_surface_behind_the_book(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    cx.simulate_keystrokes("ctrl-,");
    redraw(cx);
    cx.simulate_keystrokes("ctrl-shift-e");
    redraw(cx);
    let (book, composer) = ws.read_with(cx, |ws, _| (ws.settings.is_some(), ws.composer.is_some()));
    assert!(
        !(book && composer),
        "the composer was raised while Settings stays drawn over it"
    );
}

/// EscClosesTheTop and TypingGoesToTheTop, path: Ctrl+Shift+D, Ctrl+Shift+E,
/// then a letter and Esc. The promise: the letter goes to the surface on top
/// and Esc closes it. Today the composer opens under the design menu, takes
/// the letter, and is what Esc closes; the menu stays.
#[gpui::test]
#[ignore = "finding: under the design menu the hidden composer takes typing and Esc (verify/overlay-stack)"]
fn verify_overlay_esc_and_typing_reach_the_surface_on_top(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    cx.simulate_keystrokes("ctrl-shift-d");
    redraw(cx);
    assert!(
        ws.read_with(cx, |ws, _| ws.theme_menu.is_some()),
        "Ctrl+Shift+D opens the pane's design menu"
    );
    cx.simulate_keystrokes("ctrl-shift-e");
    redraw(cx);
    cx.simulate_keystrokes("q");
    redraw(cx);
    let typed_into_hidden = ws.read_with(cx, |ws, _| {
        ws.composer
            .as_ref()
            .is_some_and(|c| c.text.text().contains('q'))
    });
    cx.simulate_keystrokes("escape");
    redraw(cx);
    let (menu, composer) =
        ws.read_with(cx, |ws, _| (ws.theme_menu.is_some(), ws.composer.is_some()));
    assert!(
        !typed_into_hidden,
        "a letter went to the composer drawn under the design menu"
    );
    assert!(
        !(menu && !composer),
        "Esc closed the composer under the menu and left the menu on top"
    );
}
