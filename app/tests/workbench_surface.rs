//! The workbench, locked in through the surfaces a person or an agent can
//! actually reach.
//!
//! The unit tests in `workbench`, `surface` and `derive` pin the rules. This
//! file pins the CONTRACT: the verbs an agent is told about in the protocol
//! doc, the verbs a script drives the bench with, and the shape of what comes
//! back out. A rule can be correct in a function nobody calls, and most of the
//! defects this session produced were exactly that — a decision made in one
//! place and read somewhere else, or not read at all.
//!
//! Everything here runs the REAL binary. Nothing here opens a window: the
//! display variables are stripped, and a verb that tried to would hang, which
//! the timeout would report as the failure it is.

use std::io::Write;
use std::process::{Command, Stdio};

/// Run a `surface` sub-verb and give back (stdout, stderr, success).
fn surface(args: &[&str], stdin: Option<&str>) -> (String, String, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_terminal-delight"))
        .arg("surface")
        .args(args)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("TD_SESSION")
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn terminal-delight surface");
    if let (Some(text), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let _ = pipe.write_all(text.as_bytes());
    }
    let out = child.wait_with_output().expect("wait for surface");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

/// A file in a throwaway directory, removed when the test ends.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("td-wb-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        Scratch(dir)
    }
    fn write(&self, name: &str, body: &str) -> std::path::PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, body).expect("write scratch file");
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------------------
// The protocol an agent is asked to speak
// ---------------------------------------------------------------------------

#[test]
fn the_catalogue_names_every_kind_the_bench_can_draw() {
    // This is what an agent reads to find out what it may send. A kind the
    // bench renders but the catalogue omits is a feature nobody can discover;
    // a kind the catalogue promises and the bench cannot draw is worse.
    let (out, err, ok) = surface(&["--catalogue"], None);
    assert!(ok, "the catalogue verb failed: {err}");
    for kind in [
        "artifact",
        "markdown",
        "table",
        "architecture",
        "changeset",
        "decision",
        "question",
    ] {
        assert!(out.contains(kind), "the catalogue never mentions {kind}");
    }
    // And the fallback, which is the one that makes the protocol survive a
    // version it has never seen.
    assert!(
        out.contains("unclassified"),
        "the catalogue hides the fallback kind"
    );
}

#[test]
fn a_document_this_build_cannot_parse_is_refused_by_name() {
    let (_, err, ok) = surface(&["-"], Some("{\"this\": \"is not a surface\"}"));
    assert!(!ok, "nonsense was accepted");
    assert!(
        !err.trim().is_empty(),
        "a refusal with no reason is a refusal nobody can act on"
    );
}

// ---------------------------------------------------------------------------
// What gets derived from an agent that was never asked to cooperate
// ---------------------------------------------------------------------------

#[test]
fn a_deliverable_line_becomes_a_surface_with_a_human_title() {
    // The whole reason derivation exists: an agent that has never heard of
    // this protocol still puts its work on the bench, because the house style
    // already asks it to name what it made.
    //
    // And the title is the specimen Parker objected to — a content hash that
    // travelled out of a filename and into a heading.
    let scratch = Scratch::new("derive");
    let path = scratch.write(
        "t.jsonl",
        &[
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Deliverable: Bench Paste 15868dd2 — file:///tmp/pastes/15868dd2.png"}]}}"#,
            "",
        ]
        .join("\n"),
    );
    let (out, err, ok) = surface(&["--derive", path.to_str().unwrap()], None);
    assert!(ok, "derive failed: {err}");
    assert!(
        out.contains("Bench Paste"),
        "the deliverable never became a surface: {out}"
    );
    assert!(
        !out.contains("15868dd2f") && !out.contains("Paste 15868dd2"),
        "a checksum reached a human-facing title: {out}"
    );
}

#[test]
fn a_transcript_with_nothing_to_show_derives_nothing_and_says_so_calmly() {
    // Absence is an answer. An empty derive that errored would make every
    // quiet agent look broken.
    let scratch = Scratch::new("empty");
    let path = scratch.write(
        "t.jsonl",
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"just thinking out loud"}]}}"#,
    );
    let (out, err, ok) = surface(&["--derive", path.to_str().unwrap()], None);
    assert!(ok, "an empty derive must not be an error: {err}");
    assert!(
        !out.contains("Deliverable"),
        "something was invented from nothing: {out}"
    );
}

#[test]
fn a_transcript_that_does_not_exist_fails_instead_of_pretending() {
    let (_, err, ok) = surface(&["--derive", "/nonexistent/transcript.jsonl"], None);
    assert!(!ok, "a missing transcript was reported as success");
    assert!(!err.trim().is_empty(), "and said nothing about why");
}

