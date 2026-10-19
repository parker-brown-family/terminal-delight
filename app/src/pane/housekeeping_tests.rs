//! Real pane clocks and streams, with visible/local positive controls.
use super::*;
use crate::pane::harness::Pane;
use gpui::TestAppContext;
use std::io::{Read, Write};

fn step(pane: &mut Pane, ms: u64) {
    pane.cx.executor().advance_clock(Duration::from_millis(ms));
    pane.cx.run_until_parked();
}

fn hide(pane: &mut Pane) {
    pane.view.update(pane.cx, |v, _| {
        // Finish the harness's initial layout resize before the observation.
        if let Some((size, _)) = v.pending_pty.take() {
            v.grid = size.grid;
            v.cell_px = size.cell_px;
            v.session.resize(size.grid, size.cell_px.0, size.cell_px.1);
        }
        v.last_think_scan = Instant::now();
        v.last_scroll_offset = Some((0, Instant::now() - Duration::from_secs(1)));
        v.mark_shown(false);
    });
    pane.cx.run_until_parked();
}

fn run_for(pane: &mut Pane, ms: u64) {
    let mut last = Instant::now();
    let end = last + Duration::from_millis(ms);
    while Instant::now() < end {
        std::thread::sleep(Duration::from_millis(5));
        let now = Instant::now();
        pane.cx.executor().advance_clock(now.duration_since(last));
        pane.cx.run_until_parked();
        last = now;
    }
}

