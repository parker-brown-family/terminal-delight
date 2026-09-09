//! The words a client and a session host say to each other.
//!
//! One line of JSON per message on the control connection, and raw bytes on a
//! second connection per pane — keystrokes up, terminal output down. Newline
//! framing because every other seam in this codebase already uses it, and JSON
//! because the alternative is a binary format nobody can read in a log at three
//! in the morning.
//!
//! This module is pure: types, a version check, and where the socket lives.
//! Both sides depend on it and neither side's runtime does, so the contract can
//! be tested without a process on either end.
//!
//! **Identity is not on the wire.** Nothing here carries a user, a token or a
//! capability, and that is deliberate: authorisation is a property of the
//! connection — a same-user socket in a private directory — and the moment it
//! becomes a field in a message, every future transport inherits a security
//! model designed for a local pipe.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Bumped only for a change that an older peer would misread. Fields may be
/// added within a version: both sides ignore what they do not recognise, which
/// is what lets a client and host from adjacent builds keep working.
pub const PROTO_VERSION: u32 = 1;

/// Stamped into every pane the host spawns, so anything running inside a
/// terminal can find out which session and which pane it is in without
/// walking `/proc` and guessing.
pub const ENV_SESSION: &str = "TD_SESSION";
pub const ENV_PANE_ID: &str = "TD_PANE_ID";

/// A pane's durable name.
///
/// Not a process id. A pid is the address of a thing that dies, gets recycled,
/// and means nothing to a second process that was not there when it was born —
/// and addressing panes by pid is where four separate session cross-wiring
/// incidents came from. The host mints these, they outlive the shells inside
/// them, and the pid becomes a mere attribute that anyone may report.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct PaneId(pub u64);

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A terminal's size, in both the units that matter: cells for the emulator,
/// pixels per cell for anything that draws.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct PaneGeom {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl Default for PaneGeom {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        }
    }
}

/// What kind of thing is talking, which decides whether it may take a pane
/// away from whoever holds it.
///
/// Only a window attaches. Everything else — a probe asking which sessions are
/// alive, a script listing panes — connects, asks and goes, and must never
/// cost a live window its connection. Getting this wrong would mean every
/// launch stole from the window already running.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum ClientKind {
    /// A window. May attach, and attaching supersedes whoever held the pane.
    Window,
    /// A command-line client or a probe. Never attaches, never steals.
    Tool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "verb", rename_all = "kebab-case")]
pub enum Request {
    /// First line of every control connection.
    Hello { proto: u32, kind: ClientKind },
    ListPanes,
    SpawnPane {
        #[serde(default)]
        cwd: Option<String>,
        geom: PaneGeom,
    },
    /// Declare intent to attach and set the size. The pane's bytes then flow
    /// on a second connection, which is where the actual handover happens.
    AttachPane { pane: PaneId, geom: PaneGeom },
    /// Never sent on the byte stream: a size is a fact, not part of the
    /// terminal's output, and mixing them means guessing where one ends.
    Resize { pane: PaneId, geom: PaneGeom },
    /// Intent, and the only thing that kills: hang up the pane's process tree.
    ClosePane { pane: PaneId },
    /// Ask what the authoritative grid hashes to, so a client can check its own
    /// copy against it.
    GridCheck { pane: PaneId },
    Shutdown,
}

/// The answer to a write, per target, said truthfully.
///
/// A queue acknowledgement is not an outcome — it says a message was accepted,
/// not that anything happened. Every verb that changes something answers with
/// one of these instead.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome<T> {
    Ok(T),
    Err(String),
}

impl<T> Outcome<T> {
    /// Used by tests today and by the client when it arrives; kept here rather
    /// than written twice at two call sites later.
    #[allow(dead_code)]
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok(_))
    }
}

/// What a pane is running, as far as the kernel will say.
///
/// The host is what answers this now. `tcgetpgrp` answers whoever holds the
/// pseudoterminal, and a window that has stopped owning terminals cannot ask
/// the question at all.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "kebab-case")]
pub enum WireMode {
    Shell,
    Claude,
    Codex,
    Remote,
    /// Anything else, named: `vim`, `htop`, whatever is in front.
    Other(String),
}

