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

/// What the host knows about a pane. Everything absent is `None` rather than a
/// stand-in value: a pane whose working directory has not been read yet is not
/// a pane in the root directory.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaneInfo {
    pub pane: PaneId,
    pub shell_pid: u32,
    #[serde(default)]
    pub cwd: Option<String>,
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
    ShuttingDown,
    /// An unreadable line, an unknown verb, or a version that cannot be
    /// spoken. Always an answer — never a silent fall-through, which is the
    /// failure mode this whole feature started with.
    Error {
        msg: String,
    },
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

#[cfg(test)]
mod wire {
    use super::*;

    /// Round-trip every message, since the whole contract rests on both sides
    /// reading the same bytes the same way.
    #[test]
    fn every_message_survives_one_line_of_json() {
        let geom = PaneGeom::default();
        let requests = vec![
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
            Request::Shutdown,
        ];
        for request in requests {
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
        let info = PaneInfo {
            pane: PaneId(1),
            shell_pid: 4242,
            cwd: None,
            attached: true,
            ended: false,
            geom: PaneGeom::default(),
        };
        let replies = vec![
            Reply::Hello {
                proto: 1,
                session: "2".into(),
                panes: 1,
                attended: false,
            },
            Reply::Panes {
                panes: vec![info.clone()],
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
            Reply::ShuttingDown,
            Reply::Error { msg: "nope".into() },
        ];
        for reply in replies {
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
