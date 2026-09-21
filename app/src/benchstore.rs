//! A conversation's record — the surfaces it presented and the turns it took.
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
//! directory name, and a guard at one end only is a guard the third reader
//! assumes the first one applied.
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
//! **A `/clear` mints a new root.** So two segments under one root are always
//! the same conversation carried on through a compaction or an in-pane resume,
//! and [`load`] reads the WHOLE root. The boundary a person means by "start
//! again" is the root directory, and cleared work sits in a different one that is
//! never opened.
//!
//! Getting that backwards is not a cosmetic error: a bench that loaded only its
//! current segment would go blank in front of somebody mid-conversation, the
//! moment their agent compacted, with the load succeeding and simply the wrong
//! directory read.

use crate::surface::PANE_HISTORY_CAP;
use crate::vitals::Bond;
use serde_json::{json, Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Which conversation a bench is showing, and which segment of it.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct ConvKey {
    /// The conversation. A session id the ledger minted at a `startup` or a
    /// `/clear`.
    pub root: String,
    /// How far along. Incremented by a compaction or an in-pane resume, never
    /// by a clear.
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
/// `SurfaceId::sanitise` deliberately keeps `.` and `:` too. Applying the
/// root's alphabet to an id refuses `plan.v2` and `decision:1`, which are legal
/// surfaces an agent can present today.
///
/// **That refusal would not merely lose the surface, it would loop.** [`file`]
/// is the thing the caller drains the mailbox AFTER, so a surface rejected here
/// is never recorded and never removed — re-swept and re-delivered on every
/// pass, forever.
///
/// What has to be refused is a name that can leave its directory or hide from
/// the sweep: `..` anywhere, a leading `.`, and anything outside the
/// sanitiser's alphabet.
pub fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && !s.starts_with('.')
        && !s.contains("..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

impl ConvKey {
    /// `<root_dir>/<root>/<seq>/`, or [`None`] when the root could name
    /// something other than a directory under `root_dir`.
    ///
    /// Returns an `Option` rather than sanitising, because a root that fails
    /// this test is not a root with a typo in it — it is a value that reached
    /// here from somewhere it should not have, and quietly repairing it files a
    /// conversation under a name nobody chose.
    pub fn dir(&self, root_dir: &Path) -> Option<PathBuf> {
        Some(conversation_dir(root_dir, &self.root)?.join(self.seq.to_string()))
    }
}

/// `<root_dir>/<root>/`, validated. Every segment of one conversation.
pub fn conversation_dir(root_dir: &Path, root: &str) -> Option<PathBuf> {
    safe_segment(root).then(|| root_dir.join(root))
}

/// One event in a conversation's record, oldest first in `turns.jsonl`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Turn {
    /// What the person said.
    ///
    /// Two sources write this and the store does not know which: the composer
    /// sending a draft through the decoupled API (exact, because this window
    /// authored it), and the pane's scrollback latch (the terminal face's
    /// fallback). Taking lines and an ordinal from either is what lets the
    /// store ship before that boundary exists and improve when it arrives,
    /// with no migration.
    Ask {
        n: u32,
        at_ms: u64,
        lines: Vec<String>,
    },
    /// A surface that answered turn `n`.
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
    },
}

impl Turn {
    /// The turn this event belongs to.
    pub fn n(&self) -> u32 {
        match self {
            Turn::Ask { n, .. } | Turn::Said { n, .. } => *n,
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Turn::Ask { n, at_ms, lines } => {
                json!({ "t": "ask", "n": n, "at_ms": at_ms, "lines": lines })
            }
            Turn::Said { n, at_ms, id, bond } => json!({
                "t": "said", "n": n, "at_ms": at_ms,
                "id": id, "bond": bond.as_str(),
            }),
        }
    }

    fn from_json(v: &Value) -> Option<Turn> {
        let n = v.get("n")?.as_u64()? as u32;
        let at_ms = v.get("at_ms")?.as_u64()?;
        match v.get("t")?.as_str()? {
            "ask" => Some(Turn::Ask {
                n,
                at_ms,
                lines: v
                    .get("lines")?
                    .as_array()?
                    .iter()
                    .filter_map(|l| l.as_str().map(str::to_string))
                    .collect(),
            }),
            // A bond word this build does not know reads as `Guess`, the
            // weakest rung — an unrecognised claim of strength is not a strong
            // claim, and the whole point of recording the bond is that a reader
            // can refuse the weak ones.
            "said" => Some(Turn::Said {
                n,
                at_ms,
                id: v.get("id")?.as_str()?.to_string(),
                bond: match v.get("bond").and_then(Value::as_str) {
                    Some("declared") => Bond::Declared,
                    Some("birth") => Bond::Birth,
                    Some("sole") => Bond::Sole,
                    _ => Bond::Guess,
                },
            }),
            _ => None,
        }
    }
}

/// Where conversations live: `$XDG_STATE_HOME/terminal-delight/conversations`.
///
/// Beside `surfaces/`, not inside it, and named by nothing that belongs to a
/// window — a conversation outlives the window that hosted it.
/// Resolved the same way as [`crate::surfacefeed::surfaces_root`], and that is
/// not a stylistic choice: the two are a pair. A relative `XDG_STATE_HOME` —
/// or an unset `HOME`, which `unwrap_or_default` turns into an empty string —
/// would put the store under whatever directory the window happened to be
/// launched from while the mailbox it is paired with sits under `$HOME`, and
/// `terminal-delight conversation <root>` run from elsewhere would then read a
/// different store and truthfully report "no record".
pub fn store_root() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::session::home_dir().join(".local/state"));
    base.join("terminal-delight/conversations")
}

