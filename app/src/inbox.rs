//! The inbox: every agent pane as a conversation somebody means to close.
//!
//! The left spine's second face. PROJECTS shows where the work lives; INBOX
//! shows what the work wants from you. Parker, 2026-11-07: *"the left spine
//! might toggle between PROJECTS and INBOX — the inbox being very similar to
//! what T3 has done with their left spine agent attention surface"*, and, while
//! it was being built, *"organized into SLEEP squitet --- done - allowed to
//! snooze is super cool - then the inbox of LOOK AT ME at the top"*.
//!
//! The idea is T3 Code's, said by Theo in the video that introduced it: *"the
//! sidebar is an inbox. Every time you open a thread, the goal should be to
//! close it"*, and *"If the thing is working, get it out of my face. Don't
//! bother me until it's ready."* The idea is all that came across. T3 keeps
//! threads on a server and knows their state because it runs them; this
//! window reads screens, so every shelf here is decided from what the rail
//! already decided, and nothing is invented to fill a gap.
//!
//! # The shelves, top to bottom
//!
//! - **LOOK AT ME** — exactly the rail's rows, in the rail's order, plus any
//!   row whose snooze has just run out. Never folds.
//! - **WORKING** — mid-turn. Folded: a working agent is not the bottleneck.
//! - **SNOOZED** — a person said *not now*, until a time.
//! - **SLEEP & QUIET** — awake with nothing to say, resting, or asleep.
//! - **DONE** — a person said *done*.
//!
//! # What this module is
//!
//! Pure, like `attention.rs` beside it: facts in, shelves out, plus the small
//! store of what a person said and how it is written down. No gpui, no pane
//! reads, and no clock of its own — every function is handed `now` (the wall
//! clock, for a snooze that means "nine tomorrow") and `at` (this process's
//! monotonic clock, the only one a pane's `changed` can be compared against).
//! `inbox_view.rs` builds the facts and draws the shelves. A rule that can
//! live here lives here, because here it is tested without a window.
//!
//! # The rules, each a test below
//!
//! - **One ranking.** A row with a lane is on LOOK AT ME in the order
//!   `attention::project` gives it — the same call the rail makes — so the
//!   two surfaces cannot disagree about which pane comes first.
//! - **Every agent pane is on exactly one shelf**, and a shell is on none
//!   unless something in it asked for access, which the rail shows too.
//! - **Unknown is not quiet.** A pane whose screen could not be read is on
//!   LOOK AT ME in the unknown lane, shown and never counted.
//! - **A person's word holds for the turn they saw.** DONE hides a finish and
//!   SNOOZED hides a question; something NEW — a new question, a new finish,
//!   the agent starting work again — undoes the word. Compared on the
//!   monotonic clock against the moment the word was given, so a mark read
//!   back from disk takes the moment it was read: everything seen to change
//!   after that is news, and nothing seen before it is.
//! - **A snooze that runs out comes back WOKE**, on LOOK AT ME even when the
//!   agent has nothing to say, until the pane is visited.

use crate::attention::{self, AttentionKind, Priority};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long a pane that has stopped working keeps its WORKING seat when it has
/// nothing to say yet.
///
/// The badge reads a screen every 120ms, and between two tool calls an agent
/// can look idle for a scan or two. Without a hold the row would hop to SLEEP &
/// QUIET and back, under the pointer of somebody aiming at it. T3 holds its
/// working shelf for the same two seconds (`WORKING_EXIT_GRACE_MS`). A row with
/// something to say does not wait: a finish or a question goes up at once.
pub const WORKING_GRACE: Duration = Duration::from_secs(2);

/// How long a remembered pane may be missing before its marks are forgotten.
///
/// Not immediately, because a window that has just opened has not been told
/// about every pane yet — the host describes them as it gets to them — and a
/// snooze dropped in that minute would be a person's word lost to a race. Half
/// an hour is long past any startup and short enough that a closed pane's
/// marks do not pile up.
pub const FORGET_AFTER: Duration = Duration::from_secs(30 * 60);

/// A row's identity: whose marks these are.
///
/// The session host's pane id when there is one: it survives this window, a
/// restart, and an agent sleeping and waking in the pane. Not the conversation
/// id, because the session strings panes report are not unique per pane — five
/// panes once shared one (#416) — and snoozing one would have snoozed five.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Key {
    /// The session host's pane id.
    Pane(u64),
    /// A pane this window owns outright, with no host. Good for this window's
    /// life and never written down: its number means nothing to the next one.
    Window(u64),
}

impl Key {
    /// Whether this key can be written down and found again.
    pub fn saved(self) -> bool {
        matches!(self, Key::Pane(_))
    }

    fn word(self) -> Option<String> {
        match self {
            Key::Pane(id) => Some(format!("pane:{id}")),
            Key::Window(_) => None,
        }
    }

    fn parse(word: &str) -> Option<Key> {
        word.strip_prefix("pane:")?.parse().ok().map(Key::Pane)
    }
}

/// What is running in a pane, as far as the shelves care. Decided by the view
/// from the pane's mode and its rest, never guessed here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Life {
    /// An agent whose process is running.
    Awake,
    /// Frozen by the window's rest gate, its memory pushed out; a key thaws it.
    Resting,
    /// Ended with its conversation kept; a key brings it back.
    Asleep,
    /// A pane the window has not been able to describe.
    Unknown,
    /// A plain shell. On no shelf, unless it asked for access.
    Shell,
}

/// The five shelves, in the order they are drawn. The derived `Ord` is that
/// order, which is why the variants are written in this sequence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Shelf {
    LookAtMe,
    Working,
    Snoozed,
    Quiet,
    Done,
}

impl Shelf {
    pub const ALL: [Shelf; 5] = [
        Shelf::LookAtMe,
        Shelf::Working,
        Shelf::Snoozed,
        Shelf::Quiet,
        Shelf::Done,
    ];

    /// Whether the shelf folds. LOOK AT ME never does: an inbox whose top
    /// shelf can be shut is a way of not looking, and the rail beside it has
    /// no such switch either.
    pub fn folds(self) -> bool {
        self != Shelf::LookAtMe
    }

    fn word(self) -> &'static str {
        match self {
            Shelf::LookAtMe => "look",
            Shelf::Working => "working",
            Shelf::Snoozed => "snoozed",
            Shelf::Quiet => "quiet",
            Shelf::Done => "done",
        }
    }

    fn parse(word: &str) -> Option<Shelf> {
        Shelf::ALL.into_iter().find(|s| s.word() == word)
    }
}

