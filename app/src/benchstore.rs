//! A conversation's record — one file, holding what was said and what was shown.
//!
//! # Why this exists beside the mailbox rather than inside it
//!
//! Agents drop JSON into `surfaces/<window session>/<pane id>/`, and that
//! directory is a **mailbox**: any process running as this user can write to it,
//! which is what makes file drops work and what makes it unable to say who wrote
//! anything. The window sweeps it, resolves which conversation the pane is in,
//! and files what it took in here.
//!
//! So there are two directories with two owners, and the split is the isolation:
//! **no agent ever writes into this one.** There is no code path by which an
//! agent's bytes reach any conversation's record, so there is none by which they
//! reach the wrong conversation's record. The filing key is read from the
//! process, never from the payload — a session id an agent puts in its own JSON
//! is not read here.
//!
//! What that does *not* buy is forgery resistance. The ledger this keys on is an
//! ordinary user-owned file and every agent runs as that user, so the integrity
//! of a `root` rests on filesystem ownership and on nothing cryptographic. That
//! is why [`safe_segment`] runs here as well as in the hook: a `root` becomes a
//! file name, and a guard at one end only is a guard the third reader assumes
//! the first one applied.
//!
//! # Why one file per conversation
//!
//! `conversations/<root>.ws` — append-only JSON lines, oldest first. Parker's
//! decision, 2026-09-21, and the reasoning is about what grows. The number of
//! **conversations** grows; the turns inside one do not grow without bound and
//! are always read whole, because a bench draws the conversation on the
//! agent-arrived edge and never asks for turn forty on its own. A directory of
//! per-turn files answers a question nobody asks and charges a listing, a sort
//! and N opens for it — and the sort is where the first version of this module
//! shipped a defect, ordering surfaces by filename stem while promising oldest
//! first.
//!
//! One file has a total order for free. Two windows on one conversation append
//! to it and interleave cleanly, where two windows writing turn files have to
//! agree on names. And a conversation is one object: `cat`, `tail -f`, `jq`, an
//! attachment on an issue, a byte offset a relay can resume from.
//!
//! What it gives up is partial reads and per-turn deletion. Neither is a
//! requirement here. Retention deletes whole conversations, and if a single
//! conversation ever needs rolling, the `segment` records already say where a
//! compaction fell.
//!
//! # Why the store is not under a window session
//!
//! A conversation outlives the window that hosted it. Resume it tomorrow, in a
//! different window, in a different pane, and its bench is found by the only
//! name that stayed the same.
//!
//! # Why a root holds numbered segments
//!
//! From `scripts/td-agent-ledger`, quoted rather than paraphrased because an
//! earlier draft of this had it backwards:
//!
//! ```text
//! clear | startup    ->  root=$sid         seq=0          a conversation BEGINS
//! compact | resume   ->  root=$prev_root   seq=seq+1      a conversation CONTINUES
//! ```
//!
//! **A `/clear` mints a new root.** So two segments of one root are always the
//! same conversation carried on through a compaction or an in-pane resume, and
//! they share one file. The boundary a person means by "start again" is the
//! root, and cleared work sits in a different file that is never opened.
//!
//! Getting that backwards is not a cosmetic error: a reader that opened only the
//! current segment would go blank in front of somebody mid-conversation, the
//! moment their agent compacted, with the read succeeding and simply the wrong
//! thing read.
//!
//! # Why an ask says where its words came from, and who said them
//!
//! Two fields on every [`Rec::Ask`], because both absences are real and neither
//! may read as the other.
//!
//! [`Origin`] is how this window knows the words. `Hook` is the harness's own
//! `UserPromptSubmit` payload — the person's text, verbatim. `Screen` is the
//! pane's scrollback latch, which is a reading of a rendered terminal and is
//! wrong the moment the person pastes something the screen wrapped. A caption
//! that cannot say which it has is a caption claiming to be exact.
//!
//! [`Kind`] is who said them, and it exists because **the harness reports its
//! own machine turns as prompts**. Measured on this box, 2026-09-21: of 114
//! `prompt` records across the live mailboxes, 62 open with `<task-notification>`
//! or `<cross-session-message`, which no person typed. Dropping them would put
//! holes in the record and leave the next reply answering an ask the file never
//! saw; showing them turns an overview of a conversation into a log. So every
//! record is kept and [`prompt_kind`] marks it, the overview draws `Person`, and
//! an envelope this build does not recognise reads as `Person` — the safe
//! direction is to show one machine turn too many rather than to hide a question
//! somebody asked.
//!
//! # Why images live in a shared pool
//!
//! `conversations/assets/<sha256>.<ext>`, referenced from an ask by hash, never
//! inlined. A transcript that carries a screenshot as base64 stops being a file
//! anyone opens, and the same screenshot pasted to three agents is one object on
//! disk rather than three. The pool is shared across conversations for that
//! reason, and content-addressed so a reference stays true if a file moves.
//!
//! **Nothing writes to the pool yet**, because no hook on this machine hands
//! over image bytes — [`Image`] is parsed, round-tripped and reported by the
//! CLI, and [`asset_path`] answers *where it would be and whether it is there*.
//! That is the honest state: the reference is part of the format, the bytes are
//! `unavailable`, and the two are different facts.

use crate::surface::PANE_HISTORY_CAP;
use crate::vitals::Bond;
use serde_json::{json, Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The format's own version, on every line.
///
/// On every line rather than in a header, because the file is append-only and a
/// header is written once by whichever build created it — which is never the
/// build that wrote the line a reader is puzzled by.
pub const WS: &str = "0.1";

/// Which conversation a bench is showing, and which segment of it.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct ConvKey {
    /// The conversation. A session id the ledger minted at a `startup` or a
    /// `/clear`. This alone names the file.
    pub root: String,
    /// How far along. Incremented by a compaction or an in-pane resume, never
    /// by a clear. Recorded in a [`Rec::Segment`] line rather than in the path.
    pub seq: u32,
}