/// What the host knows about a pane. Everything absent is `None` rather than a
/// stand-in value: a pane whose working directory has not been read yet is not
/// a pane in the root directory, and a pane nobody has classified yet is not a
/// pane running a shell.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaneInfo {
    pub pane: PaneId,
    pub shell_pid: u32,
    /// Where the pane actually is, as of the last checkpoint. Seeded with the
    /// directory it was asked to start in, and replaced by a live reading once
    /// the host has managed to take one — those stop being the same thing the
    /// moment somebody types `cd`.
    #[serde(default)]
    pub cwd: Option<String>,
    /// The line that would put an agent back in the conversation this pane was
    /// having. `None` for a pane running no agent, and equally for one nobody
    /// has looked at yet, which is why a checkpoint never overwrites a known
    /// recipe with a reading it failed to take.
    #[serde(default)]
    pub resume: Option<String>,
    /// What is in the foreground. `None` until the watcher has classified it.
    #[serde(default)]
    pub mode: Option<WireMode>,
    /// Whether a window currently holds this pane's byte stream.
    pub attached: bool,
    /// Whether the process inside it has gone.
    pub ended: bool,
    pub geom: PaneGeom,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "reply", rename_all = "kebab-case")]
pub enum Reply {
    Hello {
        proto: u32,
        session: String,
        panes: usize,
        /// Whether any window is attached. A host with panes and nobody
        /// looking at them is precisely what a relaunch should find and adopt.
        attended: bool,
    },
    Panes {
        panes: Vec<PaneInfo>,
    },
    Spawned {
        outcome: Outcome<PaneInfo>,
    },
    Attached {
        pane: PaneId,
        outcome: Outcome<PaneInfo>,
    },
    Resized {
        pane: PaneId,
        outcome: Outcome<()>,
    },
    Closed {
        pane: PaneId,
        outcome: Outcome<ClosedPane>,
    },
    GridChecked {
        pane: PaneId,
        outcome: Outcome<GridCheck>,
    },
    ShuttingDown,
    /// An unreadable line, an unknown verb, or a version that cannot be
    /// spoken. Always an answer — never a silent fall-through, which is the
    /// failure mode this whole feature started with.
    Error {
        msg: String,
    },
}

/// One integrity probe: what the host's own copy of a terminal hashes to, and
/// how far down this client's stream that reading was taken.
///
/// The offset is what makes the comparison mean anything. A client that has not
/// yet read as far as the host had written is *behind*, which is a different
/// finding from being *wrong* — and without a stated offset the two are
/// indistinguishable, so a guard would call for a repair on every busy pane.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridCheck {
    pub pane: PaneId,
    /// Bytes written to this client's stream, the opening snapshot included, at
    /// the moment the hash was taken. The client's own clock is bytes consumed
    /// from the socket, and these count the same things.
    pub stream_offset: u64,
    pub hash: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ClosedPane {
    pub shell_pid: u32,
    /// Whether the hangup signal actually reached a process group. `false`
    /// means the pane was already gone, which is not a failure but is not a
    /// kill either.
    pub signalled: bool,
}

/// The first line of a byte-stream connection, which says which pane it
/// carries. Everything after it is the terminal's own bytes.
///
/// Written here beside the parser that reads it, so the two cannot drift. The
/// host only ever parses; the client that writes one arrives in the attach
/// slice, and until then the round-trip test is what keeps the pair honest.
#[allow(dead_code)]
pub fn stream_greeting(pane: PaneId) -> String {
    format!("stream {pane}\n")
}

/// Parse that greeting back.
///
/// Strict on everything except the line ending: this greeting is written by
/// our own code, so accepting a sloppier spelling only widens what the host
/// has to be correct about.
pub fn parse_stream_greeting(line: &str) -> Option<PaneId> {
    line.trim_end_matches(['\r', '\n'])
        .strip_prefix("stream ")?
        .parse()
        .ok()
        .map(PaneId)
}

/// Whether a peer speaking `theirs` can be understood.
///
/// Names both numbers when it cannot, because "protocol mismatch" alone sends
/// the reader to a changelog to work out which side is old.
pub fn version_check(theirs: u32) -> Result<(), String> {
    if theirs == PROTO_VERSION {
        Ok(())
    } else {
        Err(format!(
            "protocol mismatch: this build speaks {PROTO_VERSION}, the other side speaks {theirs}"
        ))
    }
}

