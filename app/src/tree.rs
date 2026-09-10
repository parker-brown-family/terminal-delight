//! The left bar's tree: PROJECT over INITIATIVE over TASK.
//!
//! The mother bar ran out of attentional space. Twelve tabs on one horizontal
//! strip is twelve titles competing for the same glance, and the strip answered
//! by wrapping onto a second row, which is more of the same problem stacked.
//! The left bar is the other axis: a collapsible vertical tree where the tabs
//! you are not working on today fold away behind the branch they belong to.
//!
//! Three things are true at once, and this module exists to keep them true:
//!
//! 1. **A tab is a TASK.** It holds sub-panes — terminals, tmux inside them, an
//!    agent or three — and it is the unit a person picks up and puts down. The
//!    tree does not replace tabs; it gives them somewhere to live.
//! 2. **The tree is two layers deep and no deeper.** PROJECT contains
//!    INITIATIVE contains task. An initiative is the tab group that already
//!    existed (a coloured band on the mother bar), read as what it always was:
//!    a run of tasks that belong to one push. The layer added here is the
//!    project above it.
//! 3. **Nothing is ever unreachable.** Folding a branch hides its tasks from the
//!    tree; scoping the mother bar hides them from the strip. Neither may hide
//!    the task you are IN, and neither may swallow an agent that is asking you
//!    a question — which is why every branch row carries the roll-up of what is
//!    happening beneath it.
//!
//! Everything here is deliberately free of gpui and of `Workspace`: the row
//! list, the roll-up and the scope rules are pure functions over plain ids, so
//! they can be tested without a window. The colours, the glyphs and the click
//! targets are the renderer's business.

/// Where one task sits in the tree.
///
/// Both halves are `Option` and they mean different things when absent, which
/// is the whole reason they are not one field: a task with no initiative may
/// still belong to a project (it sits loose under it), and a task with neither
/// is unfiled — not "in project zero", which is a real project somebody could
/// own.
///
/// A grouped task's project is its INITIATIVE's project, resolved before a
/// `Place` is built. A task never contradicts the branch it hangs from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Place {
    pub project: Option<u32>,
    pub initiative: Option<u32>,
}

impl Place {
    pub fn unfiled(&self) -> bool {
        self.project.is_none() && self.initiative.is_none()
    }
}

/// What is happening under a branch, summed from its tasks.
///
/// The counts are the mother bar's own vocabulary — the same robot, pin, tick
/// and cross a tab button shows — because #319 asked for the same semantics in
/// a new geometry, and because a person who has learned what a blinking robot
/// means must not have to learn it twice.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Roll {
    /// Agents stopped, waiting on a human. The loudest thing a branch can say.
    pub needs_input: usize,
    /// Agents mid-turn.
    pub working: usize,
    /// Finished clean, nobody has looked yet.
    pub done: usize,
    /// Finished against a wall.
    pub blocked: usize,
    /// Pinned notes left for yourself.
    pub pins: usize,
    /// Terminals under this branch.
    pub panes: usize,
    /// Tasks under this branch.
    pub tasks: usize,
}

impl Roll {
    /// One task's contribution, before folding.
    pub fn task(
        needs_input: usize,
        working: usize,
        done: usize,
        blocked: usize,
        pins: usize,
        panes: usize,
    ) -> Self {
        Self {
            needs_input,
            working,
            done,
            blocked,
            pins,
            panes,
            tasks: 1,
        }
    }

    pub fn fold(&mut self, other: &Roll) {
        self.needs_input += other.needs_input;
        self.working += other.working;
        self.done += other.done;
        self.blocked += other.blocked;
        self.pins += other.pins;
        self.panes += other.panes;
        self.tasks += other.tasks;
    }

    /// Nothing under here is asking for anything. Drives whether a folded
    /// branch row draws its badge cluster at all.
    pub fn quiet(&self) -> bool {
        self.needs_input == 0
            && self.working == 0
            && self.done == 0
            && self.blocked == 0
            && self.pins == 0
    }
}

/// One task as the tree sees it: where it sits, and what it is saying.
#[derive(Clone, Copy, Debug)]
pub struct TaskRef {
    pub place: Place,
    pub roll: Roll,
}

/// A project, as the row builder needs it. The renderer holds the colour and
/// the name; this is only the shape.
#[derive(Clone, Copy, Debug)]
pub struct ProjectRef {
    pub id: u32,
    pub collapsed: bool,
}