/// The rule a segment of a path has to satisfy before it is allowed to BE one.
///
/// The same rule the ledger hook applies to `session_id`, `source`, `prev_sid`
/// and `prev_root` on the way in. Applied again here because the file it
/// applies them in is user-writable, so this end cannot assume that end ran.
///
/// Deliberately a whitelist. A blacklist of `..` and `/` would pass a name
/// carrying a newline into a JSONL line, or a NUL into a path.
pub fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The same question for a SURFACE id, whose alphabet is wider than a root's.
///
/// A root is a session id — a uuid, so `[A-Za-z0-9_-]` covers all of it. A
/// surface id is whatever the agent called its document, and
/// `SurfaceId::sanitise` deliberately keeps `.` and `:` too. Applying the root's
/// alphabet to an id refuses `plan.v2` and `decision:1`, which are legal
/// surfaces an agent can present today.
///
/// **That refusal would not merely lose the surface, it would loop.** [`file`]
/// is the thing the caller drains the mailbox AFTER, so a surface rejected here
/// is never recorded and never removed — re-swept and re-delivered on every
/// pass, forever.
///
/// An id no longer becomes a file name, so the path-traversal half of this is no
/// longer load-bearing. It stays anyway: the id is a key a reader greps for in a
/// line-oriented file, and a `..` or a leading dot in one is a name that arrived
/// from somewhere it should not have.
pub fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('.')
        && !s.contains("..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

/// A content address: 64 lowercase hex characters and nothing else.
///
/// Checked rather than trusted for the same reason a root is: it becomes a file
/// name in a shared pool, and it arrives from whoever handed over the bytes.
pub fn safe_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `<root_dir>/<root>.ws`, validated. The whole conversation.
///
/// Returns an `Option` rather than sanitising, because a root that fails the
/// test is not a root with a typo in it — it is a value that reached here from
/// somewhere it should not have, and quietly repairing it files a conversation
/// under a name nobody chose.
pub fn transcript_path(root_dir: &Path, root: &str) -> Option<PathBuf> {
    safe_segment(root).then(|| root_dir.join(format!("{root}.ws")))
}

/// Where images live: one pool for every conversation.
pub fn assets_dir(root_dir: &Path) -> PathBuf {
    root_dir.join("assets")
}

/// Where an image with this hash would be, whether or not it is there.
///
/// The extension is presentational — the hash is the identity — so a name that
/// could leave the pool is refused rather than sanitised, same as a root.
pub fn asset_path(root_dir: &Path, sha256: &str, ext: &str) -> Option<PathBuf> {
    let ext_ok =
        !ext.is_empty() && ext.len() <= 8 && ext.bytes().all(|b| b.is_ascii_alphanumeric());
    (safe_hash(sha256) && ext_ok).then(|| assets_dir(root_dir).join(format!("{sha256}.{ext}")))
}

/// How this window knows what the person said.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// The harness's own `UserPromptSubmit` payload: the text, verbatim.
    Hook,
    /// Read off the pane's scrollback. A rendering of what was typed, not the
    /// typing — wrapped, truncated and re-flowed by a terminal.
    Screen,
}

impl Origin {
    pub fn as_str(self) -> &'static str {
        match self {
            Origin::Hook => "hook",
            Origin::Screen => "screen",
        }
    }

    /// An unrecognised word reads as [`Origin::Screen`], the weaker claim. A
    /// future build's stronger word must not be believed by this one.
    fn parse(s: Option<&str>) -> Origin {
        match s {
            Some("hook") => Origin::Hook,
            _ => Origin::Screen,
        }
    }
}

/// Who an ask came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A person typed it.
    Person,
    /// The harness generated it — a task notification, a cross-session
    /// message — and reported it through the same hook.
    System,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Person => "person",
            Kind::System => "system",
        }
    }

    /// An unrecognised word reads as [`Kind::Person`]: showing one machine turn
    /// too many is recoverable, hiding a question somebody asked is not.
    fn parse(s: Option<&str>) -> Kind {
        match s {
            Some("system") => Kind::System,
            _ => Kind::Person,
        }
    }
}

/// The envelopes the harness wraps its own turns in, measured rather than
/// guessed.
///
/// 2026-09-21, across every live mailbox on this machine: 114 `prompt` records,
/// of which 52 opened `<task-notification>` and 10 opened
/// `<cross-session-message`. Every other opening line was something a person
/// typed, including slash commands (`/thread-tie-off`) and pasted paths, which
/// is why the table is prefixes of machine envelopes and not a heuristic about
/// what a person's message looks like.
const SYSTEM_ENVELOPES: [&str; 2] = ["<task-notification>", "<cross-session-message"];

/// Whether a prompt the hook reported was typed by a person.
///
/// Reads the first non-empty line only. A `<system-reminder>` block appended to
/// somebody's message does not make the message the machine's, and the records
/// on disk show that is how reminders arrive — after the text, never instead of
/// it.
pub fn prompt_kind(text: &str) -> Kind {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty());
    match first {
        Some(l) if SYSTEM_ENVELOPES.iter().any(|e| l.starts_with(e)) => Kind::System,
        _ => Kind::Person,
    }
}

/// An image an ask carried, by reference.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Image {
    /// The content address. The bytes live at [`asset_path`].
    pub sha256: String,
    /// What it is, as the source declared it — `image/png`, say.
    pub mime: String,
    /// How big it is. `None` when whoever handed over the reference did not
    /// say, which is not the same as an empty file.
    pub bytes: Option<u64>,
}

impl Image {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("sha256".into(), json!(self.sha256));
        m.insert("mime".into(), json!(self.mime));
        if let Some(b) = self.bytes {
            m.insert("bytes".into(), json!(b));
        }
        Value::Object(m)
    }

    /// `None` for a reference without a usable address: a hash that could name
    /// a path is not repaired into one that cannot.
    fn from_json(v: &Value) -> Option<Image> {
        let sha256 = v.get("sha256")?.as_str()?.to_string();
        if !safe_hash(&sha256) {
            return None;
        }
        Some(Image {
            sha256,
            mime: v
                .get("mime")
                .and_then(Value::as_str)
                .unwrap_or("application/octet-stream")
                .to_string(),
            bytes: v.get("bytes").and_then(Value::as_u64),
        })
    }
}

/// One line of the transcript.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Rec {
    /// Where a segment of this conversation started: a startup, a resume or a
    /// compaction. Carries no turn — it is a boundary, not a thing said.
    Segment {
        seq: u32,
        at_ms: u64,
        /// The ledger's own word: `startup`, `resume`, `compact`, `clear`.
        source: String,
        /// Which program: `claude`, `codex`.
        agent: String,
        pid: u32,
    },
    /// What the person said.
    Ask {
        n: u32,
        at_ms: u64,
        origin: Origin,
        kind: Kind,
        text: String,
        images: Vec<Image>,
    },
    /// A surface that answered turn `n`, stored whole.
    ///
    /// `bond` is how strongly the pane was bound when this was filed, copied
    /// from the resolver — so a later reader can tell a filing made under
    /// `Declared` from one made under `Birth` rather than treating every row as
    /// equally attributed.
    Said {
        n: u32,
        at_ms: u64,
        id: String,
        bond: Bond,
        surface: Value,
    },
    /// A round of questions the agent put to the person.
    Asked {
        n: u32,
        at_ms: u64,
        tool_use_id: String,
        questions: Value,
    },
    /// How that round was answered, and by which road.
    Answered {
        n: u32,
        at_ms: u64,
        tool_use_id: String,
        answers: Value,
        /// How this record learned the answer: `result` when the harness's
        /// own tool result carried it. The road the answer travelled to get
        /// there — the channel's file, keys and sentence — is the channel's
        /// business and is in its `outbound.jsonl`, because a transcript that
        /// restated it would be a second copy to keep in step.
        road: String,
    },
}

