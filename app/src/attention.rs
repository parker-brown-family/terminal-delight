//! The attention projection: of everything happening, what wants a human.
//!
//! Slice 1 of `docs/plans/attention-spine`. This module is the projection and
//! nothing else — it holds no UI, reads no pane, and opens no file. It takes
//! observations and returns the ordered list the right-edge rail renders, so the
//! ordering is a product decision with tests on it rather than a detail buried in
//! a paint function.
//!
//! Three rules from the plan live here, and each one is a test below:
//!
//! - **Unknown is not idle, and not zero.** An agent whose screen could not be
//!   read is its own kind, it is shown, and it never enters the count that says
//!   how many things want you. A shell is not an idle agent and produces nothing.
//! - **Unknown age is not zero age.** An item whose transition was never observed
//!   sorts *after* every item whose was, inside its own lane. It is not the
//!   newest and it is certainly not the oldest.
//! - **Lane before age.** A six-second failure outranks a forty-minute
//!   review-ready, because the lanes are ordered by what it costs to leave them
//!   alone.
//!
//! **What slice 2 must not do here.** The `kind` arrives already decided.
//! `agent_badge` in `main.rs` is the predicate that resolves needs-input,
//! working, bell and blocked into one answer, and it has the awkward cases in its
//! test table. Slice 2 feeds this module *from* that function. Adding a second
//! classifier in here would give one pane two rankings, and the tab badge is the
//! one a person's eye checks first.

use std::time::{Duration, Instant};

/// What is in the pane. A shell is a different observation from an agent at
/// rest, and an unreadable pane is a third thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneKind {
    Agent,
    Shell,
    /// The pane exists and nothing could be established about what runs in it.
    ///
    /// Not constructed today, and that is the honest state rather than an
    /// oversight: this process launches its own panes and knows what is in each
    /// one. It becomes reachable when a client is shown a pane the host owns and
    /// has not described. Slice 1's rotation synthesised it, which is why the
    /// dead-code warning only appears now that the rotation is gone.
    #[allow(dead_code)]
    Unknown,
}

/// The lanes, in the order they are shown. The derived `Ord` is the lane order,
/// which is why the variants are written in this sequence and not alphabetically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttentionKind {
    /// An agent is asking the human something and has stopped.
    Decision,
    /// A check, a rate limit or an API said no.
    Failure,
    /// Finished, with the completion not yet acknowledged.
    ReviewReady,
    /// A known agent whose state could not be read. Shown, never counted.
    ///
    /// Named `Unknown` rather than `Unclassified` at Parker's instruction —
    /// shorter, and the word the rest of software already uses. It shares a name
    /// with [`PaneKind::Unknown`] and means a different thing: that one is "we
    /// do not know what is in the pane", this one is "we know it is an agent and
    /// cannot read its state". Both are always written qualified.
    Unknown,
}

impl AttentionKind {
    /// Whether this lane contributes to the number on the closed spine.
    ///
    /// Unknown deliberately does not: the count answers "how many things
    /// want me", and "we could not tell" is not a yes. It carries its own
    /// neutral marker instead, so the fix for the idle fallback cannot inflate
    /// the red number.
    pub fn counted(self) -> bool {
        !matches!(self, AttentionKind::Unknown)
    }

    pub fn label(self) -> &'static str {
        match self {
            AttentionKind::Decision => "decision",
            AttentionKind::Failure => "failed",
            AttentionKind::ReviewReady => "review",
            AttentionKind::Unknown => "unknown",
        }
    }
}

/// A thumb on the scale: what the human has said matters today, independent of
/// what the agents are doing.
///
/// Set by right-clicking a project, an initiative or a task in the left bar, and
/// **inherited downward** — a promoted project promotes everything under it until
/// a nearer row says otherwise. It never travels up: `tree::Roll` carries agent
/// state from panes to tabs to projects, and this goes the other way and does not
/// roll at all, so a branch's arrow says what *it* is set to and never that
/// something inside it was promoted.
///
/// Resolution happens in the tree, before an observation is made. By the time a
/// pane reaches this module its effective level is a fact, not a search.
///
/// The derived order is the sort order: promoted first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Priority {
    /// Constructed by Slice 5, when the context menus can set a level. Until
    /// then every observation is Neutral and the compiler is right that nothing
    /// builds this — the ordering key landed early with Slice 1 deliberately, so
    /// the sort is already correct on the day levels arrive. Silenced rather
    /// than faked: the tracer's rotation used to construct it, and that was the
    /// only thing keeping this warning quiet.
    #[allow(dead_code)]
    Promoted,
    #[default]
    Neutral,
    /// See [`Priority::Promoted`] — same slice, same reason.
    #[allow(dead_code)]
    Demoted,
}

impl Priority {
    /// The mark in a row's badge line. Neutral draws nothing — a tree where every
    /// row carries a glyph is a tree where none of them mean anything.
    pub fn glyph(self) -> Option<&'static str> {
        match self {
            Priority::Promoted => Some("\u{25b2}"),
            Priority::Demoted => Some("\u{25bc}"),
            Priority::Neutral => None,
        }
    }
}

/// An effective level, and whether the row was told it or set it.
///
/// The two travel together because the mark is drawn differently for each —
/// dimmer when inherited — and a caller holding only the level would have to
/// re-derive the second half from the same three inputs, which is where the two
/// would eventually disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    pub priority: Priority,
    /// True when this row carries a level set on a branch ABOVE it.
    pub inherited: bool,
}