/// How LOOK AT ME is ordered.
///
/// The one decision in here that is Parker's to make and was not made: the
/// rail's order (what the person promoted, then the lane, then whoever has
/// waited longest) or T3's (whoever came back most recently, first). The
/// rail's is the default because a window that ranks one pane two ways is the
/// defect the rail spent a slice removing; the other is one switch away, and
/// both are tested.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LookOrder {
    #[default]
    Rail,
    Returned,
}

impl LookOrder {
    fn word(self) -> &'static str {
        match self {
            LookOrder::Rail => "rail",
            LookOrder::Returned => "returned",
        }
    }

    fn parse(word: &str) -> Option<LookOrder> {
        [LookOrder::Rail, LookOrder::Returned]
            .into_iter()
            .find(|o| o.word() == word)
    }
}

/// Everything the inbox knows about one pane, as the view read it this frame.
#[derive(Clone, Debug)]
pub struct Fact {
    pub key: Key,
    /// The window's entity for this pane, which a click focuses. A location,
    /// not an identity: it does not survive the window.
    pub pane: u64,
    pub life: Life,
    /// The rail's lane, decided by `agent_badge` and `rail_kind` (or an ask).
    /// `None` is a pane that wants nothing.
    pub lane: Option<AttentionKind>,
    /// Mid-turn, by the same badge.
    pub working: bool,
    pub priority: Priority,
    /// When the lane was last seen to change. `None` when it has not been seen
    /// to change, which is a different claim from "just now".
    pub changed: Option<Instant>,
    /// The pane's name, as its own header says it.
    pub title: String,
    /// Where it sits on the left bar: `PROJECT › tab`.
    pub place: String,
    /// The model the pane's header leads with (`OPUS 5.5 MAX`), or its harness.
    pub lead: Option<String>,
    /// One line in the agent's or the person's own words: the agent's latest
    /// brief, the line behind its lane, or the note on the pane.
    pub line: Option<String>,
}

/// What a row says about itself, beside its title.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// On LOOK AT ME because the rail ranks it.
    Lane(AttentionKind),
    /// On LOOK AT ME because its snooze ran out and nobody has looked.
    Woke,
    Working,
    Snoozed,
    Idle,
    Resting,
    Asleep,
    Done,
}

impl Status {
    /// Whether the row adds to the number on the INBOX toggle. A woken row
    /// does: the person asked to be told. Unknown does not, for the rail's
    /// reason — "we could not tell" is not a yes.
    pub fn counted(self) -> bool {
        match self {
            Status::Lane(kind) => kind.counted(),
            Status::Woke => true,
            _ => false,
        }
    }
}

/// One row on one shelf.
#[derive(Clone, Debug)]
pub struct Row {
    pub fact: Fact,
    pub shelf: Shelf,
    pub status: Status,
    /// It came back from a snooze and has not been visited. Beside the
    /// status, because a woken row can also be asking something, and the
    /// question is the more useful word.
    pub woke: bool,
    /// The person's own date for this row: when a snooze wakes, or when the
    /// row was marked done.
    pub when: Option<SystemTime>,
}

/// Every agent pane, on its shelf, in its order.
#[derive(Clone, Debug, Default)]
pub struct Partition {
    pub look: Vec<Row>,
    pub working: Vec<Row>,
    pub snoozed: Vec<Row>,
    pub quiet: Vec<Row>,
    pub done: Vec<Row>,
}

impl Partition {
    pub fn shelf(&self, shelf: Shelf) -> &[Row] {
        match shelf {
            Shelf::LookAtMe => &self.look,
            Shelf::Working => &self.working,
            Shelf::Snoozed => &self.snoozed,
            Shelf::Quiet => &self.quiet,
            Shelf::Done => &self.done,
        }
    }

    /// How many rows want the person: the number the toggle carries.
    pub fn wanting(&self) -> usize {
        self.look.iter().filter(|r| r.status.counted()).count()
    }

    /// Every row, top shelf first.
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        Shelf::ALL.into_iter().flat_map(|s| self.shelf(s).iter())
    }

    pub fn is_empty(&self) -> bool {
        self.rows().next().is_none()
    }
}

/// A person said *not now*.
#[derive(Clone, Debug, PartialEq)]
pub struct Snooze {
    /// When it wakes, by the wall clock: a snooze survives a restart.
    pub until: SystemTime,
    /// When it was set, by the wall clock, for the record.
    pub since: SystemTime,
    /// When it was set, by this window's clock. Read from disk, the moment it
    /// was read. See the module's rule about a person's word.
    pub from: Instant,
}

/// A person said *done*.
#[derive(Clone, Debug, PartialEq)]
pub struct Done {
    pub at: SystemTime,
    pub from: Instant,
}

/// A snooze ran out, or the person woke it early.
#[derive(Clone, Debug, PartialEq)]
pub struct Woke {
    pub at: SystemTime,
    pub from: Instant,
}

/// What a person has said about the inbox, and nothing else.
///
/// Two of its fields are `Option` on purpose. `showing` and `order` are None
/// until somebody chooses, and the file writes nothing for None: a default
/// written to disk is indistinguishable from a decision the next time it is
/// read.
#[derive(Clone, Debug, Default)]
pub struct Store {
    pub snoozed: BTreeMap<Key, Snooze>,
    pub done: BTreeMap<Key, Done>,
    pub woke: BTreeMap<Key, Woke>,
    /// The folding shelves somebody has opened.
    pub open: BTreeSet<Shelf>,
    /// Whether the spine shows the inbox. `None`: nobody has chosen.
    pub showing: Option<bool>,
    pub order: Option<LookOrder>,
    dirty: bool,
}

impl Store {
    /// Snooze `key` until `until`. Takes it off DONE and clears a woke mark:
    /// the newest word wins.
    pub fn snooze(&mut self, key: Key, until: SystemTime, now: SystemTime, at: Instant) {
        self.done.remove(&key);
        self.woke.remove(&key);
        self.snoozed.insert(
            key,
            Snooze {
                until,
                since: now,
                from: at,
            },
        );
        self.dirty = true;
    }

    /// Bring a snoozed row back now. It lands WOKE on LOOK AT ME: somebody
    /// who reaches for a snoozed row wants to look at it.
    pub fn wake(&mut self, key: Key, now: SystemTime, at: Instant) {
        if self.snoozed.remove(&key).is_some() {
            self.woke.insert(key, Woke { at: now, from: at });
            self.dirty = true;
        }
    }