/// Where a session's host listens.
///
/// Keyed by the session, not by a process id. A pid-keyed name has to be
/// swept when it goes stale and can be re-used by an unrelated process; a
/// session-keyed one is claimed by whoever holds that session's lock, which is
/// already the thing that arbitrates ownership.
pub fn host_socket_path(key: &str) -> PathBuf {
    runtime_dir().join(format!("session-{key}.sock"))
}

/// The private, per-user directory the sockets live in — the same one the
/// existing control sockets use. Same-user access to it is the entire
/// authorisation model, which is why nothing on the wire repeats the claim.
fn runtime_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("terminal-delight");
    }
    // Matches ctl.rs's fallback: a uid-scoped directory under /tmp, created
    // 0700 by whoever binds the socket.
    PathBuf::from(format!("/tmp/terminal-delight-{}", unsafe { libc::getuid() }))
}

/// One of every request the host can be sent.
///
/// Kept beside one of every reply, and used three times over: the round-trip
/// tests read them, the conformance tests take the field names the host emits
/// from them, and a separate test proves against serde's own variant list that
/// nothing has been left out of them.
#[cfg(test)]
fn every_request() -> Vec<Request> {
    let geom = PaneGeom::default();
    vec![
        Request::Hello {
            proto: PROTO_VERSION,
            kind: ClientKind::Window,
        },
        Request::ListPanes,
        Request::SpawnPane {
            cwd: Some("/home/me".into()),
            geom,
        },
        Request::AttachPane {
            pane: PaneId(3),
            geom,
        },
        Request::Resize {
            pane: PaneId(3),
            geom,
        },
        Request::ClosePane { pane: PaneId(3) },
        Request::GridCheck { pane: PaneId(3) },
        Request::Shutdown,
    ]
}

/// One of every reply the host can send, and every shape inside them: an
/// outcome that succeeded and one that failed, a mode that is a plain name and
/// one that carries a program's.
#[cfg(test)]
fn every_reply() -> Vec<Reply> {
    let info = PaneInfo {
        pane: PaneId(1),
        shell_pid: 4242,
        cwd: Some("/home/me".into()),
        resume: Some("claude --resume 4a1c".into()),
        mode: Some(WireMode::Claude),
        attached: true,
        ended: false,
        geom: PaneGeom::default(),
    };
    let other = PaneInfo {
        pane: PaneId(2),
        mode: Some(WireMode::Other("vim".into())),
        ..info.clone()
    };
    vec![
        Reply::Hello {
            proto: PROTO_VERSION,
            session: "2".into(),
            panes: 2,
            attended: false,
        },
        Reply::Panes {
            panes: vec![info.clone(), other],
        },
        Reply::Spawned {
            outcome: Outcome::Ok(info.clone()),
        },
        Reply::Attached {
            pane: PaneId(1),
            outcome: Outcome::Err("no such pane".into()),
        },
        Reply::Resized {
            pane: PaneId(1),
            outcome: Outcome::Ok(()),
        },
        Reply::Closed {
            pane: PaneId(1),
            outcome: Outcome::Ok(ClosedPane {
                shell_pid: 4242,
                signalled: true,
            }),
        },
        Reply::GridChecked {
            pane: PaneId(1),
            outcome: Outcome::Ok(GridCheck {
                pane: PaneId(1),
                stream_offset: 8192,
                hash: 0xcbf2_9ce4_8422_2325,
            }),
        },
        Reply::ShuttingDown,
        Reply::Error { msg: "nope".into() },
    ]
}

#[cfg(test)]
mod wire {
    use super::*;

    /// Round-trip every message, since the whole contract rests on both sides
    /// reading the same bytes the same way.
    #[test]
    fn every_message_survives_one_line_of_json() {
        for request in every_request() {
            let line = serde_json::to_string(&request).expect("serialise");
            assert!(!line.contains('\n'), "a message must be one line: {line}");
            let back: Request = serde_json::from_str(&line).expect("deserialise");
            assert_eq!(
                serde_json::to_string(&back).unwrap(),
                line,
                "round trip changed the message"
            );
        }
    }

    #[test]
    fn replies_survive_one_line_of_json() {
        for reply in every_reply() {
            let line = serde_json::to_string(&reply).expect("serialise");
            assert!(!line.contains('\n'), "a reply must be one line: {line}");
            serde_json::from_str::<Reply>(&line).expect("deserialise");
        }
    }

