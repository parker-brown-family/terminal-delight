//! A reader that walks away is not an error worth a backtrace.
//!
//! Rust masks SIGPIPE at startup, so a write to a pipe nobody is reading comes
//! back as an `io` error and `println!` turns it into a panic. Every
//! `println!`-based path in this binary had that exposure, and the symptom was
//! a Rust backtrace printed *after* the output the caller asked for:
//!
//! ```text
//! $ terminal-delight skin --list | head -5
//! …five lines…
//! thread 'main' panicked at library/std/src/io/stdio.rs:1166:9:
//! failed printing to stdout: Broken pipe (os error 32)
//! ```
//!
//! Three things make this test harder to write honestly than it looks, and all
//! three were measured on the live binary before a line of it existed:
//!
//! **The obvious construction is the weak one.** `| head -1` leaves the reader
//! draining until it has what it wants, so it only breaks a producer that
//! writes more than once. `skin --list` writes line by line and panicked 6 runs
//! out of 6 — but `--help` prints its whole text in a single `println!`, and
//! under `| head -1` it was clean 6 out of 6. Pipe the same command into a
//! reader that has *already gone* and it panics every time. So this file closes
//! the read end before the child's first write rather than mid-stream;
//! otherwise it would have been green on two of the three broken paths.
//!
//! **Most of these verbs write nothing.** Given no arguments, `ctl`, `mcp`,
//! `agent-usage`, `agent-vitals`, `probe` and `surface` put zero bytes on
//! stdout — they refuse on stderr instead. A table-driven gate over "every
//! headless verb" would therefore be six cases that can never fail and one that
//! can, presented as seven passes. Every case here runs twice: once with the
//! pipe open to prove it writes at all, and once with the reader gone. The
//! first run is not ceremony; it is what stops this file becoming decoration
//! the day somebody changes what a verb prints.
//!
//! **The natural assertion is backwards.** A process with the default SIGPIPE
//! disposition is *killed by a signal*: `code()` is `None`, `signal()` is 13,
//! and `success()` is false. `assert!(status.success())` would therefore pass
//! on the broken build and fail on the fixed one. The assertion is on the
//! absence of the panic, never on a successful exit.

use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

/// Which stream the case is about. The fix is a process-wide signal
/// disposition, so it covers both, and a gate that only watched stdout would
/// miss half of what it fixed — the refusal path prints on stderr.
#[derive(Clone, Copy, Debug)]
enum Stream {
    Stdout,
    Stderr,
}

struct Case {
    /// What a person would type, used in assertion messages so a failure names
    /// the command rather than an index.
    label: &'static str,
    args: &'static [&'static str],
    stream: Stream,
}

/// The paths that write to a caller's terminal and then exit.
///
/// `skin --list` is the verb from the bug report. `--help` and `--version` go
/// through a different arm of `main` — `Launch::Reply` — which the issue never
/// named and which the issue's own repro cannot see. The unknown word is the
/// refusal, and it is the stderr case.
const CASES: &[Case] = &[
    Case {
        label: "skin --list",
        args: &["skin", "--list"],
        stream: Stream::Stdout,
    },
    Case {
        label: "--help",
        args: &["--help"],
        stream: Stream::Stdout,
    },
    Case {
        label: "--version",
        args: &["--version"],
        stream: Stream::Stdout,
    },
    Case {
        label: "an unknown word",
        args: &["not-a-verb"],
        stream: Stream::Stderr,
    },
];

/// A sacrificial HOME with the display stripped, so a regression cannot paint a
/// window on the desktop of whoever is running the suite or write into their
/// real state directory. Same shape as `dispatch_cli.rs`.
fn command(args: &[&str], home: &std::path::Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_terminal-delight"));
    cmd.args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("TD_SESSION")
        .stdin(Stdio::null());
    cmd
}