    /// Mark `key` done. Clears its snooze and its woke mark.
    pub fn mark_done(&mut self, key: Key, now: SystemTime, at: Instant) {
        self.snoozed.remove(&key);
        self.woke.remove(&key);
        self.done.insert(key, Done { at: now, from: at });
        self.dirty = true;
    }

    /// Take a row off DONE. It goes back to whichever shelf its state says.
    pub fn reopen(&mut self, key: Key) {
        if self.done.remove(&key).is_some() {
            self.dirty = true;
        }
    }

    /// The person looked at the pane. A woke mark has done its job.
    pub fn visited(&mut self, key: Key) {
        if self.woke.remove(&key).is_some() {
            self.dirty = true;
        }
    }

    pub fn is_open(&self, shelf: Shelf) -> bool {
        !shelf.folds() || self.open.contains(&shelf)
    }

    pub fn toggle_open(&mut self, shelf: Shelf) {
        if !shelf.folds() {
            return;
        }
        if !self.open.remove(&shelf) {
            self.open.insert(shelf);
        }
        self.dirty = true;
    }

    pub fn set_showing(&mut self, showing: bool) {
        if self.showing != Some(showing) {
            self.showing = Some(showing);
            self.dirty = true;
        }
    }

    pub fn set_order(&mut self, order: LookOrder) {
        if self.order != Some(order) {
            self.order = Some(order);
            self.dirty = true;
        }
    }

    /// Whether anything changed since the last call. The view saves on `true`.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Apply the clock to what the person said: snoozes that ran out wake,
    /// snoozes overtaken by a new question wake early, DONE marks overtaken by
    /// new activity lift, and marks for panes long gone are forgotten.
    pub fn advance(&mut self, facts: &[Fact], tracker: &mut Tracker, now: SystemTime, at: Instant) {
        let by_key: HashMap<Key, &Fact> = facts.iter().map(|f| (f.key, f)).collect();

        // A snooze whose hour has come.
        let due: Vec<Key> = self
            .snoozed
            .iter()
            .filter(|(_, s)| now >= s.until)
            .map(|(k, _)| *k)
            .collect();
        for key in due {
            self.snoozed.remove(&key);
            self.woke.insert(key, Woke { at: now, from: at });
            self.dirty = true;
        }

        // A snooze overtaken by something that cannot wait: T3's rule, and the
        // right one — *"It comes back early for an approval, a new question, a
        // failed run"*. Only something NEW: a question already on screen when
        // the snooze was set is the very thing the person snoozed.
        let early: Vec<Key> = self
            .snoozed
            .iter()
            .filter(|(key, s)| {
                by_key
                    .get(key)
                    .is_some_and(|f| urgent(f.lane) && f.changed.is_some_and(|c| c > s.from))
            })
            .map(|(k, _)| *k)
            .collect();
        for key in early {
            self.snoozed.remove(&key);
            self.woke.insert(key, Woke { at: now, from: at });
            self.dirty = true;
        }

        // DONE lifted by anything new: a lane that changed after the mark (a
        // new question, a new finish, a new failure), or the agent starting
        // work again. Unknown does not lift it — a screen that stopped being
        // readable is not the agent doing something.
        let lifted: Vec<Key> = self
            .done
            .iter()
            .filter(|(key, d)| {
                by_key.get(key).is_some_and(|f| {
                    let new_lane = f.lane.is_some_and(|l| l != AttentionKind::Unknown)
                        && f.changed.is_some_and(|c| c > d.from);
                    let started = tracker.started(**key).is_some_and(|s| s > d.from);
                    new_lane || started
                })
            })
            .map(|(k, _)| *k)
            .collect();
        for key in lifted {
            self.done.remove(&key);
            self.dirty = true;
        }

        // Forget what belongs to panes that have been gone a long while.
        let remembered: BTreeSet<Key> = self
            .snoozed
            .keys()
            .chain(self.done.keys())
            .chain(self.woke.keys())
            .copied()
            .collect();
        for key in remembered {
            if by_key.contains_key(&key) {
                tracker.missing.remove(&key);
                continue;
            }
            let since = *tracker.missing.entry(key).or_insert(at);
            // A window-only key can never come back.
            if !key.saved() || at.saturating_duration_since(since) >= FORGET_AFTER {
                self.snoozed.remove(&key);
                self.done.remove(&key);
                self.woke.remove(&key);
                tracker.missing.remove(&key);
                self.dirty = true;
            }
        }
    }
}

/// A lane that does not wait for a snooze to run out.
fn urgent(lane: Option<AttentionKind>) -> bool {
    matches!(
        lane,
        Some(AttentionKind::Access | AttentionKind::Decision | AttentionKind::Failure)
    )
}

/// What this window has watched each pane do, on its own clock.
///
/// The screen says whether an agent is working now; it does not say when it
/// started. That is this window's to notice, and only this window's: a start
/// observed by a window that has since been replaced is not one this window
/// saw, so nothing here is written down.
#[derive(Debug, Default)]
pub struct Tracker {
    seen: HashMap<Key, Seen>,
    missing: HashMap<Key, Instant>,
}

#[derive(Clone, Copy, Debug)]
struct Seen {
    working: bool,
    started: Option<Instant>,
    stopped: Option<Instant>,
}

impl Tracker {
    /// Note what each pane is doing at `at`. A pane seen for the first time
    /// has no start and no stop: being found working is not being seen to
    /// start.
    pub fn observe(&mut self, facts: &[Fact], at: Instant) {
        let mut next = HashMap::with_capacity(facts.len());
        for f in facts {
            let seen = match self.seen.get(&f.key) {
                None => Seen {
                    working: f.working,
                    started: None,
                    stopped: None,
                },
                Some(prev) => Seen {
                    working: f.working,
                    started: if f.working && !prev.working {
                        Some(at)
                    } else {
                        prev.started
                    },
                    stopped: if !f.working && prev.working {
                        Some(at)
                    } else {
                        prev.stopped
                    },
                },
            };
            next.insert(f.key, seen);
        }
        self.seen = next;
    }

    /// When the pane was last seen to start working.
    pub fn started(&self, key: Key) -> Option<Instant> {
        self.seen.get(&key).and_then(|s| s.started)
    }

    /// When the pane was last seen to stop working.
    pub fn stopped(&self, key: Key) -> Option<Instant> {
        self.seen.get(&key).and_then(|s| s.stopped)
    }

