//! How a work object gets from an agent onto a bench, and how an answer gets
//! back.
//!
//! Three writers, one format:
//!
//! ```text
//!   agent calls present_surface  ─┐
//!   agent writes a .json file    ─┼→  surfaces/<session>/<pane>/*.json  →  Bench
//!   agent prints a ```td block   ─┘         (this module watches it)
//! ```
//!
//! # Why a file and not an escape sequence
//!
//! The tempting pipe is the one already under the agent: bytes in the
//! pseudoterminal. It is the wrong pipe for this payload. The thing being
//! carried is durable and a terminal is not — a pane that scrolls has lost it.
//! A full-screen agent repaints, so it would re-emit its own surfaces several
//! times a second. And `gridwire`'s encoder runs in BOTH halves of the
//! client-server split, so carrying surfaces in band would put an encoder
//! change on the host, which only upgrades by dying.
//!
//! A file costs none of that. It outlives the pane, which an artifact has to;
//! any agent in any language can write one; the host never sees it; and there
//! is no version to negotiate on a hot path.
//!
//! # The bench survives a restart, and the attention row does not
//!
//! That difference is deliberate and the two surfaces are answering different
//! questions. `declare_deliverable` puts what a TURN produced on the attention
//! rail, and a turn does not survive the process that ran it — a restored pane
//! offering yesterday's report as what it just made is a stale claim. A
//! workbench holds ARTIFACTS, which are things with names that someone acts
//! on, and those are supposed to still be there on Thursday. So this directory
//! is re-read when a window opens, and the rail's row is not.
//!
//! # The answer goes back down the terminal
//!
//! There is already a two-way pipe to a program waiting for a human to say
//! something, and it is the pane. When a person presses a button, Terminal
//! Delight types one readable line into that agent's own terminal and appends
//! the same thing as JSON to an action journal beside the surfaces. An agent
//! that has never heard of this protocol still receives a plain English
//! instruction; one that has can read the journal and get the structure.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::surface::{parse_lenient, ActionReport, Post};

/// Where a session's surfaces live.
///
/// `$XDG_STATE_HOME` rather than the runtime directory: state is the one that
/// is supposed to survive a reboot, and an artifact that vanished when the
/// machine restarted would be the durability promise broken by its own
/// filesystem.
pub fn surfaces_root() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::session::home_dir().join(".local/state"));
    base.join("terminal-delight/surfaces")
}

/// One pane's drop box. The agent already knows both halves of this path: the
/// host puts `TD_SESSION` and `TD_PANE_ID` in every pane's environment.
pub fn pane_dir(session: &str, pane: u64) -> PathBuf {
    surfaces_root()
        .join(sanitise(session))
        .join(pane.to_string())
}

/// This window's own session key.
static SESSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Record which session this window resolved to, once, at startup.
///
/// Deliberately not read from `$TD_SESSION`. A Terminal Delight window is very
/// often launched from inside another Terminal Delight pane, so that variable
/// names the PARENT's session — and a window watching its parent's directory
/// would show one fleet's work on another fleet's benches.
pub fn adopt_session(key: &str) {
    let _ = SESSION.set(key.to_string());
}

pub fn session() -> Option<&'static str> {
    SESSION.get().map(String::as_str)
}

/// The directory this window watches: every pane of its own session.
pub fn session_dir() -> Option<PathBuf> {
    session().map(|key| surfaces_root().join(sanitise(key)))
}

/// Where this pane's answers are appended.
pub fn actions_path(session: &str, pane: u64) -> PathBuf {
    pane_dir(session, pane).join("actions.jsonl")
}

