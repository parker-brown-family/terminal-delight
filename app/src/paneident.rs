//! Which conversation a pane is in — one answer, for everything that asks.
//!
//! # The two answers this module replaces
//!
//! The agent wall already resolved a whole fleet at once, with every transcript
//! claimable exactly once ([`crate::vitals::assign`], terminal-delight#272).
//! Everything else — the bench's derived surfaces, `pane_events`, the tool
//! glyph, the desktop recap — asked [`crate::session::claude_transcript`] pane
//! by pane, and that function ends in `newest_jsonl`: the newest file in the
//! project directory.
//!
//! Newest-in-the-directory is the same answer for every pane that shares a
//! directory. On 2026-09-18, fourteen agents were working in one repository and
//! eight of them had no exact binding, so eight benches were reading one
//! conversation and showing that agent's deliverables as their own (#564).
//!
//! # What binds a pane
//!
//! In order: what the agent *says* (a ledger entry it pushed, a descriptor it
//! holds, `--resume` on its own command line), then when it started against when
//! the conversation opened, then elimination. Each rung is labelled on the way
//! out — see [`Bond`] — because a reader that would attribute one agent's work
//! to another has to be able to refuse the weakest one, and it can only refuse
//! what it can see.
//!
//! **Unknown is a value here.** A pane this module cannot bind gets no entry at
//! all, and a caller that finds no entry must show nothing rather than fall back
//! to a directory-wide guess. That is the whole repair: the old code could not
//! tell *I do not know which conversation this is* from *here is a conversation*.
//!
//! # What is deliberately not an input
//!
//! A pane's `claude --resume <id>` line looks like the obvious shortcut and is
//! not allowed in. That string is *synthesised* by the resolver in
//! [`crate::session::agent_resume`], which has forensic rungs of its own — and
//! it is synthesised by whichever build is running the session HOST, which on a
//! live machine is routinely older than the window reading it. Accepting it
//! would launder yesterday's guess into today's claim, and the laundered value
//! would look exactly like ground truth. The process is asked instead.

use crate::vitals::{self, Bond, Cand, FleetPane};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// What the resolver needs to know about one pane. Built from whatever the
/// caller already holds — no gpui, no window.
#[derive(Debug, Clone)]
pub struct PaneFacts {
    /// The pane's shell pid: the identity every caller keys panes by. The agent
    /// itself is a child of it, found per sweep.
    pub shell_pid: u32,
    /// Mode label as the rest of the app spells it: `CLAUDE`, `CODEX`, or
    /// something that is not an agent at all.
    pub mode: String,
    pub cwd: Option<String>,
    /// The pane's resume command, when the caller holds one.
    ///
    /// Read ONLY when the agent process cannot be read at all — see
    /// [`declared_for`]. A live process is always asked directly, because this
    /// string is synthesised by the resolver's own forensic rungs and by
    /// whichever build runs the session host, which is routinely older than the
    /// window reading it.
    pub resume: Option<String>,
}

impl PaneFacts {
    fn is_claude(&self) -> bool {
        self.mode == "CLAUDE"
    }
    fn is_codex(&self) -> bool {
        self.mode == "CODEX"
    }
}

/// A pane, bound to the conversation it is in, with the evidence that bound it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bound {
    pub session_id: String,
    pub path: PathBuf,
    pub bond: Bond,
}

impl Bound {
    /// May a reader attribute this transcript's contents to this pane?
    pub fn is_certain(&self) -> bool {
        self.bond.is_certain()
    }
}

/// Bind every pane in one pass.
///
/// One pass rather than a lookup per pane because the answer is only correct
/// collectively: two panes may not hold one conversation, and that is a fact
/// about the pair, not about either one.
pub fn resolve(panes: &[PaneFacts], home: &Path) -> HashMap<u32, Bound> {
    let mut out = resolve_claude(panes, home);
    out.extend(resolve_codex(panes, home));
    out
}

/// [`resolve`], keeping only what a reader may attribute work to.
///
/// The bench, the tool glyph and the event feed all answer this question and
/// not the looser one: showing nothing is a correct answer, and showing the
/// neighbour's deliverable is not.
pub fn certain(panes: &[PaneFacts], home: &Path) -> HashMap<u32, PathBuf> {
    resolve(panes, home)
        .into_iter()
        .filter(|(_, b)| b.is_certain())
        .map(|(pid, b)| (pid, b.path))
        .collect()
}

// ---------------------------------------------------------------------------
// claude
// ---------------------------------------------------------------------------

