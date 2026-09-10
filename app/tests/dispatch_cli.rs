//! The refusal, end to end.
//!
//! A unit test can pin `dispatch`, but the failure this guards against was
//! never in one function: it was a whole process reaching the window, adopting
//! or minting a session, and leaving a layout on disk — for a word that was a
//! typo. Only the real binary can prove that no longer happens, and only a
//! sacrificial HOME can prove nothing was written on the way out.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Wait for a child, killing it if it outstays `limit`. A regression here does
/// not fail an assertion — it *hangs*, because the process opened a window and
/// settled into the event loop. So the timeout is itself the assertion.
fn wait_or_kill(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + limit;
    loop {
        match child
            .try_wait()
            .expect("wait on the terminal-delight child")
        {
            Some(status) => return Some(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// Every path under `root`, so the test can say "nothing was written" about the
/// whole tree rather than about the one directory it thought to look in.
fn walk(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        }
        out.push(path);
    }
}

/// A sacrificial HOME: every XDG variable TD reads points inside it, and the
/// display variables are stripped so that a regression cannot paint a window on
/// the desktop of whoever is running the suite.
fn run_in_a_throwaway_home(args: &[&str]) -> (Option<ExitStatus>, String, Vec<PathBuf>) {
    let home = std::env::temp_dir().join(format!(
        "td-dispatch-{}-{}",
        std::process::id(),
        args.join("_").replace(['/', '-'], "_")
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create the throwaway HOME");

    let mut child = Command::new(env!("CARGO_BIN_EXE_terminal-delight"))
        .args(args)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("TD_SESSION")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn terminal-delight");

    let status = wait_or_kill(&mut child, Duration::from_secs(20));
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        use std::io::Read;
        let _ = pipe.read_to_string(&mut stderr);
    }

    let mut written = vec![];
    walk(&home, &mut written);
    let _ = std::fs::remove_dir_all(&home);
    (status, stderr, written)
}

#[test]
fn a_typoed_verb_costs_an_exit_code_and_nothing_else() {
    let (status, stderr, written) = run_in_a_throwaway_home(&["sevre"]);

    let status = status.expect("a typo must exit, not open a window and sit there");
    assert_eq!(status.code(), Some(2), "stderr was: {stderr}");
    assert!(
        stderr.contains("unknown command `sevre`"),
        "the refusal must name the word: {stderr}"
    );
    assert!(
        written.is_empty(),
        "a refused word wrote to disk: {written:?}"
    );
}

#[test]
fn a_verb_this_build_does_not_carry_is_still_refused() {
    // The stale-install failure in miniature: a caller invoking a verb this
    // build does not have. It must be told, not quietly given a window. (When
    // `serve` was that verb, this test named it; it is a real verb now, which
    // is why the allowlist and this test move together.)
    let (status, stderr, written) = run_in_a_throwaway_home(&["attach"]);

    assert_eq!(
        status.expect("must exit").code(),
        Some(2),
        "stderr was: {stderr}"
    );
    assert!(stderr.contains("unknown command `attach`"), "{stderr}");
    assert!(written.is_empty(), "wrote to disk: {written:?}");
}

#[test]
fn serve_without_a_session_is_refused_by_its_own_handler() {
    // Distinct from the unknown-word refusal, and that distinction is the
    // proof that dispatch reached the handler rather than the allowlist: this
    // one names the missing argument, not the word.
    let (status, stderr, written) = run_in_a_throwaway_home(&["serve"]);

    assert_eq!(
        status.expect("must exit").code(),
        Some(2),
        "stderr was: {stderr}"
    );
    assert!(stderr.contains("--session"), "{stderr}");
    assert!(
        !stderr.contains("unknown command"),
        "serve was refused by the allowlist instead of its handler: {stderr}"
    );
    assert!(written.is_empty(), "wrote to disk: {written:?}");
}

#[test]
fn version_still_answers_without_touching_the_disk() {
    // The regression that bought the flag gate in the first place: a build
    // script probing `--version` used to get a window and a session file.
    let (status, stderr, written) = run_in_a_throwaway_home(&["--version"]);

    assert_eq!(
        status.expect("must exit").code(),
        Some(0),
        "stderr was: {stderr}"
    );
    assert!(written.is_empty(), "--version wrote to disk: {written:?}");
}
