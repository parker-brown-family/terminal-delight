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

/// Where the work came from, read from the tree rather than guessed from a path.
///
/// Both halves are optional and they are optional for different reasons: a tab
/// may belong to no project at all, and a project may exist without having been
/// named yet. Neither renders as an empty string.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    pub project: Option<String>,
    pub initiative: Option<String>,
}

impl Origin {
    /// `td:client-server`, `td:—`, `—:—`. Three states, never a blank.
    pub fn label(&self) -> String {
        let dash = "\u{2014}";
        format!(
            "{}:{}",
            self.project.as_deref().unwrap_or(dash),
            self.initiative.as_deref().unwrap_or(dash)
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
                project: Some("td".into()),
                initiative: Some("client-server".into()),
            },
            reason: "because".into(),
            observed_at: secs_ago.map(|s| Instant::now() - Duration::from_secs(s)),
            source: "pane screen",
            deliverable: None,
        }
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
            project: Some("td".into()),
            initiative: Some("left-bar".into()),
        };
        let no_initiative = Origin {
            project: Some("td".into()),
            initiative: None,
        };
        let neither = Origin::default();
        assert_eq!(named.label(), "td:left-bar");
        assert_eq!(no_initiative.label(), "td:\u{2014}");
        assert_eq!(neither.label(), "\u{2014}:\u{2014}");
        for o in [&named, &no_initiative, &neither] {
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
