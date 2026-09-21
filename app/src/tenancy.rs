//! Which conversation a session id belongs to, from the agent-session ledger.
//!
//! [`crate::paneident`] answers *which conversation is this pane in* and hands
//! back a session id. This module answers the question after it: **is that id a
//! conversation, or a segment of one.** A workbench keyed by the conversation
//! needs the second answer, because an id is not fixed for the life of a
//! conversation — `SessionStart` fires again on `/clear`, on an in-pane
//! `/resume`, and on compaction, and the harness is free to mint a new id at any
//! of them.
//!
//! # What is on disk
//!
//! `scripts/td-agent-ledger` is a `SessionStart`/`SessionEnd` hook, and it now
//! records *why* each mint happened alongside the id:
//!
//! ```text
//!   agent-ledger/<pid>.json   the live entry for one agent process
//!   agent-ledger/lineage.jsonl  append-only; every mint and every end
//! ```
//!
//! Each carries `root` (the id the conversation started as), `seq` (how many
//! re-mints deep), the `prev` id this one displaced, and `join` — `declared`
//! when the payload's `source` said which kind of mint it was, `unknown` when
//! nothing did. The chain is computed by the hook at write time, so nothing here
//! walks a graph: the line naming an id already names its root.
//!
//! The per-pid file is deleted at `SessionEnd`, which is why the lineage exists
//! at all. Recognising a conversation *tomorrow* — the whole point of keying a
//! bench by it — has to survive the process going away.
//!
//! # Three answers, not two
//!
//! [`Tenancy`] separates *the ledger says this id is its own root* from *there is
//! no ledger on this machine*. Both would be an empty `Option`, and collapsing
//! them draws a confident root for a box where nothing has been measured — the
//! hook is opt-in, installed by `scripts/install-recovery-hook.sh`, and a machine
//! without it is the ordinary case rather than the broken one.
//!
//! Nothing here reads a transcript, spawns a process, or touches a pane, a window
//! or gpui. It is a path and `std`.

use std::path::{Path, PathBuf};

/// What evidence joined a segment to the conversation before it.
///
/// Absent on a root — see [`Chain::join`] — because a join nobody needed is not
/// the same as a join nobody evidenced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Join {
    /// The `SessionStart` payload named its `source`, and it was one that
    /// continues a conversation: a compaction or an in-pane resume.
    Declared,
    /// No usable `source` came with the mint. The hook continued the chain
    /// anyway, because keeping too much is recoverable by hand and emptying a
    /// bench during a compaction is not — but nothing evidenced the join, and
    /// this is where it says so.
    Unknown,
}

/// A conversation, and how deep into it a given id sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chain {
    /// The id this conversation started as. The store key.
    pub root: String,
    /// How many re-mints deep this segment is. `0` is the root itself.
    pub seq: u32,
    /// What joined this segment to the one before it, or `None` at the root.
    pub join: Option<Join>,
}

/// What the ledger can say about a subject.
///
/// Three variants on purpose. `Unchained` is a finding about the conversation —
/// the instrument was read and does not name this id, so the id is its own root.
/// `Unrecorded` is a finding about the machine — there is no instrument, so
/// nobody has looked and no root may be inferred. A caller that treats them the
/// same will draw one where nothing was measured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tenancy {
    /// The ledger names the conversation this subject belongs to.
    Chained(Chain),
    /// The ledger was read and does not name this subject. For an id that means
    /// it is its own root; for a pid it means no live entry, and the caller
    /// should fall back to the forensic chain in [`crate::session`].
    Unchained,
    /// There is no ledger on this machine — the hook is not installed, or has
    /// never run. Nobody has looked. **Not a root.**
    Unrecorded,
}

impl Tenancy {
    /// The conversation's key, when there is one to have.
    ///
    /// `None` for both absent states rather than a fallback to the id: the
    /// caller holds the id already, and choosing to use it as a key is a
    /// decision worth making at the call site instead of here.
    pub fn root(&self) -> Option<&str> {
        match self {
            Tenancy::Chained(c) => Some(c.root.as_str()),
            _ => None,
        }
    }

    /// One word for a log line or the `bindings` verb.
    ///
    /// Three words rather than a boolean, so a reader of the JSON can tell the
    /// two absences apart without having to know this type.
    pub fn word(&self) -> &'static str {
        match self {
            Tenancy::Chained(_) => "chained",
            Tenancy::Unchained => "unchained",
            Tenancy::Unrecorded => "unrecorded",
        }
    }

    /// The chain, when there is one.
    pub fn chain(&self) -> Option<&Chain> {
        match self {
            Tenancy::Chained(c) => Some(c),
            _ => None,
        }
    }
}

