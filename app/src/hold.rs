//! What a deleted branch is while it is still coming back.
//!
//! Deleting a group in the left bar does not end its shells. It moves them
//! here, where they keep running until either somebody asks for them back or
//! the window closes on them. Nothing in this module touches a process: it
//! holds a payload it never looks inside, and the shells stay alive for exactly
//! as long as this module keeps that payload from being dropped. That is the
//! whole mechanism, and it is why the module is generic — the policy below can
//! be tested to the corner without a gpui `Window` anywhere near it.
//!
//! The numbers are the ones approved on 2026-09-09 for close-undo: one pane
//! held an hour, two or more held four, counted in panes rather than in the
//! verb that closed them, capped at the ten most recent. A group is two or more
//! panes in practice, so a deleted group inherits four hours without anyone
//! having to decide again.

use std::time::{Duration, Instant};

/// Holdings kept at once. The eleventh evicts the oldest, whose payload is
/// returned so the caller can drop it — which is what ends those shells.
///
/// A cap as well as a timer, because ordinary churn would otherwise hold a
/// hundred process groups against a four-hour clock.
pub const CAP: usize = 10;

/// A lone pane is cheap to recreate and cheap to lose track of.
pub const SHORT: Duration = Duration::from_secs(60 * 60);

/// Two or more panes is somebody's working set.
pub const LONG: Duration = Duration::from_secs(4 * 60 * 60);

/// How long a holding of `panes` panes survives.
pub fn window_for(panes: usize) -> Duration {
    if panes <= 1 {
        SHORT
    } else {
        LONG
    }
}

/// Which layer of the tree was deleted. The tray says so, because recovering a
/// project and recovering one tab are not the same offer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Project,
    Initiative,
    Task,
    /// One pane out of a split — the only kind that can come back somewhere
    /// other than where it left, because a split reshapes the moment it leaves.
    Pane,
}

impl Kind {
    /// What a tray row calls this, in the words the left bar already uses.
    ///
    /// The tray offers projects, groups, tabs and panes in one list, and
    /// `Recover "UX"` means four different things across them — so each row says
    /// which it is rather than leaving it to be inferred from a name that may
    /// be shared.
    pub fn noun(self) -> &'static str {
        match self {
            Kind::Project => "project",
            Kind::Initiative => "group",
            Kind::Task => "tab",
            Kind::Pane => "pane",
        }
    }
}

/// What went into the trash, and what it is still worth.
#[derive(Debug, PartialEq, Eq)]
pub struct Holding<T> {
    pub id: u64,
    /// The branch's name as it read when it was deleted. Held rather than
    /// resolved on demand: the project it belonged to may be gone too.
    pub label: String,
    pub kind: Kind,
    pub tabs: usize,
    pub panes: usize,
    pub taken: Instant,
    pub window: Duration,
    /// The tabs themselves. Never inspected here.
    pub payload: T,
}

impl<T> Holding<T> {
    /// How long is left, or `None` once it has run out.
    ///
    /// `None` means expired, which is a different fact from "no such holding" —
    /// see [`Recovered`].
    pub fn left(&self, now: Instant) -> Option<Duration> {
        self.window
            .checked_sub(now.saturating_duration_since(self.taken))
    }

    pub fn live(&self, now: Instant) -> bool {
        self.left(now).is_some()
    }
}

/// The answer to "give me that one back".
///
/// Three states, not two. A holding that ran out of time and a holding that
/// never existed are different things to tell somebody, and collapsing them
/// into `None` would have the tray say "gone" when the honest answer is "it
/// expired four minutes ago".
#[derive(Debug, PartialEq, Eq)]
pub enum Recovered<T> {
    Ok(T),
    Expired,
    Unknown,
}

/// Everything deleted and not yet gone, newest first.
#[derive(Debug)]
pub struct Trash<T> {
    items: Vec<Holding<T>>,
    next_id: u64,
}

/// Written out rather than derived, because a derived `Default` starts
/// `next_id` at 0 while [`Trash::new`] starts it at 1 — two construction paths
/// handing out different first ids, which is the sort of difference that shows
/// up as one unreachable holding much later.
impl<T> Default for Trash<T> {
    fn default() -> Self {
        Trash::new()
    }
}

impl<T> Trash<T> {
    pub fn new() -> Self {
        Trash {
            items: Vec::new(),
            next_id: 1,
        }
    }

