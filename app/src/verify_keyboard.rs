//! Keyboard ownership, verified on the real window.
//!
//! The promise: **a letter typed into a surface that shows a text field goes
//! to that field and never to the shell behind it.** A break sends a person's
//! words to a terminal they were not looking at — into a shell prompt, or into
//! an agent's prompt as an instruction. Tests of one surface each rarely reach
//! it, because the leak depends on how the surface was opened: from a focused
//! pane, the pane keeps the focus, and a surface that reads its keys in the
//! bubble phase never hears them (2026-11-09, the Settings search).
//!
//! Every case opens a surface the way a person does, from a focused pane
//! running `cat`, types a marker, and checks both ends: the surface's field
//! holds the marker, and the shell's screen does not. The table is the
//! coverage record; `verify/keyboard/SKILL.md` says how to keep it in step.

use super::*;

/// One surface that takes typing.
struct Case {
    name: &'static str,
    /// Open it the way a person does, with the pane holding the focus.
    open: fn(&gpui::Entity<Workspace>, &mut gpui::VisualTestContext),
    /// What the surface's own field holds now, or `None` when it is not open.
    field: fn(&Workspace) -> Option<String>,
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is not drawn"));
    cx.simulate_click(at.center(), gpui::Modifiers::none());
}

fn redraw(cx: &mut gpui::VisualTestContext) {
    for _ in 0..2 {
        cx.update(|w, _| w.refresh());
        cx.run_until_parked();
    }
}

const CASES: &[Case] = &[
    Case {
        name: "Settings search (Ctrl+,)",
        open: |_, cx| cx.simulate_keystrokes("ctrl-,"),
        field: |ws| ws.settings.as_ref().map(|b| b.query.clone()),
    },
    Case {
        name: "the keys sheet's filter (F1)",
        open: |_, cx| cx.simulate_keystrokes("f1"),
        field: |ws| ws.help_open.then(|| ws.help_query.text()),
    },
    Case {
        name: "the language picker, from the keys sheet's 🌐",
        open: |_, cx| {
            cx.simulate_keystrokes("f1");
            redraw(cx);
            click(cx, "help-lang");
        },
        field: |ws| ws.lang_picker.as_ref().map(|p| p.query.text()),
    },
    Case {
        name: "the language picker, from Settings › General",
        open: |_, cx| {
            cx.simulate_keystrokes("ctrl-,");
            redraw(cx);
            click(cx, "st-open-language");
        },
        field: |ws| ws.lang_picker.as_ref().map(|p| p.query.text()),
    },
    Case {
        name: "the composer (Ctrl+Shift+E)",
        open: |_, cx| cx.simulate_keystrokes("ctrl-shift-e"),
        field: |ws| ws.composer.as_ref().map(|c| c.text.text()),
    },
    Case {
        name: "find across panes (Ctrl+Shift+F)",
        open: |_, cx| cx.simulate_keystrokes("ctrl-shift-f"),
        field: |ws| ws.find.as_ref().map(|f| f.query.text()),
    },
    Case {
        name: "the agent launcher (Alt+A)",
        open: |_, cx| cx.simulate_keystrokes("alt-a"),
        field: |ws| ws.agent_launcher.as_ref().map(|l| l.query.text()),
    },
    Case {
        name: "look up a project (Workspace::open_lookup)",
        open: |ws, cx| cx.update(|w, cx| ws.update(cx, |ws, cx| ws.open_lookup(w, cx))),
        field: |ws| ws.lookup.as_ref().map(|l| l.query.text()),
    },
];

/// The marker: letters a shell echoes as they are and no surface's own
/// chords answer to.
const MARK: &str = "qzx";

/// Every case, from a focused pane: the field holds the marker and the shell
/// does not. A case that fails names the surface and which end broke.
///
/// Mutation-tested: dropping the Settings capture call, or taking
/// `settings` out of `overlay_owns_keyboard`, fails the first case.
#[gpui::test]
fn every_surface_that_takes_typing_keeps_it_from_the_shell(cx: &mut gpui::TestAppContext) {
    let _warp = crate::warp::WARP_SERIAL
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    cx.executor().allow_parking();
    cx.update(|cx| {
        theme::init_embedded(cx, theme::house_outer());
        skin::init_embedded(cx);
    });
    let mut broken: Vec<String> = Vec::new();
    for case in CASES {
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
        let vcx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
        let pane = ws.read_with(vcx, |ws, _| ws.every_view()[0].clone());
        vcx.update(|w, cx| ws.update(cx, |ws, cx| ws.focus_active(w, cx)));
        redraw(vcx);
        let screen_has = |vcx: &mut gpui::VisualTestContext, text: &str, tries: usize| {
            for _ in 0..tries {
                redraw(vcx);
                if pane.read_with(vcx, |v, _| v.fixture_screen_text().contains(text)) {
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            false
        };
        // The pane must take typing first, or a quiet shell proves nothing.
        std::thread::sleep(std::time::Duration::from_millis(200));
        vcx.simulate_keystrokes("a b");
        assert!(
            screen_has(vcx, "ab", 1000),
            "{}: the terminal takes typing",
            case.name
        );
        (case.open)(&ws, vcx);
        redraw(vcx);
        let opened = ws.read_with(vcx, |ws, _| (case.field)(ws));
        if opened.is_none() {
            broken.push(format!("{}: did not open", case.name));
        } else {
            let keys: Vec<String> = MARK.chars().map(|c| c.to_string()).collect();
            vcx.simulate_keystrokes(&keys.join(" "));
            redraw(vcx);
            let got = ws
                .read_with(vcx, |ws, _| (case.field)(ws))
                .unwrap_or_default();
            let leaked = screen_has(vcx, MARK, 40);
            if !got.contains(MARK) {
                broken.push(format!(
                    "{}: its field holds {got:?}, not {MARK:?}",
                    case.name
                ));
            }
            if leaked {
                broken.push(format!("{}: {MARK:?} reached the shell", case.name));
            }
        }
        let stops: Vec<_> = ws
            .read_with(vcx, |ws, _| ws.every_view())
            .into_iter()
            .map(|p| p.read_with(vcx, |v, _| v.fixture_shutdown()))
            .collect();
        stops.into_iter().for_each(|s| s());
    }
    assert!(
        broken.is_empty(),
        "keyboard ownership broken:\n  {}",
        broken.join("\n  ")
    );
}