    #[test]
    fn an_unknown_field_does_not_break_a_peer_from_a_later_build() {
        // The rule that lets adjacent builds keep talking: a version is bumped
        // for a change that would be MISREAD, not for one that adds something.
        let line = r#"{"verb":"hello","proto":1,"kind":"window","future_field":true}"#;
        let parsed: Request = serde_json::from_str(line).expect("tolerate a new field");
        assert!(matches!(
            parsed,
            Request::Hello {
                kind: ClientKind::Window,
                ..
            }
        ));
    }

    #[test]
    fn a_version_mismatch_names_both_sides() {
        assert!(version_check(PROTO_VERSION).is_ok());
        let complaint = version_check(PROTO_VERSION + 7).expect_err("must refuse");
        assert!(complaint.contains(&PROTO_VERSION.to_string()), "{complaint}");
        assert!(
            complaint.contains(&(PROTO_VERSION + 7).to_string()),
            "{complaint}"
        );
    }

    #[test]
    fn the_stream_greeting_round_trips_and_rejects_nonsense() {
        assert_eq!(
            parse_stream_greeting(&stream_greeting(PaneId(12))),
            Some(PaneId(12))
        );
        for junk in ["", "stream", "stream x", "attach 4", "  stream 4"] {
            assert_eq!(parse_stream_greeting(junk), None, "accepted {junk:?}");
        }
    }

    #[test]
    fn the_socket_is_named_for_the_session_and_lives_in_the_private_directory() {
        // Not the process id: a session-keyed name is claimed by whoever holds
        // the session's lock, so it cannot go stale under a recycled pid.
        let path = host_socket_path("2");
        assert!(path.ends_with("session-2.sock"), "{path:?}");
        assert!(
            path.parent().is_some_and(|p| {
                p.ends_with("terminal-delight") || p.to_string_lossy().contains("terminal-delight")
            }),
            "socket must sit in the private runtime directory: {path:?}"
        );
    }

    #[test]
    fn nothing_on_the_wire_carries_an_identity() {
        // Authorisation belongs to the connection. If a field ever appears
        // here that names a user or a token, the next transport will inherit a
        // security model that was only ever true of a local socket.
        let mut every_message = String::new();
        for request in [
            Request::Hello {
                proto: 1,
                kind: ClientKind::Window,
            },
            Request::ListPanes,
            Request::ClosePane { pane: PaneId(1) },
        ] {
            every_message.push_str(&serde_json::to_string(&request).unwrap());
        }
        for banned in ["uid", "user", "token", "secret", "auth", "cred"] {
            assert!(
                !every_message.contains(banned),
                "the wire grew an identity field: {banned}"
            );
        }
    }
}

/// The page is the contract, and this is what makes that true.
///
/// A protocol document that describes an implementation drifts from it the
/// first time somebody adds a field in a hurry, and nothing fails. So the
/// page's own examples are fed to the host's own types, in both directions:
/// nothing may be documented that the host would not write, and nothing the
/// host writes may go undocumented.
#[cfg(test)]
mod contract {
    use std::collections::BTreeSet;

    use serde_json::Value;

    use super::*;