    /// Whether a pane that is not working any more still holds its WORKING
    /// seat. See [`WORKING_GRACE`].
    fn holding(&self, f: &Fact, at: Instant) -> bool {
        !f.working
            && f.lane.is_none()
            && self
                .stopped(f.key)
                .is_some_and(|t| at.saturating_duration_since(t) < WORKING_GRACE)
    }
}

/// Put every pane on its shelf. Reads the store and the tracker and changes
/// neither; [`partition`] runs the clock first.
pub fn shelve(facts: &[Fact], store: &Store, tracker: &Tracker, at: Instant) -> Partition {
    let mut p = Partition::default();
    for f in facts {
        // Unknown is never quiet: a pane nobody could describe takes the
        // unknown lane even if the caller forgot to give it one.
        let lane = match (f.lane, f.life) {
            (None, Life::Unknown) => Some(AttentionKind::Unknown),
            (lane, _) => lane,
        };
        // A shell is on no shelf, unless it asked: the rail shows that ask,
        // so the inbox does too.
        if f.life == Life::Shell && lane != Some(AttentionKind::Access) {
            continue;
        }
        let woke = store.woke.contains_key(&f.key);
        let row = |shelf, status, when| Row {
            fact: f.clone(),
            shelf,
            status,
            woke,
            when,
        };
        if let Some(d) = store.done.get(&f.key) {
            p.done.push(row(Shelf::Done, Status::Done, Some(d.at)));
        } else if let Some(s) = store.snoozed.get(&f.key) {
            p.snoozed
                .push(row(Shelf::Snoozed, Status::Snoozed, Some(s.until)));
        } else if let Some(lane) = lane {
            p.look.push(row(Shelf::LookAtMe, Status::Lane(lane), None));
        } else if woke {
            p.look.push(row(Shelf::LookAtMe, Status::Woke, None));
        } else if f.working || tracker.holding(f, at) {
            p.working.push(row(Shelf::Working, Status::Working, None));
        } else {
            let status = match f.life {
                Life::Resting => Status::Resting,
                Life::Asleep => Status::Asleep,
                _ => Status::Idle,
            };
            p.quiet.push(row(Shelf::Quiet, status, None));
        }
    }
    order_look(&mut p.look, store, store.order.unwrap_or_default());
    // WORKING: whoever started most recently first, the way T3 lists the
    // thread you last sent work to first. A start nobody saw sorts last.
    p.working.sort_by(|a, b| {
        newest_first(tracker.started(a.fact.key), tracker.started(b.fact.key))
            .then_with(|| a.fact.title.cmp(&b.fact.title))
    });
    // SNOOZED: whichever wakes soonest.
    p.snoozed.sort_by(|a, b| {
        a.when
            .cmp(&b.when)
            .then_with(|| a.fact.pane.cmp(&b.fact.pane))
    });
    // SLEEP & QUIET: awake before resting before asleep — how quickly a key
    // gets an answer — then most recently busy first.
    p.quiet.sort_by(|a, b| {
        quiet_rank(a.status)
            .cmp(&quiet_rank(b.status))
            .then_with(|| newest_first(tracker.stopped(a.fact.key), tracker.stopped(b.fact.key)))
            .then_with(|| a.fact.title.cmp(&b.fact.title))
    });
    // DONE: the most recent mark first.
    p.done.sort_by(|a, b| {
        b.when
            .cmp(&a.when)
            .then_with(|| a.fact.pane.cmp(&b.fact.pane))
    });
    p
}

/// Run the clock over the store, then shelve. The one entry point the view
/// calls each frame.
pub fn partition(
    facts: &[Fact],
    store: &mut Store,
    tracker: &mut Tracker,
    now: SystemTime,
    at: Instant,
) -> Partition {
    tracker.observe(facts, at);
    store.advance(facts, tracker, now, at);
    shelve(facts, store, tracker, at)
}

fn quiet_rank(status: Status) -> u8 {
    match status {
        Status::Idle => 0,
        Status::Resting => 1,
        _ => 2,
    }
}

/// Newest first, and an unseen moment after every seen one.
fn newest_first(a: Option<Instant>, b: Option<Instant>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Order LOOK AT ME.
///
/// Under [`LookOrder::Rail`] the lane rows go through `attention::project`,
/// the call the rail itself makes, so the comparator is not copied — a copy is
/// how two rankings start to drift. Woken rows with nothing to say go after
/// every known lane and before the unknown lane, oldest wake first; that keeps
/// the rail's relative order intact, since unknown is its last lane anyway.
fn order_look(look: &mut [Row], store: &Store, order: LookOrder) {
    match order {
        LookOrder::Rail => {
            let observations: Vec<attention::Observation> = look
                .iter()
                .filter_map(|r| match r.status {
                    Status::Lane(kind) => Some(attention::Observation {
                        pane: r.fact.pane,
                        pane_kind: match r.fact.life {
                            Life::Shell => attention::PaneKind::Shell,
                            Life::Unknown => attention::PaneKind::Unknown,
                            _ => attention::PaneKind::Agent,
                        },
                        priority: r.fact.priority,
                        kind: Some(kind),
                        origin: attention::Origin::default(),
                        observed_at: r.fact.changed,
                        evidence: None,
                        deliverable: None,
                        ask: None,
                    }),
                    _ => None,
                })
                .collect();
            let rank: HashMap<u64, usize> = attention::project(&observations)
                .iter()
                .enumerate()
                .map(|(i, item)| (item.pane, i))
                .collect();
            let woke_at = |r: &Row| store.woke.get(&r.fact.key).map(|w| w.from);
            look.sort_by(|a, b| {
                let tier = |r: &Row| match r.status {
                    Status::Lane(AttentionKind::Unknown) => 2,
                    Status::Lane(_) => 0,
                    _ => 1,
                };
                tier(a)
                    .cmp(&tier(b))
                    .then_with(|| match (a.status, b.status) {
                        (Status::Lane(_), Status::Lane(_)) => rank
                            .get(&a.fact.pane)
                            .unwrap_or(&usize::MAX)
                            .cmp(rank.get(&b.fact.pane).unwrap_or(&usize::MAX)),
                        _ => match (woke_at(a), woke_at(b)) {
                            (Some(x), Some(y)) => x.cmp(&y),
                            _ => a.fact.pane.cmp(&b.fact.pane),
                        },
                    })
            });
        }
        LookOrder::Returned => {
            // Whoever came back most recently, first: the later of the lane
            // changing and the snooze running out. Unknown stays last, since
            // a screen that cannot be read did not "come back".
            let back = |r: &Row| {
                let woke = store.woke.get(&r.fact.key).map(|w| w.from);
                match (r.fact.changed, woke) {
                    (Some(a), Some(b)) => Some(a.max(b)),
                    (a, b) => a.or(b),
                }
            };
            look.sort_by(|a, b| {
                let unknown = |r: &Row| r.status == Status::Lane(AttentionKind::Unknown);
                unknown(a)
                    .cmp(&unknown(b))
                    .then_with(|| newest_first(back(a), back(b)))
                    .then_with(|| a.fact.pane.cmp(&b.fact.pane))
            });
        }
    }
}

/// When a snooze choice wakes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wake {
    Hour,
    ThreeHours,
    /// Six this evening, offered only before five.
    Evening,
    /// Nine tomorrow morning.
    Morning,
    /// Nine next Monday.
    Monday,
}