/// A monotonic counter for temp file names.
///
/// **Not a timestamp.** `drop_surface` learned this the expensive way: threads
/// starting together read the clock too close to be separated by it, and three
/// writes in sixteen were still lost with nanosecond names. A counter is unique
/// by construction rather than by luck.
#[allow(dead_code)] // see the note on `ask`
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)] // see the note on `ask`: write path, wiring slice
fn write_atomic(dir: &Path, name: &str, body: &[u8]) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let temp = dir.join(format!(
        ".{name}.{}.{}.part",
        std::process::id(),
        TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temp, body)?;
    match fs::rename(&temp, dir.join(name)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&temp);
            Err(e)
        }
    }
}

#[allow(dead_code)] // see the note on `ask`
fn append_turn(root_dir: &Path, key: &ConvKey, turn: &Turn) -> io::Result<()> {
    let dir = key
        .dir(root_dir)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unsafe conversation root"))?;
    fs::create_dir_all(&dir)?;
    let line = format!("{}\n", turn.to_json());
    use std::io::Write;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("turns.jsonl"))?;
    f.write_all(line.as_bytes())
}

/// Open a turn: record what the person said.
///
/// `allow(dead_code)`, narrowly and temporarily: the WRITE path's only caller is
/// the sweep that files an arrival, which is the wiring slice and is not on this
/// branch. The read path below has a real caller ([`run_cli`]) and carries no
/// allowance. **Delete this attribute in the slice that calls it** — if it is
/// still here once `present` files a surface, it is hiding something rather than
/// waiting for something.
#[allow(dead_code)]
pub fn ask(root_dir: &Path, key: &ConvKey, n: u32, at_ms: u64, lines: &[String]) -> io::Result<()> {
    append_turn(
        root_dir,
        key,
        &Turn::Ask {
            n,
            at_ms,
            lines: lines.to_vec(),
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
#[allow(dead_code)] // see the note on `ask`
pub fn file(
    root_dir: &Path,
    key: &ConvKey,
    turn: u32,
    id: &str,
    value: &Value,
    bond: Bond,
    at_ms: u64,
) -> io::Result<()> {
    if !safe_id(id) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "unsafe id"));
    }
    let dir = key
        .dir(root_dir)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "unsafe conversation root"))?;
    // The id inside the document and the name on disk are the same thing, and
    // the filing id is the one that wins — it is the name the record is keyed
    // by and the name the turn journal points at. A document carrying a
    // DIFFERENT id is the case that matters: left alone it is written to
    // `named.json` still calling itself something else, so the stem and the
    // parsed id disagree and a retire aimed at one of them misses.
    //
    // A payload that is not an object carries no id and cannot be given one.
    // It is stored as it came rather than refused — the parser downstream is
    // lenient by design and turns an unusable document into a visible
    // `Unclassified` card, which is a better answer than a surface that
    // silently never arrives.
    let body = match value.as_object() {
        Some(m) => {
            let mut owned: Map<String, Value> = m.clone();
            owned.insert("id".into(), json!(id));
            Value::Object(owned)
        }
        None => value.clone(),
    };
    write_atomic(
        &dir.join("surfaces"),
        &format!("{id}.json"),
        body.to_string().as_bytes(),
    )?;
    append_turn(
        root_dir,
        key,
        &Turn::Said {
            n: turn,
            at_ms,
            id: id.to_string(),
            bond,
        },
    )
}