impl Level {
    /// The mark to draw, or nothing. Neutral draws nothing however it was
    /// arrived at: "explicitly neutral" and "nobody said" look identical on a
    /// row, and they should — the row is not promoted either way.
    pub fn glyph(self) -> Option<&'static str> {
        self.priority.glyph()
    }
}

impl Default for Level {
    fn default() -> Self {
        Level {
            priority: Priority::Neutral,
            inherited: false,
        }
    }
}

/// Resolve a row's level from the explicit settings above it: **nearest wins**.
///
/// Settled with the plan's fourth question: one noisy task can sit demoted
/// inside a promoted project, so a nearer setting overrules a further one rather
/// than combining with it. There is no arithmetic here on purpose — promoted
/// inside promoted is not doubly promoted, and a scheme where it were would
/// make a person's two deliberate statements produce a third they never made.
///
/// **Downward only.** A project that is promoted promotes what is under it; a
/// promoted task says nothing about its project. `tree::Roll` carries agent
/// state the other way, and mixing the two directions in one tree is how a
/// branch ends up claiming something nobody set on it.
///
/// Each argument is `Option<Priority>` and `None` means *unset*, which is a
/// different thing from `Some(Neutral)`: a person who explicitly neutralises a
/// task inside a promoted project is asking for that task to sit at neutral, and
/// an unset task is asking for nothing at all. Collapsing the two would make
/// "clear this" impossible to express.
pub fn resolve_level(
    task: Option<Priority>,
    initiative: Option<Priority>,
    project: Option<Priority>,
) -> Level {
    match (task, initiative, project) {
        (Some(p), _, _) => Level {
            priority: p,
            inherited: false,
        },
        (None, Some(p), _) | (None, None, Some(p)) => Level {
            priority: p,
            inherited: true,
        },
        (None, None, None) => Level::default(),
    }
}

/// Which task this row is about, and what it hangs from — read from the tree
/// rather than guessed from a path.
///
/// **`parent:task`, where the parent is simply the NEAREST one.** It was
/// `project:initiative`, which named two branches and never the task itself, so
/// a review row on the SKYTRAC tab under the JOB group read `—:JOB` — a dash
/// for a project that tab does not have, the group in the slot after it, and no
/// mention of the thing that actually finished. It now reads `JOB:SKYTRAC`.
///
/// Nearest, rather than a fixed rung, because the tree has three levels and a
/// task may hang from either of the two above it. A grouped task's parent is its
/// group; an ungrouped one's is its project. Asking for a specific rung means
/// picking a dash whenever the task does not use that rung, which is how the
/// first version came to print one so often.
///
/// Both halves stay optional, for different reasons: a task may hang from
/// nothing at all, and a task may simply not have been named. Neither renders as
/// an empty string, and an unnamed task shows a dash rather than its position —
/// a number in a name's place is a claim about identity that moves when somebody
/// reorders the strip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    /// The nearest branch above the task: its initiative, or its project when it
    /// belongs to no initiative.
    pub parent: Option<String>,
    /// The task's own name.
    pub task: Option<String>,
}

impl Origin {
    /// `JOB:SKYTRAC`, `JOB:—`, `—:—`. Three states, never a blank.
    pub fn label(&self) -> String {
        let dash = "\u{2014}";
        format!(
            "{}:{}",
            self.parent.as_deref().unwrap_or(dash),
            self.task.as_deref().unwrap_or(dash)
        )
    }
}

/// What a turn produced: the one artifact a person is meant to open.
///
/// Declared by the agent, never inferred from a filename — the links an agent
/// prints mix what it just made with everything it referenced, and guessing
/// between them is the kind of confident invention this whole surface refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliverable {
    /// What to call it on the row. Short enough for 300 pixels.
    pub label: String,
    /// An absolute path or a URL. Opened with the desktop's own handler, so a
    /// Markdown page goes wherever this machine has been told Markdown goes.
    pub href: String,
}

/// What kind of document a deliverable points at.
///
/// Only used to label the row — the opening itself is the desktop's decision,
/// not ours. A terminal that hard-wired its own viewer would override a choice
/// the person already made in their MIME database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Html,
    Markdown,
    Other,
}

impl DocKind {
    pub fn label(self) -> &'static str {
        match self {
            DocKind::Html => "html",
            DocKind::Markdown => "md",
            DocKind::Other => "open",
        }
    }
}

/// Classify by extension, ignoring any query or fragment.
///
/// Pure and tested, because the two shapes Parker names — an HTML report and a
/// Markdown doc — are the two a rail is most likely to be handed, and a row that
/// mislabels one is a row that lies about what a click will do.
pub fn doc_kind(href: &str) -> DocKind {
    let path = href
        .split(['?', '#'])
        .next()
        .unwrap_or(href)
        .trim_end_matches('/');
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") || lower.ends_with(".htm") {
        DocKind::Html
    } else if lower.ends_with(".md") || lower.ends_with(".markdown") {
        DocKind::Markdown
    } else {
        DocKind::Other
    }
}