fn throwaway_home(label: &str) -> std::path::PathBuf {
    let home = std::env::temp_dir().join(format!(
        "td-broken-pipe-{}-{}",
        std::process::id(),
        label.replace([' ', '/', '-'], "_")
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create the throwaway HOME");
    home
}

/// Leg A. Run the case with both pipes open and return how many bytes reached
/// the stream under test.
fn bytes_written(case: &Case) -> usize {
    let home = throwaway_home(case.label);
    let out = command(case.args, &home)
        .output()
        .expect("spawn terminal-delight");
    let _ = std::fs::remove_dir_all(&home);
    match case.stream {
        Stream::Stdout => out.stdout.len(),
        Stream::Stderr => out.stderr.len(),
    }
}

/// Leg B. Run the case with the stream under test connected to a pipe whose
/// read end is already closed, and report `(stderr_text, code, signal)`.
///
/// The reader is dropped the instant `spawn` returns. By then the child has
/// only been forked; it still has an `execve` and the whole of process startup
/// to get through before its first write, so the close wins by a margin that is
/// not a race in any meaningful sense.
fn run_with_the_reader_gone(case: &Case) -> (String, Option<i32>, Option<i32>) {
    let home = throwaway_home(case.label);
    let (reader, writer) = std::io::pipe().expect("an anonymous pipe");

    let mut cmd = command(case.args, &home);
    match case.stream {
        // stderr still needs capturing when stdout is the closed one, or the
        // panic this test exists to detect goes to the suite's own console
        // instead of into an assertion.
        Stream::Stdout => cmd.stdout(Stdio::from(writer)).stderr(Stdio::piped()),
        Stream::Stderr => cmd.stderr(Stdio::from(writer)).stdout(Stdio::null()),
    };
    let mut child = cmd.spawn().expect("spawn terminal-delight");
    drop(reader);

    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    let status = child.wait().expect("wait on terminal-delight");
    let _ = std::fs::remove_dir_all(&home);
    (stderr, status.code(), status.signal())
}

#[test]
fn a_closed_reader_is_not_a_panic() {
    // Every case is judged, and the verdicts are reported together. Asserting
    // inside the loop would stop at the first broken path and say nothing about
    // the other three — and "one path regressed" and "all four did" want
    // different repairs.
    let mut failures: Vec<String> = Vec::new();

    for case in CASES {
        // Leg A: without this, a case that prints nothing is a case that can
        // never fail, and it looks exactly like a passing one.
        let wrote = bytes_written(case);
        if wrote == 0 {
            failures.push(format!(
                "`{}` wrote nothing to {:?}, so the closed-reader leg proves nothing about it",
                case.label, case.stream
            ));
            continue;
        }

        // Leg B: the gate.
        let (stderr, code, signal) = run_with_the_reader_gone(case);
        if stderr.contains("panicked") {
            failures.push(format!(
                "`{}` panicked on a closed {:?}:\n{}",
                case.label,
                case.stream,
                stderr.trim()
            ));
        } else if code == Some(101) {
            // The stderr case cannot capture the message — the panic is printed
            // to the stream this test just closed — so the exit code is the
            // only witness it leaves behind.
            failures.push(format!(
                "`{}` exited 101 (a Rust panic) on a closed {:?}",
                case.label, case.stream
            ));
        } else if code.is_none() && signal != Some(libc::SIGPIPE) {
            failures.push(format!(
                "`{}` died of signal {signal:?}, which is neither a clean exit nor SIGPIPE",
                case.label
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {} printing paths mishandle a closed reader:\n\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n\n")
    );
}

#[test]
fn the_repro_from_the_bug_report_stays_fixed() {
    // The construction above is the thorough one; this is the sentence a person
    // typed into the issue. Keeping it means the report and the test agree, and
    // that a future change to the test harness cannot quietly stop covering the
    // case that was actually observed.
    let home = throwaway_home("head");
    let out = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{} skin --list | head -1",
            env!("CARGO_BIN_EXE_terminal-delight")
        ))
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("run"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .stdin(Stdio::null())
        .output()
        .expect("spawn the pipeline");
    let _ = std::fs::remove_dir_all(&home);

    assert!(
        !out.stdout.is_empty(),
        "head -1 read nothing, so this pipeline is not exercising the bug"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panicked"),
        "`skin --list | head -1` panicked:\n{stderr}"
    );
}