    /// The page itself, found through the crate rather than the working
    /// directory, so it is the same file whoever runs the suite and from
    /// wherever.
    fn page() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../docs/protocol/session-host-v1.md");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the protocol page is missing at {}: {e}", path.display()))
    }

    /// One fenced block, with whatever its opening line declared it to be.
    struct Block {
        declared: String,
        body: String,
        line: usize,
    }

    fn blocks(doc: &str) -> Vec<Block> {
        let mut out = Vec::new();
        let mut open: Option<(String, usize, Vec<String>)> = None;
        for (n, line) in doc.lines().enumerate() {
            if line.starts_with("```") {
                match open.take() {
                    Some((declared, at, body)) => out.push(Block {
                        declared,
                        body: body.join("\n"),
                        line: at,
                    }),
                    None => {
                        open = Some((
                            line.trim_start_matches('`').trim().to_string(),
                            n + 1,
                            Vec::new(),
                        ))
                    }
                }
            } else if let Some((_, _, body)) = open.as_mut() {
                body.push(line.to_string());
            }
        }
        assert!(
            open.is_none(),
            "a fenced block in the protocol page is never closed"
        );
        out
    }

    /// An example, as the page writes it and as the host would write it back.
    struct Checked {
        line: usize,
        written: Value,
        emitted: Value,
        tag: Option<String>,
    }

    fn reserialise<T>(block: &Block) -> Value
    where
        T: serde::de::DeserializeOwned + Serialize,
    {
        let parsed: T = serde_json::from_str(&block.body).unwrap_or_else(|e| {
            panic!(
                "the example at line {} is not something the host speaks: {e}\n{}",
                block.line, block.body
            )
        });
        serde_json::to_value(&parsed).expect("re-serialise")
    }

    /// Every JSON example on the page, parsed as the type its fence declares.
    ///
    /// A fence saying only `json` fails the suite. An example nobody said the
    /// type of cannot be checked against one, and an unchecked example is
    /// exactly the fiction this module exists to prevent.
    fn examples(doc: &str) -> Vec<Checked> {
        let mut out = Vec::new();
        for block in blocks(doc) {
            let Some(kind) = block.declared.strip_prefix("json") else {
                continue;
            };
            let written: Value = serde_json::from_str(&block.body).unwrap_or_else(|e| {
                panic!(
                    "the example at line {} is not JSON at all: {e}\n{}",
                    block.line, block.body
                )
            });
            let emitted = match kind.trim() {
                "request" => reserialise::<Request>(&block),
                "reply" => reserialise::<Reply>(&block),
                "" => panic!(
                    "the example at line {} does not say what it is — open it \
                     with ```json request or ```json reply",
                    block.line
                ),
                other => panic!(
                    "the example at line {} is declared `{other}`, which is not \
                     a message the host speaks",
                    block.line
                ),
            };
            let tag = written
                .get("verb")
                .or_else(|| written.get("reply"))
                .and_then(Value::as_str)
                .map(str::to_string);
            out.push(Checked {
                line: block.line,
                written,
                emitted,
                tag,
            });
        }
        assert!(!out.is_empty(), "the protocol page carries no examples");
        out
    }

    /// Every key the page's example uses that the host would not have written
    /// back. Serde ignores a field it does not know, so without this a page
    /// could describe a field that has never existed and every test would
    /// pass.
    fn unwritten(written: &Value, emitted: &Value, path: &str, out: &mut Vec<String>) {
        match (written, emitted) {
            (Value::Object(written), Value::Object(emitted)) => {
                for (key, value) in written {
                    let path = format!("{path}.{key}");
                    match emitted.get(key) {
                        Some(back) => unwritten(value, back, &path, out),
                        None => out.push(path),
                    }
                }
            }
            (Value::Array(written), Value::Array(emitted)) => {
                for (n, (value, back)) in written.iter().zip(emitted).enumerate() {
                    unwritten(value, back, &format!("{path}[{n}]"), out);
                }
            }
            (written, emitted) if written != emitted => {
                out.push(format!("{path} (the page says {written}, the host says {emitted})"))
            }
            _ => {}
        }
    }

    /// Every key name anywhere in a message.
    fn keys(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    out.insert(key.clone());
                    keys(value, out);
                }
            }
            Value::Array(values) => values.iter().for_each(|v| keys(v, out)),
            _ => {}
        }
    }

    /// The variant names serde itself expects, read out of the complaint it
    /// makes about one it does not.
    ///
    /// Asking the deserialiser is what keeps this honest. A hand-kept list of
    /// verbs would be one more thing to remember, and the whole point is that
    /// a verb added to the host cannot escape the page.
    fn variants_of<T: serde::de::DeserializeOwned>(tag: &str) -> Vec<String> {
        let nonsense = format!(r#"{{"{tag}":"a-verb-no-build-will-ever-speak"}}"#);
        let complaint = serde_json::from_str::<T>(&nonsense)
            .err()
            .expect("a nonsense tag must be refused")
            .to_string();
        let (_, listed) = complaint
            .split_once("expected one of ")
            .unwrap_or_else(|| panic!("serde no longer names the variants it expects: {complaint}"));
        // `a`, `b`, `c` at line 1 column 9 — the names are the odd fields
        // between backticks, and the trailing position is not one of them.
        let names: Vec<String> = listed
            .split('`')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect();
        assert!(names.len() > 1, "no variants read out of: {complaint}");
        names
    }

    fn documented_tags(doc: &str) -> BTreeSet<String> {
        examples(doc).into_iter().filter_map(|e| e.tag).collect()
    }

    #[test]
    fn every_json_example_in_the_protocol_page_parses() {
        // Parsing each one as the type its fence names IS the assertion.
        let found = examples(&page()).len();
        assert!(found >= 12, "only {found} examples — the page has thinned");
    }

    #[test]
    fn the_page_never_names_a_field_the_host_does_not_have() {
        for example in examples(&page()) {
            let mut missing = Vec::new();
            unwritten(&example.written, &example.emitted, "", &mut missing);
            assert!(
                missing.is_empty(),
                "the example at line {} names {missing:?}, which the host never writes",
                example.line
            );
        }
    }

    #[test]
    fn every_verb_and_every_reply_is_documented() {
        let documented = documented_tags(&page());
        for verb in variants_of::<Request>("verb") {
            assert!(
                documented.contains(&verb),
                "the host speaks `{verb}` and the protocol page does not mention it"
            );
        }
        for reply in variants_of::<Reply>("reply") {
            assert!(
                documented.contains(&reply),
                "the host answers `{reply}` and the protocol page does not mention it"
            );
        }
    }

    #[test]
    fn the_sample_messages_cover_every_variant() {
        // What makes the field check below airtight: the samples the field
        // names are taken from are themselves proven complete against serde.
        let sampled: BTreeSet<String> = every_request()
            .iter()
            .map(|r| serde_json::to_value(r).unwrap()["verb"].as_str().unwrap().to_string())
            .chain(
                every_reply()
                    .iter()
                    .map(|r| serde_json::to_value(r).unwrap()["reply"].as_str().unwrap().to_string()),
            )
            .collect();
        for variant in variants_of::<Request>("verb")
            .into_iter()
            .chain(variants_of::<Reply>("reply"))
        {
            assert!(
                sampled.contains(&variant),
                "`{variant}` has no sample message, so its fields are checked against nothing"
            );
        }
    }

    #[test]
    fn every_field_the_host_emits_appears_on_the_page() {
        let mut documented = BTreeSet::new();
        for example in examples(&page()) {
            keys(&example.written, &mut documented);
        }
        let mut emitted = BTreeSet::new();
        for request in every_request() {
            keys(&serde_json::to_value(&request).unwrap(), &mut emitted);
        }
        for reply in every_reply() {
            keys(&serde_json::to_value(&reply).unwrap(), &mut emitted);
        }
        let undocumented: Vec<&String> = emitted.difference(&documented).collect();
        assert!(
            undocumented.is_empty(),
            "the host writes {undocumented:?}, and the protocol page never mentions them"
        );
    }

    #[test]
    fn the_page_states_the_version_this_build_speaks() {
        let stated = format!("protocol version {PROTO_VERSION}");
        assert!(
            page().contains(&stated),
            "the page never says `{stated}`, so a bumped version leaves it describing the old one"
        );
    }

    #[test]
    fn the_page_quotes_the_refusal_this_build_writes() {
        // Not a paraphrase of the refusal: the refusal, so the words a client
        // author reads are the words their log will show.
        let refusal =
            version_check(PROTO_VERSION + 1).expect_err("a version we cannot speak is refused");
        assert!(
            page().contains(&refusal),
            "the page does not quote what the host actually writes: {refusal}"
        );
    }

    #[test]
    fn the_documented_stream_greeting_is_the_one_the_host_parses() {
        let doc = page();
        let shown: Vec<Block> = blocks(&doc)
            .into_iter()
            .filter(|b| b.declared == "stream")
            .collect();
        assert!(!shown.is_empty(), "the page never shows a stream greeting");
        for block in shown {
            for line in block.body.lines().filter(|l| !l.trim().is_empty()) {
                assert!(
                    parse_stream_greeting(line).is_some(),
                    "the greeting the page shows is one the host would refuse: {line:?}"
                );
            }
        }
    }

    #[test]
    fn the_page_names_the_socket_and_what_every_pane_is_stamped_with() {
        let doc = page();
        let socket = host_socket_path("<key>");
        let name = socket
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a socket name");
        assert!(
            doc.contains(name),
            "the page does not name the socket the host binds: {name}"
        );
        for var in [ENV_SESSION, ENV_PANE_ID] {
            assert!(
                doc.contains(var),
                "the page does not name {var}, which every pane is stamped with"
            );
        }
    }
}