impl Rec {
    /// The turn this line belongs to. `None` for a boundary.
    pub fn n(&self) -> Option<u32> {
        match self {
            Rec::Segment { .. } => None,
            Rec::Ask { n, .. }
            | Rec::Said { n, .. }
            | Rec::Asked { n, .. }
            | Rec::Answered { n, .. } => Some(*n),
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Rec::Segment {
                seq,
                at_ms,
                source,
                agent,
                pid,
            } => json!({ "ws": WS, "t": "segment", "seq": seq, "at_ms": at_ms,
                         "source": source, "agent": agent, "pid": pid }),
            Rec::Ask {
                n,
                at_ms,
                origin,
                kind,
                text,
                images,
            } => json!({ "ws": WS, "t": "ask", "n": n, "at_ms": at_ms,
                         "origin": origin.as_str(), "kind": kind.as_str(),
                         "text": text,
                         "images": images.iter().map(Image::to_json).collect::<Vec<_>>() }),
            Rec::Said {
                n,
                at_ms,
                id,
                bond,
                surface,
            } => json!({ "ws": WS, "t": "said", "n": n, "at_ms": at_ms,
                         "id": id, "bond": bond.as_str(), "surface": surface }),
            Rec::Asked {
                n,
                at_ms,
                tool_use_id,
                questions,
            } => json!({ "ws": WS, "t": "asked", "n": n, "at_ms": at_ms,
                         "tool_use_id": tool_use_id, "questions": questions }),
            Rec::Answered {
                n,
                at_ms,
                tool_use_id,
                answers,
                road,
            } => json!({ "ws": WS, "t": "answered", "n": n, "at_ms": at_ms,
                         "tool_use_id": tool_use_id, "answers": answers, "road": road }),
        }
    }

    pub fn from_json(v: &Value) -> Option<Rec> {
        let at_ms = v.get("at_ms")?.as_u64()?;
        let n = || v.get("n").and_then(Value::as_u64).map(|x| x as u32);
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        match v.get("t")?.as_str()? {
            "segment" => Some(Rec::Segment {
                seq: v.get("seq").and_then(Value::as_u64).unwrap_or(0) as u32,
                at_ms,
                source: s("source").unwrap_or_else(|| "unknown".into()),
                agent: s("agent").unwrap_or_else(|| "unknown".into()),
                pid: v.get("pid").and_then(Value::as_u64).unwrap_or(0) as u32,
            }),
            "ask" => Some(Rec::Ask {
                n: n()?,
                at_ms,
                origin: Origin::parse(v.get("origin").and_then(Value::as_str)),
                kind: Kind::parse(v.get("kind").and_then(Value::as_str)),
                text: s("text")?,
                images: v
                    .get("images")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Image::from_json).collect())
                    .unwrap_or_default(),
            }),
            // A bond word this build does not know reads as `Guess`, the
            // weakest rung — an unrecognised claim of strength is not a strong
            // claim, and the whole point of recording the bond is that a reader
            // can refuse the weak ones.
            "said" => Some(Rec::Said {
                n: n()?,
                at_ms,
                id: s("id")?,
                bond: match v.get("bond").and_then(Value::as_str) {
                    Some("declared") => Bond::Declared,
                    Some("birth") => Bond::Birth,
                    Some("sole") => Bond::Sole,
                    _ => Bond::Guess,
                },
                surface: v.get("surface")?.clone(),
            }),
            "asked" => Some(Rec::Asked {
                n: n()?,
                at_ms,
                tool_use_id: s("tool_use_id")?,
                questions: v.get("questions").cloned().unwrap_or(Value::Null),
            }),
            "answered" => Some(Rec::Answered {
                n: n()?,
                at_ms,
                tool_use_id: s("tool_use_id")?,
                answers: v.get("answers").cloned().unwrap_or(Value::Null),
                road: s("road").unwrap_or_else(|| "unknown".into()),
            }),
            _ => None,
        }
    }
}

/// Where conversations live: `$XDG_STATE_HOME/terminal-delight/conversations`.
///
/// Beside `surfaces/`, not inside it, and named by nothing that belongs to a
/// window — a conversation outlives the window that hosted it. Resolved the same
/// way as [`crate::surfacefeed::surfaces_root`], and that is not a stylistic
/// choice: the two are a pair. A relative `XDG_STATE_HOME` — or an unset `HOME`,
/// which `unwrap_or_default` turns into an empty string — would put the store
/// under whatever directory the window happened to be launched from while the
/// mailbox it is paired with sits under `$HOME`, and `terminal-delight
/// conversation <root>` run from elsewhere would then read a different store and
/// truthfully report "no record".
pub fn store_root() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::session::home_dir().join(".local/state"));
    base.join("terminal-delight/conversations")
}

/// Append one line. The only writer.
///
/// No temp file and no rename: a line is appended whole to a file opened in
/// append mode, which is the one write the kernel will not interleave with
/// another appender's. The failure this leaves reachable is a torn LAST line
/// after a crash, which [`read`] handles by name.
pub fn append(root_dir: &Path, root: &str, rec: &Rec) -> io::Result<()> {
    let path = transcript_path(root_dir, root)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unsafe conversation root"))?;
    fs::create_dir_all(root_dir)?;
    let line = format!("{}\n", rec.to_json());
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    f.write_all(line.as_bytes())
}

/// Mark where a segment of this conversation began.
pub fn segment(
    root_dir: &Path,
    key: &ConvKey,
    at_ms: u64,
    source: &str,
    agent: &str,
    pid: u32,
) -> io::Result<()> {
    append(
        root_dir,
        &key.root,
        &Rec::Segment {
            seq: key.seq,
            at_ms,
            source: source.to_string(),
            agent: agent.to_string(),
            pid,
        },
    )
}

/// Put one surface into a conversation's record.
///
/// **This does not remove the mailbox copy, and must not.** The caller drains
/// the inbox only after this returns `Ok`, so a crash between the two leaves the
/// surface in the mailbox and the worst case is one surface delivered twice.
/// Reversing the order makes the worst case a surface that existed and then did
/// not.
pub fn file(
    root_dir: &Path,
    root: &str,
    turn: u32,
    id: &str,
    value: &Value,
    bond: Bond,
    at_ms: u64,
) -> io::Result<()> {
    if !safe_id(id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "unsafe id"));
    }
    // The id the record is keyed by and the id inside the document are the same
    // thing, and the filing id is the one that wins. A document carrying a
    // DIFFERENT id is the case that matters: left alone it is stored under one
    // name still calling itself another, so a retire aimed at one of them
    // misses.
    //
    // A payload that is not an object carries no id and cannot be given one. It
    // is stored as it came rather than refused — the parser downstream is
    // lenient by design and turns an unusable document into a visible
    // `Unclassified` card, which is a better answer than a surface that silently
    // never arrives.
    let surface = match value.as_object() {
        Some(m) => {
            let mut owned: Map<String, Value> = m.clone();
            owned.insert("id".into(), json!(id));
            Value::Object(owned)
        }
        None => value.clone(),
    };
    append(
        root_dir,
        root,
        &Rec::Said {
            n: turn,
            at_ms,
            id: id.to_string(),
            bond,
            surface,
        },
    )
}

/// Record a round of questions the agent asked.
pub fn asked(
    root_dir: &Path,
    root: &str,
    n: u32,
    at_ms: u64,
    tool_use_id: &str,
    questions: &Value,
) -> io::Result<()> {
    append(
        root_dir,
        root,
        &Rec::Asked {
            n,
            at_ms,
            tool_use_id: tool_use_id.to_string(),
            questions: questions.clone(),
        },
    )
}