#[gpui::test]
fn hidden_output_does_not_dispatch_ui_events_and_activation_reads_latest(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    hide(pane);
    let events = pane.read(|v| v.output_events);
    let ticks = pane.read(|v| v.housekeeping_ticks);
    // Each write is separately observed at the parser: this cannot pass just
    // because the operating system combined all writes into a single read.
    for n in 0..40 {
        let marker = format!("hidden-{n:04}");
        attached
            .host
            .write_all(format!("\r{marker}").as_bytes())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !pane.rows().iter().any(|r| r.contains(&marker)) {
            pane.cx.run_until_parked();
            assert!(Instant::now() < deadline, "parser stopped while hidden");
            std::thread::sleep(Duration::from_millis(2));
        }
        pane.cx.run_until_parked();
    }
    assert_eq!(
        pane.read(|v| v.output_events),
        events,
        "hidden output dispatched screen events"
    );
    assert_eq!(
        pane.read(|v| v.housekeeping_ticks),
        ticks,
        "output woke the hidden clock"
    );
    // A non-screen event must still reach the UI before any clock advances.
    attached.host.write_all(b"\x1b]2;hidden-title\x07").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while pane.read(|v| v.title.clone()) != "hidden-title" {
        pane.cx.run_until_parked();
        assert!(Instant::now() < deadline, "title event was suppressed");
        std::thread::sleep(Duration::from_millis(2));
    }
    // The existing five-second background refresh still sees generation
    // changes even though no screen event set offscreen_dirty.
    pane.view.update(pane.cx, |v, cx| {
        v.offscreen_dirty = false;
        v.offscreen_flushed = Instant::now() - OFFSCREEN_REFRESH;
        v.flush_offscreen(cx);
        assert_eq!(v.hidden_generation, v.session.content_generation());
        assert!(!v.offscreen_dirty);
        assert!(v.offscreen_flushed.elapsed() < Duration::from_millis(100));
    });
    pane.read(|v| v.mark_shown(true));
    pane.redraw();
    assert!(pane.rows().iter().any(|r| r.contains("hidden-0039")));
    attached.host.write_all(b"\rVISIBLE-AFTER-HIDE").unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while pane.read(|v| v.output_events) == events {
        pane.cx.run_until_parked();
        assert!(
            Instant::now() < deadline,
            "visible output wake never resumed"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(pane.rows().iter().any(|r| r.contains("VISIBLE-AFTER-HIDE")));
}

#[gpui::test]
fn hidden_housekeeping_sleeps_and_visibility_edges_wake_it(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    hide(pane);
    // A pending CRT effect must not override the hidden cadence.
    pane.view.update(pane.cx, |v, _| v.fx.band = Some(0.25));
    let before = pane.read(|v| v.housekeeping_ticks);
    for _ in 0..10 {
        // The workspace writes visibility on every render. Output and those
        // repeated writes must not interrupt the hidden clock.
        pane.read(|v| v.mark_shown(false));
        attached.host.write_all(b"\rstreaming").unwrap();
        step(pane, 100);
    }
    let hidden = pane.read(|v| v.housekeeping_ticks) - before;
    assert!(
        hidden <= 2,
        "{hidden} housekeeping callbacks in one hidden second"
    );

    // Waking on the visibility edge must be immediate even with a second
    // left on the timer; a following identical write must do nothing.
    let before = pane.read(|v| v.housekeeping_ticks);
    pane.read(|v| v.mark_shown(true));
    pane.cx.run_until_parked();
    assert!(pane.read(|v| v.housekeeping_ticks) > before);
    let before = pane.read(|v| v.housekeeping_ticks);
    for _ in 0..100 {
        pane.read(|v| v.mark_shown(true));
    }
    pane.cx.run_until_parked();
    assert_eq!(pane.read(|v| v.housekeeping_ticks), before);

    pane.view.update(pane.cx, |v, _| v.fx.band = None);
    // Keyboard focus is independent of visibility. A visible unfocused pane
    // retains the ordinary cadence.
    let _other = pane.cx.update(|window, cx| {
        let other = cx.focus_handle();
        window.focus(&other, cx);
        other
    });
    let focus = pane.read(|v| v.focus_handle.clone());
    pane.cx
        .update(|window, _| assert!(!focus.is_focused(window)));
    for _ in 0..10 {
        step(pane, 100);
    }
    assert!(pane.read(|v| v.housekeeping_ticks) - before >= 5);
    pane.view.update(pane.cx, |v, _| {
        v.fx.band = Some(0.25);
        v.mark_shown(false);
        v.mark_shown(true);
    });
    pane.cx.run_until_parked();
    let before = pane.read(|v| v.housekeeping_ticks);
    for _ in 0..10 {
        step(pane, 33);
    }
    assert!(
        pane.read(|v| v.housekeeping_ticks) - before >= 8,
        "visible CRT effects lost their frame cadence"
    );
}

#[gpui::test]
fn only_locally_owned_ptys_get_a_foreground_watcher(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    hide(&mut attached.pane);
    step(&mut attached.pane, 800);
    assert_eq!(
        attached.pane.read(|v| v.foreground_polls),
        0,
        "a host-owned PTY must not start a local foreground watcher"
    );
    let mut local = Pane::running(cx, "exec cat");
    step(&mut local, 800);
    assert!(
        local.read(|v| v.foreground_polls) >= 1,
        "a genuinely local PTY must retain foreground detection"
    );
}

#[gpui::test]
fn a_hidden_stream_and_colour_query_do_not_wait_for_housekeeping(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    hide(pane);
    let before = pane.read(|v| v.housekeeping_ticks);
    attached.host.set_nonblocking(true).unwrap();
    attached
        .host
        .write_all(b"fresh hidden output\r\n\x1b]11;?\x07")
        .unwrap();
    let mut reply = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        pane.cx.run_until_parked(); // Deliberately never advance the clock.
        let mut bytes = [0; 256];
        match attached.host.read(&mut bytes) {
            Ok(n) if n > 0 => reply.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            other => panic!("query stream: {other:?}"),
        }
        if reply.windows(4).any(|s| s == b"rgb:") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "hidden colour query never answered: {reply:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(pane
        .rows()
        .iter()
        .any(|r| r.contains("fresh hidden output")));
    assert_eq!(
        pane.read(|v| v.housekeeping_ticks),
        before,
        "byte processing must not wake housekeeping"
    );
}

#[gpui::test]
fn hidden_resize_interrupts_sleep_and_keeps_the_last_target(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    hide(pane);
    run_for(pane, 30);
    attached.resizes.lock().unwrap().clear();
    let resize = |pane: &mut Pane, width| {
        pane.cx.simulate_resize(gpui::size(px(width), px(650.0)));
        pane.cx.run_until_parked();
    };
    resize(pane, 1000.0);
    run_for(pane, 60);
    resize(pane, 1200.0);
    let latest = pane.read(|v| v.pending_pty.expect("a real window resize was staged").0);
    run_for(pane, 95);
    assert!(
        attached.resizes.lock().unwrap().is_empty(),
        "resize fired before settle"
    );
    run_for(pane, 110);
    let sent = attached.resizes.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "only the final size is announced: {sent:?}");
    assert_eq!(sent[0].num_cols as usize, latest.grid.cols);
    assert_eq!(pane.read(|v| v.grid), latest.grid);

    attached.resizes.lock().unwrap().clear();
    resize(pane, 1000.0);
    resize(pane, 1200.0); // Returning to the actual size cancels the resize.
    run_for(pane, 200);
    assert!(attached.resizes.lock().unwrap().is_empty());
}