// ---------------------------------------------------------------------------
// The scripted half of the bench — the verbs that exist because every gesture
// on this surface belongs to a hand, and CI has none.
// ---------------------------------------------------------------------------

/// A pid no control socket can belong to, so these tests resolve to a window
/// that cannot exist.
///
/// `u32::MAX` is not a pid Linux hands out — `/proc/sys/kernel/pid_max` tops out
/// far below it — so the socket for it is guaranteed absent and the send fails
/// for the one reason these tests are about.
const NO_SUCH_WINDOW: &str = "4294967295";

/// Run `ctl` against a window that cannot exist. The point is the PARSE: a verb
/// this build does not carry must be refused before anything is sent, and one it
/// does carry must get as far as looking for a window.
///
/// **The `--pid` pin is load-bearing, and removing it types into somebody's
/// terminal.** This helper used to say it ran "with no window listening" and
/// enforced nothing: it cleared `DISPLAY`, `WAYLAND_DISPLAY` and `TD_SESSION`,
/// none of which `ctl` consults when choosing a target. Without a pin the scope
/// is resolved against the LIVE DESKTOP — `owning_td_pid()` walks the process
/// tree for the terminal-delight window the test is running inside, and the
/// workspace fallback looks for open windows — so a person or an agent running
/// `cargo test` from inside a pane has `bench type "half a sentence"` delivered
/// into their own composer. Parker, watching it arrive while working: *"half a
/// sentence keeps getting autofilled into our text area"*.
///
/// **CI could never have caught it.** CI has no window, so resolution falls
/// through to "none running" there and the test is green on exactly the machine
/// where the bug cannot happen. It is also why it was intermittent rather than
/// constant: whether it landed depended on which workspace was in front.
fn ctl(args: &[&str]) -> (String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_terminal-delight"))
        .arg("ctl")
        .args(args)
        // After the verb, per the usage line. Every call in this file routes
        // through here, so no test can opt out of the pin by accident.
        .arg("--pid")
        .arg(NO_SUCH_WINDOW)
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("TD_SESSION")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("spawn terminal-delight ctl");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (text, out.status.success())
}

#[test]
fn every_bench_verb_this_build_carries_is_spelled_the_way_it_is_documented() {
    // The usage line is the only documentation these verbs have, so it is
    // tested against the parser rather than trusted. `bench type` is the
    // newest and the reason this test exists: it was added so that a caret
    // in a half-typed line could be photographed without borrowing somebody's
    // keyboard, and a verb that is not wired is worse than no verb, because
    // the harness then quietly types into whatever window has focus.
    let (usage, _) = ctl(&["nonsense-verb"]);
    for verb in ["bench on", "choose <n>", "say <text>", "type <text>"] {
        assert!(usage.contains(verb), "the usage line never offers `{verb}`");
    }
}

#[test]
fn a_bench_verb_with_nothing_to_say_is_refused_rather_than_sent_empty() {
    for args in [
        vec!["bench", "say"],
        vec!["bench", "type"],
        vec!["bench", "choose"],
    ] {
        let (text, ok) = ctl(&args);
        assert!(
            !ok,
            "`{}` was accepted with no argument: {text}",
            args.join(" ")
        );
    }
}

#[test]
fn a_bench_verb_that_parses_gets_as_far_as_looking_for_a_window() {
    // With no window listening this cannot succeed, and it must fail for the
    // RIGHT reason — no window — rather than by being rejected as a typo.
    // That distinction is what proves the verb is wired end to end.
    let (text, _) = ctl(&["bench", "type", "half a sentence"]);
    let lower = text.to_lowercase();
    assert!(
        !lower.contains("usage"),
        "a documented verb was refused as unknown: {text}"
    );
}

#[test]
fn a_scripted_verb_never_reaches_a_window_a_person_is_using() {
    // The guard on the whole file, and the reason it exists: `bench type` PUTS
    // TEXT IN A COMPOSER, and unpinned it resolves its target against the live
    // desktop. Every test here must fail to find a window, and must fail because
    // the pid it was given cannot exist rather than because none happened to be
    // open — the second is a property of the machine, the first of the test.
    let (text, ok) = ctl(&["bench", "type", "half a sentence"]);
    assert!(!ok, "a send SUCCEEDED — it reached a live window: {text}");
    assert!(
        text.contains(NO_SUCH_WINDOW),
        "the failure never names the impossible pid, so the pin is not in \
         effect and this test is passing by luck: {text}"
    );
}
