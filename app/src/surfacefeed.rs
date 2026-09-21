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

/// This session's tag: what the bench types in front of every line it puts
/// into a pane. See [`crate::hostproto::session_tag`].
static TAG: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Record the session tag, once, beside the key.
pub fn adopt_tag(tag: String) {
    let _ = TAG.set(tag);
}

pub fn tag() -> Option<&'static str> {
    TAG.get().map(String::as_str)
}

/// The directory this window watches: every pane of its own session.
pub fn session_dir() -> Option<PathBuf> {
    session().map(|key| surfaces_root().join(sanitise(key)))
}

/// Where this pane's answers are appended.
pub fn actions_path(session: &str, pane: u64) -> PathBuf {
    pane_dir(session, pane).join("actions.jsonl")
}

/// Where the window records what it did through the agent channel, before it
/// does it. See `docs/spec/td-agent-channel.md` §4.
pub fn outbound_path(session: &str, pane: u64) -> PathBuf {
    pane_dir(session, pane).join("outbound.jsonl")
}

/// The window's liveness marker for a pane — what a hook reads before it
/// holds a picker for the bench. Written beside and renamed, so a hook never
/// reads half of it.
pub fn write_marker(dir: &Path, bench_open: bool, now_ms: u64) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let value = crate::channel::marker(bench_open, now_ms, std::process::id());
    let temp = dir.join(format!(".bench.{}.part", std::process::id()));
    fs::write(&temp, serde_json::to_vec(&value)?)?;
    fs::rename(&temp, dir.join("bench.json"))
}

/// The answer to a question a hook is holding open, as the file it is
/// polling for. Under `answers/`, named by the same filter the hook applies
/// to the tool-use id, written beside and renamed.
pub fn write_answers(
    dir: &Path,
    tool_use_id: &str,
    answers: &serde_json::Map<String, Value>,
) -> std::io::Result<PathBuf> {
    let key = crate::channel::file_key(tool_use_id);
    let folder = dir.join("answers");
    fs::create_dir_all(&folder)?;
    let value = crate::channel::answers_json(tool_use_id, answers);
    let temp = folder.join(format!(".{key}.{}.part", std::process::id()));
    fs::write(&temp, serde_json::to_vec(&value)?)?;
    let path = folder.join(format!("{key}.json"));
    fs::rename(&temp, &path)?;
    Ok(path)
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
    /// How far into each pane's inbound journal the reader has got, in bytes.
    /// An unchanged journal then costs one `metadata` call.
    offsets: HashMap<PathBuf, u64>,
}