    /// Put a branch in, and hand back whatever had to be evicted to make room.
    ///
    /// The caller drops the returned payloads, which is what ends their shells.
    /// Expired holdings are swept in the same pass, so a `take` never counts a
    /// dead entry against the cap.
    pub fn take(
        &mut self,
        kind: Kind,
        label: String,
        tabs: usize,
        panes: usize,
        payload: T,
        now: Instant,
    ) -> (u64, Vec<T>) {
        let mut dropped = self.sweep(now);
        let id = self.next_id;
        self.next_id += 1;
        self.items.insert(
            0,
            Holding {
                id,
                label,
                kind,
                tabs,
                panes,
                taken: now,
                window: window_for(panes),
                payload,
            },
        );
        while self.items.len() > CAP {
            dropped.push(self.items.pop().expect("over cap").payload);
        }
        (id, dropped)
    }

    /// Take one back out. Expiry is checked here rather than trusted from the
    /// last sweep, so a holding cannot be recovered a moment after it died
    /// because nothing happened to trigger a sweep in between.
    pub fn recover(&mut self, id: u64, now: Instant) -> Recovered<Holding<T>> {
        let Some(at) = self.items.iter().position(|h| h.id == id) else {
            return Recovered::Unknown;
        };
        if !self.items[at].live(now) {
            return Recovered::Expired;
        }
        Recovered::Ok(self.items.remove(at))
    }

    /// Drop everything past its window. Returns the payloads, for the caller to
    /// let go of.
    pub fn sweep(&mut self, now: Instant) -> Vec<T> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.items.len() {
            if self.items[i].live(now) {
                i += 1;
            } else {
                out.push(self.items.remove(i).payload);
            }
        }
        out
    }

    /// Empty it deliberately. Returns every payload, live or not.
    pub fn empty(&mut self) -> Vec<T> {
        std::mem::take(&mut self.items)
            .into_iter()
            .map(|h| h.payload)
            .collect()
    }

    /// What the tray should offer, newest first — and only what is still real.
    ///
    /// The whole point of the contextual tray: a person must never be shown
    /// "Recover MARKETING" for a group whose shells ended twenty minutes ago.
    pub fn live_items(&self, now: Instant) -> impl Iterator<Item = &Holding<T>> {
        self.items.iter().filter(move |h| h.live(now))
    }

    /// How many offers the tray has. Drives the count on the trash can.
    pub fn live_len(&self, now: Instant) -> usize {
        self.live_items(now).count()
    }

    /// Whether anything is held at all, expired or not.
    ///
    /// Distinct from `live_len(now) == 0`: a trash holding three corpses that
    /// nothing has swept yet is not empty, and a caller deciding whether there
    /// is cleanup to do needs that answer rather than the tray's.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// "3h 41m left" — what the tray puts beside each offer.
