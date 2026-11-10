//! Replays of the overlay-stack model's counterexamples against the real
//! window (`verify/overlay-stack/`). Each test states the PROMISE and is
//! ignored while the code broke it; since 2026-11-10 each holds and runs
//! with the rest of the suite (`cargo test --locked verify_overlay`).

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
/// chord raises is the one on top. Before 2026-11-10 the composer opened
/// inside the screen box, drawn under the Settings book; now raising it puts
/// the book away.
#[gpui::test]
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
/// and Esc closes it. Before 2026-11-10 the composer opened under the design
/// menu, took the letter, and was what Esc closed; the menu stayed. Now
/// raising the composer puts the menu away.
#[gpui::test]
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
    // Hidden: the composer took the letter while the menu was drawn over it.
    let typed_into_hidden = ws.read_with(cx, |ws, _| {
        ws.theme_menu.is_some()
            && ws
                .composer
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

/// RaisedIsOnTop, path: Ctrl+, then Ctrl+Shift+D. A pane's design menu is
/// drawn under the Settings book, so raising it puts the book away rather
/// than opening behind it.
#[gpui::test]
fn verify_overlay_a_panes_design_menu_never_opens_behind_the_book(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let (ws, cx) = window(cx);
    redraw(cx);
    cx.simulate_keystrokes("ctrl-,");
    redraw(cx);
    assert!(
        ws.read_with(cx, |ws, _| ws.settings.is_some()),
        "the book is up"
    );
    cx.simulate_keystrokes("ctrl-shift-d");
    redraw(cx);
    let (book, menu) = ws.read_with(cx, |ws, _| (ws.settings.is_some(), ws.theme_menu.is_some()));
    assert!(
        menu,
        "Ctrl+Shift+D reached the pane and raised its design menu"
    );
    assert!(
        !book,
        "the menu was raised while the book stays drawn over it"
    );
}
