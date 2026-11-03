//! Rest and sleep on a real attached pane: the state the window reports for
//! it, and that an asleep pane lets no key through to the shell behind it.
use super::*;
use crate::agentrest::{Asleep, Dormancy};
use crate::hud::AgentState;
use crate::pane::harness::Pane;
use gpui::TestAppContext;
use std::io::Read;

fn asleep() -> Asleep {
    Asleep {
        since_ms: 1_792_480_845_491,
        session: "80915041-fe1f-413a-9f48-95217cb9ec6b".into(),
        wake_file: "/nonexistent/wake.sh".into(),
        model: Some("opus".into()),
        program: Some("CLAUDE".into()),
    }
}

fn state(pane: &mut Pane) -> AgentState {
    pane.read(|v| v.agent_status().state)
}

#[gpui::test]
fn an_asleep_pane_reads_asleep_and_still_counts_as_an_agent(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    pane.running_as(PaneMode::Shell);
    assert!(
        !pane.read(|v| v.counts_as_agent()),
        "a shell is not an agent"
    );
    pane.view
        .update(pane.cx, |v, _| v.rest.dormancy = Dormancy::Asleep(asleep()));
    assert!(
        pane.read(|v| v.counts_as_agent()),
        "its agent is asleep, not gone"
    );
    assert_eq!(state(pane), AgentState::Asleep);
}

#[gpui::test]
fn a_finish_bell_cannot_turn_rest_into_done(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    pane.running_as(PaneMode::Claude);
    pane.view.update(pane.cx, |v, _| {
        v.bell = true;
        v.rest.dormancy = Dormancy::Resting {
            since: Instant::now(),
        };
    });
    assert_eq!(state(pane), AgentState::Resting);
    // The control: the same pane awake shows the bell's finish.
    pane.view
        .update(pane.cx, |v, _| v.rest.dormancy = Dormancy::Awake);
    assert_eq!(state(pane), AgentState::Finished);
}

#[gpui::test]
fn a_key_in_an_asleep_pane_asks_to_wake_and_reaches_nothing(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let view = attached.pane.view.clone();
    let woke = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let seen = woke.clone();
    attached.pane.cx.update(|_, cx| {
        cx.subscribe(&view, move |_, _: &WakeAsleep, _| seen.set(seen.get() + 1))
            .detach()
    });
    attached.pane.view.update(attached.pane.cx, |v, _| {
        v.rest.dormancy = Dormancy::Asleep(asleep())
    });
    // A pane ignores keys for its first 150 ms (`terminal_key`); a person
    // waking a sleeper has been away far longer than that.
    std::thread::sleep(Duration::from_millis(200));
    attached.pane.keys("x");
    attached.pane.cx.run_until_parked();
    assert_eq!(woke.get(), 1, "the key became a wake request");

    // The control first leaves its byte on the stream, so read everything
    // that has arrived and look for each key by its own byte.
    attached
        .pane
        .view
        .update(attached.pane.cx, |v, _| v.rest.dormancy = Dormancy::Awake);
    attached.pane.keys("y");
    attached.host.set_nonblocking(true).unwrap();
    let mut got = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !got.contains(&b'y') && Instant::now() < deadline {
        attached.pane.cx.run_until_parked();
        let mut buf = [0u8; 256];
        match attached.host.read(&mut buf) {
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => panic!("{e}"),
        }
    }
    assert!(
        got.contains(&b'y'),
        "an awake pane's key reaches the terminal"
    );
    assert!(
        !got.contains(&b'x'),
        "an asleep pane's key reached the shell: {got:?}"
    );
    assert_eq!(woke.get(), 1, "an awake key asks for nothing");
}