/// Keep a session key from walking out of its own directory.
///
/// Dots are dropped rather than kept-and-checked. A filter that allowed them
/// and then looked for `..` is one clever encoding away from being wrong;
/// a path segment made only of letters, digits, dashes and underscores cannot
/// name a parent at all.
fn sanitise(key: &str) -> String {
    let cleaned: String = key
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(64)
        .collect();
    if cleaned.is_empty() {
        "default".into()
    } else {
        cleaned
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// the sweep
// ---------------------------------------------------------------------------

/// What one file looked like last time, so an unchanged directory costs a
/// readdir and nothing else.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Stamp {
    modified_ms: u64,
    len: u64,
}

/// Remembers which files have already been taken, per pane.
#[derive(Default, Debug)]
pub struct Feed {
    seen: HashMap<PathBuf, Stamp>,
}

/// One pane's worth of arrivals.
#[derive(Debug)]
pub struct Arrivals {
    pub pane: u64,
    pub posts: Vec<Post>,
}

impl Feed {
    pub fn new() -> Feed {
        Feed::default()
    }

    /// Read every pane directory under `root/session`, and return what is new.
    ///
    /// Runs on the background pool: it is a readdir per live pane and a read
    /// only of files whose size or mtime moved.
    pub fn sweep(&mut self, session_dir: &Path, now: u64) -> Vec<Arrivals> {
        let Ok(entries) = fs::read_dir(session_dir) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(pane) = path
                .file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.parse::<u64>().ok())
            else {
                continue; // not a pane directory; leave whatever it is alone
            };
            let posts = self.sweep_pane(&path, now);
            if !posts.is_empty() {
                out.push(Arrivals { pane, posts });
            }
        }
        out
    }

    /// One pane directory, oldest file first so the bench sees arrivals in the
    /// order they were made rather than in whatever order the directory hands
    /// them over.
    pub fn sweep_pane(&mut self, dir: &Path, now: u64) -> Vec<Post> {
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut candidates: Vec<(PathBuf, Stamp)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue; // actions.jsonl lives here too, and is ours not theirs
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let stamp = Stamp {
                modified_ms: meta
                    .modified()
                    .ok()
                    .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                len: meta.len(),
            };
            if self.seen.get(&path) == Some(&stamp) {
                continue;
            }
            candidates.push((path, stamp));
        }
        candidates.sort_by_key(|(path, stamp)| (stamp.modified_ms, path.clone()));

        let mut posts = Vec::new();
        for (path, stamp) in candidates {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            // A file being written right now parses as nothing. Do NOT record
            // the stamp in that case: the next sweep must try again, or a
            // surface is lost to having been caught mid-write.
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            self.seen.insert(path.clone(), stamp);
            let fallback = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("surface");
            posts.push(parse_lenient(&value, now, fallback));
        }
        posts
    }
}

// ---------------------------------------------------------------------------
// the transcript fence
// ---------------------------------------------------------------------------

/// Pull `td` fenced blocks out of a piece of agent prose.
///
/// The zero-cooperation transport: an agent that prints
///
/// ````text
/// ```td
/// { "td": "0.1", "kind": "markdown", ... }
/// ```
/// ````
///
/// has presented a surface, whether or not it knows this window exists. Read
/// from the agent's own JSONL transcript rather than from the screen —
/// [`crate::mcp_tail`] is already doing that walk for the tab face — so this
/// never guesses at a rendering the way scraping the grid would.
pub fn fenced_blocks(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("```td") {
        let after = &rest[at + 5..];
        // ```td and ```tdsp both count; ```tdx does not.
        let after = match after.strip_prefix("sp") {
            Some(s) => s,
            None => after,
        };
        let Some(nl) = after.find('\n') else { break };
        if !after[..nl].trim().is_empty() {
            rest = &after[nl..];
            continue; // ```tdsomething — a different language, left alone
        }
        let body_start = nl + 1;
        let Some(end) = after[body_start..].find("```") else {
            break; // an unclosed fence is a block still being written
        };
        let body = &after[body_start..body_start + end];
        if let Ok(value) = serde_json::from_str::<Value>(body.trim()) {
            out.push(value);
        }
        rest = &after[body_start + end..];
    }
    out
}

// ---------------------------------------------------------------------------
// the answer
// ---------------------------------------------------------------------------

/// Append a person's answer to the pane's action journal.
///
/// Best-effort on purpose: the typed line into the agent's terminal is the
/// channel that matters, and a full disk must not swallow the button press.
pub fn journal(path: &Path, report: &ActionReport) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut line = serde_json::to_string(&report.to_json()).unwrap_or_default();
    line.push('\n');
    file.write_all(line.as_bytes())
}

/// Write a surface from inside this process — the CLI's front door, and the
/// path every test uses to put something on a bench.
pub fn drop_surface(dir: &Path, name: &str, value: &Value) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{}.json", sanitise(name)));
    // Written beside and renamed into place, so a sweep can never read half a
    // file. The parse guard above would survive it; this means it never has to.
    let temp = dir.join(format!(".{}.json.part", sanitise(name)));
    fs::write(&temp, serde_json::to_vec_pretty(value)?)?;
    fs::rename(&temp, &path)?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// the demo