/// One thing seen about one pane. Slice 1 synthesises these; slice 2 observes
/// them.
#[derive(Debug, Clone)]
pub struct Observation {
    pub pane: u64,
    pub pane_kind: PaneKind,
    /// Already resolved through the tree — see [`Priority`].
    pub priority: Priority,
    /// `None` for a pane that wants nothing — a working agent, a resting one, a
    /// shell. Such an observation produces no row.
    pub kind: Option<AttentionKind>,
    pub origin: Origin,
    /// Why this row exists, in words a person reads rather than a code.
    pub reason: String,
    /// When the state was seen to change. `None` means nothing has been seen to
    /// change yet, which is a different claim from "it changed just now".
    pub observed_at: Option<Instant>,
    /// Where the fact came from, shown on the row beside its time.
    pub source: &'static str,
    /// The line the classifier actually read. See [`AttentionItem::evidence`].
    pub evidence: Option<String>,
    /// What this turn produced, if the agent declared anything.
    pub deliverable: Option<Deliverable>,
}

/// A row in the queue.
#[derive(Debug, Clone)]
pub struct AttentionItem {
    pub pane: u64,
    pub priority: Priority,
    pub kind: AttentionKind,
    pub origin: Origin,
    pub reason: String,
    pub observed_at: Option<Instant>,
    pub source: &'static str,
    /// The line on the pane's screen that the classifier matched, captured at
    /// the moment it matched.
    ///
    /// `source` names the instrument; this is what the instrument read. A row
    /// saying "prompt · 4m" asks to be believed; a row that also shows
    /// `Do you want to proceed?` can be checked, and checked against the wrong
    /// pane is how a person catches a misfire in a second rather than by opening
    /// the terminal.
    ///
    /// `None` is a real answer and stays one. The unknown lane has no matched
    /// line by definition — the screen is precisely what could not be read — and
    /// a bell that rang on a clean finish matched no failure phrase either. The
    /// renderer draws the absence and never a placeholder sentence.
    pub evidence: Option<String>,
    /// Opened by a plain click, and never by focusing the pane. Reading a row
    /// and visiting its terminal are two different acts.
    pub deliverable: Option<Deliverable>,
}

impl AttentionItem {
    /// How long this has been waiting, or `None` when nothing was observed.
    ///
    /// The caller renders `None` as an absence. Returning `Duration::ZERO` here
    /// would be the same defect as the idle fallback: a value that reads as a
    /// measurement and is not one.
    pub fn age(&self, now: Instant) -> Option<Duration> {
        self.observed_at.map(|t| now.saturating_duration_since(t))
    }
}

/// What the closed spine shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Decision + failure + review-ready. The number a person reads as "things
    /// that want me".
    pub wanting: usize,
    /// Shown separately, and never folded into the one above.
    pub unknown: usize,
}

/// Turn observations into the ordered queue.
///
/// Shells and panes wanting nothing drop out. What is left is ordered by lane,
/// then oldest first, then by pane so the list is stable frame to frame.
pub fn project(observations: &[Observation]) -> Vec<AttentionItem> {
    let mut items: Vec<AttentionItem> = observations
        .iter()
        .filter(|o| o.pane_kind != PaneKind::Shell)
        .filter_map(|o| {
            o.kind.map(|kind| AttentionItem {
                pane: o.pane,
                priority: o.priority,
                kind,
                origin: o.origin.clone(),
                reason: o.reason.clone(),
                observed_at: o.observed_at,
                source: o.source,
                evidence: o.evidence.clone(),
                deliverable: o.deliverable.clone(),
            })
        })
        .collect();

    // What the human promoted comes first, then the lane, then the oldest — and
    // an item with no observed time sorts after every item that has one, because
    // we cannot claim it has waited.
    //
    // Priority sits ABOVE the lane deliberately: promoting a project is a person
    // saying "this is what I am doing today", and a surface that then buries it
    // under someone else's rate limit has overruled him with a heuristic.
    items.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.kind.cmp(&b.kind))
            .then_with(|| match (a.observed_at, b.observed_at) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.pane.cmp(&b.pane))
    });
    items
}

/// The closed spine's numbers, counted from the projected rows.
pub fn counts(items: &[AttentionItem]) -> Counts {
    let mut c = Counts::default();
    for it in items {
        if it.kind.counted() {
            c.wanting += 1;
        } else {
            c.unknown += 1;
        }
    }
    c
}

/// The counted lanes that are actually present, with how many rows each holds,
/// in lane order.
///
/// What the closed spine draws: one number per lane rather than a single total,
/// because a total plus a set of colours still cannot tell two-blocked-and-
/// two-review from one-blocked-and-three-review, and that is exactly the
/// question the spine exists to answer at a glance.
///
/// [`AttentionKind::Unknown`] is excluded — it is shown by its own marker and
/// never as a numeral. A count of things nobody could read, sitting in a row of
/// counts of things that want you, reads as a fourth kind of wanting.
///
/// An absent lane produces no entry rather than a zero: it has no rows in the
/// queue either, and a column of noughts is noise on a surface twenty pixels
/// wide.
pub fn lane_counts(items: &[AttentionItem]) -> Vec<(AttentionKind, usize)> {
    let mut lanes: Vec<(AttentionKind, usize)> = Vec::new();
    for it in items.iter().filter(|it| it.kind.counted()) {
        match lanes.iter_mut().find(|(k, _)| *k == it.kind) {
            Some((_, n)) => *n += 1,
            None => lanes.push((it.kind, 1)),
        }
    }
    lanes.sort_by_key(|(k, _)| *k);
    lanes
}