/// One pane's worth of arrivals.
#[derive(Debug)]
pub struct Arrivals {
    pub pane: u64,
    pub posts: Vec<Post>,
    /// What the pane's inbound journal gained — the agent channel's half.
    pub events: Vec<crate::channel::Inbound>,
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
            let events = self.tail_inbound(&path);
            if !posts.is_empty() || !events.is_empty() {
                out.push(Arrivals {
                    pane,
                    posts,
                    events,
                });
            }
        }
        out
    }

    /// New lines of the pane's inbound journal since the last sweep.
    ///
    /// A byte offset per file. A trailing line with no newline yet is a line
    /// still being written and is left for the next sweep, offset unmoved —
    /// the same rule [`Feed::sweep_pane`] applies to a file caught mid-write.
    /// A journal that shrank was rotated or truncated, and is read from the
    /// start again rather than from an offset past its end.
    pub fn tail_inbound(&mut self, dir: &Path) -> Vec<crate::channel::Inbound> {
        use std::io::{Read, Seek, SeekFrom};
        let path = dir.join("inbound.jsonl");
        let Ok(meta) = fs::metadata(&path) else {
            return Vec::new();
        };
        let len = meta.len();
        let at = self.offsets.get(&path).copied().unwrap_or(0);
        let at = if len < at { 0 } else { at };
        if len == at {
            return Vec::new();
        }
        let Ok(mut file) = fs::File::open(&path) else {
            return Vec::new();
        };
        if file.seek(SeekFrom::Start(at)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::with_capacity((len - at) as usize);
        if file.take(len - at).read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        let Some(complete) = buf.iter().rposition(|b| *b == b'\n').map(|i| i + 1) else {
            return Vec::new();
        };
        let text = String::from_utf8_lossy(&buf[..complete]);
        let mut out = Vec::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            out.push(crate::channel::Inbound::parse_line(line).unwrap_or(
                crate::channel::Inbound::Unknown {
                    type_name: "a line that is not a record".into(),
                },
            ));
        }
        self.offsets.insert(path, at + complete as u64);
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
            // The channel's liveness marker is a `.json` in the same directory
            // and it is not a surface: swept as one it would land on every
            // bench as an `unclassified` card, once a second, forever.
            if path.file_name().and_then(|n| n.to_str()) == Some("bench.json") {
                continue;
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
            let mut post = parse_lenient(&value, now, fallback);
            // A file in the drop box: any process running as this user could
            // have put it there, and the card says exactly that.
            if let Some(s) = post.surface.as_mut() {
                s.origin = crate::surface::Origin::FileDrop;
            }
            posts.push(post);
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

/// Append one JSON event that is not an action report — a launch, say — to a
/// journal of its own beside the action ones. Same append, same one-object-
/// per-line shape, so a reader walking either file needs one parser.
pub fn journal_event(path: &Path, event: &Value) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let mut line = serde_json::to_string(event).unwrap_or_default();
    line.push('\n');
    file.write_all(line.as_bytes())
}

/// Write a surface from inside this process — the CLI's front door, and the
/// path every test uses to put something on a bench.
pub fn drop_surface(dir: &Path, name: &str, value: &Value) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let safe = sanitise(name);
    let path = dir.join(format!("{safe}.json"));
    // THE FILE'S NAME AND THE DOCUMENT'S ID ARE THE SAME THING, so make it so
    // on the way out. `parse` invents an `anon-<hash>` id for a document that
    // carries none — and invents it from the content, so the same surface
    // written twice is fine but a surface filed under a name nobody put inside
    // it comes back from disk answering to something else. The row it was
    // meant to update becomes a second row, and a retire aimed at the id it
    // had while it was live no longer matches the file holding it.
    //
    // Only fills a gap: a document that names itself is left exactly as the
    // agent sent it, because the id is the agent's to choose.
    let value = &match value.as_object() {
        Some(map) if !map.contains_key("id") => {
            let mut owned = map.clone();
            owned.insert("id".into(), Value::String(safe.clone()));
            Value::Object(owned)
        }
        _ => value.clone(),
    };
    // Written beside and renamed into place, so a sweep can never read half a
    // file. The parse guard above would survive it; this means it never has to.
    //
    // **The temp name carries the writer's identity, and that is not cosmetic.**
    // It used to be `.{name}.json.part`, one path derived only from the surface
    // id — so two writers updating the SAME id at the same time both wrote that
    // one file, the first rename took it, and the second failed with `ENOENT`.
    // The loser's surface was silently dropped and the error named the wrong
    // cause: `cannot write: No such file or directory` reads as a missing
    // directory, which sends whoever debugs it to `create_dir_all` rather than
    // to the collision. Found by running the adversarial suite in parallel,
    // where it failed two or three cases per run and a different set each time;
    // serially it always passed. Updating one surface repeatedly is the NORMAL
    // path — `present_surface` re-sends the same id to update a row in place —
    // so this is reachable by one agent on its own, not only by two.
    // A COUNTER, not a timestamp. The first fix here used nanoseconds and still
    // lost three writes out of sixteen: threads starting together read the
    // clock too close to be separated by it, so the name was unique only if the
    // clock happened to be fine-grained enough. A monotonic counter is unique by
    // construction within the process, and the pid separates processes.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let ticket = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temp = dir.join(format!(".{safe}.{}.{ticket}.json.part", std::process::id()));
    if let Err(err) = fs::write(&temp, serde_json::to_vec_pretty(value)?) {
        let _ = fs::remove_file(&temp);
        return Err(err);
    }
    if let Err(err) = fs::rename(&temp, &path) {
        // Never leave a `.part` behind for the sweep to trip over.
        let _ = fs::remove_file(&temp);
        return Err(err);
    }
    Ok(path)
}