// ---------------------------------------------------------------------------

/// One of every kind, written through the real transport.
///
/// `TD_WORKBENCH_DEMO=1` puts these on the first pane's bench at startup. They
/// are ordinary files in the ordinary directory — nothing about the demo path
/// is special-cased in the UI, which is the point: what a screenshot of this
/// shows is what an agent would get, not a mock of it.
pub fn demo_surfaces() -> Vec<(&'static str, Value)> {
    // Numbered, because a directory listing is sorted and the first file is
    // the one the bench opens on. Left to the kind names, the demo opened on
    // `demo-architecture` — alphabetically first, and not the thing anyone
    // should see first.
    vec![
        (
            "01-decision",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "decision", "id": "demo-decision",
                "title": "Where the workbench lives",
                "weight": { "effort": "large", "complexity": "involved",
                            "confidence": "inferred",
                            "foundation": { "system": "pane chrome", "depth": "subsystem" } },
                "model": {
                    "question": "Does the artifact rail belong to the pane or to the window?",
                    "options": [
                        { "name": "In the pane", "recommended": true,
                          "case": "Scoped to one conversation, and it inherits the pane's skin.",
                          "cost": "Twenty panes means twenty rails; none of them is a fleet view." },
                        { "name": "In the window",
                          "case": "One place to look across the whole wall.",
                          "cost": "A rail that mixes twenty conversations is a feed." }
                    ],
                    "consequences": [
                        "The attention spine keeps answering 'which pane wants me'.",
                        "A fleet-wide view, if it is ever wanted, is a second surface."
                    ]
                }
            }),
        ),
        (
            "02-architecture",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "architecture", "id": "demo-architecture",
                "title": "How a surface reaches the bench",
                "weight": { "confidence": "measured" },
                "model": {
                    "nodes": [
                        { "id": "agent", "label": "agent", "state": "running", "group": "pane" },
                        { "id": "verb", "label": "present_surface", "group": "mcp" },
                        { "id": "file", "label": "surfaces/<pane>/*.json", "group": "disk" },
                        { "id": "feed", "label": "surfacefeed", "state": "1s sweep", "group": "window" },
                        { "id": "bench", "label": "the bench", "group": "window" }
                    ],
                    "edges": [
                        { "from": "agent", "to": "verb", "label": "one JSON document" },
                        { "from": "verb", "to": "file" },
                        { "from": "file", "to": "feed", "label": "watched" },
                        { "from": "feed", "to": "bench" },
                        { "from": "bench", "to": "agent", "label": "[workbench] …" }
                    ]
                }
            }),
        ),
        (
            "03-changeset",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "changeset", "id": "demo-changeset",
                "title": "Flatten the tube while the bench is up",
                "weight": { "effort": "small", "complexity": "moderate", "confidence": "measured",
                            "foundation": { "system": "CRT pass", "depth": "component" } },
                "model": {
                    "repository": "terminal-delight",
                    "hunks": [
                        { "id": "pane.rs#warp", "file": "app/src/pane.rs",
                          "patch": "-let (k1, k2) = warp_coeffs(th.warp);\n+let (k1, k2) = if on_bench {\n+    (0.0, 0.0)\n+} else {\n+    warp_coeffs(th.warp)\n+};" },
                        { "id": "pane.rs#body", "file": "app/src/pane.rs",
                          "patch": "+.child(if on_bench { bench_el } else { grid })" }
                    ]
                }
            }),
        ),
        (
            "04-report",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "artifact", "id": "demo-report",
                "title": "The Workbench brief",
                "weight": { "confidence": "measured" },
                "model": { "href": "/home/parker/Work/reports/2026-09-17-the-workbench.html",
                           "mime": "text/html",
                           "summary": "Why the agent describes meaning and the window owns the pixels" }
            }),
        ),
        (
            "05-table",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "table", "id": "demo-table",
                "title": "Transports, compared",
                "model": {
                    "columns": ["transport", "needs", "durable", "crosses the split"],
                    "rows": [
                        ["MCP verb", "a connection", "no", "yes"],
                        ["file drop", "a filesystem", "yes", "yes"],
                        ["```td fence", "nothing", "yes", "yes"],
                        ["escape sequence", "an encoder change", "no", null]
                    ]
                }
            }),
        ),
        (
            "06-unclassified",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "hologram", "id": "demo-unclassified",
                "title": "A kind this build has never heard of",
                "model": { "spin": 3 }
            }),
        ),
    ]
}