fn resolve_claude(panes: &[PaneFacts], home: &Path) -> HashMap<u32, Bound> {
    let mut cands: Vec<Cand> = Vec::new();
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut by_dir: HashMap<String, Vec<usize>> = HashMap::new();

    for p in panes.iter().filter(|p| p.is_claude()) {
        let Some(cwd) = p.cwd.as_deref() else {
            continue;
        };
        let slug = crate::session::claude_slug(cwd);
        if by_dir.contains_key(&slug) {
            continue;
        }
        let dir = home.join(".claude/projects").join(&slug);
        let mut idx = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|x| x == "jsonl") {
                    idx.push(cands.len());
                    cands.push(vitals::edges_cached(&path));
                    paths.push(path);
                }
            }
        }
        by_dir.insert(slug, idx);
    }

    let fleet: Vec<FleetPane> = panes
        .iter()
        .filter(|p| p.is_claude())
        .map(|p| {
            // The pane pid is the shell; the agent is a child of it, and it is
            // the CHILD that holds the descriptors and dates the conversation.
            let agent = vitals::agent_under(p.shell_pid).unwrap_or(p.shell_pid);
            FleetPane {
                pid: p.shell_pid,
                declared: declared_for(agent, p, home),
                started_at: crate::session::proc_start_unix(agent).map(|s| s as i64),
                cands: p
                    .cwd
                    .as_deref()
                    .and_then(|c| by_dir.get(&crate::session::claude_slug(c)).cloned())
                    .unwrap_or_default(),
            }
        })
        .collect();

    // Elimination is only sound over the WHOLE set of agents in a directory, and
    // a window can only see its own panes. Terminal Delight runs several windows
    // on one machine — a scratch instance, a second session — so ask the machine
    // how many agents are actually in each directory and refuse to call anything
    // forced where this window is looking at a subset.
    let crowded: HashMap<String, bool> = panes
        .iter()
        .filter(|p| p.is_claude())
        .filter_map(|p| p.cwd.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .map(|cwd| {
            let mine = panes
                .iter()
                .filter(|p| p.is_claude() && p.cwd.as_deref() == Some(cwd.as_str()))
                .count();
            let machine = crate::session::live_agents_in("claude", &cwd);
            (cwd, !sees_everyone(mine, machine))
        })
        .collect();

    let by_pid: HashMap<u32, Option<&str>> = panes
        .iter()
        .map(|p| (p.shell_pid, p.cwd.as_deref()))
        .collect();

    vitals::assign_bonded(&fleet, &cands)
        .into_iter()
        .map(|(pid, (ci, bond))| {
            let unseen = by_pid
                .get(&pid)
                .and_then(|c| *c)
                .and_then(|c| crowded.get(c))
                .copied()
                .unwrap_or(false);
            let bond = match bond {
                Bond::Sole if unseen => Bond::Guess,
                other => other,
            };
            (
                pid,
                Bound {
                    session_id: cands[ci].id.clone(),
                    path: paths[ci].clone(),
                    bond,
                },
            )
        })
        .collect()
}

/// How long a process's claim is reused before it is asked again.
///
/// Several readers resolve the window on their own cadence — the bench sweep,
/// the tool sweep, the MCP push feed — and each pass would otherwise read
/// `/proc/<pid>/fd` for every agent pane, which is fifty readlinks a pane. Two
/// seconds is shorter than any of those cadences and shorter than the gap
/// between a `/clear` and anything being written under the new id, so a rotation
/// is never served stale for longer than one sweep.
const CLAIM_TTL: Duration = Duration::from_secs(2);

type Claims = Mutex<HashMap<(u32, PathBuf), (Instant, Option<String>)>>;
static CLAIMS: OnceLock<Claims> = OnceLock::new();

/// What this pane's agent says its session is, asked of the process itself.
///
/// Memoised per agent pid, including the *negative* answer: a pane that has
/// nothing to say is the common case on a machine without the ledger hook, and
/// re-proving it three times a second costs the same as proving it.
fn declared_for(agent: u32, pane: &PaneFacts, home: &Path) -> Option<String> {
    // Keyed by home as well as pid. A pid is unique on a machine but not across
    // ROOTS: two tests in one process share a pid and have different homes, and
    // keying on the pid alone let one test's "this pane claims nothing" answer
    // stand in for another's ledger entry.
    let key = (agent, home.to_path_buf());
    let cell = CLAIMS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(map) = cell.lock() {
        if let Some((at, id)) = map.get(&key) {
            if at.elapsed() < CLAIM_TTL {
                return id.clone();
            }
        }
    }
    let cmdline = crate::session::proc_cmdline_of(agent);
    let id = crate::session::declared_session(agent, pane.cwd.as_deref(), &cmdline, home)
        // Nothing but the pane's own resume line is left, and it is taken only
        // when there is no process to ask: a pid with no `/proc` entry is a pane
        // whose agent has gone, or one this process cannot see into. The
        // laundering this rule exists to prevent needs a LIVE agent whose real
        // session differs from the synthesised one, and that agent is always
        // readable — so the guard closes the path exactly where the harm is.
        .or_else(|| {
            (!std::path::Path::new(&format!("/proc/{agent}")).exists())
                .then(|| crate::session::resume_session_id(pane.resume.as_deref()?))
                .flatten()
        });
    if let Ok(mut map) = cell.lock() {
        // A window that has seen a thousand panes over a long day should not
        // carry a thousand entries; they are all two seconds stale anyway.
        if map.len() > 512 {
            map.clear();
        }
        map.insert(key, (Instant::now(), id.clone()));
    }
    id
}