/// Take a surface off the disk, by the id it was written under.
///
/// The counterpart to [`drop_surface`], and it exists for the same reason the
/// write does: a retire that only reached the live window would come straight
/// back on the next restart, because the restore path reads this directory.
/// Returns whether a file was actually there — absent is the normal case for a
/// surface that was never persisted, not a failure.
pub fn retire_surface(dir: &Path, name: &str) -> bool {
    fs::remove_file(dir.join(format!("{}.json", sanitise(name)))).is_ok()
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
            // WRITTEN THE WAY AGENTS ACTUALLY WRITE ONE, which this fixture
            // was not. It said `href` and nothing else, so the demo — the one
            // screen anybody looks at on purpose — exercised the single
            // spelling that already worked, while the cards arriving on real
            // benches (`open` for the file, `served` for the same page over
            // http, the finding underneath) were landing as `unclassified`
            // with their JSON on show and no click that could open them. A
            // fixture that covers only the working case is how a broken one
            // goes unseen.
            "04-report",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "artifact", "id": "demo-report",
                "title": "The Workbench brief",
                "weight": { "confidence": "measured" },
                "model": { "what": "Why the agent describes meaning and the window owns the pixels",
                           "open": "/home/parker/Work/reports/2026-09-17-the-workbench.html",
                           "served": "http://127.0.0.1:8731/2026-09-17-the-workbench.html",
                           "mime": "text/html",
                           "finding": "Eleven figures — and the only one of three briefs on the same plan that got a reaction." }
            }),
        ),
        (
            // AS WIDE AS A REAL ONE. The cells here were two words each, so
            // the demo's table fitted any pane at any width and the fixture
            // could not show what a table does to a card: four equal columns,
            // every cell clipped at forty characters, and the last one drawn
            // off the right edge of the pane.
            "05-table",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "table", "id": "demo-table",
                "title": "Transports, compared",
                "model": {
                    "columns": ["transport", "what it needs", "durable", "how it behaves when the window is not there"],
                    "rows": [
                        ["MCP verb", "a live connection to this window", "no",
                         "The call fails, and the agent finds out immediately."],
                        ["file drop", "a filesystem and the pane's own directory", "yes",
                         "The document waits on disk and lands when a window opens."],
                        ["```td fence", "nothing at all", "yes",
                         "It stays in the transcript, where a later reader still finds it."],
                        ["escape sequence", "a change to the encoder on both ends", "no", null]
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
        (
            "07-response",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "response", "id": "demo-response",
                "title": "The launcher's two bugs, and what the overview is now",
                "weight": { "effort": "medium", "complexity": "moderate", "confidence": "measured",
                            "foundation": { "system": "workbench", "depth": "component" } },
                "model": {
                    "layman": "Two bugs in the launch panel. One: the effort row offered invented words that were turned into a polite sentence instead of a real setting — now it offers the harness's own levels and passes them as a flag. Two: the panel's height was computed for one row of buttons and it has four, so a short list of matches got no room at all.",
                    "technical": "launcher::Effort is now the union of the harnesses' own levels, ordered; Harness::efforts lists what each takes and clamp_effort finds the nearest when the harness changes. Recipe::command_line emits --effort <id> for Claude and -c model_reasoning_effort=<id> for Codex, and the briefing carries no effort prose. launcher::panel_height sums the chrome the render actually draws (PANEL_CHROME_H) and adds a row per match; a test holds every count up to the cap gets its rows on top of the chrome.",
                    "evidence": {
                        "tests": "cargo test --locked, all green",
                        "claude --help": "--effort <low|medium|high|xhigh|max>, 2.1.270",
                        "on a screen": null
                    },
                    "next": [
                        "install the build and open the launcher on a filter matching three projects",
                        "let one agent finish a turn and read its reply here"
                    ],
                    "doubts": [
                        { "claim": "Codex accepts xhigh", "why": "read off the binary's strings, not its documentation", "confidence": "inferred" },
                        { "claim": "the chrome height holds on a 720-pixel window", "confidence": "hunch" }
                    ]
                }
            }),
        ),
        // The comments board, and the demo is the one place it can be seeded
        // at all: everywhere else a comment is typed by a person, and there is
        // nobody to type one into a screenshot.
        //
        // It arrives through the file transport like the rest of this list,
        // which means it arrives as a DROP and not as `Origin::Person` — so
        // the card reads `dropped as a file · writer unknown` and the row
        // carries its stamp with no `you`. That is not the demo cheating; it is
        // exactly what a board looks like when it is read back off disk after a
        // restart, which is the state a screenshot is most likely to catch.
        (
            "08-comment",
            serde_json::json!({
                "td": TDSP_DEMO_VERSION, "kind": "comment", "id": "demo-comment",
                "model": {
                    "body": "check the phosphor bleed on the LIVE chip at 40% contrast\n\nMight be the chip's own background alpha rather than the border. Look at it before touching the skin file — the two corners in the composer are literals, so a square skin would not square them either."
                }
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
            // Name the DIRECTORY it was writing into. `cannot write: No such
            // file or directory` with no path reads as a missing folder no
            // matter what actually failed, which is exactly how the temp-name
            // collision above stayed hidden.
            Err(err) => eprintln!(
                "terminal-delight surface: cannot write {name:?} into {}: {err}",
                dir.display()
            ),
        }
    }
    i32::from(wrote == 0) * 2
}

// ---------------------------------------------------------------------------
// retention
// ---------------------------------------------------------------------------

/// How many surfaces one pane keeps ON DISK.
///
/// Eight times [`crate::surface::PANE_HISTORY_CAP`], and deliberately not equal
/// to it. That constant's own comment promises "the store on disk is what
/// remembers, and this is what is at hand" — a disk cap equal to the shelf cap
/// would make that sentence false, because nothing would be remembered that was
/// not already at hand. This number is here to bound a runaway writer, not to
/// decide what is worth keeping.
pub const PANE_DISK_CAP: usize = 512;

/// How long a session's directory outlives the session that wrote it.
///
/// The growth that was actually measured is not files piling up inside one
/// pane — it is whole session directories. Nine appeared in one day on this
/// machine, eight of them one-shot demo and screenshot keys that will never be
/// opened again, and every one of them was still there. A session that is gone
/// cannot produce a surface, so its directory has a final mtime, and that is
/// the honest clock to age it by.
pub const DEAD_SESSION_DAYS: u64 = 30;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// What one prune did — and, separately, what it declined to judge.
///
/// `unknown` is its own field on purpose. An entry whose age could not be read
/// is not an entry of age zero, and the two must not reach the caller as the
/// same number: one says nothing was old enough to delete, the other says the
/// question could not be asked. Anything counted here was KEPT.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Pruned {
    /// Whole session directories removed, their panes with them.
    pub sessions: usize,
    /// Individual surface files removed for sitting past [`PANE_DISK_CAP`].
    pub files: usize,
    /// Entries kept because their age could not be established.
    pub unknown: usize,
}