/// An initiative (a tab group), as the row builder needs it.
#[derive(Clone, Copy, Debug)]
pub struct InitiativeRef {
    pub id: u32,
    pub project: Option<u32>,
    pub collapsed: bool,
}

/// Which branch of the tree the mother bar is showing.
///
/// The tree is always whole; the STRIP is what narrows. Scoping is the payoff
/// of the whole feature — a strip that only ever carries one push's worth of
/// tabs is a strip that stops wrapping — but it is only safe because the tree
/// beside it never narrows and because [`Scope::widened_for`] refuses to leave
/// the active task outside the frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Scope {
    #[default]
    All,
    Project(u32),
    Initiative(u32),
}

impl Scope {
    /// Does the strip show a task in this place?
    pub fn shows(&self, place: &Place) -> bool {
        match self {
            Scope::All => true,
            Scope::Project(p) => place.project == Some(*p),
            Scope::Initiative(g) => place.initiative == Some(*g),
        }
    }

    /// The scope to switch to so `place` is visible, or `None` when this scope
    /// already shows it.
    ///
    /// Widening lands on the task's PROJECT rather than snapping straight back
    /// to `All`: activating a task in another project is a move to that
    /// project, and dumping the whole session back onto the strip would undo
    /// the narrowing the person chose. A task with no project has nowhere
    /// narrower to be than everywhere.
    pub fn widened_for(&self, place: &Place) -> Option<Scope> {
        if self.shows(place) {
            return None;
        }
        Some(match place.project {
            Some(p) => Scope::Project(p),
            None => Scope::All,
        })
    }

    /// Clicking the branch you are already scoped to backs out to `All` — the
    /// toggle that means a scope can always be undone by clicking the same row
    /// twice, without hunting for a "show everything" control.
    pub fn toggled(&self, to: Scope) -> Scope {
        if *self == to {
            Scope::All
        } else {
            to
        }
    }
}

/// One line of the left bar.
///
/// `depth` is indentation in steps, not pixels. `roll` on a branch is the sum
/// beneath it whether or not it is folded — a folded branch is exactly when the
/// roll-up matters.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Project {
        id: u32,
        depth: u8,
        roll: Roll,
        collapsed: bool,
    },
    Initiative {
        id: u32,
        depth: u8,
        roll: Roll,
        collapsed: bool,
    },
    Task {
        /// Index into the workspace's tab list — the identity of the tab.
        index: usize,
        depth: u8,
    },
    /// The divider above the loose tasks. Only drawn when there is something
    /// for it to be loose FROM (see [`rows`]).
    Unfiled { depth: u8 },
}

/// Flatten the tree into the lines the left bar draws, top to bottom.
///
/// Order is projects in their stored order, each followed by its initiatives
/// (each followed by that initiative's tasks) and then its loose tasks; then
/// the initiatives that belong to no project; then, under a divider, the tasks
/// that belong to nothing.
///
/// Two rules earn their keep here:
///
/// - **The active task's branches force-expand.** A fold that hides the task
///   you are in is a window that has lost your place, and the mother bar's tab
///   groups already made this promise for the same reason.
/// - **The unfiled divider is suppressed when the tree has no other sections.**
///   A session that has never been organised is a flat list of tasks, and
///   labelling it "unfiled" says nothing while costing a line.
pub fn rows(
    projects: &[ProjectRef],
    initiatives: &[InitiativeRef],
    tasks: &[TaskRef],
    active: Option<usize>,
) -> Vec<Row> {
    // Branches the active task hangs from never fold.
    let active_place = active.and_then(|i| tasks.get(i)).map(|t| t.place);
    let pinned_project = active_place.and_then(|p| p.project);
    let pinned_initiative = active_place.and_then(|p| p.initiative);

    let live_projects: Vec<u32> = projects.iter().map(|p| p.id).collect();
    let known = |p: Option<u32>| p.filter(|id| live_projects.contains(id));

    let roll_of = |pred: &dyn Fn(&Place) -> bool| {
        let mut roll = Roll::default();
        for t in tasks.iter().filter(|t| pred(&t.place)) {
            roll.fold(&t.roll);
        }
        roll
    };

    let mut out: Vec<Row> = vec![];

    let push_initiative = |out: &mut Vec<Row>, ini: &InitiativeRef, depth: u8| {
        let roll = roll_of(&|pl: &Place| pl.initiative == Some(ini.id));
        let collapsed = ini.collapsed && pinned_initiative != Some(ini.id);
        out.push(Row::Initiative {
            id: ini.id,
            depth,
            roll,
            collapsed,
        });
        if collapsed {
            return;
        }
        for (i, t) in tasks.iter().enumerate() {
            if t.place.initiative == Some(ini.id) {
                out.push(Row::Task {
                    index: i,
                    depth: depth + 1,
                });
            }
        }
    };

    for p in projects {
        let roll = roll_of(&|pl: &Place| pl.project == Some(p.id));
        let collapsed = p.collapsed && pinned_project != Some(p.id);
        out.push(Row::Project {
            id: p.id,
            depth: 0,
            roll,
            collapsed,
        });
        if collapsed {
            continue;
        }
        for ini in initiatives
            .iter()
            .filter(|i| known(i.project) == Some(p.id))
        {
            push_initiative(&mut out, ini, 1);
        }
        for (i, t) in tasks.iter().enumerate() {
            if t.place.project == Some(p.id) && t.place.initiative.is_none() {
                out.push(Row::Task { index: i, depth: 1 });
            }
        }
    }

    // Initiatives nobody has filed under a project yet — which is every group in
    // a session that predates this feature, so this branch is the migration.
    for ini in initiatives.iter().filter(|i| known(i.project).is_none()) {
        push_initiative(&mut out, ini, 0);
    }

    let loose: Vec<usize> = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| t.place.unfiled())
        .map(|(i, _)| i)
        .collect();
    if !loose.is_empty() {
        // A divider only earns its line when it divides something.
        if !out.is_empty() {
            out.push(Row::Unfiled { depth: 0 });
        }
        for i in loose {
            out.push(Row::Task { index: i, depth: 0 });
        }
    }
    out
}