/// Record how a round was answered.
pub fn answered(
    root_dir: &Path,
    root: &str,
    n: u32,
    at_ms: u64,
    tool_use_id: &str,
    answers: &Value,
    road: &str,
) -> io::Result<()> {
    append(
        root_dir,
        root,
        &Rec::Answered {
            n,
            at_ms,
            tool_use_id: tool_use_id.to_string(),
            answers: answers.clone(),
            road: road.to_string(),
        },
    )
}

/// Every line of one conversation, in the order it was written.
pub fn records(root_dir: &Path, root: &str) -> Vec<Rec> {
    read(root_dir, root).0
}

/// Every line, plus what could not be read.
///
/// A line that does not parse is counted rather than skipped in silence, and the
/// LAST line failing is counted separately: an append-only file's normal crash
/// shape is a torn tail, and a reader that cannot tell that from a corrupt
/// record in the middle reports a data loss every time a window is killed.
fn read(root_dir: &Path, root: &str) -> (Vec<Rec>, usize, bool) {
    let Some(path) = transcript_path(root_dir, root) else {
        return (Vec::new(), 0, false);
    };
    let Ok(body) = fs::read_to_string(&path) else {
        return (Vec::new(), 0, false);
    };
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    let last = lines.len().saturating_sub(1);
    let mut out = Vec::new();
    let mut unreadable = 0usize;
    let mut torn_tail = false;
    for (i, line) in lines.iter().enumerate() {
        match serde_json::from_str::<Value>(line)
            .ok()
            .as_ref()
            .and_then(Rec::from_json)
        {
            Some(r) => out.push(r),
            None if i == last => torn_tail = true,
            None => unreadable += 1,
        }
    }
    (out, unreadable, torn_tail)
}

/// The whole conversation's surfaces, oldest first, capped at
/// [`PANE_HISTORY_CAP`].
///
/// Returns the raw documents rather than parsed surfaces, because that is what
/// the mailbox holds and what the bench's own parser already takes — a round
/// trip through a typed struct would be a second format to keep in step.
pub fn load(root_dir: &Path, root: &str) -> Loaded {
    let (recs, unreadable, torn_tail) = read(root_dir, root);

    // Keyed by id so a surface re-presented — a card updated in place, which is
    // the normal path for `present_surface` — is ONE row holding the current
    // copy, and spends ONE slot of the cap.
    let mut newest: std::collections::HashMap<String, (u64, u64, Value)> =
        std::collections::HashMap::new();
    for r in &recs {
        if let Rec::Said {
            at_ms, id, surface, ..
        } = r
        {
            match newest.get_mut(id) {
                // A re-presented surface keeps its ORIGINAL place in the
                // conversation and takes its LATEST content: it is the row that
                // was already there, updated, not a new thing said at the
                // bottom.
                Some(slot) => {
                    slot.0 = slot.0.min(*at_ms);
                    if *at_ms >= slot.1 {
                        slot.1 = *at_ms;
                        slot.2 = surface.clone();
                    }
                }
                None => {
                    newest.insert(id.clone(), (*at_ms, *at_ms, surface.clone()));
                }
            }
        }
    }

    let mut out: Vec<(String, u64, Value)> = newest
        .into_iter()
        .map(|(k, (first, _, v))| (k, first, v))
        .collect();
    // Oldest first, by WHEN — never by id. Ids are not monotonic: `derive.rs`
    // mints `ask-<hash>`, `surface.rs` mints `anon-<hash>`, and an agent may
    // supply any slug it likes, so sorting by the id is sorting by nothing.
    out.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    if out.len() > PANE_HISTORY_CAP {
        out.drain(..out.len() - PANE_HISTORY_CAP);
    }
    Loaded {
        surfaces: out.into_iter().map(|(k, _, v)| (k, v)).collect(),
        unreadable,
        torn_tail,
    }
}

/// What a conversation's surfaces came back as, including what did not.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Oldest first, deduplicated by id, capped at [`PANE_HISTORY_CAP`].
    pub surfaces: Vec<(String, Value)>,
    /// Lines that are there and could not be read, excluding a torn tail. **Not
    /// folded into the count above**: "this conversation presented four things"
    /// and "it presented five and one of them is corrupt" are different answers,
    /// and only one of them tells a reader to go and look.
    pub unreadable: usize,
    /// The last line was incomplete — the ordinary shape of a window killed
    /// mid-append, and a different finding from a corrupt record in the middle.
    pub torn_tail: bool,
}

/// The ordinal the next ask should carry.
///
/// Counted from the record rather than held in memory, so a window restart
/// mid-conversation does not restart the numbering and file turn 1's reply under
/// turn 0.
pub fn next_turn(root_dir: &Path, root: &str) -> u32 {
    records(root_dir, root)
        .iter()
        .filter_map(Rec::n)
        .max()
        .map_or(0, |n| n + 1)
}

/// Which conversation a session id belongs to.
///
/// The road the window takes, and it is deliberately the SESSION id rather than
/// the agent's pid. The pid road ([`crate::tenancy::tenancy_for`]) reads the
/// live per-process entry and is the stronger instrument, but it needs a pid
/// the window does not hold: the binding it already computes every sweep is
/// [`crate::paneident::certain`], which is a session id and refuses to answer
/// at all where two agents under one shell make the pane ambiguous. Refusing is
/// what "neither means no filing" asks for, so the weaker-looking road carries
/// the stronger guarantee here.
///
/// The three answers the ledger can give, and what each means for the key:
///
/// - `Chained` — the conversation compacted or resumed. Its root is the id it
///   started as, which is the whole point of the ledger.
/// - `Unchained` — the ledger was read and does not name this id, so **the id
///   is its own root**, in the ledger's own words.
/// - `Unrecorded` — there is no ledger on this machine. No root may be inferred
///   *from the ledger*, and none is: the id is used as the root because the
///   pane's own binding is a different instrument that did answer. What that
///   costs is visible and honest — on a machine with no hook, a compaction
///   mints an id nothing ties to the old one, so the bench starts empty rather
///   than silently showing somebody else's record.
pub fn key_for_session(session_id: &str, home: &Path) -> Option<ConvKey> {
    if !safe_segment(session_id) {
        return None;
    }
    Some(match crate::tenancy::tenancy_of(session_id, home) {
        crate::tenancy::Tenancy::Chained(c) => ConvKey {
            root: c.root,
            seq: c.seq,
        },
        _ => ConvKey {
            root: session_id.to_string(),
            seq: 0,
        },
    })
}