///
/// Rounded down, and never to "0m": a holding with thirty seconds on it says
/// "under a minute", because an offer reading "0m left" invites a click that
/// will fail between the reading and the pressing.
pub fn remaining_label(left: Duration) -> String {
    let secs = left.as_secs();
    if secs < 60 {
        return "under a minute".to_string();
    }
    let mins = secs / 60;
    let (h, m) = (mins / 60, mins % 60);
    if h > 0 {
        format!("{h}h {m}m left")
    } else {
        format!("{m}m left")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;
    /// A payload that says whether it is still alive, so a test can assert on
    /// the thing that actually matters — a shell ending — rather than on a
    /// bookkeeping side effect. Dropping this is the stand-in for dropping a
    /// `Tab`, which is what ends a terminal in window-owned mode.
    use std::rc::Rc;

    #[derive(Debug, Clone)]
    struct Shell {
        name: &'static str,
        ended: Rc<RefCell<Vec<&'static str>>>,
    }
    impl Drop for Shell {
        fn drop(&mut self) {
            self.ended.borrow_mut().push(self.name);
        }
    }
    impl PartialEq for Shell {
        fn eq(&self, other: &Self) -> bool {
            self.name == other.name
        }
    }
    impl Eq for Shell {}

    fn ledger() -> Rc<RefCell<Vec<&'static str>>> {
        Rc::new(RefCell::new(Vec::new()))
    }

    fn t0() -> Instant {
        Instant::now()
    }

    fn after(base: Instant, d: Duration) -> Instant {
        base + d
    }

    fn mins(n: u64) -> Duration {
        Duration::from_secs(n * 60)
    }

    #[test]
    fn one_pane_is_held_an_hour_and_two_or_more_for_four() {
        assert_eq!(window_for(1), SHORT);
        assert_eq!(window_for(2), LONG);
        assert_eq!(window_for(14), LONG);
    }

    /// A branch with no panes at all is still a branch somebody deleted. It
    /// takes the short window rather than zero: an empty group is the cheapest
    /// thing in here to keep, and a window of zero would make "recover" an
    /// offer that is already gone by the time it is drawn.
    #[test]
    fn an_empty_branch_is_held_rather_than_expiring_on_arrival() {
        assert_eq!(window_for(0), SHORT);
        let mut trash: Trash<()> = Trash::new();
        let now = t0();
        let (id, _) = trash.take(Kind::Initiative, "EMPTY".into(), 0, 0, (), now);
        assert!(matches!(trash.recover(id, now), Recovered::Ok(_)));
    }

    #[test]
    fn a_group_outlives_a_lone_tab() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        let (tab, _) = trash.take(Kind::Task, "one".into(), 1, 1, (), now);
        let (group, _) = trash.take(Kind::Initiative, "UX".into(), 4, 6, (), now);

        let later = after(now, mins(90));
        assert_eq!(
            trash.recover(tab, later),
            Recovered::Expired,
            "a single pane should be an hour"
        );
        assert!(
            matches!(trash.recover(group, later), Recovered::Ok(_)),
            "a six-pane group should still have hours on it"
        );
    }

    /// Expired and never-existed are different answers.
    #[test]
    fn an_expired_holding_says_so_rather_than_saying_it_never_existed() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        let (id, _) = trash.take(Kind::Task, "gone".into(), 1, 1, (), now);
        let later = after(now, mins(61));

        assert_eq!(trash.recover(id, later), Recovered::Expired);
        assert_eq!(trash.recover(9999, later), Recovered::Unknown);
    }

    /// The tray must never offer something that is already dead.
    #[test]
    fn the_tray_lists_only_what_is_still_recoverable() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        trash.take(Kind::Task, "short".into(), 1, 1, (), now);
        trash.take(Kind::Initiative, "long".into(), 3, 5, (), now);

        assert_eq!(trash.live_len(now), 2);
        let later = after(now, mins(61));
        assert_eq!(trash.live_len(later), 1);
        let names: Vec<&str> = trash.live_items(later).map(|h| h.label.as_str()).collect();
        assert_eq!(names, vec!["long"]);
    }

    #[test]
    fn the_newest_deletion_is_offered_first() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        for name in ["first", "second", "third"] {
            trash.take(Kind::Initiative, name.into(), 2, 2, (), now);
        }
        let order: Vec<&str> = trash.live_items(now).map(|h| h.label.as_str()).collect();
        assert_eq!(order, vec!["third", "second", "first"]);
    }

    /// The eleventh deletion ends the oldest one's shells, and hands them back
    /// so the caller is the one that drops them.
    #[test]
    fn going_over_the_cap_evicts_the_oldest_and_ends_its_shells() {
        let now = t0();
        let ended = ledger();
        let mut trash: Trash<Shell> = Trash::new();

        for i in 0..CAP {
            let name: &'static str = Box::leak(format!("group-{i}").into_boxed_str());
            let (_, dropped) = trash.take(
                Kind::Initiative,
                name.into(),
                2,
                2,
                Shell {
                    name,
                    ended: ended.clone(),
                },
                now,
            );
            assert!(dropped.is_empty(), "nothing should evict inside the cap");
        }
        assert_eq!(trash.live_len(now), CAP);
        assert!(
            ended.borrow().is_empty(),
            "no shell ended while under the cap"
        );

        let (_, dropped) = trash.take(
            Kind::Initiative,
            "one-too-many".into(),
            2,
            2,
            Shell {
                name: "one-too-many",
                ended: ended.clone(),
            },
            now,
        );
        assert_eq!(dropped.len(), 1, "exactly one holding is evicted");
        assert_eq!(dropped[0].name, "group-0", "and it is the oldest");
        assert!(
            ended.borrow().is_empty(),
            "eviction must not end the shell itself — the caller owns that drop"
        );
        drop(dropped);
        assert_eq!(*ended.borrow(), vec!["group-0"]);
        assert_eq!(trash.live_len(now), CAP);
    }

    /// Recovering hands the payload back intact, and stops holding it.
    #[test]
    fn recovering_returns_the_shells_and_does_not_end_them() {
        let now = t0();
        let ended = ledger();
        let mut trash: Trash<Shell> = Trash::new();
        let (id, _) = trash.take(
            Kind::Initiative,
            "UX".into(),
            4,
            6,
            Shell {
                name: "ux",
                ended: ended.clone(),
            },
            now,
        );

        let got = trash.recover(id, after(now, mins(30)));
        let Recovered::Ok(h) = got else {
            panic!("should still be recoverable at 30 minutes");
        };
        assert_eq!(h.label, "UX");
        assert_eq!(h.tabs, 4);
        assert_eq!(h.panes, 6);
        assert!(
            ended.borrow().is_empty(),
            "a recovered group's shells were never ended"
        );
        assert_eq!(trash.live_len(now), 0, "and it is no longer in the trash");

        // recovering the same id twice is Unknown, not a second copy
        assert_eq!(trash.recover(id, now), Recovered::Unknown);
    }

    /// A holding that dies with nobody watching is swept, not resurrected.
    #[test]
    fn a_sweep_ends_exactly_the_holdings_past_their_window() {
        let now = t0();
        let ended = ledger();
        let mut trash: Trash<Shell> = Trash::new();
        trash.take(
            Kind::Task,
            "lone".into(),
            1,
            1,
            Shell {
                name: "lone",
                ended: ended.clone(),
            },
            now,
        );
        trash.take(
            Kind::Initiative,
            "big".into(),
            3,
            4,
            Shell {
                name: "big",
                ended: ended.clone(),
            },
            now,
        );

        let dead = trash.sweep(after(now, mins(61)));
        assert_eq!(dead.len(), 1);
        drop(dead);
        assert_eq!(*ended.borrow(), vec!["lone"]);
        assert_eq!(trash.live_len(after(now, mins(61))), 1);
    }

    /// Emptying it deliberately takes everything, including what still had time.
    #[test]
    fn emptying_the_trash_takes_the_live_ones_too() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        trash.take(Kind::Task, "a".into(), 1, 1, (), now);
        trash.take(Kind::Initiative, "b".into(), 2, 3, (), now);
        assert_eq!(trash.empty().len(), 2);
        assert!(trash.is_empty());
        assert_eq!(trash.live_len(now), 0);
    }

    /// An id is never reused, so a stale reference resolves to nothing rather
    /// than to somebody else's group — the same rule the project and group ids
    /// already follow.
    #[test]
    fn an_id_is_never_handed_out_twice() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        let (first, _) = trash.take(Kind::Task, "a".into(), 1, 1, (), now);
        trash.recover(first, now);
        let (second, _) = trash.take(Kind::Task, "b".into(), 1, 1, (), now);
        assert_ne!(first, second);
        assert_eq!(trash.recover(first, now), Recovered::Unknown);
    }

    #[test]
    fn a_holding_about_to_expire_never_reads_as_zero_minutes() {
        assert_eq!(remaining_label(Duration::from_secs(30)), "under a minute");
        assert_eq!(remaining_label(Duration::from_secs(59)), "under a minute");
        assert_eq!(remaining_label(mins(48)), "48m left");
        assert_eq!(remaining_label(mins(60)), "1h 0m left");
        assert_eq!(remaining_label(mins(221)), "3h 41m left");
    }

    #[test]
    fn a_taken_branch_reports_the_time_it_has_left() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        trash.take(Kind::Initiative, "UX".into(), 4, 6, (), now);
        let h = trash.live_items(now).next().expect("one holding");
        assert_eq!(h.left(now), Some(LONG));
        assert_eq!(h.left(after(now, mins(19))), Some(LONG - mins(19)));
        assert_eq!(h.left(after(now, mins(241))), None);
    }

    /// A take that happens long after the last one sweeps the dead in passing,
    /// so a session nobody has touched for a day does not carry nine corpses
    /// against the cap.
    #[test]
    fn taking_sweeps_what_died_while_nothing_was_happening() {
        let now = t0();
        let ended = ledger();
        let mut trash: Trash<Shell> = Trash::new();
        trash.take(
            Kind::Task,
            "old".into(),
            1,
            1,
            Shell {
                name: "old",
                ended: ended.clone(),
            },
            now,
        );

        let (_, dropped) = trash.take(
            Kind::Task,
            "new".into(),
            1,
            1,
            Shell {
                name: "new",
                ended: ended.clone(),
            },
            after(now, mins(61)),
        );
        assert_eq!(
            dropped.len(),
            1,
            "the hour-old holding was swept in passing"
        );
        drop(dropped);
        assert_eq!(*ended.borrow(), vec!["old"]);
    }

    /// A tab dragged into the bay is held like anything else dragged there —
    /// for an hour, since it is one pane's worth — rather than being closed.
    #[test]
    fn a_lone_tab_is_held_like_any_other_branch() {
        let now = t0();
        let mut trash: Trash<()> = Trash::new();
        let (id, _) = trash.take(Kind::Task, "tab 3".into(), 1, 1, (), now);
        assert!(matches!(
            trash.recover(id, after(now, mins(59))),
            Recovered::Ok(_)
        ));
    }
}