impl Pruned {
    fn add(&mut self, other: Pruned) {
        self.sessions += other.sessions;
        self.files += other.files;
        self.unknown += other.unknown;
    }
}

/// Bound the surface store: drop dead sessions whole, cap what one pane keeps.
///
/// `live` is this window's own session key, and it is not optional in spirit —
/// a caller that does not yet know which directory is its own must not run
/// this, because the rule protecting the live session cannot be applied by a
/// process that cannot name it. Passing `None` therefore prunes nothing at all
/// rather than guessing.
///
/// Nothing here deletes a directory this module could not itself have made: a
/// name that does not survive [`sanitise`] unchanged was written by something
/// else and is left exactly where it is, as is anything that is not a plain
/// directory.
pub fn prune(root: &Path, live: Option<&str>, now: u64) -> Pruned {
    let mut out = Pruned::default();
    let Some(live) = live.map(sanitise) else {
        return out; // no session key: the protection cannot be applied, so nothing is
    };
    let Ok(entries) = fs::read_dir(root) else {
        return out; // no store yet is not a failure; it is the ordinary first run
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if sanitise(name) != name {
            continue; // not a name this module writes
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            out.unknown += 1;
            continue;
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        if name == live {
            out.add(cap_session(&path)); // ours: capped, never removed, however old
            continue;
        }
        match newest_ms(&path) {
            None => out.unknown += 1, // age unknown is not age zero — keep it
            Some(newest) if now.saturating_sub(newest) > DEAD_SESSION_DAYS * DAY_MS => {
                if fs::remove_dir_all(&path).is_ok() {
                    out.sessions += 1;
                }
            }
            Some(_) => out.add(cap_session(&path)),
        }
    }
    out
}

/// The newest mtime anywhere one level down, or `None` if nothing can be read.
///
/// Files first, because a file's mtime is when an agent last actually wrote
/// something; the directory's own mtime is the fallback, which is what an empty
/// session has instead of an answer.
fn newest_ms(session: &Path) -> Option<u64> {
    let mut newest: Option<u64> = None;
    let mut consider = |p: &Path| {
        if let Some(ms) = mtime_ms(p) {
            newest = Some(newest.map_or(ms, |n: u64| n.max(ms)));
        }
    };
    if let Ok(panes) = fs::read_dir(session) {
        for pane in panes.flatten() {
            let dir = pane.path();
            if !dir.is_dir() {
                consider(&dir);
                continue;
            }
            if let Ok(files) = fs::read_dir(&dir) {
                for file in files.flatten() {
                    consider(&file.path());
                }
            }
        }
    }
    newest.or_else(|| mtime_ms(session))
}

fn mtime_ms(path: &Path) -> Option<u64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// Apply [`PANE_DISK_CAP`] to every pane directory of one session.
fn cap_session(session: &Path) -> Pruned {
    let mut out = Pruned::default();
    let Ok(panes) = fs::read_dir(session) else {
        return out;
    };
    for pane in panes.flatten() {
        let dir = pane.path();
        if dir.is_dir() {
            out.add(cap_pane(&dir));
        }
    }
    out
}

/// Keep the newest [`PANE_DISK_CAP`] surfaces in one pane directory.
///
/// Only `.json` is a candidate, which is the same line [`Feed::sweep_pane`]
/// draws: `actions.jsonl` is this window's own journal of what a person
/// pressed, and a retention rule for surfaces has no business deleting it. A
/// file whose mtime cannot be read is not sortable against the others, so it is
/// never a candidate for deletion — it is counted as unknown and kept.
fn cap_pane(dir: &Path) -> Pruned {
    let mut out = Pruned::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    let mut dated: Vec<(u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(meta) = fs::symlink_metadata(&path) else {
            out.unknown += 1;
            continue;
        };
        if meta.is_dir() {
            continue; // a directory named `something.json` is not a surface
        }
        // Through the link rather than at it, because the age that decides this
        // is the age of what was written. A link pointing at nothing therefore
        // has no age at all, and something with no age is never a candidate.
        match mtime_ms(&path) {
            Some(ms) => dated.push((ms, path)),
            None => out.unknown += 1,
        }
    }
    if dated.len() <= PANE_DISK_CAP {
        return out;
    }
    // Newest first, and among a tie the later name first, so a filesystem whose
    // mtime resolution is coarse still evicts deterministically.
    dated.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    for (_, path) in dated.into_iter().skip(PANE_DISK_CAP) {
        if fs::remove_file(&path).is_ok() {
            out.files += 1;
        }
    }
    out
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

    /// What `present_surface` now relies on: a surface written by id can be
    /// read straight back by the path a window opening runs, and retiring it
    /// takes it off the disk rather than only off the live bench.
    ///
    /// The second half is the one worth having. A retire that reached only the
    /// window would look right for the rest of the session and put the surface
    /// back on the next restart, which is the same shape of fault as #567 and
    /// would be found the same slow way.
    #[test]
    fn a_presented_surface_survives_a_sweep_and_a_retire_removes_it() {
        let scratch = Scratch::new("persist");

        let path = drop_surface(scratch.path(), "carried", &a_doc("By the verb")).unwrap();
        assert!(path.exists(), "the verb's surface must reach the disk");

        // the path a window opening takes
        let posts = Feed::new().sweep_pane(scratch.path(), NOW);
        assert_eq!(posts.len(), 1, "a fresh window reads it back");
        assert_eq!(posts[0].id.as_str(), "carried");

        assert!(
            retire_surface(scratch.path(), "carried"),
            "retire must find the file it wrote"
        );
        assert!(!path.exists());
        assert!(
            Feed::new().sweep_pane(scratch.path(), NOW).is_empty(),
            "a retired surface must not come back when a window opens"
        );

        // Retiring something that was never persisted is the normal case for a
        // surface an older build presented, and is not a failure.
        assert!(!retire_surface(scratch.path(), "never-here"));
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
    fn a_dropped_file_is_stamped_as_a_drop_whatever_it_claims() {
        // The payload does not get to say who wrote it: a `writer` or an
        // `origin` field in the JSON is ignored, and the card reads `writer
        // unknown`, because any process running as this user can put a file
        // here and nothing about the bytes says which one did.
        let scratch = Scratch::new("origin");
        let mut feed = Feed::new();
        let mut doc = a_doc("dropped");
        doc["origin"] = json!("this pane's agent");
        doc["writer"] = json!({ "pane": 7 });
        drop_surface(scratch.path(), "s", &doc).unwrap();
        let posts = feed.sweep_pane(scratch.path(), NOW);
        let s = posts[0].surface.as_ref().expect("a surface");
        assert_eq!(s.origin, crate::surface::Origin::FileDrop);
        assert!(s.origin.label().contains("writer unknown"));
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
        // A demo is a screenshot waiting to happen, and a screenshot full of
        // `unclassified` blocks would be a protocol bump nobody noticed. Every
        // payload must parse strictly except one — the unknown-kind specimen,
        // which must NOT, or it has stopped demonstrating anything.
        //
        // Counted against the LIST rather than against a literal. It was `6`
        // and `1`, which meant adding the comments board's demo failed this
        // test for the one reason it is not meant to catch: the list got
        // longer. The property is `all but the specimen`, and written that way
        // it still fails the moment a payload silently degrades — which is the
        // regression this exists for.
        let total = demo_surfaces().len();
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
        assert_eq!(unclassified, 1, "exactly one unknown-kind specimen");
        assert_eq!(typed, total - 1, "every other demo payload must parse");
    }

    #[test]
    fn the_demos_artifact_is_shaped_like_the_ones_that_broke() {
        // The fixture is the verification. A demo artifact written the one
        // way that always worked is a screen nobody can learn anything from,
        // and it is why an artifact card that could not be opened survived on
        // real benches: the thing people look at was not the thing people
        // send. Reverting this payload to a bare `href` passes every other
        // test in this file, so the assertion has to be here.
        let (_, doc) = demo_surfaces()
            .into_iter()
            .find(|(name, _)| name.contains("report"))
            .expect("the demo artifact");
        let model = doc
            .get("model")
            .and_then(Value::as_object)
            .expect("a model");
        assert!(
            !model.contains_key("href"),
            "the demo artifact went back to the spelling that never failed"
        );
        let s = crate::surface::parse(&doc, NOW)
            .expect("it still parses")
            .surface
            .expect("a surface");
        let crate::surface::Kind::Artifact(a) = &s.kind else {
            panic!("the demo artifact is no longer an artifact: {:?}", s.kind);
        };
        assert!(a.href.ends_with(".html"), "{}", a.href);
        assert!(
            !a.notes.is_empty(),
            "and it carries the keys that are drawn under it"
        );
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
        // The decision and the changeset: both are things a person answers.
        assert_eq!(bench.rows_for(crate::surface::Shelf::Decisions).len(), 2);
        // On the overview the newest reply stands in for a card without
        // having been opened — the feed's own default, not an arrival's.
        assert_eq!(
            bench.showing().map(|s| s.kind.id()),
            Some("response"),
            "the overview shows the reply"
        );
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

    // -----------------------------------------------------------------------
    // retention
    // -----------------------------------------------------------------------

    /// Move a file's mtime to an exact instant, so age is a fact of the test
    /// rather than a fact of how long the test took to run.
    fn age_file(path: &Path, ms: u64) {
        let when = UNIX_EPOCH + std::time::Duration::from_millis(ms);
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open to age");
        file.set_times(fs::FileTimes::new().set_modified(when))
            .expect("set mtime");
    }

    fn a_surface_in(root: &Path, session: &str, pane: u64, name: &str) -> PathBuf {
        let dir = root.join(session).join(pane.to_string());
        drop_surface(&dir, name, &a_doc(name)).expect("drop")
    }

    #[test]
    fn a_session_nobody_has_written_to_in_a_month_is_removed_whole() {
        let scratch = Scratch::new("prune-dead");
        let root = scratch.path();
        let old = a_surface_in(root, "wbshot123", 1, "s");
        age_file(&old, NOW - 40 * DAY_MS);

        let out = prune(root, Some("live"), NOW);

        assert_eq!(out.sessions, 1, "the dead session directory is gone");
        assert_eq!(out.unknown, 0);
        assert!(!root.join("wbshot123").exists());
    }

    #[test]
    fn a_session_written_to_this_week_is_left_alone() {
        let scratch = Scratch::new("prune-recent");
        let root = scratch.path();
        let recent = a_surface_in(root, "yesterday", 1, "s");
        age_file(&recent, NOW - 2 * DAY_MS);

        let out = prune(root, Some("live"), NOW);

        assert_eq!(out.sessions, 0);
        assert!(root.join("yesterday").exists());
    }

    #[test]
    fn this_windows_own_session_is_never_removed_however_old_it_looks() {
        let scratch = Scratch::new("prune-live");
        let root = scratch.path();
        let mine = a_surface_in(root, "live", 1, "s");
        age_file(&mine, NOW - 400 * DAY_MS);

        let out = prune(root, Some("live"), NOW);

        assert_eq!(out.sessions, 0, "the live session outranks its own mtime");
        assert!(mine.exists());
    }

    #[test]
    fn a_prune_that_cannot_name_the_live_session_deletes_nothing() {
        let scratch = Scratch::new("prune-nokey");
        let root = scratch.path();
        let old = a_surface_in(root, "ancient", 1, "s");
        age_file(&old, NOW - 400 * DAY_MS);

        let out = prune(root, None, NOW);

        assert_eq!(out, Pruned::default(), "no key, no judgement, no deletions");
        assert!(old.exists());
    }

    #[test]
    fn a_directory_this_module_did_not_write_is_left_where_it_is() {
        let scratch = Scratch::new("prune-foreign");
        let root = scratch.path();
        let foreign = root.join("someone.elses.backup");
        fs::create_dir_all(&foreign).unwrap();
        let file = foreign.join("keep.json");
        fs::write(&file, b"{}").unwrap();
        age_file(&file, NOW - 400 * DAY_MS);

        let out = prune(root, Some("live"), NOW);

        assert_eq!(out.sessions, 0);
        assert!(
            file.exists(),
            "a name this module cannot have written is not ours to delete"
        );
    }

    #[test]
    fn a_pane_over_the_disk_cap_keeps_the_newest_and_drops_the_oldest() {
        let scratch = Scratch::new("prune-cap");
        let root = scratch.path();
        let over = 5;
        for n in 0..PANE_DISK_CAP + over {
            let path = a_surface_in(root, "live", 1, &format!("s{n:04}"));
            // Lower index, older file — an hour apart so no filesystem's mtime
            // resolution can blur the order the test is asserting.
            age_file(&path, NOW - ((PANE_DISK_CAP + over - n) as u64) * 3_600_000);
        }

        let out = prune(root, Some("live"), NOW);

        assert_eq!(out.files, over, "exactly the overflow is deleted");
        assert_eq!(out.sessions, 0, "capping a pane never removes the session");
        let pane = root.join("live").join("1");
        let left = fs::read_dir(&pane).unwrap().count();
        assert_eq!(left, PANE_DISK_CAP);
        assert!(!pane.join("s0000.json").exists(), "the oldest went");
        assert!(
            pane.join(format!("s{:04}.json", PANE_DISK_CAP + over - 1))
                .exists(),
            "the newest stayed"
        );
    }

    #[test]
    fn a_pane_under_the_disk_cap_loses_nothing() {
        let scratch = Scratch::new("prune-under");
        let root = scratch.path();
        for n in 0..8 {
            let path = a_surface_in(root, "live", 1, &format!("s{n}"));
            age_file(&path, NOW - 900 * DAY_MS); // old, and irrelevant: the cap is a count
        }

        let out = prune(root, Some("live"), NOW);

        assert_eq!(out.files, 0);
        assert_eq!(
            fs::read_dir(root.join("live").join("1")).unwrap().count(),
            8
        );
    }

    #[test]
    fn the_action_journal_is_never_pruned_however_full_the_pane_is() {
        let scratch = Scratch::new("prune-journal");
        let root = scratch.path();
        for n in 0..PANE_DISK_CAP + 3 {
            let path = a_surface_in(root, "live", 1, &format!("s{n:04}"));
            age_file(&path, NOW - ((PANE_DISK_CAP + 3 - n) as u64) * 3_600_000);
        }
        let journal = root.join("live").join("1").join("actions.jsonl");
        fs::write(&journal, b"{\"answer\":\"yes\"}\n").unwrap();
        age_file(&journal, NOW - 900 * DAY_MS);

        let out = prune(root, Some("live"), NOW);

        assert_eq!(
            out.files, 3,
            "only surfaces are counted, and only surfaces went"
        );
        assert!(
            journal.exists(),
            "the journal of what a person pressed is not a surface"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_surface_with_no_readable_age_is_counted_unknown_and_kept() {
        let scratch = Scratch::new("prune-unknown");
        let root = scratch.path();
        let pane = root.join("live").join("1");
        fs::create_dir_all(&pane).unwrap();
        let ghost = pane.join("ghost.json");
        std::os::unix::fs::symlink("/nonexistent/gone.json", &ghost).unwrap();

        let out = prune(root, Some("live"), NOW);

        assert_eq!(
            out.unknown, 1,
            "an age that cannot be read is its own answer"
        );
        assert_eq!(
            out.files, 0,
            "and it is never the answer that deletes something"
        );
        assert!(fs::symlink_metadata(&ghost).is_ok());
    }

    /// Concurrent updates of ONE surface id must all land.
    ///
    /// The shipped path re-sends the same id to update a row in place, so two
    /// writes racing on one name is the normal case rather than an exotic one.
    /// With a temp name derived only from the id, both writers wrote the same
    /// `.part`, the first rename took it and the second failed `ENOENT` — the
    /// surface silently lost, under an error that named a missing directory.
    ///
    /// Sixteen threads is enough to lose a race reliably; the old code failed
    /// this on every run, and never in the same places twice.
    #[test]
    fn concurrent_writers_of_one_surface_id_do_not_eat_each_other() {
        let dir = std::env::temp_dir().join(format!("td-race-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch");

        let failures = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        std::thread::scope(|scope| {
            for i in 0..16 {
                let dir = dir.clone();
                let failures = std::sync::Arc::clone(&failures);
                scope.spawn(move || {
                    let doc = serde_json::json!({ "td": "0.4", "kind": "response", "n": i });
                    if let Err(err) = drop_surface(&dir, "same-id", &doc) {
                        failures.lock().unwrap().push(format!("writer {i}: {err}"));
                    }
                });
            }
        });

        let failures = failures.lock().unwrap();
        assert!(
            failures.is_empty(),
            "a concurrent update was lost: {failures:?}"
        );
        // And exactly one file, still whole — the point of the rename.
        let landed: Vec<_> = fs::read_dir(&dir)
            .expect("read scratch")
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(landed, vec!["same-id.json".to_string()], "left: {landed:?}");
        let text = fs::read_to_string(dir.join("same-id.json")).expect("read");
        assert!(
            serde_json::from_str::<Value>(&text).is_ok(),
            "a reader could see a half-written file: {text}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_inbound_journal_is_read_by_offset_and_a_half_written_line_waits() {
        use std::io::Write;
        let dir =
            std::env::temp_dir().join(format!("td-inbound-{}-{}", std::process::id(), now_ms()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("inbound.jsonl");
        let mut feed = Feed::new();
        assert!(feed.tail_inbound(&dir).is_empty(), "no journal yet");
        {
            let mut f = fs::File::create(&path).unwrap();
            writeln!(f, r#"{{"td":"0.1","type":"prompt","text":"one"}}"#).unwrap();
            writeln!(f, r#"{{"td":"0.1","type":"reply","text":"two"}}"#).unwrap();
            // The third line is still being written: no newline yet.
            write!(f, r#"{{"td":"0.1","type":"pro"#).unwrap();
        }
        let got = feed.tail_inbound(&dir);
        assert_eq!(got.len(), 2, "two whole lines, the half-written one waits");
        assert!(
            matches!(&got[0], crate::channel::Inbound::Prompt { text: Some(t), .. } if t == "one")
        );
        assert!(
            matches!(&got[1], crate::channel::Inbound::Reply { text: Some(t), .. } if t == "two")
        );
        assert!(feed.tail_inbound(&dir).is_empty(), "nothing new");
        {
            let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
            writeln!(f, r#"mpt","text":"three"}}"#).unwrap();
            writeln!(f, "not json at all").unwrap();
        }
        let got = feed.tail_inbound(&dir);
        assert_eq!(got.len(), 2);
        assert!(
            matches!(&got[0], crate::channel::Inbound::Prompt { text: Some(t), .. } if t == "three")
        );
        assert!(
            matches!(&got[1], crate::channel::Inbound::Unknown { .. }),
            "a line that is not a record is kept as unknown, never dropped: {:?}",
            got[1]
        );
        // A journal that shrank is read from the start again.
        fs::write(
            &path,
            "{\"td\":\"0.1\",\"type\":\"reply\",\"text\":\"again\"}\n",
        )
        .unwrap();
        let got = feed.tail_inbound(&dir);
        assert_eq!(got.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_marker_and_the_answer_file_are_whole_when_read() {
        let dir =
            std::env::temp_dir().join(format!("td-marker-{}-{}", std::process::id(), now_ms()));
        write_marker(&dir, true, 5_000).unwrap();
        let text = fs::read_to_string(dir.join("bench.json")).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert!(crate::channel::marker_holds(&v, 5_000));
        assert_eq!(v["window"], std::process::id());
        write_marker(&dir, false, 6_000).unwrap();
        let v: Value =
            serde_json::from_str(&fs::read_to_string(dir.join("bench.json")).unwrap()).unwrap();
        assert!(!crate::channel::marker_holds(&v, 6_000), "terminal face");
        // The marker is a `.json` in the mailbox and it is NOT a surface.
        let mut feed = Feed::new();
        assert!(
            feed.sweep_pane(&dir, 7_000).is_empty(),
            "the marker was swept onto the bench as a surface"
        );

        let mut answers = serde_json::Map::new();
        answers.insert("Which drink?".into(), Value::String("Coffee".into()));
        let path = write_answers(&dir, "toolu_01/../x", &answers).unwrap();
        assert!(path.ends_with("answers/toolu_01x.json"), "{path:?}");
        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["answers"]["Which drink?"], "Coffee");
        assert_eq!(
            v["tool_use_id"], "toolu_01/../x",
            "the id itself travels whole"
        );
        // No `.part` left behind for anything to trip over.
        let stray: Vec<_> = fs::read_dir(dir.join("answers"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".part"))
            .collect();
        assert!(stray.is_empty(), "{stray:?}");
        let _ = fs::remove_dir_all(&dir);
    }
}