#[gpui::test]
fn hidden_prompt_is_detected_by_the_one_second_fallback(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    hide(pane);
    pane.running_as(PaneMode::Codex);
    attached
        .host
        .write_all(b"Do you want to proceed?\r\n  1. Yes\r\n  esc to cancel\r\n")
        .unwrap();
    run_for(pane, 1150);
    assert!(
        pane.read(|v| v.needs_input),
        "hidden permission prompt was missed"
    );
    attached
        .host
        .write_all(b"\x1b[2J\x1b[Hstill thinking (esc to interrupt)\r\n")
        .unwrap();
    run_for(pane, 1100);
    assert!(
        !pane.read(|v| v.needs_input),
        "answered prompt stayed latched"
    );
    assert!(
        pane.read(|v| v.agent_thinking == Some(true)),
        "hidden work must still be detected"
    );
}

#[gpui::test]
fn hidden_completion_keeps_its_short_debounce_without_a_busy_loop(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    hide(pane);
    pane.running_as(PaneMode::Codex);
    pane.view.update(pane.cx, |v, _| {
        v.agent_thinking = Some(false);
        v.think_since = Some(Instant::now() - Duration::from_secs(2));
        v.not_thinking_since = Some(Instant::now());
        v.wake_housekeeping();
    });
    pane.cx.run_until_parked();
    run_for(pane, 200);
    assert!(!pane.read(|v| v.bell), "completion rang before debounce");
    run_for(pane, 230);
    assert!(
        pane.read(|v| v.bell),
        "completion waited for the one-second fallback"
    );
    let before = pane.read(|v| v.housekeeping_ticks);
    run_for(pane, 150);
    assert!(
        pane.read(|v| v.housekeeping_ticks) - before <= 1,
        "a consumed completion deadline kept the pane awake"
    );

    pane.view.update(pane.cx, |v, _| {
        v.bell = false;
        v.think_since = Some(Instant::now() - Duration::from_millis(200));
        v.not_thinking_since = Some(Instant::now() - Duration::from_millis(100));
        assert!(
            v.completion_deadline().is_none(),
            "short spells cannot schedule a bell"
        );
        v.think_since = Some(Instant::now() - Duration::from_secs(2));
        v.not_thinking_since = Some(Instant::now() - Duration::from_secs(1));
        v.agent_thinking = None;
        assert!(
            v.completion_deadline().is_none(),
            "unobserved thinking is not a stop"
        );
        v.agent_thinking = Some(false);
        assert!(
            v.housekeeping_delay() >= Duration::from_millis(33),
            "expired deadlines cannot spin"
        );
    });
}

#[gpui::test]
fn hidden_completion_waits_for_scroll_settle(cx: &mut TestAppContext) {
    let mut attached = Pane::attached(cx);
    let pane = &mut attached.pane;
    hide(pane);
    pane.running_as(PaneMode::Codex);
    pane.view.update(pane.cx, |v, _| {
        v.agent_thinking = Some(false);
        v.think_since = Some(Instant::now() - Duration::from_secs(3));
        v.not_thinking_since = Some(Instant::now() - Duration::from_secs(1));
        v.last_scroll_offset = Some((0, Instant::now()));
        v.wake_housekeeping();
    });
    pane.cx.run_until_parked();
    let before = pane.read(|v| v.housekeeping_ticks);
    run_for(pane, 100);
    assert!(
        !pane.read(|v| v.bell),
        "scrolling manufactured a completion"
    );
    run_for(pane, 250);
    assert!(
        pane.read(|v| v.bell),
        "settling did not release the real completion"
    );
    assert!(
        pane.read(|v| v.housekeeping_ticks) - before <= 12,
        "expired deadline spun while waiting for scroll-settle"
    );
}