/// `terminal-delight conversation <root>` — read a conversation's bench back
/// without a window.
///
/// The headless instrument for this store, and the same argument `bindings` is
/// for the resolver: a record nobody can inspect is a record nobody can debug,
/// and this one decides what a person sees on a bench.
pub fn run_cli(args: &[String]) -> i32 {
    // The first NON-FLAG argument. `--json` passes `safe_segment` — hyphens are
    // legal in a session id — so reading `args.first()` blind makes
    // `conversation --json` report on a conversation called `--json`, print "no
    // record" and exit 0. A reader chasing a missing record would get a
    // confident wrong answer with a successful exit.
    let Some(root) = args.iter().find(|a| !a.starts_with('-')) else {
        eprintln!("usage: terminal-delight conversation <root> [--json]");
        return 2;
    };
    if !safe_segment(root) {
        eprintln!("not a conversation root: {root:?}");
        return 2;
    }
    let dir = store_root();
    let Loaded {
        surfaces,
        unreadable,
        torn_tail,
    } = load(&dir, root);
    let recs = records(&dir, root);
    let path = transcript_path(&dir, root);

    if args.iter().any(|a| a == "--json") {
        println!(
            "{}",
            json!({
                "root": root,
                "path": path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                "exists": path.as_ref().is_some_and(|p| p.exists()),
                "next_turn": next_turn(&dir, root),
                "unreadable": unreadable,
                "torn_tail": torn_tail,
                "surfaces": surfaces.iter().map(|(id, v)| json!({
                    "id": id, "title": v.get("title"), "kind": v.get("kind"),
                })).collect::<Vec<_>>(),
                "records": recs.iter().map(Rec::to_json).collect::<Vec<_>>(),
            })
        );
        return 0;
    }

    match &path {
        Some(p) if p.exists() => println!("{}", p.display()),
        // Said rather than left blank: "no such conversation" and "a
        // conversation that presented nothing" are different answers and the
        // empty listing below looks identical for both.
        Some(p) => {
            println!("{} (no record — nothing has been filed here)", p.display());
            return 0;
        }
        None => return 2,
    }
    println!("next turn: {}", next_turn(&dir, root));
    for r in &recs {
        match r {
            Rec::Segment {
                seq, source, agent, ..
            } => println!("  ---  {agent} {source} -> segment {seq}"),
            Rec::Ask {
                n,
                origin,
                kind,
                text,
                images,
                ..
            } => {
                let head = text.lines().next().unwrap_or("");
                println!(
                    "  {n:>3}  {:<6} {head} [{}]",
                    kind.as_str(),
                    origin.as_str()
                );
                for im in images {
                    let at = asset_path(&dir, &im.sha256, ext_for(&im.mime));
                    let state = match &at {
                        Some(p) if p.exists() => "present".to_string(),
                        // The reference is a fact and the bytes are a separate
                        // fact. Nothing writes the pool yet, so this is the
                        // ordinary answer and it says so rather than reading as
                        // a loss.
                        Some(p) => format!("unavailable at {}", p.display()),
                        None => "unusable reference".to_string(),
                    };
                    println!("       image {} {} — {state}", &im.sha256[..12], im.mime);
                }
            }
            Rec::Said { n, id, bond, .. } => {
                println!("  {n:>3}  agent  {id}  [{}]", bond.as_str())
            }
            Rec::Asked { n, tool_use_id, .. } => println!("  {n:>3}  asked  {tool_use_id}"),
            Rec::Answered {
                n,
                tool_use_id,
                road,
                ..
            } => println!("  {n:>3}  answer {tool_use_id} via {road}"),
        }
    }
    if unreadable > 0 || torn_tail {
        println!(
            "{} surfaces ({unreadable} unreadable{})",
            surfaces.len(),
            if torn_tail { ", torn last line" } else { "" }
        );
    } else {
        println!("{} surfaces", surfaces.len());
    }
    for (id, v) in &surfaces {
        println!(
            "  {id}  {}",
            v.get("title").and_then(Value::as_str).unwrap_or("—")
        );
    }
    0
}