/// The version the demo payloads claim. Kept beside them so a protocol bump
/// that would reject them fails a test here rather than showing a bench full
/// of `unclassified` in a screenshot.
const TDSP_DEMO_VERSION: &str = crate::surface::TDSP_VERSION;

/// Write the demo set into a pane's directory.
pub fn seed_demo(dir: &Path) -> std::io::Result<usize> {
    let mut n = 0;
    for (name, doc) in demo_surfaces() {
        drop_surface(dir, name, &doc)?;
        n += 1;
    }
    Ok(n)
}

// ---------------------------------------------------------------------------
// the command line
// ---------------------------------------------------------------------------

/// `terminal-delight surface [file]` — the transport that needs nothing.
///
/// No MCP connection, no Rust, no cooperation beyond being able to run a
/// command: an agent writes a document (or just prints prose with a fenced
/// `td` block in it) and pipes it here. The pane it lands on is the pane it
/// was run from, read out of the environment the host already set.
///
/// Exit codes are for scripts: 0 accepted, 2 nothing usable in the input, 3
/// not running inside a Terminal Delight pane.
pub fn run_cli(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--catalogue" || a == "--catalog") {
        println!(
            "{}",
            serde_json::to_string_pretty(&crate::surface::catalogue()).unwrap_or_default()
        );
        return 0;
    }
    // `--derive <transcript>` — what this build would put on a bench for an
    // agent that never called anything. The read-back verb for the derived
    // half: a data layer nobody can inspect is a data layer nobody can debug,
    // and this is how a missing surface gets diagnosed without a screenshot.
    if let Some(at) = args.iter().position(|a| a == "--derive") {
        let Some(path) = args.get(at + 1) else {
            eprintln!("terminal-delight surface --derive <transcript.jsonl>");
            return 2;
        };
        // A file that is not there is not a file with nothing in it.
        //
        // `from_transcript` cannot tell the two apart — it answers with an
        // empty list either way — so this verb was reporting a typo'd path as
        // a quiet, successful "nothing derivable", exit code and all. Anyone
        // diagnosing a missing surface would have read that as *the transcript
        // holds nothing worth showing* and gone looking in the wrong half of
        // the system. Found by the regression suite on its first run.
        let file = Path::new(path);
        if !file.exists() {
            eprintln!("terminal-delight surface --derive: no such transcript: {path}");
            return 2;
        }
        let posts = crate::derive::from_transcript(file, now_ms());
        if posts.is_empty() {
            println!("nothing derivable in {path}");
            return 0;
        }
        for post in &posts {
            let Some(s) = post.surface.as_ref() else {
                continue;
            };
            println!(
                "{:<12} {:<14} {}\n{:>13}{}",
                s.kind.id(),
                post.id.as_str(),
                s.title,
                "",
                s.subtitle()
            );
        }
        return 0;
    }

    let (Ok(key), Ok(pane)) = (
        std::env::var("TD_SESSION"),
        std::env::var("TD_PANE_ID").map(|p| p.parse::<u64>()),
    ) else {
        eprintln!(
            "terminal-delight surface: not inside a Terminal Delight pane \
             (TD_SESSION and TD_PANE_ID are unset)."
        );
        return 3;
    };
    let Ok(pane) = pane else {
        eprintln!("terminal-delight surface: TD_PANE_ID is not a number.");
        return 3;
    };

    let text = match args.iter().find(|a| !a.starts_with('-')) {
        Some(path) => match fs::read_to_string(path) {
            Ok(t) => t,
            Err(err) => {
                eprintln!("terminal-delight surface: cannot read {path}: {err}");
                return 2;
            }
        },
        None => {
            use std::io::Read;
            let mut buf = String::new();
            if let Err(err) = std::io::stdin().read_to_string(&mut buf) {
                eprintln!("terminal-delight surface: cannot read stdin: {err}");
                return 2;
            }
            buf
        }
    };

    // A whole document, or the fenced blocks inside a piece of prose. Trying
    // the document first means an agent that pipes clean JSON never has its
    // payload searched for fences it does not have.
    let docs: Vec<Value> = match serde_json::from_str::<Value>(text.trim()) {
        Ok(v) => vec![v],
        Err(_) => fenced_blocks(&text),
    };
    if docs.is_empty() {
        eprintln!(
            "terminal-delight surface: nothing usable in the input — expected a \
             TDSP document or a ```td fenced block. `--catalogue` prints the kinds."
        );
        return 2;
    }

    let dir = pane_dir(&key, pane);
    let mut wrote = 0;
    for (i, doc) in docs.iter().enumerate() {
        // Named by the surface's own id where it has one, so a second run of
        // the same document updates the row instead of stacking a duplicate.
        let name = doc
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}-{i}", now_ms()));
        match drop_surface(&dir, &name, doc) {
            Ok(path) => {
                wrote += 1;
                println!("{}", path.display());
            }
            Err(err) => eprintln!("terminal-delight surface: cannot write: {err}"),
        }
    }
    i32::from(wrote == 0) * 2
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: u64 = 1_758_000_000_000;

    /// A scratch directory that cleans up after itself, since this crate has no
    /// tempfile dependency and one test directory is not worth adding one.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!(
                "td-surfacefeed-{}-{}-{:?}",
                name,
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).expect("scratch");
            Scratch(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn a_doc(title: &str) -> Value {
        json!({ "td": "0.1", "kind": "markdown", "title": title, "model": { "body": "b" } })
    }

    #[test]
    fn a_dropped_file_is_taken_once_and_not_again() {
        let scratch = Scratch::new("once");
        let mut feed = Feed::new();
        drop_surface(scratch.path(), "first", &a_doc("First")).unwrap();

        let posts = feed.sweep_pane(scratch.path(), NOW);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].surface.as_ref().unwrap().title, "First");

        assert!(
            feed.sweep_pane(scratch.path(), NOW).is_empty(),
            "an unchanged file is not delivered twice"
        );
    }

    #[test]
    fn a_rewritten_file_is_taken_again() {
        let scratch = Scratch::new("rewrite");
        let mut feed = Feed::new();
        drop_surface(scratch.path(), "s", &a_doc("First")).unwrap();
        feed.sweep_pane(scratch.path(), NOW);
        // A longer title means a longer file, so the stamp moves even on a
        // filesystem whose mtime resolution is coarse.
        drop_surface(scratch.path(), "s", &a_doc("Second and longer")).unwrap();
        let posts = feed.sweep_pane(scratch.path(), NOW);
        assert_eq!(posts.len(), 1);
        assert_eq!(
            posts[0].surface.as_ref().unwrap().title,
            "Second and longer"
        );
    }

    #[test]
    fn a_half_written_file_is_retried_rather_than_recorded() {
        let scratch = Scratch::new("partial");
        let mut feed = Feed::new();
        let path = scratch.path().join("broken.json");
        fs::write(&path, b"{ \"td\": \"0.1\", \"kind\": \"mark").unwrap();
        assert!(
            feed.sweep_pane(scratch.path(), NOW).is_empty(),
            "nothing yet"
        );

        fs::write(&path, serde_json::to_vec(&a_doc("Complete")).unwrap()).unwrap();
        let posts = feed.sweep_pane(scratch.path(), NOW);
        assert_eq!(
            posts.len(),
            1,
            "the finished file is picked up on the next pass"
        );
        assert_eq!(posts[0].surface.as_ref().unwrap().title, "Complete");
    }

    #[test]
    fn garbage_json_still_becomes_a_surface_rather_than_vanishing() {
        let scratch = Scratch::new("garbage");
        let mut feed = Feed::new();
        fs::write(
            scratch.path().join("weird.json"),
            serde_json::to_vec(&json!({ "hello": "world" })).unwrap(),
        )
        .unwrap();
        let posts = feed.sweep_pane(scratch.path(), NOW);
        assert_eq!(posts.len(), 1);
        let s = posts[0].surface.as_ref().unwrap();
        assert_eq!(s.kind.id(), "unclassified");
        assert_eq!(
            s.title, "weird",
            "the filename names it when the payload does not"
        );
    }

    #[test]
    fn only_json_is_read_so_the_journal_is_not_mistaken_for_a_surface() {
        let scratch = Scratch::new("journal");
        let mut feed = Feed::new();
        fs::write(scratch.path().join("actions.jsonl"), b"{\"td\":\"0.1\"}\n").unwrap();
        fs::write(scratch.path().join("notes.txt"), b"hello").unwrap();
        assert!(feed.sweep_pane(scratch.path(), NOW).is_empty());
    }

    #[test]
    fn files_arrive_oldest_first() {
        let scratch = Scratch::new("order");
        let mut feed = Feed::new();
        // Written with explicit, distinct mtimes rather than trusting the clock
        // to tick between two writes in the same millisecond.
        for (name, title) in [("a", "First"), ("b", "Second"), ("c", "Third")] {
            drop_surface(scratch.path(), name, &a_doc(title)).unwrap();
        }
        let posts = feed.sweep_pane(scratch.path(), NOW);
        assert_eq!(posts.len(), 3);
        let titles: Vec<&str> = posts
            .iter()
            .map(|p| p.surface.as_ref().unwrap().title.as_str())
            .collect();
        // Same-millisecond writes fall back to filename order, which is the
        // order they were created in here — the assertion that matters is that
        // it is deterministic, not which rule broke the tie.
        assert_eq!(titles, vec!["First", "Second", "Third"]);
    }

    #[test]
    fn the_session_sweep_maps_directories_to_pane_ids() {
        let scratch = Scratch::new("session");
        let mut feed = Feed::new();
        for pane in ["7", "12", "not-a-pane"] {
            let dir = scratch.path().join(pane);
            drop_surface(&dir, "s", &a_doc(&format!("pane {pane}"))).unwrap();
        }
        let mut arrivals = feed.sweep(scratch.path(), NOW);
        arrivals.sort_by_key(|a| a.pane);
        assert_eq!(
            arrivals.len(),
            2,
            "a directory that is not a pane id is left alone"
        );
        assert_eq!(arrivals[0].pane, 7);
        assert_eq!(arrivals[1].pane, 12);
    }

    #[test]
    fn a_fresh_window_reads_the_whole_directory_back() {
        // The durability promise: surfaces are artifacts, and a window that
        // opens tomorrow finds yesterday's still on the bench. A new window is
        // a new `Feed` with nothing recorded, so this is the property that
        // makes that true rather than a separate restore path.
        let scratch = Scratch::new("restart");
        let mut first = Feed::new();
        for name in ["a", "b", "c"] {
            drop_surface(scratch.path(), name, &a_doc(name)).unwrap();
        }
        assert_eq!(first.sweep_pane(scratch.path(), NOW).len(), 3);
        assert!(first.sweep_pane(scratch.path(), NOW).is_empty());

        let mut after_restart = Feed::new();
        assert_eq!(
            after_restart.sweep_pane(scratch.path(), NOW).len(),
            3,
            "the bench is rebuilt from disk, not from memory"
        );
    }

    #[test]
    fn a_fenced_block_in_prose_is_a_surface() {
        let text = "Here is what I found.\n\n```td\n{\"td\":\"0.1\",\"kind\":\"markdown\",\
                    \"title\":\"Finding\",\"model\":{\"body\":\"b\"}}\n```\n\nAnything else.";
        let blocks = fenced_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0]["title"], json!("Finding"));
    }

    #[test]
    fn several_fences_in_one_message_all_count() {
        let text = "```td\n{\"a\":1}\n```\nmiddle\n```td\n{\"b\":2}\n```";
        assert_eq!(fenced_blocks(text).len(), 2);
    }

    #[test]
    fn a_tdsp_fence_counts_and_a_lookalike_does_not() {
        assert_eq!(fenced_blocks("```tdsp\n{\"a\":1}\n```").len(), 1);
        assert_eq!(
            fenced_blocks("```tdx\n{\"a\":1}\n```").len(),
            0,
            "a language that merely starts with td is somebody else's"
        );
    }

    #[test]
    fn an_unclosed_fence_is_left_for_when_it_finishes() {
        assert!(fenced_blocks("```td\n{\"a\":1}\n").is_empty());
    }

    #[test]
    fn a_fence_holding_prose_rather_than_json_is_skipped_quietly() {
        assert!(fenced_blocks("```td\nnot json at all\n```").is_empty());
    }

    #[test]
    fn the_journal_appends_one_line_per_answer() {
        use crate::surface::{Action, SurfaceId};
        let scratch = Scratch::new("append");
        let path = scratch.path().join("actions.jsonl");
        for verb in [Action::Approve, Action::Reject] {
            journal(
                &path,
                &ActionReport {
                    surface: SurfaceId("s1".into()),
                    action: verb,
                    target: None,
                    comment: None,
                },
            )
            .unwrap();
        }
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text
            .lines()
            .all(|l| serde_json::from_str::<Value>(l).is_ok()));
    }

    #[test]
    fn a_session_key_cannot_walk_out_of_its_own_directory() {
        let dir = pane_dir("../../etc", 1);
        assert!(!dir.to_string_lossy().contains(".."), "{dir:?}");
        assert!(dir.starts_with(surfaces_root()));
    }

    #[test]
    fn every_demo_payload_is_one_this_build_actually_renders() {
        // A demo is a screenshot waiting to happen, and a screenshot of six
        // `unclassified` blocks would be a protocol bump nobody noticed. Five
        // of the six must parse strictly; the sixth is the unknown-kind
        // specimen and must NOT, or it has stopped demonstrating anything.
        let mut typed = 0;
        let mut unclassified = 0;
        for (name, doc) in demo_surfaces() {
            match crate::surface::parse(&doc, NOW) {
                Ok(post) => {
                    let s = post.surface.expect("a surface");
                    assert_ne!(s.kind.id(), "unclassified", "{name}");
                    typed += 1;
                }
                Err(why) => {
                    assert!(name.contains("unclassified"), "{name} is broken: {why}");
                    unclassified += 1;
                }
            }
        }
        assert_eq!(typed, 5);
        assert_eq!(unclassified, 1);
    }

    #[test]
    fn the_demo_covers_every_shelf_so_no_tab_is_photographed_empty() {
        use crate::surface::Shelf;
        let mut shelves: Vec<Shelf> = Vec::new();
        for (_, doc) in demo_surfaces() {
            let post = crate::surface::parse_lenient(&doc, NOW, "demo");
            if let Some(s) = post.surface {
                let shelf = s.kind.shelf();
                if !shelves.contains(&shelf) {
                    shelves.push(shelf);
                }
            }
        }
        for shelf in Shelf::ALL {
            assert!(
                shelves.contains(&shelf),
                "{shelf:?} has nothing in the demo"
            );
        }
    }

    #[test]
    fn the_demo_arrives_decision_first() {
        // Files land in the same millisecond, so the sweep's tie-break is the
        // filename — which is why the demo's are numbered. Nothing is
        // selected by an arrival any more (the rail is a shelf, not a remote
        // control), so what this pins is the ORDER, which is what the rail
        // shows and what the numbering exists to control.
        let scratch = Scratch::new("demo-order");
        seed_demo(scratch.path()).expect("seeded");
        let mut feed = Feed::new();
        let posts = feed.sweep_pane(scratch.path(), NOW);
        let kinds: Vec<&str> = posts
            .iter()
            .filter_map(|p| p.surface.as_ref())
            .map(|s| s.kind.id())
            .collect();
        assert_eq!(kinds.first(), Some(&"decision"), "{kinds:?}");

        let mut bench = crate::workbench::Bench::new();
        bench.set_face(crate::workbench::Face::Workbench);
        for post in posts {
            bench.apply(post);
        }
        assert!(
            bench.selected().is_none(),
            "an arrival opens nothing; the conversation is the default"
        );
        assert_eq!(bench.rows_for(crate::surface::Shelf::Decisions).len(), 1);
    }
    #[test]
    fn seeding_the_demo_lands_as_readable_files() {
        let scratch = Scratch::new("demo");
        let n = seed_demo(scratch.path()).expect("seeded");
        let mut feed = Feed::new();
        let posts = feed.sweep_pane(scratch.path(), NOW);
        assert_eq!(
            posts.len(),
            n,
            "every seeded file is picked up by the real sweep"
        );
    }

    #[test]
    fn a_pane_directory_is_where_the_agents_environment_says_it_is() {
        // The contract the launch briefing relies on: an agent can compute this
        // path itself from TD_SESSION and TD_PANE_ID, both of which the host
        // already puts in every pane's environment.
        let dir = pane_dir("abc123", 7);
        assert!(dir.ends_with("abc123/7"), "{dir:?}");
        assert_eq!(actions_path("abc123", 7), dir.join("actions.jsonl"));
    }
}
