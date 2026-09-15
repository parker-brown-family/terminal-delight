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
    Unclassified,
}

impl AttentionKind {
    /// Whether this lane contributes to the number on the closed spine.
    ///
    /// Unclassified deliberately does not: the count answers "how many things
    /// want me", and "we could not tell" is not a yes. It carries its own
    /// neutral marker instead, so the fix for the idle fallback cannot inflate
    /// the red number.
    pub fn counted(self) -> bool {
        !matches!(self, AttentionKind::Unclassified)
    }

    pub fn label(self) -> &'static str {
        match self {
            AttentionKind::Decision => "decision",
            AttentionKind::Failure => "failed",
            AttentionKind::ReviewReady => "review",
            AttentionKind::Unclassified => "unclassified",
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

/// One thing seen about one pane. Slice 1 synthesises these; slice 2 observes
/// them.
#[derive(Debug, Clone)]
pub struct Observation {
    pub pane: u64,
    pub pane_kind: PaneKind,
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
}

/// A row in the queue.
#[derive(Debug, Clone)]
pub struct AttentionItem {
    pub pane: u64,
    pub kind: AttentionKind,
    pub origin: Origin,
    pub reason: String,
    pub observed_at: Option<Instant>,
    pub source: &'static str,
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
    pub unclassified: usize,
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
                kind,
                origin: o.origin.clone(),
                reason: o.reason.clone(),
                observed_at: o.observed_at,
                source: o.source,
            })
        })
        .collect();

    // Lane first. Then oldest first — and an item with no observed time sorts
    // after every item that has one, because we cannot claim it has waited.
    items.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
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
            c.unclassified += 1;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(pane: u64, kind: Option<AttentionKind>, secs_ago: Option<u64>) -> Observation {
        Observation {
            pane,
            pane_kind: PaneKind::Agent,
            kind,
            origin: Origin {
                project: Some("td".into()),
                initiative: Some("client-server".into()),
            },
            reason: "because".into(),
            observed_at: secs_ago.map(|s| Instant::now() - Duration::from_secs(s)),
            source: "pane screen",
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
        assert_eq!(items.iter().map(|i| i.pane).collect::<Vec<_>>(), vec![2, 3, 1]);
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
    fn unclassified_is_shown_but_never_counted() {
        let items = project(&[
            obs(1, Some(AttentionKind::Decision), Some(5)),
            obs(2, Some(AttentionKind::Unclassified), Some(5)),
            obs(3, Some(AttentionKind::Unclassified), None),
        ]);
        assert_eq!(items.len(), 3, "an unreadable agent still gets a row");
        let c = counts(&items);
        assert_eq!(c.wanting, 1);
        assert_eq!(c.unclassified, 2, "and it is counted where it cannot inflate");
    }

    #[test]
    fn unclassified_sits_below_every_real_lane() {
        let items = project(&[
            obs(1, Some(AttentionKind::Unclassified), Some(9999)),
            obs(2, Some(AttentionKind::ReviewReady), Some(1)),
        ]);
        assert_eq!(items[0].kind, AttentionKind::ReviewReady);
    }

    #[test]
    fn a_shell_is_not_an_idle_agent() {
        let mut shell = obs(1, Some(AttentionKind::Unclassified), Some(5));
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
        let mut unknown = obs(1, Some(AttentionKind::Unclassified), Some(5));
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
}