/// A file extension for a declared mime type. Presentational only — the hash is
/// the identity — so anything unrecognised gets a name that says so rather than
/// a guess that looks authoritative.
fn ext_for(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsync::Scratch;

    fn doc(title: &str) -> Value {
        json!({ "td": "0.4", "kind": "markdown", "title": title,
                "model": { "body": title } })
    }

    fn titles(v: &[(String, Value)]) -> Vec<String> {
        v.iter()
            .map(|(_, d)| d["title"].as_str().unwrap_or("?").to_string())
            .collect()
    }

    fn said(s: &Scratch, root: &str, n: u32, id: &str, title: &str, at: u64) {
        file(s.path(), root, n, id, &doc(title), Bond::Declared, at).unwrap();
    }

    fn person(s: &Scratch, root: &str, n: u32, at: u64, text: &str) {
        append(
            s.path(),
            root,
            &Rec::Ask {
                n,
                at_ms: at,
                origin: Origin::Hook,
                kind: Kind::Person,
                text: text.into(),
                images: vec![],
            },
        )
        .unwrap()
    }

    /// The case an earlier version of this design broke.
    ///
    /// A compaction CONTINUES a conversation, so its segments share one file and
    /// the load takes all of it. A design that isolated segments would empty a
    /// bench in front of somebody mid-conversation, with every step succeeding.
    #[test]
    fn a_compaction_keeps_the_whole_conversation() {
        let s = Scratch::new("bs-compact");
        said(&s, "rootA", 0, "one", "Before the compaction", 1);
        segment(
            s.path(),
            &ConvKey {
                root: "rootA".into(),
                seq: 1,
            },
            2,
            "compact",
            "claude",
            42,
        )
        .unwrap();
        said(&s, "rootA", 1, "two", "After the compaction", 3);

        let got = load(s.path(), "rootA").surfaces;

        assert_eq!(got.len(), 2, "both sides of the compaction load");
        assert_eq!(
            titles(&got),
            vec!["Before the compaction", "After the compaction"],
            "oldest first, across the whole conversation"
        );
        assert!(
            records(s.path(), "rootA")
                .iter()
                .any(|r| matches!(r, Rec::Segment { seq: 1, source, .. } if source == "compact")),
            "and the boundary is in the record, where a directory used to be"
        );
    }

    /// And the case it was trying to protect, which sits one level up.
    #[test]
    fn a_clear_starts_an_empty_bench() {
        let s = Scratch::new("bs-clear");
        said(&s, "rootA", 0, "old", "Before the clear", 1);

        // A clear mints a NEW ROOT, so the cleared work is in a file this load
        // never opens.
        assert!(load(s.path(), "rootB").surfaces.is_empty());
        assert_eq!(
            load(s.path(), "rootA").surfaces.len(),
            1,
            "and is not destroyed"
        );
    }

    /// One conversation is one file, which is the property the whole shape is
    /// chosen for: a thing you can hand to somebody.
    #[test]
    fn a_conversation_is_one_file_and_two_are_two() {
        let s = Scratch::new("bs-one-file");
        said(&s, "rootA", 0, "a", "A", 1);
        said(&s, "rootB", 0, "b", "B", 2);

        let a = transcript_path(s.path(), "rootA").unwrap();
        let b = transcript_path(s.path(), "rootB").unwrap();
        assert!(a.is_file() && b.is_file());
        assert_eq!(a.extension().unwrap(), "ws");
        let names: Vec<String> = fs::read_dir(s.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 2, "no directories, no sidecars: {names:?}");
    }

    /// The store is addressed by conversation, not by window session, so the
    /// same root read from anywhere finds the same record.
    #[test]
    fn a_conversation_outlives_the_window_that_made_it() {
        let s = Scratch::new("bs-outlive");
        said(&s, "r", 0, "a", "Made in one window", 1);
        // Nothing about the reader names a window, a session or a pane.
        assert_eq!(
            titles(&load(s.path(), "r").surfaces),
            vec!["Made in one window"]
        );
    }

    /// A root that could name a path yields no file, rather than a sanitised
    /// one — a repaired root files a conversation under a name nobody chose.
    #[test]
    fn a_root_that_could_name_a_path_is_refused() {
        let s = Scratch::new("bs-path");
        for bad in [
            "../../etc/passwd",
            "a/b",
            "",
            "with space",
            "nul\0byte",
            "a\nb",
            "a.ws",
        ] {
            assert!(!safe_segment(bad), "{bad:?} must not be a segment");
            assert!(
                transcript_path(s.path(), bad).is_none(),
                "{bad:?} must not resolve to a file"
            );
            assert!(
                file(s.path(), bad, 0, "x", &doc("x"), Bond::Declared, 1).is_err(),
                "{bad:?} must not be filed under"
            );
        }
        assert!(safe_segment("470a6cfd-35e7-41cb-8a8c-b3616d932a5f"));
    }

    /// An id is a key a reader greps for, and it arrives from the same untrusted
    /// place a root does.
    #[test]
    fn a_surface_id_that_could_name_a_path_is_refused() {
        let s = Scratch::new("bs-id");
        assert!(file(s.path(), "r", 0, "../escape", &doc("x"), Bond::Declared, 1).is_err());
    }

    /// The turn record is what lets an OLDER card be captioned, which is the
    /// thing the bench cannot do from memory.
    #[test]
    fn every_surface_records_the_turn_it_answered() {
        let s = Scratch::new("bs-turns");
        person(&s, "r", 0, 10, "what is the key");
        said(&s, "r", 0, "a", "first answer", 11);
        person(&s, "r", 1, 20, "and what about clear");
        file(
            s.path(),
            "r",
            1,
            "b",
            &doc("second answer"),
            Bond::Birth,
            21,
        )
        .unwrap();

        let t = records(s.path(), "r");
        assert_eq!(t.len(), 4, "two asks and two answers");
        assert_eq!(
            t[0],
            Rec::Ask {
                n: 0,
                at_ms: 10,
                origin: Origin::Hook,
                kind: Kind::Person,
                text: "what is the key".into(),
                images: vec![],
            }
        );
        assert_eq!(
            t[3].n(),
            Some(1),
            "the second answer belongs to the second turn"
        );
        assert!(
            matches!(
                &t[3],
                Rec::Said {
                    bond: Bond::Birth,
                    ..
                }
            ),
            "and carries how strongly the pane was bound when it was filed"
        );
    }

    /// A window restart mid-conversation must not restart the numbering, or turn
    /// 1's reply is filed under turn 0.
    #[test]
    fn the_next_turn_is_counted_from_the_record_not_from_memory() {
        let s = Scratch::new("bs-next");
        assert_eq!(next_turn(s.path(), "r"), 0, "nothing said yet");
        person(&s, "r", 0, 10, "one");
        assert_eq!(next_turn(s.path(), "r"), 1);
        person(&s, "r", 1, 20, "two");
        assert_eq!(next_turn(s.path(), "r"), 2, "counted across the whole file");
        // A boundary carries no turn and must not be counted as one.
        segment(
            s.path(),
            &ConvKey {
                root: "r".into(),
                seq: 1,
            },
            25,
            "compact",
            "claude",
            7,
        )
        .unwrap();
        assert_eq!(next_turn(s.path(), "r"), 2, "a segment mark is not a turn");
    }

    /// An append-only file's normal crash shape is a torn last line, and it is a
    /// different finding from a corrupt record in the middle.
    #[test]
    fn a_torn_last_line_is_told_apart_from_a_corrupt_one() {
        let s = Scratch::new("bs-torn");
        person(&s, "r", 0, 10, "kept");
        said(&s, "r", 0, "a", "also kept", 11);
        let p = transcript_path(s.path(), "r").unwrap();
        let mut body = fs::read_to_string(&p).unwrap();
        body.push_str("{\"t\":\"ask\",\"n\":1,\"at_m");
        fs::write(&p, &body).unwrap();

        let got = load(s.path(), "r");
        assert_eq!(
            records(s.path(), "r").len(),
            2,
            "everything before the tear"
        );
        assert!(got.torn_tail, "and the tear is reported as a tear");
        assert_eq!(got.unreadable, 0, "not as a corrupt record");

        // The same damage in the MIDDLE is a real loss and counts as one.
        let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
        lines.insert(1, "{\"t\":\"said\",\"n\":0,\"at_m".into());
        fs::write(&p, lines.join("\n")).unwrap();
        let got = load(s.path(), "r");
        assert_eq!(got.unreadable, 1);
        assert!(got.torn_tail);
    }

    /// A bond word from a future build is read as the WEAKEST rung, never the
    /// strongest — an unrecognised claim of strength is not a strong claim.
    #[test]
    fn an_unknown_bond_word_reads_as_the_weakest_rung() {
        let s = Scratch::new("bs-bond");
        said(&s, "r", 0, "a", "x", 1);
        let p = transcript_path(s.path(), "r").unwrap();
        let body = fs::read_to_string(&p)
            .unwrap()
            .replace("\"declared\"", "\"cryptographic\"");
        fs::write(&p, body).unwrap();

        assert!(matches!(
            records(s.path(), "r").first(),
            Some(Rec::Said {
                bond: Bond::Guess,
                ..
            })
        ));
    }

    /// The same rule for the two words on an ask, in the two directions their
    /// consequences point.
    #[test]
    fn an_unknown_origin_is_weak_and_an_unknown_kind_is_shown() {
        let s = Scratch::new("bs-words");
        person(&s, "r", 0, 10, "typed");
        let p = transcript_path(s.path(), "r").unwrap();
        let body = fs::read_to_string(&p)
            .unwrap()
            .replace("\"hook\"", "\"quantum\"")
            .replace("\"person\"", "\"oracle\"");
        fs::write(&p, body).unwrap();

        match records(s.path(), "r").first() {
            Some(Rec::Ask { origin, kind, .. }) => {
                assert_eq!(
                    *origin,
                    Origin::Screen,
                    "an unknown origin must not claim to be verbatim"
                );
                assert_eq!(
                    *kind,
                    Kind::Person,
                    "an unknown kind must not hide the turn"
                );
            }
            other => panic!("expected an ask, got {other:?}"),
        }
    }

    /// The harness reports its own turns through the same hook as a person's.
    ///
    /// Measured 2026-09-21 across every live mailbox on this machine: 62 of 114
    /// `prompt` records were machine envelopes. The fixtures below are those two
    /// openings verbatim, and the person cases include the ones that look
    /// machine-ish — a slash command and a pasted path.
    #[test]
    fn a_machine_turn_is_marked_and_a_person_who_looks_like_one_is_not() {
        for t in [
            "<task-notification>\n<task-id>bm5lyukcz</task-id>\n</task-notification>",
            "<cross-session-message from=\"uds:/run/user/1000/cc-socks/38\">hi</cross-session-message>",
            "\n\n<task-notification>\nlate\n</task-notification>",
        ] {
            assert_eq!(prompt_kind(t), Kind::System, "machine: {t:?}");
        }
        for t in [
            "/thread-tie-off",
            "/tmp/tmp.iCROdsCXQq/agent.sh",
            "ok, you are to get all the terminal delight work into main",
            "<p>some html i pasted</p>",
            "",
            "look at this\n<task-notification>quoted, not sent by the harness</task-notification>",
        ] {
            assert_eq!(prompt_kind(t), Kind::Person, "person: {t:?}");
        }
    }

    /// A reminder appended to somebody's message does not make the message the
    /// machine's. This is how reminders actually arrive.
    #[test]
    fn a_system_reminder_after_a_persons_words_leaves_it_theirs() {
        let t = "let's get to building now\n\n<system-reminder>\nbe careful\n</system-reminder>";
        assert_eq!(prompt_kind(t), Kind::Person);
    }

    /// The cap keeps the newest, and drops by time rather than by name.
    #[test]
    fn the_history_cap_keeps_the_newest_of_the_conversation() {
        let s = Scratch::new("bs-cap");
        let total = PANE_HISTORY_CAP + 10;
        for i in 0..total {
            // An id that sorts BACKWARDS against time. Sequential zero-padded
            // ids were the original fixture, and they are the one shape that
            // makes an id-ordered load look correct — so that test could not
            // fail against the bug it was named for.
            said(
                &s,
                "r",
                i as u32,
                &format!("s{:04}", total - 1 - i),
                &format!("surface {i:04}"),
                i as u64,
            );
        }
        let got = load(s.path(), "r").surfaces;
        assert_eq!(got.len(), PANE_HISTORY_CAP);
        assert_eq!(
            got.last().unwrap().1["title"].as_str().unwrap(),
            format!("surface {:04}", total - 1),
            "the newest survives the cap"
        );
        assert_eq!(
            got.first().unwrap().1["title"].as_str().unwrap(),
            format!("surface {:04}", total - PANE_HISTORY_CAP),
            "and the cap drops the OLDEST, not a lexical slice"
        );
    }

    /// Ordering comes from the record, never from the id.
    ///
    /// Real ids are not monotonic — `derive.rs` mints `ask-<hash>`,
    /// `surface.rs` mints `anon-<hash>`, and an agent may supply any slug it
    /// likes — so a load sorted by the id is sorted by nothing, and the bench
    /// draws its cards scrambled.
    #[test]
    fn ordering_follows_the_record_not_the_id() {
        let s = Scratch::new("bs-order-by-time");
        said(&s, "r", 0, "zzz-first", "first", 100);
        said(&s, "r", 0, "mmm-second", "second", 200);
        said(&s, "r", 0, "aaa-third", "third", 300);

        assert_eq!(
            titles(&load(s.path(), "r").surfaces),
            vec!["first", "second", "third"],
            "by when they arrived, not by what they are called"
        );
    }

    /// A card updated later is one row, in its original place, with its current
    /// content — because that is what updating a card means.
    #[test]
    fn a_surface_updated_later_is_one_row_that_stays_where_it_was() {
        let s = Scratch::new("bs-dupe");
        said(&s, "r", 0, "same", "Before", 10);
        said(&s, "r", 0, "later", "Something after it", 20);
        said(&s, "r", 1, "same", "After", 30);

        let got = load(s.path(), "r").surfaces;
        assert_eq!(got.len(), 2, "one id, one row");
        assert_eq!(
            titles(&got),
            vec!["After", "Something after it"],
            "the updated card keeps its place and takes the new content"
        );
    }

    /// An id the surface sanitiser allows must be fileable.
    ///
    /// A root's alphabet is a uuid's; an id's is wider. Applying the narrow one
    /// here refuses a legal surface forever: `file` is what the caller drains
    /// the mailbox after, so a rejected surface is never recorded AND never
    /// removed, and comes back on every sweep.
    #[test]
    fn an_id_the_sanitiser_allows_can_be_filed() {
        let s = Scratch::new("bs-wide-id");
        for id in ["plan.v2", "decision:1", "ask-live-9f2c", "anon-0badf00d"] {
            assert!(safe_id(id), "{id:?} is a legal surface id");
            file(s.path(), "r", 0, id, &doc(id), Bond::Declared, 1)
                .unwrap_or_else(|e| panic!("{id:?} should file: {e}"));
        }
        assert_eq!(load(s.path(), "r").surfaces.len(), 4);
        for bad in ["../escape", ".hidden", "a..b", "with space", "a/b", ""] {
            assert!(!safe_id(bad), "{bad:?} must not be an id");
        }
    }

    /// The filing id wins over one the document carries.
    #[test]
    fn a_document_whose_id_disagrees_is_rewritten_to_the_filed_name() {
        let s = Scratch::new("bs-id-clash");
        let mut d = doc("x");
        d["id"] = json!("something-else");
        file(s.path(), "r", 0, "named", &d, Bond::Declared, 1).unwrap();

        let (key, v) = load(s.path(), "r").surfaces.pop().unwrap();
        assert_eq!(key, "named");
        assert_eq!(
            v["id"].as_str(),
            Some("named"),
            "the key and the id inside must agree, or a retire aimed at one misses"
        );
    }

    /// A flag is not a conversation root.
    #[test]
    fn a_leading_flag_is_not_taken_as_the_root() {
        assert_eq!(run_cli(&["--json".to_string()]), 2);
    }

    /// Filing does not touch the mailbox. The caller drains after this returns
    /// `Ok`, so a crash between them costs a duplicate rather than the surface.
    #[test]
    fn filing_writes_the_store_and_says_nothing_about_the_inbox() {
        let s = Scratch::new("bs-inbox");
        let inbox = s.path().join("inbox");
        fs::create_dir_all(&inbox).unwrap();
        fs::write(inbox.join("a.json"), doc("still here").to_string()).unwrap();

        said(&s, "r", 0, "a", "filed", 1);

        assert!(
            inbox.join("a.json").exists(),
            "the store has no business removing the mailbox copy"
        );
    }

    /// A document that names no id gets the one it was filed under, so it comes
    /// back answering to the same thing.
    #[test]
    fn a_document_with_no_id_is_filed_carrying_the_one_it_was_given() {
        let s = Scratch::new("bs-id-fill");
        said(&s, "r", 0, "named", "x", 1);
        let (key, v) = load(s.path(), "r").surfaces.pop().unwrap();
        assert_eq!(key, "named");
        assert_eq!(
            v["id"].as_str(),
            Some("named"),
            "the name it is keyed by is inside it too"
        );
    }

    /// A question round and its answer are in the record, so a resumed bench can
    /// show a question as answered rather than offering it again.
    #[test]
    fn an_answered_round_comes_back_answered() {
        let s = Scratch::new("bs-round");
        person(&s, "r", 0, 10, "pick one");
        asked(
            s.path(),
            "r",
            0,
            11,
            "toolu_01J1",
            &json!([{ "q": "Drink?", "options": ["Coffee", "Tea"] }]),
        )
        .unwrap();
        answered(
            s.path(),
            "r",
            0,
            12,
            "toolu_01J1",
            &json!({ "Drink?": "Coffee" }),
            "file",
        )
        .unwrap();

        let recs = records(s.path(), "r");
        let answered_ids: Vec<&str> = recs
            .iter()
            .filter_map(|r| match r {
                Rec::Answered { tool_use_id, .. } => Some(tool_use_id.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(answered_ids, vec!["toolu_01J1"]);
        assert!(
            recs.iter()
                .any(|r| matches!(r, Rec::Asked { tool_use_id, .. }
                                         if tool_use_id == "toolu_01J1")),
            "and the question it answers is there too"
        );
    }

    /// An image is a reference to a shared pool, and the reference surviving
    /// says nothing about the bytes being there.
    #[test]
    fn an_image_reference_round_trips_and_its_bytes_are_a_separate_fact() {
        let s = Scratch::new("bs-image");
        let sha = "9".repeat(64);
        append(
            s.path(),
            "r",
            &Rec::Ask {
                n: 0,
                at_ms: 10,
                origin: Origin::Hook,
                kind: Kind::Person,
                text: "look at this".into(),
                images: vec![Image {
                    sha256: sha.clone(),
                    mime: "image/png".into(),
                    bytes: Some(183220),
                }],
            },
        )
        .unwrap();

        match records(s.path(), "r").first() {
            Some(Rec::Ask { images, .. }) => {
                assert_eq!(images.len(), 1);
                assert_eq!(images[0].sha256, sha);
                assert_eq!(images[0].bytes, Some(183220));
            }
            other => panic!("expected an ask, got {other:?}"),
        }
        let p = asset_path(s.path(), &sha, "png").unwrap();
        assert!(p.starts_with(assets_dir(s.path())), "one pool, shared");
        assert!(
            !p.exists(),
            "nothing writes the pool yet, and the reader must say unavailable rather than lose the reference"
        );
    }

    /// A hash that could name a path is dropped rather than repaired, and a
    /// missing size is `None` rather than zero.
    #[test]
    fn an_unusable_image_reference_is_refused_and_a_missing_size_is_not_zero() {
        for bad in ["../../etc/passwd", "", "ABCDEF", &"g".repeat(64), "9"] {
            assert!(!safe_hash(bad), "{bad:?} must not be a content address");
            assert!(asset_path(Path::new("/tmp"), bad, "png").is_none());
        }
        let sha = "a".repeat(64);
        assert!(safe_hash(&sha));
        assert!(asset_path(Path::new("/tmp"), &sha, "../x").is_none());

        let im = Image::from_json(&json!({ "sha256": sha, "mime": "image/png" })).unwrap();
        assert_eq!(
            im.bytes, None,
            "a size nobody stated is unknown, not an empty file"
        );
        assert!(Image::from_json(&json!({ "sha256": "../x", "mime": "image/png" })).is_none());
    }

    /// A home with a ledger in it, for the key resolver's three answers.
    struct Led(PathBuf);

    impl Led {
        fn new(name: &str) -> Led {
            let root = std::env::temp_dir().join(format!("td-bskey-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join(".local/state/terminal-delight/agent-ledger")).unwrap();
            Led(root)
        }
        /// A machine where the hook has never been installed.
        fn bare(name: &str) -> Led {
            let root =
                std::env::temp_dir().join(format!("td-bsbare-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Led(root)
        }
        fn lineage(&self, lines: &[&str]) {
            fs::write(
                self.0
                    .join(".local/state/terminal-delight/agent-ledger/lineage.jsonl"),
                lines.join("\n"),
            )
            .unwrap();
        }
    }

    impl Drop for Led {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const SID_A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
    const SID_B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";

    /// A conversation that compacted keys on the id it STARTED as, which is
    /// the whole reason the ledger is consulted at all.
    #[test]
    fn a_compacted_conversation_keys_on_the_id_it_started_as() {
        let led = Led::new("chained");
        led.lineage(&[
            &format!(
                r#"{{"event":"start","session_id":"{SID_A}","pid":7,"ts":1,"root":"{SID_A}","seq":0}}"#
            ),
            &format!(
                r#"{{"event":"start","session_id":"{SID_B}","pid":7,"ts":2,"root":"{SID_A}","seq":1,"join":"declared"}}"#
            ),
        ]);

        let k = key_for_session(SID_B, &led.0).unwrap();
        assert_eq!(k.root, SID_A, "the segment files under the conversation");
        assert_eq!(k.seq, 1);
        // And the id it started as still resolves to itself.
        assert_eq!(key_for_session(SID_A, &led.0).unwrap().root, SID_A);
    }

    /// An id the ledger does not name is its own root, in the ledger's own
    /// words. A machine with no ledger at all gets the same key by a different
    /// road, and the difference is recorded where it is knowable — in the
    /// ledger — rather than invented here.
    #[test]
    fn an_unchained_id_and_an_unrecorded_machine_both_key_on_the_id() {
        let led = Led::new("unchained");
        led.lineage(&[&format!(
            r#"{{"event":"start","session_id":"{SID_A}","pid":7,"ts":1,"root":"{SID_A}","seq":0}}"#
        )]);
        let k = key_for_session(SID_B, &led.0).unwrap();
        assert_eq!((k.root.as_str(), k.seq), (SID_B, 0));

        let bare = Led::bare("unrecorded");
        let k = key_for_session(SID_B, &bare.0).unwrap();
        assert_eq!(
            (k.root.as_str(), k.seq),
            (SID_B, 0),
            "no instrument is not no conversation"
        );
    }

    /// A session id that could name a path yields no key at all, so nothing is
    /// filed for it — the refusal, not a repair.
    #[test]
    fn a_session_id_that_could_name_a_path_yields_no_key() {
        let led = Led::bare("unsafe");
        for bad in ["../../etc/passwd", "a/b", "", "a b"] {
            assert!(key_for_session(bad, &led.0).is_none(), "{bad:?}");
        }
    }

    /// Every record shape survives a write and a read, including the ones with
    /// no writer on the bench yet.
    #[test]
    fn every_record_shape_round_trips() {
        let all = [
            Rec::Segment {
                seq: 2,
                at_ms: 1,
                source: "resume".into(),
                agent: "codex".into(),
                pid: 99,
            },
            Rec::Ask {
                n: 0,
                at_ms: 2,
                origin: Origin::Screen,
                kind: Kind::System,
                text: "two\nlines".into(),
                images: vec![],
            },
            Rec::Said {
                n: 0,
                at_ms: 3,
                id: "a".into(),
                bond: Bond::Sole,
                surface: doc("x"),
            },
            Rec::Asked {
                n: 0,
                at_ms: 4,
                tool_use_id: "t".into(),
                questions: json!([1, 2]),
            },
            Rec::Answered {
                n: 0,
                at_ms: 5,
                tool_use_id: "t".into(),
                answers: json!({ "a": "b" }),
                road: "keys".into(),
            },
        ];
        for r in &all {
            let j = r.to_json();
            assert_eq!(j["ws"].as_str(), Some(WS), "every line carries a version");
            assert_eq!(Rec::from_json(&j).as_ref(), Some(r), "did not survive: {j}");
            assert!(
                !j.to_string().contains('\n'),
                "a record with a newline in it would split its own line"
            );
        }
    }
}