/// Every segment of one conversation, in order, numerically.
///
/// **Numerically, not by name.** `10` sorts before `2` lexically, and a
/// nine-segment conversation is an ordinary week for an agent that compacts.
fn segments(root_dir: &Path, root: &str) -> Vec<(u32, PathBuf)> {
    let Some(dir) = conversation_dir(root_dir, root) else {
        return Vec::new();
    };
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<(u32, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let n: u32 = e.file_name().to_str()?.parse().ok()?;
            e.path().is_dir().then(|| (n, e.path()))
        })
        .collect();
    out.sort_by_key(|(n, _)| *n);
    out
}

/// The whole conversation's surfaces, oldest first, capped at
/// [`PANE_HISTORY_CAP`] **across the root** rather than per segment.
///
/// Returns the raw documents rather than parsed surfaces, because that is what
/// the mailbox holds and what the bench's own parser already takes — a
/// round trip through a typed struct would be a second format to keep in step.
pub fn load(root_dir: &Path, root: &str) -> Loaded {
    // When each surface arrived, from the turn journal. This is the ordering
    // fact, and it is the only one that survives a caller choosing its own ids.
    let mut when: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for t in asks(root_dir, root) {
        if let Turn::Said { id, at_ms, .. } = t {
            // A surface re-presented under one id keeps the LATEST stamp: it is
            // one row that was updated, not two rows.
            let slot = when.entry(id).or_insert(at_ms);
            *slot = (*slot).max(at_ms);
        }
    }

    let mut unreadable = 0usize;
    // Keyed by id so a surface updated after a compaction — written into a new
    // segment rather than over the old file — is ONE row and spends ONE slot of
    // the cap, rather than coming back twice under the same name.
    let mut newest: std::collections::HashMap<String, (Option<u64>, Value)> =
        std::collections::HashMap::new();

    for (_, seg) in segments(root_dir, root) {
        let Ok(rd) = fs::read_dir(seg.join("surfaces")) else {
            continue;
        };
        for e in rd.flatten() {
            let path = e.path();
            if !path.extension().is_some_and(|x| x == "json") {
                continue;
            }
            let Some(stem) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
            else {
                unreadable += 1;
                continue;
            };
            let parsed = fs::read_to_string(&path)
                .ok()
                .and_then(|b| serde_json::from_str::<Value>(&b).ok());
            let Some(v) = parsed else {
                // Counted rather than dropped in silence. `write_atomic` does
                // no fsync before its rename, so a truncated file is a
                // reachable post-crash state — and a bench that comes back one
                // card short with nothing saying so is indistinguishable from
                // a conversation that presented one card fewer.
                unreadable += 1;
                continue;
            };
            // Record first, file mtime second. Both are real measurements; the
            // record is preferred because the mtime moves if anything ever
            // rewrites the file.
            let at = when.get(&stem).copied().or_else(|| mtime_ms(&path));
            // Later segment wins: it is the current copy of that id.
            newest.insert(stem, (at, v));
        }
    }

    let mut out: Vec<(String, Option<u64>, Value)> =
        newest.into_iter().map(|(k, (at, v))| (k, at, v)).collect();
    // Oldest first, by WHEN — never by id. Ids are not monotonic: `derive.rs`
    // mints `ask-<hash>`, `surface.rs` mints `anon-<hash>`, and an agent may
    // supply any slug it likes, so sorting by the filename stem is sorting by
    // nothing. A surface whose arrival cannot be established sorts LAST, so the
    // cap below drops something whose age is known rather than something whose
    // age is not.
    out.sort_by(|a, b| {
        (a.1.is_none(), a.1, &a.0)
            .partial_cmp(&(b.1.is_none(), b.1, &b.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if out.len() > PANE_HISTORY_CAP {
        out.drain(..out.len() - PANE_HISTORY_CAP);
    }
    Loaded {
        surfaces: out.into_iter().map(|(k, _, v)| (k, v)).collect(),
        unreadable,
    }
}

/// What a conversation's surfaces came back as, including what did not.
#[derive(Debug, Default)]
pub struct Loaded {
    /// Oldest first, deduplicated by id, capped at [`PANE_HISTORY_CAP`].
    pub surfaces: Vec<(String, Value)>,
    /// Files that are there and could not be read or parsed. **Not folded into
    /// the count above**: "this conversation presented four things" and "it
    /// presented five and one of them is corrupt" are different answers, and
    /// only one of them tells a reader to go and look.
    pub unreadable: usize,
}

fn mtime_ms(path: &Path) -> Option<u64> {
    let t = fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64)
}

/// The turn record for a whole conversation, oldest first.
///
/// This is what lets a card that is NOT the newest one be captioned with the
/// ask that prompted it — the thing the bench cannot do today, because the pane
/// latches exactly one human message off its scrollback.
pub fn asks(root_dir: &Path, root: &str) -> Vec<Turn> {
    let mut out = Vec::new();
    for (_, seg) in segments(root_dir, root) {
        let Ok(body) = fs::read_to_string(seg.join("turns.jsonl")) else {
            continue;
        };
        for line in body.lines() {
            // A line this build cannot parse is skipped rather than ending the
            // read: the file is append-only and a torn last line is the normal
            // shape of a crash, not a reason to lose the turns before it.
            if let Some(t) = serde_json::from_str(line)
                .ok()
                .as_ref()
                .and_then(Turn::from_json)
            {
                out.push(t);
            }
        }
    }
    out
}

/// The ordinal the next ask should carry.
///
/// Counted from the record rather than held in memory, so a window restart
/// mid-conversation does not restart the numbering and overwrite turn 1.
pub fn next_turn(root_dir: &Path, root: &str) -> u32 {
    asks(root_dir, root)
        .iter()
        .map(Turn::n)
        .max()
        .map_or(0, |n| n + 1)
}

/// `terminal-delight conversation <root>` — read a conversation's bench back
/// without a window.
///
/// The headless instrument for this store, and the same argument `bindings` is
/// for the resolver: a record nobody can inspect is a record nobody can debug,
/// and this one decides what a person sees on a bench. It is also how the
/// wiring slice will be checked — file a surface, run this, see it.
pub fn run_cli(args: &[String]) -> i32 {
    // The first NON-FLAG argument. `--json` passes `safe_segment` — hyphens are
    // legal in a session id — so reading `args.first()` blind makes
    // `conversation --json` report on a conversation called `--json`, print
    // "no record" and exit 0. A reader chasing a missing record would get a
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
    } = load(&dir, root);
    let turns = asks(&dir, root);
    if args.iter().any(|a| a == "--json") {
        println!(
            "{}",
            json!({
                "root": root,
                "dir": conversation_dir(&dir, root).map(|p| p.to_string_lossy().into_owned()),
                "next_turn": next_turn(&dir, root),
                "unreadable": unreadable,
                "surfaces": surfaces.iter().map(|(id, v)| json!({
                    "id": id, "title": v.get("title"), "kind": v.get("kind"),
                })).collect::<Vec<_>>(),
                "turns": turns.iter().map(Turn::to_json).collect::<Vec<_>>(),
            })
        );
        return 0;
    }
    match conversation_dir(&dir, root) {
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
    for t in &turns {
        match t {
            Turn::Ask { n, lines, .. } => println!("  {n:>3}  you    {}", lines.join(" ")),
            Turn::Said { n, id, bond, .. } => {
                println!("  {n:>3}  agent  {id}  [{}]", bond.as_str())
            }
        }
    }
    if unreadable > 0 {
        println!("{} surfaces ({unreadable} unreadable)", surfaces.len());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsync::Scratch;

    /// A key, briefly. The struct has public fields; this is only shorter.
    fn key(root: &str, seq: u32) -> ConvKey {
        ConvKey {
            root: root.into(),
            seq,
        }
    }

    fn doc(title: &str) -> Value {
        json!({ "td": "0.4", "kind": "markdown", "title": title,
                "model": { "body": title } })
    }

    fn titles(v: &[(String, Value)]) -> Vec<String> {
        v.iter()
            .map(|(_, d)| d["title"].as_str().unwrap_or("?").to_string())
            .collect()
    }

    /// The case an earlier version of this design broke.
    ///
    /// A compaction CONTINUES a conversation, so its segments are siblings and
    /// the load takes all of them. A design that isolated segments would empty
    /// a bench in front of somebody mid-conversation, with every step
    /// succeeding.
    #[test]
    fn a_compaction_keeps_the_whole_conversation() {
        let s = Scratch::new("bs-compact");
        let a0 = key("rootA", 0);
        let a1 = key("rootA", 1);
        file(
            s.path(),
            &a0,
            0,
            "one",
            &doc("Before the compaction"),
            Bond::Declared,
            1,
        )
        .unwrap();
        file(
            s.path(),
            &a1,
            1,
            "two",
            &doc("After the compaction"),
            Bond::Declared,
            2,
        )
        .unwrap();

        let got = load(s.path(), "rootA").surfaces;

        assert_eq!(got.len(), 2, "both segments load");
        assert_eq!(
            titles(&got),
            vec!["Before the compaction", "After the compaction"],
            "oldest first, across the whole root"
        );
    }

    /// And the case it was trying to protect, which sits one level up.
    #[test]
    fn a_clear_starts_an_empty_bench() {
        let s = Scratch::new("bs-clear");
        file(
            s.path(),
            &key("rootA", 0),
            0,
            "old",
            &doc("Before the clear"),
            Bond::Declared,
            1,
        )
        .unwrap();

        // A clear mints a NEW ROOT, so the cleared work is in a directory this
        // load never opens.
        assert!(load(s.path(), "rootB").surfaces.is_empty());
        assert_eq!(
            load(s.path(), "rootA").surfaces.len(),
            1,
            "and is not destroyed"
        );
    }

    #[test]
    fn segments_load_in_numeric_not_lexical_order() {
        let s = Scratch::new("bs-order");
        for n in 0..12u32 {
            file(
                s.path(),
                &key("r", n),
                n,
                &format!(
                    "seg-{n}-{}",
                    "abcdefghijkl".chars().nth(n as usize).unwrap()
                ),
                &doc(&format!("segment {n}")),
                Bond::Declared,
                n as u64,
            )
            .unwrap();
        }
        let got = titles(&load(s.path(), "r").surfaces);
        assert_eq!(got.first().unwrap(), "segment 0");
        assert_eq!(
            got.last().unwrap(),
            "segment 11",
            "11 must come after 9, which lexical order gets wrong"
        );
        assert_eq!(got[9], "segment 9", "and 10 must not jump ahead of 2");
    }

    /// The store is addressed by conversation, not by window session, so the
    /// same root read from anywhere finds the same record.
    #[test]
    fn a_conversation_outlives_the_window_that_made_it() {
        let s = Scratch::new("bs-outlive");
        file(
            s.path(),
            &key("r", 0),
            0,
            "a",
            &doc("Made in one window"),
            Bond::Declared,
            1,
        )
        .unwrap();
        // Nothing about the reader names a window, a session or a pane.
        assert_eq!(
            titles(&load(s.path(), "r").surfaces),
            vec!["Made in one window"]
        );
    }

    /// A root that could name a path yields no directory, rather than a
    /// sanitised one — a repaired root files a conversation under a name
    /// nobody chose.
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
        ] {
            assert!(!safe_segment(bad), "{bad:?} must not be a segment");
            assert!(
                key(bad, 0).dir(s.path()).is_none(),
                "{bad:?} must not resolve to a directory"
            );
            assert!(
                file(s.path(), &key(bad, 0), 0, "x", &doc("x"), Bond::Declared, 1).is_err(),
                "{bad:?} must not be filed under"
            );
        }
        assert!(safe_segment("470a6cfd-35e7-41cb-8a8c-b3616d932a5f"));
    }

    /// An id is a filename too, and it arrives from the same untrusted place.
    #[test]
    fn a_surface_id_that_could_name_a_path_is_refused() {
        let s = Scratch::new("bs-id");
        assert!(file(
            s.path(),
            &key("r", 0),
            0,
            "../escape",
            &doc("x"),
            Bond::Declared,
            1
        )
        .is_err());
    }

    /// The turn record is what lets an OLDER card be captioned, which is the
    /// thing the bench cannot do today.
    #[test]
    fn every_surface_records_the_turn_it_answered() {
        let s = Scratch::new("bs-turns");
        let k = key("r", 0);
        ask(s.path(), &k, 0, 10, &["what is the key".into()]).unwrap();
        file(
            s.path(),
            &k,
            0,
            "a",
            &doc("first answer"),
            Bond::Declared,
            11,
        )
        .unwrap();
        ask(s.path(), &k, 1, 20, &["and what about clear".into()]).unwrap();
        file(s.path(), &k, 1, "b", &doc("second answer"), Bond::Birth, 21).unwrap();

        let t = asks(s.path(), "r");
        assert_eq!(t.len(), 4, "two asks and two answers");
        assert_eq!(
            t[0],
            Turn::Ask {
                n: 0,
                at_ms: 10,
                lines: vec!["what is the key".into()]
            }
        );
        assert_eq!(t[3].n(), 1, "the second answer belongs to the second turn");
        assert!(
            matches!(
                &t[3],
                Turn::Said {
                    bond: Bond::Birth,
                    ..
                }
            ),
            "and carries how strongly the pane was bound when it was filed"
        );
    }

    /// A window restart mid-conversation must not restart the numbering, or
    /// turn 1 is written over turn 0's ask.
    #[test]
    fn the_next_turn_is_counted_from_the_record_not_from_memory() {
        let s = Scratch::new("bs-next");
        let k = key("r", 0);
        assert_eq!(next_turn(s.path(), "r"), 0, "nothing said yet");
        ask(s.path(), &k, 0, 10, &["one".into()]).unwrap();
        assert_eq!(next_turn(s.path(), "r"), 1);
        // A compaction moves to a new segment and the count carries across it.
        let k1 = key("r", 1);
        ask(s.path(), &k1, 1, 20, &["two".into()]).unwrap();
        assert_eq!(next_turn(s.path(), "r"), 2, "counted across the whole root");
    }

    /// An append-only file's normal crash shape is a torn last line.
    #[test]
    fn a_torn_last_line_does_not_lose_the_turns_before_it() {
        let s = Scratch::new("bs-torn");
        let k = key("r", 0);
        ask(s.path(), &k, 0, 10, &["kept".into()]).unwrap();
        let p = k.dir(s.path()).unwrap().join("turns.jsonl");
        let mut body = fs::read_to_string(&p).unwrap();
        body.push_str("{\"t\":\"ask\",\"n\":1,\"at_m");
        fs::write(&p, body).unwrap();

        let t = asks(s.path(), "r");
        assert_eq!(t.len(), 1, "the whole line before the tear survives");
        assert_eq!(t[0].n(), 0);
    }

    /// A bond word from a future build is read as the WEAKEST rung, never the
    /// strongest — an unrecognised claim of strength is not a strong claim.
    #[test]
    fn an_unknown_bond_word_reads_as_the_weakest_rung() {
        let s = Scratch::new("bs-bond");
        let k = key("r", 0);
        file(s.path(), &k, 0, "a", &doc("x"), Bond::Declared, 1).unwrap();
        let p = k.dir(s.path()).unwrap().join("turns.jsonl");
        let body = fs::read_to_string(&p)
            .unwrap()
            .replace("declared", "cryptographic");
        fs::write(&p, body).unwrap();

        assert!(matches!(
            asks(s.path(), "r").first(),
            Some(Turn::Said {
                bond: Bond::Guess,
                ..
            })
        ));
    }

    /// The cap is across the root, so a long conversation shows its newest
    /// surfaces rather than the newest of every segment.
    #[test]
    fn the_history_cap_applies_across_the_whole_root() {
        let s = Scratch::new("bs-cap");
        let total = PANE_HISTORY_CAP + 10;
        for i in 0..total {
            file(
                s.path(),
                &key("r", (i / 8) as u32),
                i as u32,
                // An id that sorts BACKWARDS against time. Sequential
                // zero-padded ids were the original fixture, and they are the
                // one shape that makes an id-ordered load look correct — so
                // this test could not fail against the bug it is named for.
                &format!("s{:04}", total - 1 - i),
                &doc(&format!("surface {i:04}")),
                Bond::Declared,
                i as u64,
            )
            .unwrap();
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
    /// likes — so a load sorted by the filename stem is sorted by nothing, and
    /// the bench draws its cards scrambled.
    #[test]
    fn ordering_follows_the_record_not_the_id() {
        let s = Scratch::new("bs-order-by-time");
        let k = key("r", 0);
        // Filed oldest first, named so that sorting by id reverses them.
        file(
            s.path(),
            &k,
            0,
            "zzz-first",
            &doc("first"),
            Bond::Declared,
            100,
        )
        .unwrap();
        file(
            s.path(),
            &k,
            0,
            "mmm-second",
            &doc("second"),
            Bond::Declared,
            200,
        )
        .unwrap();
        file(
            s.path(),
            &k,
            0,
            "aaa-third",
            &doc("third"),
            Bond::Declared,
            300,
        )
        .unwrap();

        assert_eq!(
            titles(&load(s.path(), "r").surfaces),
            vec!["first", "second", "third"],
            "by when they arrived, not by what they are called"
        );
    }

    /// A surface re-presented after a compaction is ONE row.
    ///
    /// `file` writes into the current segment, so an update lands beside the
    /// old copy rather than over it. Returning both shows a stale document and
    /// the current one under one name, and spends two slots of the cap on a
    /// conversation that presented one thing.
    #[test]
    fn a_surface_updated_after_a_compaction_is_one_row_not_two() {
        let s = Scratch::new("bs-dupe");
        file(
            s.path(),
            &key("r", 0),
            0,
            "same",
            &doc("Before"),
            Bond::Declared,
            10,
        )
        .unwrap();
        file(
            s.path(),
            &key("r", 1),
            1,
            "same",
            &doc("After"),
            Bond::Declared,
            20,
        )
        .unwrap();

        let got = load(s.path(), "r").surfaces;
        assert_eq!(got.len(), 1, "one id, one row");
        assert_eq!(
            got[0].1["title"].as_str(),
            Some("After"),
            "and it is the current copy, not the one from the earlier segment"
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
            file(s.path(), &key("r", 0), 0, id, &doc(id), Bond::Declared, 1)
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
        file(s.path(), &key("r", 0), 0, "named", &d, Bond::Declared, 1).unwrap();

        let (stem, v) = load(s.path(), "r").surfaces.pop().unwrap();
        assert_eq!(stem, "named");
        assert_eq!(
            v["id"].as_str(),
            Some("named"),
            "the stem and the id inside must agree, or a retire aimed at one misses"
        );
    }

    /// A file that is there and cannot be read is counted, not dropped.
    #[test]
    fn an_unreadable_surface_is_counted_rather_than_dropped_in_silence() {
        let s = Scratch::new("bs-corrupt");
        let k = key("r", 0);
        file(s.path(), &k, 0, "good", &doc("readable"), Bond::Declared, 1).unwrap();
        fs::write(
            k.dir(s.path()).unwrap().join("surfaces/torn.json"),
            "{\"td\":\"0.4\",\"kin",
        )
        .unwrap();

        let got = load(s.path(), "r");
        assert_eq!(got.surfaces.len(), 1, "the readable one still loads");
        assert_eq!(
            got.unreadable, 1,
            "and the torn one is reported, not hidden"
        );
    }

    /// A flag is not a conversation root.
    ///
    /// `--json` passes `safe_segment` — hyphens are legal in a session id — so
    /// reading the first argument blind reports on a conversation called
    /// `--json`, prints "no record" and exits 0. Someone chasing a missing
    /// record would get a confident wrong answer with a successful exit.
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

        file(
            s.path(),
            &key("r", 0),
            0,
            "a",
            &doc("filed"),
            Bond::Declared,
            1,
        )
        .unwrap();

        assert!(
            inbox.join("a.json").exists(),
            "the store has no business removing the mailbox copy"
        );
    }

    /// A document that names no id gets the one it was filed under, so it
    /// comes back answering to the same thing.
    #[test]
    fn a_document_with_no_id_is_filed_carrying_the_one_it_was_given() {
        let s = Scratch::new("bs-id-fill");
        file(
            s.path(),
            &key("r", 0),
            0,
            "named",
            &doc("x"),
            Bond::Declared,
            1,
        )
        .unwrap();
        let (stem, v) = load(s.path(), "r").surfaces.pop().unwrap();
        assert_eq!(stem, "named");
        assert_eq!(
            v["id"].as_str(),
            Some("named"),
            "the file name is inside it too"
        );
    }

    /// Re-filing the same id replaces it rather than stacking, which is the
    /// normal path: `present_surface` re-sends an id to update a row.
    #[test]
    fn filing_one_id_twice_leaves_one_surface() {
        let s = Scratch::new("bs-twice");
        let k = key("r", 0);
        file(s.path(), &k, 0, "same", &doc("First"), Bond::Declared, 1).unwrap();
        file(s.path(), &k, 0, "same", &doc("Second"), Bond::Declared, 2).unwrap();
        let got = load(s.path(), "r").surfaces;
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].1["title"].as_str(), Some("Second"));
    }
}