impl Wake {
    pub const ALL: [Wake; 5] = [
        Wake::Hour,
        Wake::ThreeHours,
        Wake::Evening,
        Wake::Morning,
        Wake::Monday,
    ];

    /// When this choice wakes, from `now`. `None` when the choice does not
    /// apply now (this evening, after five) or the C library cannot place the
    /// moment, which the menu shows by leaving the choice out.
    pub fn at(self, now: SystemTime) -> Option<SystemTime> {
        match self {
            Wake::Hour => Some(now + Duration::from_secs(3600)),
            Wake::ThreeHours => Some(now + Duration::from_secs(3 * 3600)),
            Wake::Evening => {
                let (_, hour) = local_weekday_hour(now)?;
                (hour < 17).then(|| local_at(now, 0, 18, 0)).flatten()
            }
            Wake::Morning => local_at(now, 1, 9, 0),
            Wake::Monday => {
                let (weekday, _) = local_weekday_hour(now)?;
                // `tm_wday` counts from Sunday = 0. Today being Monday means
                // next week's: a snooze to "Monday" on a Monday morning that
                // woke an hour later would be a lie about what was chosen.
                let ahead = match (8 - weekday) % 7 {
                    0 => 7,
                    n => n,
                };
                local_at(now, ahead, 9, 0)
            }
        }
    }

    /// The choices that apply at `now`, soonest first.
    pub fn menu(now: SystemTime) -> Vec<(Wake, SystemTime)> {
        let mut out: Vec<(Wake, SystemTime)> = Wake::ALL
            .into_iter()
            .filter_map(|w| w.at(now).map(|t| (w, t)))
            .filter(|(_, t)| *t > now)
            .collect();
        out.sort_by_key(|(_, t)| *t);
        out
    }
}

/// The local weekday (Sunday = 0) and hour at `now`.
fn local_weekday_hour(now: SystemTime) -> Option<(i32, i32)> {
    let tm = local_tm(now)?;
    Some((tm.tm_wday, tm.tm_hour))
}

fn local_tm(now: SystemTime) -> Option<libc::tm> {
    let secs: libc::time_t = now
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs()
        .try_into()
        .ok()?;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: `localtime_r` is the reentrant form: it writes into the `tm` we
    // own and holds no state another thread could race for.
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return None;
    }
    Some(tm)
}

/// `days` ahead of `now`'s local date, at `hour:minute` local time. `mktime`
/// normalises a day past the month's end and works out daylight saving for
/// the target date itself (`tm_isdst = -1`), which is the whole reason this
/// goes through the C library rather than adding seconds: "nine tomorrow" is
/// not always twenty-four hours from nine today.
fn local_at(now: SystemTime, days: i32, hour: i32, minute: i32) -> Option<SystemTime> {
    let mut tm = local_tm(now)?;
    tm.tm_mday = tm.tm_mday.checked_add(days)?;
    tm.tm_hour = hour;
    tm.tm_min = minute;
    tm.tm_sec = 0;
    tm.tm_isdst = -1;
    // SAFETY: `mktime` reads and normalises the `tm` we own.
    let t = unsafe { libc::mktime(&mut tm) };
    if t == -1 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(u64::try_from(t).ok()?))
}

// ---- written down ----