// ---------------------------------------------------------------------------
// codex
// ---------------------------------------------------------------------------

/// Codex rollouts are not in a per-project directory: they are found by reading
/// the head of each rollout for a cwd, newest first, which gives every codex
/// pane in one directory the same file — the same hazard, reached another way.
///
/// There is no fleet pass for them yet, so they get the honest half of one: a
/// pane alone in its directory is bound by elimination, panes that share one are
/// bound only if they name their own session, and — whatever the directories say
/// — two panes may not both be certain of ONE rollout.
///
/// That last rule is not theoretical. The head match is `contains(cwd)`, so a
/// pane in `/home/parker` and a pane in `/home/parker/PROJECT` can select the
/// same newest rollout while each is alone in its own directory: two `sole`
/// bonds, one file, both certain. Measured on this machine the day the claude
/// half was fixed — the bug class survives in the path nobody was looking at.
fn resolve_codex(panes: &[PaneFacts], home: &Path) -> HashMap<u32, Bound> {
    let mut out = HashMap::new();
    for p in panes.iter().filter(|p| p.is_codex()) {
        let Some(cwd) = p.cwd.as_deref() else {
            continue;
        };
        let Some(path) = crate::session::codex_transcript(cwd, home) else {
            continue;
        };
        let Some(id) = crate::session::rollout_uuid(&path) else {
            continue;
        };
        let agent = vitals::agent_under(p.shell_pid).unwrap_or(p.shell_pid);
        let declared = crate::session::codex_declared_session(agent, home);
        let sharers = panes
            .iter()
            .filter(|q| q.is_codex() && q.cwd.as_deref() == Some(cwd))
            .count();
        let bond = codex_bond(declared.as_deref() == Some(id.as_str()), sharers);
        out.insert(
            p.shell_pid,
            Bound {
                session_id: id,
                path,
                bond,
            },
        );
    }
    demote_shared_rollouts(&mut out);
    out
}

/// Two panes pointing at one rollout, neither of them having named it, are both
/// guessing however alone each looked in its own directory.
fn demote_shared_rollouts(bound: &mut HashMap<u32, Bound>) {
    let mut seen: HashMap<PathBuf, usize> = HashMap::new();
    for b in bound.values() {
        *seen.entry(b.path.clone()).or_default() += 1;
    }
    for b in bound.values_mut() {
        if seen.get(&b.path).copied().unwrap_or(0) > 1 && b.bond != Bond::Declared {
            b.bond = Bond::Guess;
        }
    }
}

/// Pure: may this window reason by elimination about a directory?
///
/// `mine` is how many agent panes this window has there; `machine` is how many
/// agent processes are there in total. Elimination is an argument about a closed
/// set — *nothing else could own this* — so it is only sound when the set the
/// window can see is the whole set.
///
/// A census of zero is a broken census, not an empty machine: the window is
/// looking at `mine` panes whose processes exist by construction. It is read as
/// *cannot tell*, and cannot-tell keeps the previous behaviour rather than
/// blanking every bench on a machine whose `/proc` has stopped answering — a
/// state in which the pane list itself would already be empty.
fn sees_everyone(mine: usize, machine: usize) -> bool {
    machine <= mine
}