/// The tasks the mother bar draws under `scope`, in tab order.
///
/// Kept beside [`rows`] on purpose: the strip and the tree answer the same
/// question about the same session and must never disagree about which tasks
/// exist.
pub fn in_scope(places: &[Place], scope: Scope) -> Vec<usize> {
    places
        .iter()
        .enumerate()
        .filter(|(_, place)| scope.shows(place))
        .map(|(i, _)| i)
        .collect()
}

/// What the scope is HIDING from the strip, summed — the price of narrowing it.
///
/// The tree shows this branch by branch, but the tree can be closed, and a
/// scoped strip with the tree closed is the one arrangement where an agent
/// could stop and ask a question with nothing on screen to say so. This is what
/// the mother bar's own out-of-scope chip reads.
pub fn out_of_scope(tasks: &[TaskRef], scope: Scope) -> Roll {
    let mut roll = Roll::default();
    for t in tasks.iter().filter(|t| !scope.shows(&t.place)) {
        roll.fold(&t.roll);
    }
    roll
}

/// What a branch is saying, as one short line of glyphs, loudest first.
///
/// Returned as a string rather than elements so the summary can be tested and
/// so a tooltip, a title bar and a row can all say the same thing.
pub fn roll_glyphs(roll: &Roll) -> String {
    let mut out = String::new();
    let mut push = |glyph: &str, n: usize| {
        if n == 0 {
            return;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(glyph);
        if n > 1 {
            out.push_str(&n.to_string());
        }
    };
    push("🤖", roll.needs_input + roll.working);
    push("✅", roll.done);
    push("❌", roll.blocked);
    push("📌", roll.pins);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(project: Option<u32>, initiative: Option<u32>) -> TaskRef {
        TaskRef {
            place: Place {
                project,
                initiative,
            },
            roll: Roll::task(0, 0, 0, 0, 0, 1),
        }
    }

    fn loud(project: Option<u32>, initiative: Option<u32>, needs_input: usize) -> TaskRef {
        TaskRef {
            place: Place {
                project,
                initiative,
            },
            roll: Roll::task(needs_input, 0, 0, 0, 0, 1),
        }
    }

    fn project(id: u32, collapsed: bool) -> ProjectRef {
        ProjectRef { id, collapsed }
    }

    fn initiative(id: u32, project: Option<u32>, collapsed: bool) -> InitiativeRef {
        InitiativeRef {
            id,
            project,
            collapsed,
        }
    }

    #[test]
    fn a_session_that_has_never_been_organised_is_a_flat_list_with_no_divider() {
        // The migration case: every existing session opens into this. A tree
        // that greeted it with an "UNFILED" header over a list of every tab
        // would be labelling the whole window as a mistake.
        let tasks = vec![task(None, None), task(None, None), task(None, None)];
        let rows = rows(&[], &[], &tasks, Some(0));
        assert_eq!(
            rows,
            vec![
                Row::Task { index: 0, depth: 0 },
                Row::Task { index: 1, depth: 0 },
                Row::Task { index: 2, depth: 0 },
            ]
        );
    }

    #[test]
    fn existing_tab_groups_become_top_level_initiatives_carrying_their_tasks() {
        // A session with groups but no projects — every session that has ever
        // used the colour bands. The tree must be recognisable as the strip.
        let tasks = vec![task(None, Some(7)), task(None, Some(7)), task(None, None)];
        let rows = rows(&[], &[initiative(7, None, false)], &tasks, Some(0));
        assert_eq!(
            rows,
            vec![
                Row::Initiative {
                    id: 7,
                    depth: 0,
                    roll: Roll {
                        panes: 2,
                        tasks: 2,
                        ..Default::default()
                    },
                    collapsed: false,
                },
                Row::Task { index: 0, depth: 1 },
                Row::Task { index: 1, depth: 1 },
                Row::Unfiled { depth: 0 },
                Row::Task { index: 2, depth: 0 },
            ]
        );
    }

    #[test]
    fn the_two_layers_nest_and_loose_tasks_hang_under_their_project() {
        let tasks = vec![
            task(Some(1), Some(10)),
            task(Some(1), None),
            task(Some(2), None),
        ];
        let rows = rows(
            &[project(1, false), project(2, false)],
            &[initiative(10, Some(1), false)],
            &tasks,
            Some(0),
        );
        let shape: Vec<(&str, u8)> = rows
            .iter()
            .map(|r| match r {
                Row::Project { depth, .. } => ("project", *depth),
                Row::Initiative { depth, .. } => ("initiative", *depth),
                Row::Task { depth, .. } => ("task", *depth),
                Row::Unfiled { depth } => ("unfiled", *depth),
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                ("project", 0),
                ("initiative", 1),
                ("task", 2),
                ("task", 1),
                ("project", 0),
                ("task", 1),
            ]
        );
    }

    #[test]
    fn a_collapsed_branch_hides_its_tasks_but_keeps_their_roll_up() {
        // The point of folding: the tasks go away, what they are SAYING does
        // not. A folded project holding a blinking robot still blinks.
        let tasks = vec![loud(Some(1), Some(10), 1), loud(Some(1), None, 2)];
        let rows = rows(
            &[project(1, true)],
            &[initiative(10, Some(1), false)],
            &tasks,
            None,
        );
        assert_eq!(
            rows,
            vec![Row::Project {
                id: 1,
                depth: 0,
                roll: Roll {
                    needs_input: 3,
                    panes: 2,
                    tasks: 2,
                    ..Default::default()
                },
                collapsed: true,
            }]
        );
    }

    #[test]
    fn the_branch_holding_the_active_task_refuses_to_fold() {
        // Both layers, together: a folded project inside which sits a folded
        // initiative holding the task you are in. Neither fold may apply, or
        // the window has lost your place.
        let tasks = vec![task(Some(1), Some(10))];
        let rows = rows(
            &[project(1, true)],
            &[initiative(10, Some(1), true)],
            &tasks,
            Some(0),
        );
        assert_eq!(
            rows,
            vec![
                Row::Project {
                    id: 1,
                    depth: 0,
                    roll: Roll {
                        panes: 1,
                        tasks: 1,
                        ..Default::default()
                    },
                    collapsed: false,
                },
                Row::Initiative {
                    id: 10,
                    depth: 1,
                    roll: Roll {
                        panes: 1,
                        tasks: 1,
                        ..Default::default()
                    },
                    collapsed: false,
                },
                Row::Task { index: 0, depth: 2 },
            ]
        );
    }

    #[test]
    fn a_fold_elsewhere_still_folds_while_the_active_branch_stays_open() {
        let tasks = vec![task(Some(1), None), task(Some(2), None)];
        let rows = rows(&[project(1, true), project(2, true)], &[], &tasks, Some(1));
        let folded: Vec<(u32, bool)> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Project { id, collapsed, .. } => Some((*id, *collapsed)),
                _ => None,
            })
            .collect();
        assert_eq!(folded, vec![(1, true), (2, false)]);
    }

    #[test]
    fn an_initiative_whose_project_vanished_falls_back_to_the_top_level() {
        // A dangling reference must never swallow the tasks under it. The
        // group's project id is filtered against the live list, exactly as the
        // restore path filters a tab's group id.
        let tasks = vec![task(None, Some(10))];
        let rows = rows(&[], &[initiative(10, Some(99), false)], &tasks, None);
        assert_eq!(
            rows.first(),
            Some(&Row::Initiative {
                id: 10,
                depth: 0,
                roll: Roll {
                    panes: 1,
                    tasks: 1,
                    ..Default::default()
                },
                collapsed: false,
            })
        );
        assert_eq!(rows.last(), Some(&Row::Task { index: 0, depth: 1 }));
    }

    #[test]
    fn every_task_appears_exactly_once_however_the_tree_is_shaped() {
        // The invariant the whole bar rests on: the tree is the complete index
        // of the session. A task that appears twice is a lie about how much
        // work is open; one that appears nowhere is work you cannot reach.
        let tasks = vec![
            task(Some(1), Some(10)),
            task(Some(1), None),
            task(Some(2), Some(11)),
            task(None, Some(12)),
            task(None, None),
        ];
        let rows = rows(
            &[project(1, false), project(2, false)],
            &[
                initiative(10, Some(1), false),
                initiative(11, Some(2), false),
                initiative(12, None, false),
            ],
            &tasks,
            None,
        );
        let mut seen: Vec<usize> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Task { index, .. } => Some(*index),
                _ => None,
            })
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn scope_narrows_the_strip_by_branch() {
        let places: Vec<Place> = [
            task(Some(1), Some(10)),
            task(Some(1), None),
            task(Some(2), None),
            task(None, None),
        ]
        .iter()
        .map(|t| t.place)
        .collect();
        assert_eq!(in_scope(&places, Scope::All), vec![0, 1, 2, 3]);
        assert_eq!(in_scope(&places, Scope::Project(1)), vec![0, 1]);
        assert_eq!(in_scope(&places, Scope::Initiative(10)), vec![0]);
    }

    #[test]
    fn what_the_scope_hides_is_counted_so_the_strip_can_say_so() {
        // The safety catch on scoping: an agent that stops to ask a question in
        // a project you are not looking at must still be able to interrupt you.
        let tasks = vec![
            loud(Some(1), None, 0),
            loud(Some(2), None, 1),
            loud(None, None, 2),
        ];
        let hidden = out_of_scope(&tasks, Scope::Project(1));
        assert_eq!(hidden.needs_input, 3);
        assert_eq!(hidden.tasks, 2);
        // nothing is hidden when nothing is narrowed
        assert_eq!(out_of_scope(&tasks, Scope::All), Roll::default());
        assert!(out_of_scope(&tasks, Scope::All).quiet());
    }

    #[test]
    fn activating_a_task_outside_the_scope_widens_to_its_project_not_to_everything() {
        let elsewhere = Place {
            project: Some(2),
            initiative: None,
        };
        assert_eq!(
            Scope::Project(1).widened_for(&elsewhere),
            Some(Scope::Project(2))
        );
        // already visible → nothing moves
        assert_eq!(Scope::Project(2).widened_for(&elsewhere), None);
        assert_eq!(Scope::All.widened_for(&elsewhere), None);
        // an unfiled task has no narrower home than everywhere
        assert_eq!(
            Scope::Project(1).widened_for(&Place::default()),
            Some(Scope::All)
        );
    }

    #[test]
    fn scoping_to_the_branch_you_are_already_on_backs_out_to_everything() {
        assert_eq!(Scope::Project(1).toggled(Scope::Project(1)), Scope::All);
        assert_eq!(
            Scope::Project(1).toggled(Scope::Project(2)),
            Scope::Project(2)
        );
        assert_eq!(
            Scope::All.toggled(Scope::Initiative(9)),
            Scope::Initiative(9)
        );
    }

    #[test]
    fn the_roll_up_line_is_loudest_first_and_silent_when_there_is_nothing_to_say() {
        assert_eq!(roll_glyphs(&Roll::default()), "");
        assert!(Roll::default().quiet());
        let roll = Roll {
            needs_input: 1,
            working: 2,
            done: 1,
            blocked: 0,
            pins: 3,
            panes: 9,
            tasks: 4,
        };
        assert_eq!(roll_glyphs(&roll), "🤖3 ✅ 📌3");
        assert!(!roll.quiet());
    }

    #[test]
    fn a_working_agent_and_a_waiting_one_count_as_agents_not_as_two_kinds_of_row() {
        // The tab strip draws one badge per agent and lets the loudest state
        // win per pane. A branch row has no room for four robots, so it counts
        // them — but it must not drop the waiting one into a different bucket
        // and report "1 robot" when three are in flight.
        let roll = Roll {
            needs_input: 1,
            working: 2,
            ..Default::default()
        };
        assert_eq!(roll_glyphs(&roll), "🤖3");
    }
}