/// The order a person is currently looking at, held still while they look.
///
/// **The projection re-sorts every frame, and that is right until somebody is
/// reading it.** A queue ordered by urgency and age is a queue whose rows move:
/// a bell three panes away re-sorts the list under a cursor, and the row that
/// was second when the hand started moving is fourth when the key lands. The
/// ordering is not wrong — the moment to apply it is.
///
/// So the order is frozen when the queue opens and released when it closes.
/// While held, rows keep their positions, rows that leave simply vanish from
/// their slot, and **arrivals are appended at the bottom** rather than inserted
/// where their urgency says they belong. A row that jumped in above the cursor
/// would be exactly the defect this exists to prevent, and appending is visible:
/// a new decision at the bottom of a held list is one keystroke away and
/// obviously new, where a silently reordered list is neither.
///
/// `frozen` is a list of pane keys, not of rows: the queue re-projects from live
/// panes every frame and the held thing is the *sequence*, never a stale copy of
/// what the rows said. A held row therefore still ticks its age, still changes
/// lane, and still disappears the instant it stops wanting anything.
pub fn hold_order(frozen: &[u64], items: Vec<AttentionItem>) -> Vec<AttentionItem> {
    if frozen.is_empty() {
        return items;
    }
    let mut held: Vec<AttentionItem> = Vec::with_capacity(items.len());
    let mut rest: Vec<AttentionItem> = Vec::new();
    let mut pool: Vec<Option<AttentionItem>> = items.into_iter().map(Some).collect();
    for key in frozen {
        if let Some(slot) = pool
            .iter_mut()
            .find(|s| s.as_ref().is_some_and(|it| it.pane == *key))
        {
            held.push(slot.take().expect("just matched a present slot"));
        }
    }
    rest.extend(pool.into_iter().flatten());
    held.extend(rest);
    held
}

/// The keys of a queue, in the order it is currently drawn.
pub fn order_of(items: &[AttentionItem]) -> Vec<u64> {
    items.iter().map(|it| it.pane).collect()
}

/// What the painter records about the rows it drew: which pane, in which lane.
pub fn shown_keys(items: &[AttentionItem]) -> Vec<(u64, AttentionKind)> {
    items.iter().map(|it| (it.pane, it.kind)).collect()
}

/// Which rows the person has already looked at, and which arrived since.
///
/// **The bell's own contract, extended to the queue rather than duplicated.** A
/// finish bell is acknowledged by looking at the pane, because looking IS the
/// acknowledgement; a queue row is acknowledged by the queue having been open
/// while the row was in it, for the same reason and with the same failure mode
/// if we chose otherwise. A second, cleverer notion of seen — dwell time, a
/// hover, a scroll position — would be a claim about attention that a terminal
/// has no instrument to make.
///
/// Seen is keyed by **pane and lane together**, so a pane that was reviewed and
/// then blocks is unseen again. The pane has not changed; what it wants has, and
/// the row is about what it wants.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    marks: std::collections::HashMap<u64, AttentionKind>,
}

impl Seen {
    /// Has this row arrived, or changed what it wants, since the last look?
    pub fn is_unseen(&self, it: &AttentionItem) -> bool {
        self.marks.get(&it.pane) != Some(&it.kind)
    }

    /// How many of these rows the person has not looked at yet.
    pub fn unseen_count(&self, items: &[AttentionItem]) -> usize {
        items.iter().filter(|it| self.is_unseen(it)).count()
    }

    /// Record every row here as looked at. Called when the queue closes, not
    /// when it opens: the rows that were on screen for the duration are the ones
    /// that were seen, and a row that arrived while the panel was open is one of
    /// them.
    ///
    /// Takes the pairs the painter recorded rather than projected rows, because
    /// what was on screen is the claim being made. See `Workspace::rail_close`.
    pub fn mark(&mut self, shown: &[(u64, AttentionKind)]) {
        for (pane, kind) in shown {
            self.marks.insert(*pane, *kind);
        }
    }

    /// Forget panes that are no longer anywhere in the queue.
    ///
    /// Without this the map grows for the life of the window, and — worse than
    /// the memory — a pane that leaves the queue and comes back hours later
    /// returns already marked seen, so a genuinely new finish arrives wearing
    /// yesterday's acknowledgement. Called on the same edge as [`Self::mark`].
    pub fn retain_live(&mut self, shown: &[(u64, AttentionKind)]) {
        let live: std::collections::HashSet<u64> = shown.iter().map(|(p, _)| *p).collect();
        self.marks.retain(|k, _| live.contains(k));
    }
}

/// Where a key press moves the cursor, or `None` when the key is not ours.
///
/// **No wrapping, deliberately.** The list is ordered by what it costs to leave
/// a thing alone, so down from the last row landing on the first is the surface
/// quietly telling a person they have reached the end when they have reached the
/// beginning. A cursor that stops is a cursor you can hold a key against.
///
/// `end` on an empty queue is 0 rather than an underflow, and every caller
/// clamps against the live length anyway: the queue re-projects between
/// keystrokes and a row can leave it between one press and the next.
pub fn move_cursor(cursor: usize, len: usize, key: &str) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let last = len - 1;
    let next = match key {
        "down" | "j" | "tab" => cursor.saturating_add(1).min(last),
        "up" | "k" => cursor.saturating_sub(1),
        "home" => 0,
        "end" => last,
        // 1..9 jump straight to a row, the way the left bar's number keys do.
        // Out of range is ignored rather than clamped: pressing 7 on a queue of
        // three meant a row that is not there, and moving to the last one is a
        // different instruction than the one given.
        d if d.len() == 1 && d.chars().next().is_some_and(|c| ('1'..='9').contains(&c)) => {
            let i = d.chars().next().unwrap() as usize - '1' as usize;
            if i > last {
                return None;
            }
            i
        }
        _ => return None,
    };
    Some(next)
}