impl Join {
    /// One word, matching what the hook writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Join::Declared => "declared",
            Join::Unknown => "unknown",
        }
    }
}

fn ledger_dir(home: &Path) -> PathBuf {
    home.join(".local/state/terminal-delight/agent-ledger")
}

/// An id out of the ledger reaches a store path and, through
/// [`crate::session`], a command line. Anything that is not a plain id is
/// refused here rather than filtered by whoever happens to use it.
fn plain_id(v: Option<&str>) -> Option<String> {
    let v = v?;
    (!v.is_empty()
        && v.len() <= 128
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
    .then(|| v.to_string())
}

/// Read one ledger record's chain, or nothing if it is not a usable one.
///
/// A record missing `root` falls back to its own `session_id`, which is what an
/// entry written by the pre-lineage hook looks like: that hook recorded a real
/// id and no chain, so reading it as a root of depth zero is the truth about it
/// rather than a guess.
fn chain_of_record(v: &serde_json::Value) -> Option<Chain> {
    let sid = plain_id(v.get("session_id").and_then(|x| x.as_str()))?;
    let root = plain_id(v.get("root").and_then(|x| x.as_str())).unwrap_or(sid);
    // A seq that is present and unreadable is a corrupt record, not a zero.
    let seq = match v.get("seq") {
        None => 0,
        Some(s) => u32::try_from(s.as_u64()?).ok()?,
    };
    let join = match v.get("join").and_then(|x| x.as_str()) {
        Some("declared") => Some(Join::Declared),
        Some("unknown") => Some(Join::Unknown),
        // An unrecognised word is not a join. The hook only ever writes the two
        // above, so anything else was written by something that is not the hook.
        Some(_) => None,
        None => None,
    };
    Some(Chain { root, seq, join })
}

/// The conversation a LIVE agent process is in, from its own ledger entry.
///
/// `Unchained` here means the ledger exists and has no entry for this pid —
/// the agent has ended, or is running without the hook. The caller falls back
/// to [`crate::session`]'s forensic chain exactly as it does today.
pub fn tenancy_for(pid: u32, home: &Path) -> Tenancy {
    let dir = ledger_dir(home);
    if !dir.is_dir() {
        return Tenancy::Unrecorded;
    }
    let Ok(body) = std::fs::read_to_string(dir.join(format!("{pid}.json"))) else {
        return Tenancy::Unchained;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
        return Tenancy::Unchained; // a corrupt entry is not a conversation
    };
    match chain_of_record(&v) {
        Some(chain) => Tenancy::Chained(chain),
        None => Tenancy::Unchained,
    }
}

/// The conversation a session id belongs to, after its process is gone.
///
/// Walks the durable lineage rather than the per-pid file, because that file is
/// deleted at `SessionEnd` and this is the question a bench asks when a
/// conversation is brought back tomorrow.
///
/// The LAST line naming the id wins. Each line already carries the chain the
/// hook computed at write time, so a later mint of the same id supersedes an
/// earlier one without anything here having to reason about order.
pub fn tenancy_of(session_id: &str, home: &Path) -> Tenancy {
    let dir = ledger_dir(home);
    if !dir.is_dir() {
        return Tenancy::Unrecorded;
    }
    let Ok(body) = std::fs::read_to_string(dir.join("lineage.jsonl")) else {
        // The directory exists but nothing has ever written a chain — an older
        // hook, or one that has not fired yet. The id's tenancy has not been
        // recorded, which is not the same as it having none.
        return Tenancy::Unrecorded;
    };
    let mut found = None;
    for line in body.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue; // a half-written or foreign line is skipped, never fatal
        };
        if v.get("session_id").and_then(|x| x.as_str()) != Some(session_id) {
            continue;
        }
        if let Some(chain) = chain_of_record(&v) {
            found = Some(chain);
        }
    }
    match found {
        Some(chain) => Tenancy::Chained(chain),
        None => Tenancy::Unchained,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ledger directory that cleans itself up, so a failing test cannot leave
    /// a fake ledger behind for the next one to read.
    struct Fake(PathBuf);

    impl Fake {
        fn new(name: &str) -> Fake {
            let root =
                std::env::temp_dir().join(format!("td-tenancy-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join(".local/state/terminal-delight/agent-ledger"))
                .unwrap();
            Fake(root)
        }
        /// A home with no ledger directory at all.
        fn bare(name: &str) -> Fake {
            let root =
                std::env::temp_dir().join(format!("td-tenancy-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            Fake(root)
        }
        fn home(&self) -> &Path {
            &self.0
        }
        fn dir(&self) -> PathBuf {
            ledger_dir(&self.0)
        }
        fn entry(&self, pid: u32, body: &str) {
            std::fs::write(self.dir().join(format!("{pid}.json")), body).unwrap();
        }
        fn lineage(&self, lines: &[&str]) {
            std::fs::write(self.dir().join("lineage.jsonl"), lines.join("\n")).unwrap();
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const A: &str = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
    const B: &str = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";

    // ---- the machine has no instrument ----

    #[test]
    fn no_ledger_at_all_is_unrecorded_and_not_a_root() {
        let f = Fake::bare("nohook");
        assert_eq!(tenancy_for(1234, f.home()), Tenancy::Unrecorded);
        assert_eq!(tenancy_of(A, f.home()), Tenancy::Unrecorded);
        // The distinction this type exists for: neither answer offers a key.
        assert_eq!(tenancy_of(A, f.home()).root(), None);
        assert_eq!(tenancy_of(A, f.home()), Tenancy::Unrecorded);
    }

    #[test]
    fn a_ledger_with_no_lineage_yet_is_unrecorded_rather_than_unchained() {
        // The pre-lineage hook wrote per-pid files and no chain. An id it never
        // recorded has not been found to be a root; it has not been looked for.
        let f = Fake::new("nolineage");
        f.entry(7, &format!(r#"{{"session_id":"{A}","pid":7,"ts":1}}"#));
        assert_eq!(tenancy_of(A, f.home()), Tenancy::Unrecorded);
    }

    // ---- a live process ----

    #[test]
    fn a_live_entry_naming_a_chain_reads_back_whole() {
        let f = Fake::new("live");
        f.entry(
            7,
            &format!(
                r#"{{"session_id":"{B}","pid":7,"ts":9,"root":"{A}","seq":2,"source":"compact","prev":"{A}","join":"declared"}}"#
            ),
        );
        let Tenancy::Chained(c) = tenancy_for(7, f.home()) else {
            panic!("expected a chain")
        };
        assert_eq!(c.root, A, "the bench follows the conversation, not the id");
        assert_eq!(c.seq, 2);
        assert_eq!(c.join, Some(Join::Declared));
        assert_ne!(c.seq, 0);
    }

    #[test]
    fn a_root_entry_has_no_join_rather_than_an_empty_one() {
        let f = Fake::new("root");
        f.entry(
            7,
            &format!(r#"{{"session_id":"{A}","pid":7,"ts":1,"root":"{A}","seq":0}}"#),
        );
        let Tenancy::Chained(c) = tenancy_for(7, f.home()) else {
            panic!("expected a chain")
        };
        assert_eq!(c.join, None);
        assert_eq!(c.seq, 0);
        assert_eq!(c.root, A);
    }

    #[test]
    fn an_unevidenced_join_is_readable_as_such() {
        let f = Fake::new("unknownjoin");
        f.entry(
            7,
            &format!(
                r#"{{"session_id":"{B}","pid":7,"ts":9,"root":"{A}","seq":1,"join":"unknown"}}"#
            ),
        );
        let Tenancy::Chained(c) = tenancy_for(7, f.home()) else {
            panic!("expected a chain")
        };
        assert_eq!(c.join, Some(Join::Unknown), "never silently declared");
    }

    #[test]
    fn a_pre_lineage_entry_reads_as_a_root_of_its_own_id() {
        // The old hook wrote session_id/pid/ts and nothing else. That entry is
        // not corrupt and it is not unknown — it is a conversation of depth 0.
        let f = Fake::new("oldentry");
        f.entry(7, &format!(r#"{{"session_id":"{A}","pid":7,"ts":1}}"#));
        assert_eq!(
            tenancy_for(7, f.home()),
            Tenancy::Chained(Chain {
                root: A.into(),
                seq: 0,
                join: None
            })
        );
    }

    #[test]
    fn a_pid_the_ledger_does_not_hold_is_unchained_not_unrecorded() {
        let f = Fake::new("nopid");
        f.entry(7, &format!(r#"{{"session_id":"{A}","pid":7,"ts":1}}"#));
        assert_eq!(tenancy_for(99, f.home()), Tenancy::Unchained);
    }

    // ---- after the process is gone ----

    #[test]
    fn the_lineage_answers_for_an_id_whose_process_has_ended() {
        let f = Fake::new("dead");
        f.lineage(&[
            &format!(r#"{{"event":"start","session_id":"{A}","pid":7,"ts":1,"root":"{A}","seq":0}}"#),
            &format!(r#"{{"event":"start","session_id":"{B}","pid":7,"ts":2,"root":"{A}","seq":1,"join":"declared"}}"#),
            &format!(r#"{{"event":"end","session_id":"{B}","pid":7,"ts":3,"root":"{A}","seq":1}}"#),
        ]);
        assert_eq!(tenancy_of(B, f.home()).root(), Some(A));
        assert_eq!(tenancy_of(A, f.home()).root(), Some(A));
    }

    #[test]
    fn an_id_the_lineage_has_never_seen_is_unchained() {
        let f = Fake::new("unseen");
        f.lineage(&[&format!(
            r#"{{"event":"start","session_id":"{A}","pid":7,"ts":1,"root":"{A}","seq":0}}"#
        )]);
        assert_eq!(tenancy_of(B, f.home()), Tenancy::Unchained);
        assert!(
            tenancy_of(B, f.home()).measured(),
            "we looked, and it is a root"
        );
    }

    #[test]
    fn the_last_line_naming_an_id_wins() {
        // A /clear re-roots an id that had been a segment. The later record is
        // the true one, and nothing here has to reason about order to get that.
        let f = Fake::new("last");
        f.lineage(&[
            &format!(r#"{{"event":"start","session_id":"{B}","ts":1,"root":"{A}","seq":1,"join":"declared"}}"#),
            &format!(r#"{{"event":"start","session_id":"{B}","ts":2,"root":"{B}","seq":0}}"#),
        ]);
        let Tenancy::Chained(c) = tenancy_of(B, f.home()) else {
            panic!("expected a chain")
        };
        assert_eq!(c.root, B);
        assert_eq!(c.seq, 0);
        assert_eq!(c.join, None);
    }

    // ---- refusing what cannot be trusted ----

    #[test]
    fn junk_lines_are_skipped_rather_than_fatal() {
        let f = Fake::new("junk");
        f.lineage(&[
            "not json at all",
            "{",
            "",
            &format!(r#"{{"event":"start","session_id":"{A}","ts":1,"root":"{A}","seq":0}}"#),
        ]);
        assert_eq!(tenancy_of(A, f.home()).root(), Some(A));
    }

    #[test]
    fn a_root_that_could_name_a_path_is_refused() {
        let f = Fake::new("evilroot");
        f.entry(
            7,
            &format!(r#"{{"session_id":"{A}","pid":7,"ts":1,"root":"../../etc/passwd","seq":1}}"#),
        );
        // The record is unusable, not half-usable: nothing downstream receives
        // a root it could join to a directory.
        let t = tenancy_for(7, f.home());
        assert_ne!(t.root(), Some("../../etc/passwd"));
        assert_eq!(t.root(), Some(A), "falls back to the id, which IS plain");
    }

    #[test]
    fn a_session_id_that_is_not_a_plain_id_yields_no_chain() {
        let f = Fake::new("evilsid");
        f.entry(7, r#"{"session_id":"a; rm -rf /","pid":7,"ts":1}"#);
        assert_eq!(tenancy_for(7, f.home()), Tenancy::Unchained);
    }

    #[test]
    fn a_seq_that_cannot_be_read_makes_the_record_unusable() {
        // Present-and-unreadable is corrupt. It must not collapse to depth 0,
        // which would read as "this is the root of its own conversation".
        let f = Fake::new("badseq");
        f.entry(
            7,
            &format!(r#"{{"session_id":"{A}","pid":7,"ts":1,"seq":"lots"}}"#),
        );
        assert_eq!(tenancy_for(7, f.home()), Tenancy::Unchained);
    }

    #[test]
    fn a_join_word_the_hook_never_writes_is_not_a_join() {
        let f = Fake::new("badjoin");
        f.entry(
            7,
            &format!(
                r#"{{"session_id":"{B}","pid":7,"ts":1,"root":"{A}","seq":1,"join":"probably"}}"#
            ),
        );
        let Tenancy::Chained(c) = tenancy_for(7, f.home()) else {
            panic!("expected a chain")
        };
        assert_eq!(c.join, None, "an unrecognised word never reads as declared");
    }

    #[test]
    fn a_corrupt_entry_is_not_a_conversation() {
        let f = Fake::new("corrupt");
        f.entry(7, "{not json");
        assert_eq!(tenancy_for(7, f.home()), Tenancy::Unchained);
    }
}