/// Pure: how well evidenced a codex binding is.
///
/// `declared` — the process named this rollout itself. `sharers` — how many
/// codex panes sit in that directory, this one included, so 1 means alone.
fn codex_bond(declared: bool, sharers: usize) -> Bond {
    match (declared, sharers) {
        (true, _) => Bond::Declared,
        (false, 0 | 1) => Bond::Sole,
        (false, _) => Bond::Guess,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vitals::{Cand, FleetPane};

    fn cand(id: &str, began: i64, spoke: i64) -> Cand {
        Cand {
            id: id.into(),
            began: Some(began),
            spoke: Some(spoke),
        }
    }

    fn pane(pid: u32, declared: Option<&str>, started: i64, cands: &[usize]) -> FleetPane {
        FleetPane {
            pid,
            declared: declared.map(str::to_string),
            started_at: Some(started),
            cands: cands.to_vec(),
        }
    }

    /// #564 as a test. Three fresh panes in one directory, none of them naming a
    /// session, and one transcript that was written into most recently. The old
    /// per-pane rung gave that file to all three.
    #[test]
    fn panes_sharing_a_directory_never_share_a_conversation() {
        let cands = vec![
            cand("aaaa", 1_000_000, 1_009_000), // the one everybody used to get
            cand("bbbb", 1_000_500, 1_002_000),
            cand("cccc", 1_001_000, 1_003_000),
        ];
        // Started hours before anything was born, so no birth match can fire.
        let panes = vec![
            pane(1, None, 900_000, &[0, 1, 2]),
            pane(2, None, 900_100, &[0, 1, 2]),
            pane(3, None, 900_200, &[0, 1, 2]),
        ];
        let got = vitals::assign_bonded(&panes, &cands);
        let mut seen: Vec<usize> = got.values().map(|(ci, _)| *ci).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), got.len(), "one transcript, at most one pane");
        assert!(
            got.values().all(|(_, b)| *b == Bond::Guess),
            "nothing here is evidenced, so nothing here is certain: {got:?}"
        );
    }

    /// The common case must not regress: one agent, one directory, no claim of
    /// any kind. It is still bound — by elimination, which is certain enough to
    /// act on, because no other pane could be the subject.
    #[test]
    fn the_only_agent_in_a_directory_is_bound_by_elimination() {
        let cands = vec![cand("aaaa", 1_000_000, 1_009_000)];
        let panes = vec![pane(7, None, 900_000, &[0])];
        let got = vitals::assign_bonded(&panes, &cands);
        assert_eq!(got[&7].1, Bond::Sole);
        assert!(got[&7].1.is_certain());
    }

    /// Elimination cascades: binding the forced pair can force the next one.
    #[test]
    fn elimination_repeats_until_nothing_is_forced() {
        let cands = vec![
            cand("aaaa", 1_000_000, 1_001_000),
            cand("bbbb", 1_000_100, 1_002_000),
        ];
        let panes = vec![
            pane(1, Some("aaaa"), 900_000, &[0, 1]), // claims aaaa
            pane(2, None, 900_000, &[0, 1]),         // bbbb is then forced
        ];
        let got = vitals::assign_bonded(&panes, &cands);
        assert_eq!(got[&1].1, Bond::Declared);
        assert_eq!(got[&2].1, Bond::Sole);
        assert_ne!(got[&1].0, got[&2].0);
    }

    /// Two panes, one transcript, nothing to tell them apart. Elimination must
    /// NOT fire: "the only one left" is an argument about a closed set, and with
    /// two panes able to take it the set is not closed. One of them gets it as a
    /// guess — somebody is probably there — and no reader may act on that.
    ///
    /// This is the shape the crowded-directory test does not reach: with three
    /// candidates each pane's eligible set has three entries, so the forcing rung
    /// never engages and the contest check is never exercised.
    #[test]
    fn elimination_refuses_a_transcript_two_panes_could_own() {
        let cands = vec![cand("aaaa", 1_000_000, 1_009_000)];
        let a = pane(1, None, 900_000, &[0]);
        let b = pane(2, None, 900_100, &[0]);
        let got = vitals::assign_bonded(&[a.clone(), b.clone()], &cands);
        assert!(
            got.values().all(|(_, bond)| *bond != Bond::Sole),
            "a contested transcript was called forced: {got:?}"
        );
        assert_eq!(got.len(), 1, "one of them holds it, not both: {got:?}");
        assert!(got.values().all(|(_, bond)| *bond == Bond::Guess));
        // And which one must not depend on the order the window listed them.
        assert_eq!(got, vitals::assign_bonded(&[b, a], &cands));
    }

    /// A claim outranks a close birth: the agent saying which conversation it is
    /// in is better evidence than two timestamps agreeing.
    #[test]
    fn a_claim_outranks_a_birth_match() {
        let cands = vec![
            cand("aaaa", 1_000_000, 1_000_500),
            cand("bbbb", 1_000_002, 1_000_900),
        ];
        // Pane 1 starts exactly when bbbb was born, but names aaaa.
        let panes = vec![
            pane(1, Some("aaaa"), 1_000_002, &[0, 1]),
            pane(2, None, 1_000_000, &[0, 1]),
        ];
        let got = vitals::assign_bonded(&panes, &cands);
        assert_eq!(cands[got[&1].0].id, "aaaa");
        assert_eq!(got[&1].1, Bond::Declared);
        assert_eq!(cands[got[&2].0].id, "bbbb");
    }

    /// A pane with nothing to bind it to gets no entry — not the newest file,
    /// not an empty string, not a zero. Absence is the answer.
    #[test]
    fn a_pane_with_no_candidates_is_absent_rather_than_defaulted() {
        let cands = vec![cand("aaaa", 1_000_000, 1_001_000)];
        let panes = vec![pane(9, None, 900_000, &[])];
        let got = vitals::assign_bonded(&panes, &cands);
        assert!(!got.contains_key(&9));
    }

    /// Order in, same answer out — the resolver may not depend on which pane the
    /// window happened to iterate first.
    #[test]
    fn the_answer_does_not_depend_on_pane_order() {
        let cands = vec![
            cand("aaaa", 1_000_000, 1_005_000),
            cand("bbbb", 1_000_050, 1_006_000),
            cand("cccc", 1_000_090, 1_007_000),
        ];
        let a = pane(1, None, 1_000_000, &[0, 1, 2]);
        let b = pane(2, None, 1_000_050, &[0, 1, 2]);
        let c = pane(3, Some("cccc"), 1_000_090, &[0, 1, 2]);
        let fwd = vitals::assign_bonded(&[a.clone(), b.clone(), c.clone()], &cands);
        let rev = vitals::assign_bonded(&[c, b, a], &cands);
        assert_eq!(fwd, rev);
    }

    /// Injectivity under fan-out: many panes, few transcripts, nothing declared.
    /// Whatever the rungs decide, no transcript may be handed out twice.
    #[test]
    fn no_transcript_is_ever_handed_to_two_panes() {
        for n_panes in 1..8u32 {
            for n_cands in 0..6usize {
                let cands: Vec<Cand> = (0..n_cands)
                    .map(|i| {
                        cand(
                            &format!("c{i}"),
                            1_000_000 + i as i64 * 10,
                            1_001_000 + i as i64,
                        )
                    })
                    .collect();
                let all: Vec<usize> = (0..n_cands).collect();
                let panes: Vec<FleetPane> = (1..=n_panes)
                    .map(|p| pane(p, None, 999_000 + p as i64, &all))
                    .collect();
                let got = vitals::assign_bonded(&panes, &cands);
                let mut seen: Vec<usize> = got.values().map(|(ci, _)| *ci).collect();
                let before = seen.len();
                seen.sort_unstable();
                seen.dedup();
                assert_eq!(
                    before,
                    seen.len(),
                    "{n_panes} panes over {n_cands} transcripts handed one out twice"
                );
                assert!(got.len() <= n_cands, "more bindings than transcripts");
            }
        }
    }

    /// Terminal Delight runs more than one window on this machine, and a window
    /// binding by elimination is arguing about a set it can only half see.
    #[test]
    fn elimination_needs_the_whole_set_not_just_this_window_s_panes() {
        assert!(sees_everyone(1, 1), "the only pane, the only agent");
        assert!(sees_everyone(3, 3));
        assert!(
            !sees_everyone(1, 2),
            "a second agent in that directory belongs to another window"
        );
        // A census that cannot count is not a census of nothing. Reading it as
        // an empty machine would be the same mistake in the other direction.
        assert!(
            sees_everyone(2, 0),
            "unreadable /proc keeps prior behaviour"
        );
    }

    /// Two panes deliberately resumed onto one conversation really are both in
    /// it. Exclusivity is a rule about inference, and must not take a transcript
    /// away from a pane that named it correctly.
    #[test]
    fn two_panes_naming_one_session_both_get_it() {
        let cands = vec![cand("aaaa", 1_000_000, 1_001_000)];
        let panes = vec![
            pane(1, Some("aaaa"), 900_000, &[0]),
            pane(2, Some("aaaa"), 900_100, &[0]),
        ];
        let got = vitals::assign_bonded(&panes, &cands);
        assert_eq!(got[&1], (0, Bond::Declared));
        assert_eq!(got[&2], (0, Bond::Declared));
    }

    /// One rollout, two panes that each looked alone: nobody is certain. Found
    /// live — a pane in `/home/parker` and one in `/home/parker/PROJECT` both
    /// selected the same newest rollout, because the head match is a substring.
    #[test]
    fn two_codex_panes_on_one_rollout_are_both_demoted() {
        let mut bound = HashMap::new();
        let shared = PathBuf::from("/r/one.jsonl");
        for (pid, bond) in [(1u32, Bond::Sole), (2, Bond::Sole)] {
            bound.insert(
                pid,
                Bound {
                    session_id: "one".into(),
                    path: shared.clone(),
                    bond,
                },
            );
        }
        bound.insert(
            3,
            Bound {
                session_id: "two".into(),
                path: PathBuf::from("/r/two.jsonl"),
                bond: Bond::Sole,
            },
        );
        demote_shared_rollouts(&mut bound);
        assert_eq!(bound[&1].bond, Bond::Guess);
        assert_eq!(bound[&2].bond, Bond::Guess);
        assert_eq!(
            bound[&3].bond,
            Bond::Sole,
            "an uncontested one is untouched"
        );

        // A pane that NAMED the rollout keeps it; the one guessing beside it does not.
        let mut named = HashMap::new();
        named.insert(
            1,
            Bound {
                session_id: "one".into(),
                path: shared.clone(),
                bond: Bond::Declared,
            },
        );
        named.insert(
            2,
            Bound {
                session_id: "one".into(),
                path: shared,
                bond: Bond::Sole,
            },
        );
        demote_shared_rollouts(&mut named);
        assert_eq!(named[&1].bond, Bond::Declared);
        assert_eq!(named[&2].bond, Bond::Guess);
    }

    #[test]
    fn a_codex_pane_alone_in_its_directory_is_certain_and_a_crowded_one_is_not() {
        assert_eq!(codex_bond(false, 1), Bond::Sole);
        assert_eq!(codex_bond(false, 3), Bond::Guess);
        assert_eq!(codex_bond(true, 3), Bond::Declared);
        assert!(!codex_bond(false, 2).is_certain());
    }

    /// The bond is the whole point of the strict view, so prove the filter
    /// actually drops the weak rung rather than passing everything through.
    #[test]
    fn the_strict_view_keeps_the_evidenced_rungs_only() {
        let certain = [Bond::Declared, Bond::Birth, Bond::Sole];
        assert!(certain.iter().all(|b| b.is_certain()));
        assert!(!Bond::Guess.is_certain());
    }

    // -----------------------------------------------------------------------
    // against a real directory
    // -----------------------------------------------------------------------

    /// A throwaway `$HOME` holding project transcripts and, optionally, ledger
    /// entries. Removed on drop even when the test fails.
    struct Home(PathBuf);

    impl Home {
        fn new(tag: &str) -> Home {
            let dir = std::env::temp_dir().join(format!(
                "td-paneident-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            std::fs::remove_dir_all(&dir).ok();
            std::fs::create_dir_all(&dir).unwrap();
            Home(dir)
        }

        /// A transcript for `cwd` whose records carry these two times.
        fn transcript(&self, cwd: &str, id: &str, began: &str, spoke: &str) {
            let dir = self
                .0
                .join(".claude/projects")
                .join(crate::session::claude_slug(cwd));
            std::fs::create_dir_all(&dir).unwrap();
            let body = format!(
                "{{\"type\":\"user\",\"timestamp\":\"{began}\"}}\n\
                 {{\"type\":\"assistant\",\"timestamp\":\"{spoke}\"}}\n"
            );
            std::fs::write(dir.join(format!("{id}.jsonl")), body).unwrap();
        }

        /// A ledger entry, in the exact shape the SessionStart hook writes.
        fn ledger(&self, pid: u32, id: &str) {
            let dir = self.0.join(".local/state/terminal-delight/agent-ledger");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(format!("{pid}.json")),
                format!("{{\"session_id\":\"{id}\",\"pid\":{pid},\"ts\":1}}\n"),
            )
            .unwrap();
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).ok();
        }
    }

    fn facts(pid: u32, cwd: &str) -> PaneFacts {
        PaneFacts {
            shell_pid: pid,
            mode: "CLAUDE".into(),
            cwd: Some(cwd.into()),
            resume: None,
        }
    }

    /// The whole bug, end to end, through the real resolver: two panes, one
    /// directory, three transcripts, and nothing on this machine that says which
    /// pane is in which conversation. The old path gave both panes the newest
    /// file. This one gives neither pane anything to show.
    ///
    /// The pids are this test process's own, which no `claude` runs under, so
    /// every exact rung genuinely finds nothing — the state a pane is in before
    /// the ledger hook has ever run for it.
    #[test]
    fn two_unidentifiable_panes_in_one_directory_are_told_nothing() {
        let home = Home::new("crowd");
        let cwd = "/work/crowded";
        home.transcript(
            cwd,
            "aaaa",
            "2026-09-18T20:00:00.000Z",
            "2026-09-18T20:01:00.000Z",
        );
        home.transcript(
            cwd,
            "bbbb",
            "2026-09-18T20:02:00.000Z",
            "2026-09-18T20:03:00.000Z",
        );
        home.transcript(
            cwd,
            "cccc",
            "2026-09-18T20:04:00.000Z",
            "2026-09-18T21:00:00.000Z",
        );
        let me = std::process::id();
        let panes = vec![facts(me, cwd), facts(me + 1, cwd)];

        let strict = certain(&panes, &home.0);
        assert!(
            strict.is_empty(),
            "nothing is evidenced, so nothing may be attributed: {strict:?}"
        );

        // The looser view still answers — the wall would rather draw a probable
        // card than none — but it may never answer twice with one file.
        let loose = resolve(&panes, &home.0);
        let mut paths: Vec<&PathBuf> = loose.values().map(|b| &b.path).collect();
        paths.sort();
        let n = paths.len();
        paths.dedup();
        assert_eq!(
            n,
            paths.len(),
            "one transcript reached two panes: {loose:?}"
        );
        assert!(loose.values().all(|b| b.bond == Bond::Guess));
    }

    /// The same directory, one pane. Nothing has claimed anything, but nothing
    /// else could be the subject either, so elimination binds it — and this is
    /// the case that must keep working, because it is nearly every machine.
    #[test]
    fn one_pane_in_a_directory_is_still_bound() {
        let home = Home::new("alone");
        let cwd = "/work/alone";
        home.transcript(
            cwd,
            "aaaa",
            "2026-09-18T20:00:00.000Z",
            "2026-09-18T20:30:00.000Z",
        );
        let me = std::process::id();
        let strict = certain(&[facts(me, cwd)], &home.0);
        assert!(strict.get(&me).is_some_and(|p| p.ends_with("aaaa.jsonl")));
    }

    /// A pushed ledger entry binds a pane no matter how crowded the directory,
    /// which is what makes the hook worth installing: the panes that name
    /// themselves are certain, and only the silent ones fall back.
    #[test]
    fn a_ledger_entry_binds_a_pane_in_a_crowd() {
        // This test's pane pid is the test process itself, and it depends on
        // that process having no agent child — the sibling test below spawns
        // one on purpose, and `agent_under` would then hand the ledger lookup
        // that child's pid. Same lock, so the two can never overlap.
        let _guard = crate::testsync::forks_and_locks();
        let home = Home::new("ledger");
        let cwd = "/work/ledger";
        home.transcript(
            cwd,
            "aaaa",
            "2026-09-18T20:00:00.000Z",
            "2026-09-18T20:01:00.000Z",
        );
        home.transcript(
            cwd,
            "bbbb",
            "2026-09-18T20:02:00.000Z",
            "2026-09-18T21:00:00.000Z",
        );
        let me = std::process::id();
        // The ledger is keyed by the AGENT pid; with no `claude` child under
        // this test process the resolver falls back to the pane pid itself,
        // which is the pid this entry is written for.
        home.ledger(me, "aaaa");
        let panes = vec![facts(me, cwd), facts(me + 1, cwd)];

        let bound = resolve(&panes, &home.0);
        assert_eq!(bound[&me].bond, Bond::Declared);
        assert!(bound[&me].path.ends_with("aaaa.jsonl"));
        assert_eq!(bound[&me].session_id, "aaaa");
        // And the pane that said nothing is left with the other file at most —
        // never with the claimed one.
        assert!(bound
            .get(&(me + 1))
            .is_none_or(|b| b.path.ends_with("bbbb.jsonl")));
        assert!(certain(&panes, &home.0).contains_key(&me));
    }

    /// The pane's own resume line may not bind a LIVE process. It is
    /// synthesised by the resolver's forensic rungs and by whichever build runs
    /// the session host, so trusting it for a process we could simply ask would
    /// launder a directory-wide guess into a claim.
    #[test]
    fn a_live_process_is_asked_rather_than_believed_about_itself() {
        let home = Home::new("launder");
        let cwd = "/work/launder";
        home.transcript(
            cwd,
            "aaaa",
            "2026-09-18T20:00:00.000Z",
            "2026-09-18T20:01:00.000Z",
        );
        home.transcript(
            cwd,
            "bbbb",
            "2026-09-18T20:02:00.000Z",
            "2026-09-18T21:00:00.000Z",
        );
        let me = std::process::id();
        // This process is alive and has no claim of its own. Its pane carries a
        // resume line naming a session anyway — the shape TD synthesises.
        let mut named = facts(me, cwd);
        named.resume = Some("claude --resume aaaa".into());
        let panes = vec![named, facts(me + 1, cwd)];

        let bound = resolve(&panes, &home.0);
        assert!(
            bound.get(&me).is_none_or(|b| b.bond != Bond::Declared),
            "a live process's resume line was taken as a claim: {bound:?}"
        );
        assert!(
            certain(&panes, &home.0).is_empty(),
            "two panes, no evidence, and one of them holding a synthesised line"
        );
    }

    /// A pane can hold two agents at once — suspend one with ctrl+Z and start
    /// another — and then "the pane's agent" has two candidates. The terminal
    /// already knows which one a keystroke reaches; the child walk finds the one
    /// left behind, because it is older.
    ///
    /// Measured on this machine before the fix: a pane whose foreground group was
    /// the live agent, bound to the stopped one's conversation on the bench, the
    /// wall and the tool glyph.
    #[test]
    fn a_pane_with_a_suspended_agent_binds_to_the_one_you_are_typing_to() {
        let live = 1390176;
        let suspended = 1384250;
        // The foreground group is the live agent: it wins outright, and the walk
        // is never even asked.
        assert_eq!(
            vitals::pick_agent(Some((live, "claude".into())), || panic!("walk was asked")),
            Some(live)
        );
        // No foreground group at all — a pane whose terminal cannot be read —
        // falls back to the walk, exactly as before.
        assert_eq!(
            vitals::pick_agent(None, || Some(suspended)),
            Some(suspended)
        );
        // A foreground process that is NOT an agent (a pager, a git command)
        // leaves an agent running behind it, and that agent is still the pane's.
        assert_eq!(
            vitals::pick_agent(Some((999, "less".into())), || Some(suspended)),
            Some(suspended)
        );
        // Codex counts too — the rule is about agents, not about one of them.
        assert_eq!(
            vitals::pick_agent(Some((live, "codex".into())), || None),
            Some(live)
        );
        // And a pane with nothing under it stays unbound rather than inventing.
        assert_eq!(
            vitals::pick_agent(Some((999, "bash".into())), || None),
            None
        );
    }

    /// `pick_agent` proves the RULE; this proves the rule is plugged in.
    ///
    /// The first version of the test above passed with the wiring deleted,
    /// because it called the pure function directly — the same sleeping-guard
    /// shape as the elimination test earlier in this file. The branch itself
    /// cannot be exercised from a unit test: a test harness has no controlling
    /// terminal, so `tpgid` is -1 and the foreground arm is unreachable. What CAN
    /// be asserted is that the caller still asks the kernel at all.
    #[test]
    fn the_agent_lookup_still_asks_the_kernel_which_process_is_in_front() {
        let src = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/vitals.rs"),
        )
        .unwrap();
        let body = src
            .split("pub fn agent_under")
            .nth(1)
            .expect("agent_under still exists");
        let code: String = body
            .lines()
            .take_while(|l| !l.starts_with("/// Is this the command name"))
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            code.contains("foreground_pid"),
            "agent_under went back to guessing which of a pane's agents is its own"
        );
    }

    /// The walk half, end to end against a real process tree: a child whose
    /// command name is `claude` is found under its parent. Proves the comm match
    /// and the descent, which the pure test cannot.
    #[test]
    fn a_real_child_named_like_an_agent_is_found_under_its_parent() {
        // Spawning a process called `claude` changes what every other test in
        // this file sees under its own pid — see the note on the ledger test.
        let _guard = crate::testsync::forks_and_locks();
        let dir = std::env::temp_dir().join(format!("td-agentunder-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("claude");
        // `comm` comes from the executable name, so a copy of `sleep` called
        // `claude` is indistinguishable from one to every reader here.
        std::fs::copy("/usr/bin/sleep", &fake).unwrap();
        let mut child = match std::process::Command::new(&fake).arg("30").spawn() {
            Ok(c) => c,
            Err(_) => return, // no /usr/bin/sleep: nothing to prove, nothing to fail
        };
        // The child's comm is set by exec, which has not necessarily happened
        // the instant spawn returns.
        let mut found = None;
        for _ in 0..50 {
            found = vitals::agent_under(std::process::id());
            if found == Some(child.id()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let got = found;
        let _ = child.kill();
        let _ = child.wait();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(got, Some(child.id()), "the walk did not find the agent");
    }

    /// An empty project directory is not a directory full of possibilities.
    #[test]
    fn a_directory_with_no_transcripts_binds_nobody() {
        let home = Home::new("empty");
        let me = std::process::id();
        assert!(resolve(&[facts(me, "/work/empty")], &home.0).is_empty());
    }

    /// Panes in different repositories cannot take each other's conversations,
    /// however the times line up — the candidate set is per directory.
    #[test]
    fn a_neighbouring_directory_is_not_a_candidate() {
        let home = Home::new("split");
        home.transcript(
            "/work/one",
            "aaaa",
            "2026-09-18T20:00:00.000Z",
            "2026-09-18T20:30:00.000Z",
        );
        home.transcript(
            "/work/two",
            "bbbb",
            "2026-09-18T20:00:00.000Z",
            "2026-09-18T20:30:00.000Z",
        );
        let me = std::process::id();
        let bound = certain(
            &[facts(me, "/work/one"), facts(me + 1, "/work/two")],
            &home.0,
        );
        assert!(bound[&me].ends_with("aaaa.jsonl"));
        assert!(bound[&(me + 1)].ends_with("bbbb.jsonl"));
    }

    /// The deleted fallback, as a standing rule: nothing in the live-pane path
    /// may resolve a transcript by taking the newest file in a directory.
    ///
    /// A source scan, because the guarantee is "no caller anywhere" and no unit
    /// test can say that. Comments are stripped first — a rule that its own
    /// explanation can satisfy is not a rule, and this file is full of prose
    /// naming the thing it forbids.
    #[test]
    fn no_reader_resolves_a_pane_by_the_newest_file_in_a_directory() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let allowed = ["session.rs"]; // the directory listing itself lives there
        let mut offenders = Vec::new();
        for entry in std::fs::read_dir(&src).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if allowed.contains(&name.as_str()) {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap();
            let code: String = body
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
                .collect::<Vec<_>>()
                .join("\n");
            // Built rather than written, so this file's own scan does not
            // count as a caller of the thing it forbids — the guard has to be
            // able to police the module that hosts it.
            for banned in [
                format!("newest_{}", "jsonl"),
                format!("claude_transcript{}", "("),
            ] {
                if code.contains(&banned) {
                    offenders.push(format!("{name} reaches for {banned}"));
                }
            }
        }
        assert!(
            offenders.is_empty(),
            "a pane's conversation is resolved by paneident, not by file times: {offenders:?}"
        );
    }
}