/// How long a row says it has waited, or a dash.
///
/// The dash is the point: a row whose transition was never observed renders an
/// absence, not a zero. Every caller goes through here so no surface can invent
/// one.
pub fn age_label(d: Option<Duration>) -> String {
    match d {
        None => "\u{2014}".to_string(),
        Some(d) => {
            let secs = d.as_secs();
            if secs < 60 {
                format!("{secs}s")
            } else if secs < 3600 {
                format!("{}m", secs / 60)
            } else {
                format!("{}h", secs / 3600)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(pane: u64, kind: Option<AttentionKind>, secs_ago: Option<u64>) -> Observation {
        Observation {
            pane,
            pane_kind: PaneKind::Agent,
            priority: Priority::Neutral,
            kind,
            origin: Origin {
                parent: Some("td".into()),
                task: Some("client-server".into()),
            },
            reason: "because".into(),
            observed_at: secs_ago.map(|s| Instant::now() - Duration::from_secs(s)),
            source: "pane screen",
            evidence: None,
            deliverable: None,
        }
    }

    /// A projected row, built straight rather than through an observation —
    /// the lifecycle functions take items and care about two fields.
    fn row(pane: u64, kind: AttentionKind) -> AttentionItem {
        AttentionItem {
            pane,
            priority: Priority::Neutral,
            kind,
            origin: Origin::default(),
            reason: String::new(),
            observed_at: None,
            source: "",
            evidence: None,
            deliverable: None,
        }
    }

    #[test]
    fn a_held_order_survives_a_reprojection_that_would_resort_it() {
        let frozen = vec![7, 3, 9];
        // The projection hands them back in its own order; the hold wins.
        let fresh = vec![
            row(9, AttentionKind::Decision),
            row(3, AttentionKind::Failure),
            row(7, AttentionKind::ReviewReady),
        ];
        assert_eq!(order_of(&hold_order(&frozen, fresh)), vec![7, 3, 9]);
    }

    #[test]
    fn a_row_that_arrives_while_the_queue_is_open_lands_at_the_bottom() {
        let frozen = vec![7, 3];
        let fresh = vec![
            // A decision outranks everything and would sort first.
            row(1, AttentionKind::Decision),
            row(3, AttentionKind::ReviewReady),
            row(7, AttentionKind::ReviewReady),
        ];
        assert_eq!(
            order_of(&hold_order(&frozen, fresh)),
            vec![7, 3, 1],
            "an arrival must never insert itself above the cursor"
        );
    }

    #[test]
    fn a_row_that_leaves_vacates_its_slot_and_moves_nobody_above_it() {
        let frozen = vec![7, 3, 9];
        let fresh = vec![
            row(7, AttentionKind::Decision),
            row(9, AttentionKind::Decision),
        ];
        assert_eq!(order_of(&hold_order(&frozen, fresh)), vec![7, 9]);
    }

    #[test]
    fn nothing_frozen_leaves_the_projection_exactly_as_it_came() {
        let fresh = vec![
            row(4, AttentionKind::Decision),
            row(2, AttentionKind::Failure),
        ];
        assert_eq!(order_of(&hold_order(&[], fresh)), vec![4, 2]);
    }

    #[test]
    fn a_frozen_key_for_a_pane_that_is_gone_is_simply_skipped() {
        // The pane closed while the queue was open. It must not leave a hole,
        // and it must not resurrect as an empty row.
        let frozen = vec![7, 42, 3];
        let fresh = vec![
            row(3, AttentionKind::Decision),
            row(7, AttentionKind::Decision),
        ];
        assert_eq!(order_of(&hold_order(&frozen, fresh)), vec![7, 3]);
    }

    #[test]
    fn everything_is_unseen_before_the_first_look() {
        let seen = Seen::default();
        let items = vec![
            row(1, AttentionKind::Decision),
            row(2, AttentionKind::Failure),
        ];
        assert_eq!(seen.unseen_count(&items), 2);
    }

    #[test]
    fn marking_a_queue_makes_its_rows_seen_and_nothing_else() {
        let mut seen = Seen::default();
        let items = vec![row(1, AttentionKind::Decision)];
        seen.mark(&shown_keys(&items));
        assert!(!seen.is_unseen(&items[0]));
        assert!(
            seen.is_unseen(&row(2, AttentionKind::Decision)),
            "a pane nobody has looked at is not seen because a neighbour was"
        );
    }

    #[test]
    fn a_seen_pane_that_changes_lane_is_unseen_again() {
        let mut seen = Seen::default();
        seen.mark(&shown_keys(&[row(1, AttentionKind::ReviewReady)]));
        assert!(
            seen.is_unseen(&row(1, AttentionKind::Failure)),
            "the pane is the same; what it wants is not, and the row is about what it wants"
        );
    }

    #[test]
    fn a_pane_that_leaves_the_queue_entirely_comes_back_unseen() {
        let mut seen = Seen::default();
        let looked_at = vec![
            row(1, AttentionKind::ReviewReady),
            row(2, AttentionKind::Decision),
        ];
        seen.mark(&shown_keys(&looked_at));
        // Pane 1 was dealt with and dropped out; only pane 2 is still queued.
        seen.retain_live(&shown_keys(&looked_at[1..]));
        assert!(
            seen.is_unseen(&row(1, AttentionKind::ReviewReady)),
            "an hour later this is a new finish, not the one already acknowledged"
        );
        assert!(!seen.is_unseen(&looked_at[1]));
    }

    #[test]
    fn the_cursor_stops_at_both_ends_rather_than_wrapping() {
        assert_eq!(move_cursor(0, 3, "up"), Some(0));
        assert_eq!(move_cursor(2, 3, "down"), Some(2));
        assert_eq!(move_cursor(0, 3, "down"), Some(1));
        assert_eq!(move_cursor(2, 3, "up"), Some(1));
    }

    #[test]
    fn the_cursor_declines_every_key_that_is_not_its_own() {
        for key in ["a", "left", "right", "f1", "enter", "escape", "0"] {
            assert_eq!(
                move_cursor(1, 5, key),
                None,
                "{key} is not a navigation key"
            );
        }
    }

    #[test]
    fn a_number_key_jumps_to_that_row_and_is_ignored_past_the_end() {
        assert_eq!(move_cursor(0, 5, "3"), Some(2));
        assert_eq!(move_cursor(0, 5, "1"), Some(0));
        assert_eq!(
            move_cursor(0, 3, "7"),
            None,
            "7 on a queue of three named a row that is not there — do not clamp it to the last"
        );
    }

    #[test]
    fn an_empty_queue_takes_no_cursor_at_all() {
        for key in ["down", "up", "home", "end", "1"] {
            assert_eq!(move_cursor(0, 0, key), None, "{key} on an empty queue");
        }
    }

    #[test]
    fn the_nearest_explicit_level_wins_over_every_further_one() {
        use Priority::*;
        // A noisy task, demoted, inside a project the person promoted.
        let lvl = resolve_level(Some(Demoted), None, Some(Promoted));
        assert_eq!(lvl.priority, Demoted);
        assert!(!lvl.inherited, "the task was told to be this, by name");
        // And an initiative beats the project above it.
        assert_eq!(
            resolve_level(None, Some(Demoted), Some(Promoted)).priority,
            Demoted
        );
    }

    #[test]
    fn a_level_travels_down_and_is_marked_as_inherited() {
        use Priority::*;
        let lvl = resolve_level(None, None, Some(Promoted));
        assert_eq!(lvl.priority, Promoted);
        assert!(lvl.inherited, "nobody set this on the task itself");
        let from_group = resolve_level(None, Some(Promoted), None);
        assert!(from_group.inherited);
    }

    #[test]
    fn nothing_set_anywhere_is_neutral_and_not_inherited() {
        let lvl = resolve_level(None, None, None);
        assert_eq!(lvl.priority, Priority::Neutral);
        assert!(!lvl.inherited);
        assert_eq!(lvl.glyph(), None, "a neutral row carries no mark");
    }

    #[test]
    fn an_explicit_neutral_is_not_the_same_as_unset() {
        use Priority::*;
        // The person cleared this one task inside a promoted project. That is an
        // instruction, and it must not fall through to the project's level.
        let cleared = resolve_level(Some(Neutral), None, Some(Promoted));
        assert_eq!(cleared.priority, Neutral);
        assert!(!cleared.inherited);
        // Where nobody said anything, the project's level does travel.
        let unset = resolve_level(None, None, Some(Promoted));
        assert_eq!(unset.priority, Promoted);
    }

    #[test]
    fn levels_do_not_compound_when_two_rungs_agree() {
        use Priority::*;
        // Promoted inside promoted is promoted. There is no louder state, and
        // inventing one would make two deliberate statements produce a third.
        assert_eq!(
            resolve_level(Some(Promoted), Some(Promoted), Some(Promoted)).priority,
            Promoted
        );
    }

    #[test]
    fn the_two_marks_are_the_two_levels_and_neutral_draws_nothing() {
        assert_eq!(Priority::Promoted.glyph(), Some("\u{25b2}"));
        assert_eq!(Priority::Demoted.glyph(), Some("\u{25bc}"));
        assert_eq!(Priority::Neutral.glyph(), None);
    }

    #[test]
    fn home_and_end_reach_the_ends_of_a_long_queue() {
        assert_eq!(move_cursor(9, 40, "home"), Some(0));
        assert_eq!(move_cursor(9, 40, "end"), Some(39));
    }

    #[test]
    fn lane_beats_age() {
        let items = project(&[
            obs(1, Some(AttentionKind::ReviewReady), Some(2400)),
            obs(2, Some(AttentionKind::Failure), Some(6)),
            obs(3, Some(AttentionKind::Decision), Some(60)),
        ]);
        let lanes: Vec<_> = items.iter().map(|i| i.kind).collect();
        assert_eq!(
            lanes,
            vec![
                AttentionKind::Decision,
                AttentionKind::Failure,
                AttentionKind::ReviewReady
            ],
            "a six-second failure outranks a forty-minute review"
        );
    }

    #[test]
    fn oldest_first_inside_a_lane() {
        let items = project(&[
            obs(1, Some(AttentionKind::Decision), Some(10)),
            obs(2, Some(AttentionKind::Decision), Some(2400)),
            obs(3, Some(AttentionKind::Decision), Some(600)),
        ]);
        assert_eq!(
            items.iter().map(|i| i.pane).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    /// Three rows, and both input orders, deliberately.
    ///
    /// Two earlier versions of this test passed against a comparator that sorted
    /// unobserved items *first*. Swapping that arm makes the ordering
    /// inconsistent rather than reversed, and an insertion sort only ever asks
    /// "is this later element smaller", so it never reached the broken arm: the
    /// unobserved row got pushed to the back by the comparisons that still
    /// worked. Feeding the same three rows in both orders forces the arm to run
    /// and pins the real property — the queue does not depend on the order
    /// observations arrive in.
    #[test]
    fn unknown_age_sorts_last_in_its_lane_whatever_order_it_arrives_in() {
        let unobserved = obs(1, Some(AttentionKind::Decision), None);
        let recent = obs(2, Some(AttentionKind::Decision), Some(5));
        let old = obs(3, Some(AttentionKind::Decision), Some(600));

        for arrival in [
            vec![unobserved.clone(), recent.clone(), old.clone()],
            vec![recent.clone(), old.clone(), unobserved.clone()],
        ] {
            let items = project(&arrival);
            assert_eq!(
                items.iter().map(|i| i.pane).collect::<Vec<_>>(),
                vec![3, 2, 1],
                "oldest observed first, then the unobserved row, in every arrival order"
            );
            assert_eq!(
                items[2].age(Instant::now()),
                None,
                "and the unobserved row never reports an age"
            );
        }
    }

    #[test]
    fn unknown_is_shown_but_never_counted() {
        let items = project(&[
            obs(1, Some(AttentionKind::Decision), Some(5)),
            obs(2, Some(AttentionKind::Unknown), Some(5)),
            obs(3, Some(AttentionKind::Unknown), None),
        ]);
        assert_eq!(items.len(), 3, "an unreadable agent still gets a row");
        let c = counts(&items);
        assert_eq!(c.wanting, 1);
        assert_eq!(c.unknown, 2, "and it is counted where it cannot inflate");
    }

    /// The spine's numbers: one per lane present, in lane order, and the
    /// distinction the old single total could not make. Two blocked and two
    /// review must not look like one blocked and three review, which is what a
    /// count of four plus a pair of colours amounts to.
    #[test]
    fn the_spine_counts_each_lane_separately_and_in_lane_order() {
        let items = project(&[
            obs(1, Some(AttentionKind::ReviewReady), Some(5)),
            obs(2, Some(AttentionKind::Decision), Some(5)),
            obs(3, Some(AttentionKind::ReviewReady), Some(5)),
            obs(4, Some(AttentionKind::Decision), Some(5)),
        ]);
        assert_eq!(
            lane_counts(&items),
            vec![
                (AttentionKind::Decision, 2),
                (AttentionKind::ReviewReady, 2)
            ],
            "two of each, loudest lane first, and failure absent rather than zero"
        );
    }

    /// The unreadable lane never appears as a numeral beside the counted ones.
    /// It has its own marker, because a number saying eighteen next to a number
    /// saying two invites reading both as things that want you — and the whole
    /// point of the split is that one of them does not.
    #[test]
    fn the_spine_never_gives_the_unknown_lane_a_number() {
        let items = project(&[
            obs(1, Some(AttentionKind::Unknown), None),
            obs(2, Some(AttentionKind::Unknown), None),
            obs(3, Some(AttentionKind::ReviewReady), Some(5)),
        ]);
        assert_eq!(
            lane_counts(&items),
            vec![(AttentionKind::ReviewReady, 1)],
            "eighteen unreadable panes must not become an eighteen in the pill"
        );
        assert_eq!(
            counts(&items).unknown,
            2,
            "it is still known, just not there"
        );
    }

    /// Nothing waiting produces no lanes at all rather than three zeroes — the
    /// caller draws a single dim nought, because an empty pill and a pill that
    /// was never computed look the same and only one of them is true.
    #[test]
    fn a_quiet_fleet_produces_no_lanes_rather_than_zeroes() {
        let items = project(&[obs(1, None, None), obs(2, None, None)]);
        assert!(items.is_empty());
        assert!(lane_counts(&items).is_empty());
    }

    #[test]
    fn unknown_sits_below_every_real_lane() {
        let items = project(&[
            obs(1, Some(AttentionKind::Unknown), Some(9999)),
            obs(2, Some(AttentionKind::ReviewReady), Some(1)),
        ]);
        assert_eq!(items[0].kind, AttentionKind::ReviewReady);
    }

    #[test]
    fn a_shell_is_not_an_idle_agent() {
        let mut shell = obs(1, Some(AttentionKind::Unknown), Some(5));
        shell.pane_kind = PaneKind::Shell;
        assert!(
            project(&[shell]).is_empty(),
            "a shell produces no row even when something classified it"
        );
    }

    #[test]
    fn a_pane_wanting_nothing_produces_no_row() {
        assert!(project(&[obs(1, None, Some(5))]).is_empty());
    }

    #[test]
    fn an_unknown_pane_kind_still_gets_its_row() {
        let mut unknown = obs(1, Some(AttentionKind::Unknown), Some(5));
        unknown.pane_kind = PaneKind::Unknown;
        assert_eq!(project(&[unknown]).len(), 1);
    }

    #[test]
    fn origin_renders_three_states_and_never_a_blank() {
        let named = Origin {
            parent: Some("JOB".into()),
            task: Some("SKYTRAC".into()),
        };
        let unnamed_task = Origin {
            parent: Some("JOB".into()),
            task: None,
        };
        let neither = Origin::default();
        assert_eq!(named.label(), "JOB:SKYTRAC");
        assert_eq!(
            unnamed_task.label(),
            "JOB:\u{2014}",
            "an unnamed task shows a dash, never its position — a number in a \
             name's place moves when somebody reorders the strip"
        );
        assert_eq!(neither.label(), "\u{2014}:\u{2014}");
        for o in [&named, &unnamed_task, &neither] {
            assert!(!o.label().contains("::"), "no half is ever empty");
        }
    }

    #[test]
    fn order_is_stable_when_two_rows_tie() {
        let items = project(&[
            obs(7, Some(AttentionKind::Decision), None),
            obs(3, Some(AttentionKind::Decision), None),
        ]);
        assert_eq!(items.iter().map(|i| i.pane).collect::<Vec<_>>(), vec![3, 7]);
    }

    fn at(pane: u64, p: Priority, kind: AttentionKind, secs_ago: u64) -> Observation {
        let mut o = obs(pane, Some(kind), Some(secs_ago));
        o.priority = p;
        o
    }

    #[test]
    fn a_promoted_review_outranks_a_neutral_decision() {
        let items = project(&[
            at(1, Priority::Neutral, AttentionKind::Decision, 60),
            at(2, Priority::Promoted, AttentionKind::ReviewReady, 60),
        ]);
        assert_eq!(
            items.iter().map(|i| i.pane).collect::<Vec<_>>(),
            vec![2, 1],
            "what the human promoted comes before what the heuristic ranked highest"
        );
    }

    #[test]
    fn a_demoted_decision_sinks_below_a_neutral_review() {
        let items = project(&[
            at(1, Priority::Demoted, AttentionKind::Decision, 60),
            at(2, Priority::Neutral, AttentionKind::ReviewReady, 60),
        ]);
        assert_eq!(items.iter().map(|i| i.pane).collect::<Vec<_>>(), vec![2, 1]);
    }

    #[test]
    fn a_demoted_row_is_still_shown_and_still_counted() {
        let items = project(&[at(1, Priority::Demoted, AttentionKind::Decision, 60)]);
        assert_eq!(items.len(), 1, "demoting is an ordering, never a mute");
        assert_eq!(counts(&items).wanting, 1);
    }

    #[test]
    fn inside_one_level_the_lane_order_still_holds() {
        let items = project(&[
            at(1, Priority::Promoted, AttentionKind::ReviewReady, 60),
            at(2, Priority::Promoted, AttentionKind::Decision, 60),
            at(3, Priority::Promoted, AttentionKind::Failure, 60),
        ]);
        assert_eq!(
            items.iter().map(|i| i.pane).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn priority_orders_before_age_within_one_lane() {
        let items = project(&[
            at(1, Priority::Neutral, AttentionKind::Decision, 3600),
            at(2, Priority::Promoted, AttentionKind::Decision, 5),
        ]);
        assert_eq!(
            items.iter().map(|i| i.pane).collect::<Vec<_>>(),
            vec![2, 1],
            "an hour of waiting does not outrank what he said matters today"
        );
    }

    #[test]
    fn a_deliverable_is_labelled_by_what_it_is() {
        assert_eq!(
            doc_kind("/home/p/reports/2026-09-15-thing.html"),
            DocKind::Html
        );
        assert_eq!(doc_kind("/home/p/docs/plan.md"), DocKind::Markdown);
        assert_eq!(doc_kind("/home/p/docs/PLAN.MD"), DocKind::Markdown);
        assert_eq!(doc_kind("notes.markdown"), DocKind::Markdown);
        assert_eq!(doc_kind("http://127.0.0.1:8753/a.html?v=2"), DocKind::Html);
        assert_eq!(doc_kind("http://127.0.0.1:8753/a.html#top"), DocKind::Html);
        assert_eq!(
            doc_kind("https://github.com/x/y/pull/419"),
            DocKind::Other,
            "a pull request is openable and is not a document we can name"
        );
        assert_eq!(doc_kind("/tmp/out.log"), DocKind::Other);
    }

    #[test]
    fn a_row_without_a_declared_deliverable_offers_nothing() {
        let items = project(&[obs(1, Some(AttentionKind::ReviewReady), Some(5))]);
        assert!(
            items[0].deliverable.is_none(),
            "nothing is inferred for a turn that declared nothing"
        );
    }

    #[test]
    fn a_declared_deliverable_survives_the_projection() {
        let mut o = obs(1, Some(AttentionKind::ReviewReady), Some(5));
        o.deliverable = Some(Deliverable {
            label: "The Projection".into(),
            href: "/home/parker/Work/reports/2026-09-15-slice1-projection.html".into(),
        });
        let items = project(&[o]);
        let d = items[0].deliverable.as_ref().expect("carried through");
        assert_eq!(d.label, "The Projection");
        assert_eq!(doc_kind(&d.href), DocKind::Html);
    }

    #[test]
    fn an_unobserved_row_renders_a_dash_and_never_a_zero() {
        assert_eq!(age_label(None), "\u{2014}");
        assert_eq!(age_label(Some(Duration::from_secs(0))), "0s");
        assert_eq!(age_label(Some(Duration::from_secs(41 * 60))), "41m");
        assert_eq!(age_label(Some(Duration::from_secs(2 * 3600))), "2h");
    }

    #[test]
    fn neutral_is_the_default_and_changes_nothing() {
        assert_eq!(Priority::default(), Priority::Neutral);
        assert_eq!(
            Priority::Neutral.glyph(),
            None,
            "a neutral row carries no mark"
        );
        assert!(Priority::Promoted.glyph().is_some());
        assert!(Priority::Demoted.glyph().is_some());
    }
}