/// Where this session's inbox is kept: beside the session's own state file,
/// in a directory of its own so a session's layout and its inbox can be
/// cleared independently.
pub fn path_for(config: &Path, session: &str) -> PathBuf {
    config.join("inbox").join(format!("{session}.json"))
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct File {
    v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    showing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    open: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    snoozed: BTreeMap<String, FileSnooze>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    done: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    woke: BTreeMap<String, u64>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FileSnooze {
    until_ms: u64,
    since_ms: u64,
}

fn ms(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn from_ms(ms: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms)
}

/// The store as JSON. Window-only keys are left out: they would mean nothing
/// to the next window.
pub fn encode(store: &Store) -> String {
    let file = File {
        v: 1,
        showing: store.showing,
        order: store.order.map(|o| o.word().to_string()),
        open: store.open.iter().map(|s| s.word().to_string()).collect(),
        snoozed: store
            .snoozed
            .iter()
            .filter_map(|(k, s)| {
                Some((
                    k.word()?,
                    FileSnooze {
                        until_ms: ms(s.until),
                        since_ms: ms(s.since),
                    },
                ))
            })
            .collect(),
        done: store
            .done
            .iter()
            .filter_map(|(k, d)| Some((k.word()?, ms(d.at))))
            .collect(),
        woke: store
            .woke
            .iter()
            .filter_map(|(k, w)| Some((k.word()?, ms(w.at))))
            .collect(),
    };
    serde_json::to_string_pretty(&file).unwrap_or_else(|_| "{\"v\":1}".into())
}

/// Read a store back. Every mark takes `at` as the moment it was given on this
/// window's clock — see the module's rule about a person's word. A file that
/// cannot be read is an empty store and an error the caller can show, never a
/// half-applied one.
pub fn decode(text: &str, at: Instant) -> Result<Store, String> {
    let file: File = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if file.v != 1 {
        return Err(format!(
            "inbox file version {} is newer than this window",
            file.v
        ));
    }
    let mut store = Store {
        showing: file.showing,
        order: file.order.as_deref().and_then(LookOrder::parse),
        open: file.open.iter().filter_map(|w| Shelf::parse(w)).collect(),
        ..Store::default()
    };
    for (word, s) in file.snoozed {
        if let Some(key) = Key::parse(&word) {
            store.snoozed.insert(
                key,
                Snooze {
                    until: from_ms(s.until_ms),
                    since: from_ms(s.since_ms),
                    from: at,
                },
            );
        }
    }
    for (word, at_ms) in file.done {
        if let Some(key) = Key::parse(&word) {
            store.done.insert(
                key,
                Done {
                    at: from_ms(at_ms),
                    from: at,
                },
            );
        }
    }
    for (word, at_ms) in file.woke {
        if let Some(key) = Key::parse(&word) {
            store.woke.insert(
                key,
                Woke {
                    at: from_ms(at_ms),
                    from: at,
                },
            );
        }
    }
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(n: u64) -> Fact {
        Fact {
            key: Key::Pane(n),
            pane: 1000 + n,
            life: Life::Awake,
            lane: None,
            working: false,
            priority: Priority::Neutral,
            changed: None,
            title: format!("pane {n}"),
            place: String::new(),
            lead: None,
            line: None,
        }
    }

    fn shelf_of(p: &Partition, n: u64) -> Option<Shelf> {
        p.rows()
            .find(|r| r.fact.key == Key::Pane(n))
            .map(|r| r.shelf)
    }

    fn look_order(p: &Partition) -> Vec<u64> {
        p.look
            .iter()
            .map(|r| match r.fact.key {
                Key::Pane(n) | Key::Window(n) => n,
            })
            .collect()
    }

    fn wall() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_800_000_000)
    }

    #[test]
    fn every_agent_pane_lands_on_exactly_one_shelf_and_a_shell_on_none() {
        let t0 = Instant::now();
        let mut asking = fact(1);
        asking.lane = Some(AttentionKind::Decision);
        asking.changed = Some(t0);
        let mut working = fact(2);
        working.working = true;
        let mut asleep = fact(3);
        asleep.life = Life::Asleep;
        let mut resting = fact(4);
        resting.life = Life::Resting;
        let idle = fact(5);
        let mut shell = fact(6);
        shell.life = Life::Shell;
        let facts = vec![asking, working, asleep, resting, idle, shell];
        let p = shelve(&facts, &Store::default(), &Tracker::default(), t0);
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
        assert_eq!(shelf_of(&p, 2), Some(Shelf::Working));
        assert_eq!(shelf_of(&p, 3), Some(Shelf::Quiet));
        assert_eq!(shelf_of(&p, 4), Some(Shelf::Quiet));
        assert_eq!(shelf_of(&p, 5), Some(Shelf::Quiet));
        assert_eq!(shelf_of(&p, 6), None, "a shell is not a conversation");
        assert_eq!(p.rows().count(), 5, "each agent pane once, no more");
    }

    #[test]
    fn a_shell_that_asked_for_access_is_on_look_at_me_as_the_rail_shows_it() {
        let mut shell = fact(1);
        shell.life = Life::Shell;
        shell.lane = Some(AttentionKind::Access);
        let p = shelve(
            &[shell],
            &Store::default(),
            &Tracker::default(),
            Instant::now(),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
    }

    #[test]
    fn unknown_is_never_quiet_and_never_counted() {
        let mut unknown = fact(1);
        unknown.life = Life::Unknown;
        // The caller forgot the lane: the module does not file it as idle.
        let p = shelve(
            &[unknown],
            &Store::default(),
            &Tracker::default(),
            Instant::now(),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
        assert_eq!(p.look[0].status, Status::Lane(AttentionKind::Unknown));
        assert_eq!(p.wanting(), 0, "we could not tell is not a yes");
    }

    #[test]
    fn look_at_me_is_ordered_by_the_rails_own_projection() {
        let t0 = Instant::now();
        let at = |s: u64| Some(t0 + Duration::from_secs(s));
        // A neutral decision, an older review, a promoted review, a failure,
        // and a newer decision: the rail sorts promoted, lane, oldest first.
        let rows = [
            (1, AttentionKind::Decision, Priority::Neutral, at(30)),
            (2, AttentionKind::ReviewReady, Priority::Neutral, at(5)),
            (3, AttentionKind::ReviewReady, Priority::Promoted, at(40)),
            (4, AttentionKind::Failure, Priority::Neutral, at(20)),
            (5, AttentionKind::Decision, Priority::Neutral, at(10)),
        ];
        let facts: Vec<Fact> = rows
            .iter()
            .map(|&(n, lane, priority, changed)| {
                let mut f = fact(n);
                f.lane = Some(lane);
                f.priority = priority;
                f.changed = changed;
                f
            })
            .collect();
        let p = shelve(&facts, &Store::default(), &Tracker::default(), t0);
        // Built independently from the rail's own call: the two must agree.
        let rail: Vec<u64> = attention::project(
            &facts
                .iter()
                .map(|f| attention::Observation {
                    pane: f.pane,
                    pane_kind: attention::PaneKind::Agent,
                    priority: f.priority,
                    kind: f.lane,
                    origin: attention::Origin::default(),
                    observed_at: f.changed,
                    evidence: None,
                    deliverable: None,
                    ask: None,
                })
                .collect::<Vec<_>>(),
        )
        .iter()
        .map(|i| i.pane - 1000)
        .collect();
        assert_eq!(look_order(&p), rail);
        assert_eq!(look_order(&p), vec![3, 5, 1, 4, 2]);
    }

    #[test]
    fn the_returned_order_puts_the_newest_return_first_and_unknown_last() {
        let t0 = Instant::now();
        let mut old = fact(1);
        old.lane = Some(AttentionKind::Decision);
        old.changed = Some(t0);
        let mut new = fact(2);
        new.lane = Some(AttentionKind::ReviewReady);
        new.changed = Some(t0 + Duration::from_secs(9));
        let mut unknown = fact(3);
        unknown.life = Life::Unknown;
        unknown.lane = Some(AttentionKind::Unknown);
        unknown.changed = Some(t0 + Duration::from_secs(99));
        let store = Store {
            order: Some(LookOrder::Returned),
            ..Store::default()
        };
        let p = shelve(&[old, unknown, new], &store, &Tracker::default(), t0);
        assert_eq!(look_order(&p), vec![2, 1, 3]);
    }

    #[test]
    fn a_woken_row_with_nothing_to_say_sits_after_the_known_lanes_and_before_unknown() {
        let t0 = Instant::now();
        let mut review = fact(1);
        review.lane = Some(AttentionKind::ReviewReady);
        review.changed = Some(t0);
        let woken = fact(2);
        let mut unknown = fact(3);
        unknown.life = Life::Unknown;
        let mut store = Store::default();
        store.woke.insert(
            Key::Pane(2),
            Woke {
                at: wall(),
                from: t0,
            },
        );
        let p = shelve(&[unknown, woken, review], &store, &Tracker::default(), t0);
        assert_eq!(look_order(&p), vec![1, 2, 3]);
        assert_eq!(p.look[1].status, Status::Woke);
        assert_eq!(p.wanting(), 2, "a woken row was asked for, so it counts");
    }

    #[test]
    fn a_snooze_hides_the_question_it_was_set_on_until_its_hour() {
        let t0 = Instant::now();
        let mut asking = fact(1);
        asking.lane = Some(AttentionKind::Decision);
        asking.changed = Some(t0);
        let facts = vec![asking];
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        store.snooze(
            Key::Pane(1),
            wall() + Duration::from_secs(3600),
            wall(),
            t0 + Duration::from_secs(1),
        );
        let p = partition(
            &facts,
            &mut store,
            &mut tracker,
            wall() + Duration::from_secs(60),
            t0 + Duration::from_secs(61),
        );
        assert_eq!(
            shelf_of(&p, 1),
            Some(Shelf::Snoozed),
            "the question it was set on does not wake it"
        );
        // The hour comes.
        let p = partition(
            &facts,
            &mut store,
            &mut tracker,
            wall() + Duration::from_secs(3600),
            t0 + Duration::from_secs(3601),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
        assert!(p.look[0].woke, "it comes back marked as woken");
        assert!(store.snoozed.is_empty());
    }

    #[test]
    fn a_new_question_wakes_a_snooze_early_and_a_new_finish_does_not() {
        let t0 = Instant::now();
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        store.snooze(Key::Pane(1), wall() + Duration::from_secs(3600), wall(), t0);
        store.snooze(Key::Pane(2), wall() + Duration::from_secs(3600), wall(), t0);
        let mut asking = fact(1);
        asking.lane = Some(AttentionKind::Decision);
        asking.changed = Some(t0 + Duration::from_secs(5));
        let mut finished = fact(2);
        finished.lane = Some(AttentionKind::ReviewReady);
        finished.changed = Some(t0 + Duration::from_secs(5));
        let p = partition(
            &[asking, finished],
            &mut store,
            &mut tracker,
            wall() + Duration::from_secs(10),
            t0 + Duration::from_secs(10),
        );
        assert_eq!(
            shelf_of(&p, 1),
            Some(Shelf::LookAtMe),
            "a question cannot wait out a snooze"
        );
        assert_eq!(shelf_of(&p, 2), Some(Shelf::Snoozed), "a finish can");
    }

    #[test]
    fn done_holds_for_the_turn_it_was_given_and_lifts_on_the_next() {
        let t0 = Instant::now();
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        let mut finished = fact(1);
        finished.lane = Some(AttentionKind::ReviewReady);
        finished.changed = Some(t0);
        let p = partition(&[finished.clone()], &mut store, &mut tracker, wall(), t0);
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
        store.mark_done(Key::Pane(1), wall(), t0 + Duration::from_secs(1));
        let p = partition(
            &[finished.clone()],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(2),
        );
        assert_eq!(
            shelf_of(&p, 1),
            Some(Shelf::Done),
            "the finish it was given on stays hidden"
        );
        // The person goes back in and gives it more work: it starts again.
        let mut busy = finished.clone();
        busy.lane = None;
        busy.working = true;
        let p = partition(
            &[busy],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(3),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::Working), "new work lifts DONE");
        assert!(store.done.is_empty());
    }

    #[test]
    fn a_new_finish_after_done_comes_back_to_look_at_me() {
        let t0 = Instant::now();
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        store.mark_done(Key::Pane(1), wall(), t0);
        let mut finished = fact(1);
        finished.lane = Some(AttentionKind::ReviewReady);
        finished.changed = Some(t0 + Duration::from_secs(4));
        let p = partition(
            &[finished],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(5),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
    }

    #[test]
    fn an_unreadable_screen_does_not_lift_done() {
        let t0 = Instant::now();
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        store.mark_done(Key::Pane(1), wall(), t0);
        let mut unknown = fact(1);
        unknown.life = Life::Unknown;
        unknown.lane = Some(AttentionKind::Unknown);
        unknown.changed = Some(t0 + Duration::from_secs(4));
        let p = partition(
            &[unknown],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(5),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::Done));
    }

    #[test]
    fn being_found_working_is_not_being_seen_to_start() {
        // A window that opens on a busy agent must not lift a DONE read from
        // disk: nobody saw that turn begin after the mark.
        let t0 = Instant::now();
        let text = encode(&{
            let mut s = Store::default();
            s.mark_done(Key::Pane(1), wall(), t0);
            s
        });
        let mut store = decode(&text, t0).expect("round trip");
        let mut tracker = Tracker::default();
        let mut busy = fact(1);
        busy.working = true;
        let p = partition(
            &[busy],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(1),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::Done));
    }

    #[test]
    fn a_pane_that_stops_working_holds_its_seat_for_the_grace_and_no_longer() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        let mut store = Store::default();
        let mut f = fact(1);
        f.working = false;
        partition(&[f.clone()], &mut store, &mut tracker, wall(), t0);
        f.working = true;
        partition(
            &[f.clone()],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(1),
        );
        f.working = false;
        let p = partition(
            &[f.clone()],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(2),
        );
        assert_eq!(
            shelf_of(&p, 1),
            Some(Shelf::Working),
            "one idle scan is not a finish"
        );
        let p = partition(
            &[f.clone()],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(5),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::Quiet));
        // Something to say goes up at once, grace or not.
        f.lane = Some(AttentionKind::ReviewReady);
        f.changed = Some(t0 + Duration::from_secs(6));
        f.working = true;
        partition(
            &[f.clone()],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(6),
        );
        f.working = false;
        let p = partition(
            &[f],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(7),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
    }

    #[test]
    fn working_lists_the_most_recent_start_first_and_an_unseen_start_last() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        let mut store = Store::default();
        let mut a = fact(1);
        let mut b = fact(2);
        let mut c = fact(3);
        c.working = true; // found working: no start seen
        partition(
            &[a.clone(), b.clone(), c.clone()],
            &mut store,
            &mut tracker,
            wall(),
            t0,
        );
        a.working = true;
        partition(
            &[a.clone(), b.clone(), c.clone()],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(1),
        );
        b.working = true;
        let p = partition(
            &[a, b, c],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(2),
        );
        let order: Vec<Key> = p.working.iter().map(|r| r.fact.key).collect();
        assert_eq!(order, vec![Key::Pane(2), Key::Pane(1), Key::Pane(3)]);
    }

    #[test]
    fn quiet_lists_awake_then_resting_then_asleep() {
        let mut asleep = fact(1);
        asleep.life = Life::Asleep;
        let mut resting = fact(2);
        resting.life = Life::Resting;
        let idle = fact(3);
        let p = shelve(
            &[asleep, resting, idle],
            &Store::default(),
            &Tracker::default(),
            Instant::now(),
        );
        let order: Vec<Status> = p.quiet.iter().map(|r| r.status).collect();
        assert_eq!(order, vec![Status::Idle, Status::Resting, Status::Asleep]);
    }

    #[test]
    fn waking_by_hand_lands_on_look_at_me_and_visiting_clears_it() {
        let t0 = Instant::now();
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        store.snooze(Key::Pane(1), wall() + Duration::from_secs(3600), wall(), t0);
        store.wake(Key::Pane(1), wall(), t0 + Duration::from_secs(1));
        let p = partition(
            &[fact(1)],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(2),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::LookAtMe));
        store.visited(Key::Pane(1));
        let p = partition(
            &[fact(1)],
            &mut store,
            &mut tracker,
            wall(),
            t0 + Duration::from_secs(3),
        );
        assert_eq!(shelf_of(&p, 1), Some(Shelf::Quiet));
    }

    #[test]
    fn the_newest_word_wins_between_snooze_and_done() {
        let t0 = Instant::now();
        let mut store = Store::default();
        store.mark_done(Key::Pane(1), wall(), t0);
        store.snooze(Key::Pane(1), wall() + Duration::from_secs(60), wall(), t0);
        assert!(store.done.is_empty());
        store.mark_done(Key::Pane(1), wall(), t0);
        assert!(store.snoozed.is_empty());
    }

    #[test]
    fn marks_for_a_pane_long_gone_are_forgotten_and_not_before() {
        let t0 = Instant::now();
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        store.mark_done(Key::Pane(1), wall(), t0);
        store.take_dirty();
        partition(&[], &mut store, &mut tracker, wall(), t0);
        partition(
            &[],
            &mut store,
            &mut tracker,
            wall(),
            t0 + FORGET_AFTER - Duration::from_secs(1),
        );
        assert!(
            store.done.contains_key(&Key::Pane(1)),
            "a window still being told about its panes keeps the mark"
        );
        partition(&[], &mut store, &mut tracker, wall(), t0 + FORGET_AFTER);
        assert!(store.done.is_empty());
        assert!(store.take_dirty());
    }

    #[test]
    fn a_pane_that_comes_back_resets_its_absence() {
        let t0 = Instant::now();
        let mut store = Store::default();
        let mut tracker = Tracker::default();
        store.mark_done(Key::Pane(1), wall(), t0);
        partition(&[], &mut store, &mut tracker, wall(), t0);
        partition(
            &[fact(1)],
            &mut store,
            &mut tracker,
            wall(),
            t0 + FORGET_AFTER / 2,
        );
        partition(&[], &mut store, &mut tracker, wall(), t0 + FORGET_AFTER);
        assert!(store.done.contains_key(&Key::Pane(1)));
    }

    #[test]
    fn the_store_round_trips_and_window_keys_are_never_written() {
        let t0 = Instant::now();
        let mut store = Store::default();
        store.snooze(Key::Pane(7), wall() + Duration::from_secs(90), wall(), t0);
        store.mark_done(Key::Pane(8), wall(), t0);
        store.mark_done(Key::Window(9), wall(), t0);
        store.woke.insert(
            Key::Pane(10),
            Woke {
                at: wall(),
                from: t0,
            },
        );
        store.toggle_open(Shelf::Done);
        store.set_showing(true);
        let text = encode(&store);
        let back = decode(&text, t0).expect("decode");
        assert_eq!(
            back.done.len(),
            1,
            "a window-only key means nothing to the next window: {text}"
        );
        assert_eq!(
            back.snoozed[&Key::Pane(7)].until,
            wall() + Duration::from_secs(90)
        );
        assert_eq!(back.done[&Key::Pane(8)].at, wall());
        assert!(!back.done.contains_key(&Key::Window(9)));
        assert!(back.woke.contains_key(&Key::Pane(10)));
        assert!(back.is_open(Shelf::Done));
        assert_eq!(back.showing, Some(true));
    }

    #[test]
    fn an_unchosen_setting_is_not_written_down() {
        let text = encode(&Store::default());
        assert!(!text.contains("showing"), "{text}");
        assert!(!text.contains("order"), "{text}");
        let back = decode(&text, Instant::now()).expect("decode");
        assert_eq!(back.showing, None);
        assert_eq!(back.order, None);
    }

    #[test]
    fn a_file_from_a_newer_window_is_refused_whole() {
        assert!(decode("{\"v\":2,\"done\":{\"pane:1\":5}}", Instant::now()).is_err());
        assert!(decode("not json", Instant::now()).is_err());
    }

    #[test]
    fn look_at_me_never_folds() {
        let mut store = Store::default();
        store.toggle_open(Shelf::LookAtMe);
        assert!(store.is_open(Shelf::LookAtMe));
        assert!(!store.is_open(Shelf::Working));
        store.toggle_open(Shelf::Working);
        assert!(store.is_open(Shelf::Working));
    }

    #[test]
    fn the_snooze_menu_is_in_the_future_and_soonest_first() {
        let now = SystemTime::now();
        let menu = Wake::menu(now);
        assert!(menu.iter().all(|(_, t)| *t > now));
        assert!(menu.windows(2).all(|w| w[0].1 <= w[1].1));
        assert_eq!(menu.first().map(|(w, _)| *w), Some(Wake::Hour));
        assert!(menu.iter().any(|(w, _)| *w == Wake::Morning));
    }

    #[test]
    fn morning_and_monday_land_at_nine_local() {
        let now = SystemTime::now();
        for w in [Wake::Morning, Wake::Monday] {
            let at = w.at(now).expect("placeable");
            let tm = local_tm(at).expect("local");
            assert_eq!((tm.tm_hour, tm.tm_min), (9, 0), "{w:?}");
            if w == Wake::Monday {
                assert_eq!(tm.tm_wday, 1);
                let gap = at.duration_since(now).expect("future");
                assert!(gap <= Duration::from_secs(8 * 24 * 3600));
            }
        }
    }
}
