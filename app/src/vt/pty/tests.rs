use super::*;
use std::time::{Duration, Instant};

fn stubborn() -> PtySource {
    ready("/bin/sh", "trap '' HUP TERM; printf ready; exec sleep 30")
}

fn ready(program: &str, script: &str) -> PtySource {
    let mut pty = spawn(
        &Options {
            shell: Some((program.into(), vec!["-c".into(), script.into()])),
            ..Options::default()
        },
        WindowSize {
            num_lines: 24,
            num_cols: 80,
            cell_width: 8,
            cell_height: 16,
        },
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = Vec::new();
    let mut buf = [0; 64];
    while !bytes.windows(5).any(|s| s == b"ready") {
        match pty.read(&mut buf) {
            Ok(n) => bytes.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
            other => panic!("child readiness: {other:?}"),
        }
        assert!(Instant::now() < deadline, "child never became ready");
        std::thread::sleep(Duration::from_millis(5));
    }
    pty
}

#[test]
fn closing_a_stubborn_child_finishes_without_waiting_for_its_natural_exit() {
    let pty = stubborn();
    let pid = pty.child().id();
    let thread = std::thread::spawn(move || drop(pty));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !thread.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    let finished = thread.is_finished();
    if !finished {
        // Failure cleanup only: the blocked owner still holds this unreaped child.
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }
    thread.join().unwrap();
    assert!(
        finished,
        "PTY cleanup waited indefinitely for a child ignoring HUP"
    );
    assert_eq!(
        unsafe { libc::kill(pid as i32, 0) },
        -1,
        "child must be reaped"
    );
}

#[test]
fn a_monitor_does_not_keep_a_closed_terminal_alive() {
    let mut pty = stubborn();
    let monitor = pty.monitor();
    assert!(monitor.file().is_some());
    let report = pty.close().unwrap();
    assert!(report.exit_observed, "{report:?}");
    assert!(report.kill_sent);
    assert!(monitor.file().is_none());
    assert!(pty.close().is_none(), "close is idempotent");
}

#[test]
fn an_already_reaped_child_is_never_signalled() {
    let mut pty = ready("/bin/sh", "printf ready");
    let until = Instant::now() + Duration::from_secs(3);
    while pty.reap().is_none() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let report = pty.close().unwrap();
    assert!(report.exit_observed);
    assert!(!report.hangup_sent && !report.kill_sent);
}

#[test]
fn close_reaches_a_stubborn_job_in_another_process_group() {
    let mut pty = ready("/bin/bash", "set -m; trap '' HUP TERM; /bin/sh -c 'trap \"\" HUP TERM; printf ready; exec sleep 30' & wait");
    let sid = pty.child().id() as i32;
    let scan = session_jobs(sid, Duration::from_secs(2));
    assert!(scan.error.is_none(), "{:?}", scan.error);
    let jobs = scan.held;
    assert!(!jobs.is_empty());
    assert!(jobs
        .iter()
        .any(|(pid, _)| unsafe { libc::getpgid(*pid) } != sid));
    let mut unrelated = stubborn();
    let report = pty.close().unwrap();
    assert!(report.exit_observed, "{report:?}");
    assert_eq!(report.observed_jobs_remaining, Some(0), "{report:?}");
    assert!(jobs.iter().all(|(_, fd)| job_exited(fd) == Some(true)));
    assert!(unrelated
        .child
        .as_mut()
        .unwrap()
        .try_wait()
        .unwrap()
        .is_none());
    unrelated.close();
}

#[test]
fn dropping_notifiers_leaves_the_child_running_and_a_late_close_has_no_target() {
    let session = crate::term::spawn_program(
        crate::term::GridSize { cols: 80, rows: 24 },
        (8, 16),
        "/bin/cat",
        &[],
    )
    .unwrap();
    let monitor = session.master.clone().unwrap();
    let close = session.notifier.clone();
    drop(session);
    std::thread::sleep(Duration::from_millis(25));
    assert!(
        monitor.file().is_some(),
        "a disconnected client is not a close"
    );
    let report = close
        .close()
        .expect("the owner acknowledges explicit close");
    assert!(report.exit_observed, "{report:?}");
    assert!(monitor.file().is_none());
    let mut replacement = stubborn();
    assert!(
        close.close().is_none(),
        "a completed channel cannot address a replacement"
    );
    assert!(replacement
        .child
        .as_mut()
        .unwrap()
        .try_wait()
        .unwrap()
        .is_none());
    replacement.close();
}
