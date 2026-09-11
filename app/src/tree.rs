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
    let live_initiatives: Vec<u32> = initiatives.iter().map(|i| i.id).collect();
    let known_initiative = |g: Option<u32>| g.filter(|id| live_initiatives.contains(id));

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
            if known(t.place.project) == Some(p.id)
                && known_initiative(t.place.initiative).is_none()
            {
                out.push(Row::Task { index: i, depth: 1 });
            }
        }
    }

    // Initiatives nobody has filed under a project yet — which is every group in
    // a session that predates this feature, so this branch is the migration.
    for ini in initiatives.iter().filter(|i| known(i.project).is_none()) {
        push_initiative(&mut out, ini, 0);
    }

    // Everything that hangs from nothing that exists.
    //
    // Not the same test as [`Place::unfiled`], and the difference is a task
    // that would otherwise be listed NOWHERE: one pointing at a project or an
    // initiative that is no longer in the session. The restore path filters
    // those ids, so in a running window it cannot happen — but "the tree is the
    // complete index of the session" is this module's promise, not the caller's
    // discipline, and a tree that quietly drops a task has broken it whether or
    // not the caller was careful. Found by the shape sweep in the tests, which
    // is exactly what a sweep is for.
    let loose: Vec<usize> = tasks
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            known(t.place.project).is_none() && known_initiative(t.place.initiative).is_none()
        })
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

/// Which row a cursor is over, and what a release there would mean.
///
/// Two verbs, because a tree needs both and they are not the same gesture:
/// **Into** joins a branch and lets the order be whatever it was; **Before** and
/// **After** place the dragged thing at an exact seat. The renderer decides
/// which one from where in the row's height the cursor sits — the middle of a
/// branch header is *into*, its edges and a task's two halves are *between* —
/// and this module decides what each one lands on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RowId {
    Project(u32),
    Initiative(u32),
    /// A tab, by index.
    Task(usize),
    /// The divider above the loose tasks. Dropping here means "file this under
    /// nothing", which is a real answer and not a failure to aim.
    Unfiled,
}

/// What a release would do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drop {
    Into(RowId),
    Before(RowId),
    After(RowId),
}

/// Where a dragged TASK ends up: the branch it joins, and the tab slot it takes.
///
/// `slot` is an insertion index in the pre-removal tab list — the same index
/// space `Workspace::move_tab` speaks — and `None` means "join the branch and
/// keep your place in the order", which is what dropping onto a branch header
/// means.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Landing {
    pub place: Place,
    pub slot: Option<usize>,
}

/// Where a dragged INITIATIVE ends up: the project it joins, and the tab slot
/// its whole run of tasks slides to (`Workspace::move_group`'s index space).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct BranchLanding {
    pub project: Option<u32>,
    pub slot: Option<usize>,
}

/// The project an initiative hangs from, filtered to one that exists.
fn project_of(initiatives: &[InitiativeRef], gid: u32) -> Option<u32> {
    initiatives
        .iter()
        .find(|i| i.id == gid)
        .and_then(|i| i.project)
}

/// Resolve a task drag's release.
///
/// Returns `None` for a drop that means nothing — a task cannot be seated
/// before a project header, because "between two projects" is not a place a
/// task can be.
pub fn land_task(places: &[Place], initiatives: &[InitiativeRef], drop: Drop) -> Option<Landing> {
    let branch_place = |row: RowId| -> Option<Place> {
        Some(match row {
            RowId::Project(p) => Place {
                project: Some(p),
                initiative: None,
            },
            RowId::Initiative(g) => Place {
                project: project_of(initiatives, g),
                initiative: Some(g),
            },
            RowId::Unfiled => Place::default(),
            RowId::Task(_) => return None,
        })
    };
    match drop {
        // Onto a branch: join it, and take the seat after its last task so a
        // filed task lands where the eye was pointing rather than wherever its
        // old tab index happens to fall.
        Drop::Into(row) => {
            let place = branch_place(row)?;
            let last = places
                .iter()
                .enumerate()
                .filter(|(_, p)| **p == place)
                .map(|(i, _)| i)
                .next_back();
            Some(Landing {
                place,
                slot: last.map(|i| i + 1),
            })
        }
        // Onto a task: take that task's branch, and the seat on the side the
        // cursor was on.
        Drop::Before(RowId::Task(j)) => Some(Landing {
            place: *places.get(j)?,
            slot: Some(j),
        }),
        Drop::After(RowId::Task(j)) => Some(Landing {
            place: *places.get(j)?,
            slot: Some(j + 1),
        }),
        // The edges of a branch header, for a task, mean the same as its
        // middle: there is no "between two branches" for a task to sit in.
        Drop::Before(row) | Drop::After(row) => land_task(places, initiatives, Drop::Into(row)),
    }
}

/// Resolve an initiative drag's release: which project it joins, and where its
/// run of tasks slides to.
pub fn land_initiative(
    places: &[Place],
    initiatives: &[InitiativeRef],
    moving: u32,
    drop: Drop,
) -> Option<BranchLanding> {
    let tasks_of = |gid: u32| -> Vec<usize> {
        places
            .iter()
            .enumerate()
            .filter(|(_, p)| p.initiative == Some(gid))
            .map(|(i, _)| i)
            .collect()
    };
    match drop {
        Drop::Into(RowId::Project(p)) => Some(BranchLanding {
            project: Some(p),
            slot: None,
        }),
        Drop::Into(RowId::Unfiled) | Drop::Before(RowId::Unfiled) | Drop::After(RowId::Unfiled) => {
            Some(BranchLanding {
                project: None,
                slot: None,
            })
        }
        // Onto another initiative: adopt ITS project (the one it is sitting in,
        // which is what the eye is pointing at) and slide to that side of it.
        Drop::Before(RowId::Initiative(g)) | Drop::After(RowId::Initiative(g)) if g != moving => {
            let seats = tasks_of(g);
            let slot = match drop {
                Drop::Before(_) => seats.first().copied(),
                _ => seats.last().map(|i| i + 1),
            };
            Some(BranchLanding {
                project: project_of(initiatives, g),
                slot,
            })
        }
        // An initiative cannot be seated inside another initiative, or beside a
        // task: the tree is two layers deep and stays that way.
        _ => None,
    }
}

/// Move `moving` to sit immediately before or after `neighbour` in a list of
/// ids, preserving everything else. Used for the project layer, whose order is
/// its own — unlike initiatives and tasks, which take their order from the tab
/// list the mother bar draws.
pub fn reorder(ids: &mut Vec<u32>, moving: u32, neighbour: u32, after: bool) {
    if moving == neighbour {
        return;
    }
    let Some(from) = ids.iter().position(|id| *id == moving) else {
        return;
    };
    ids.remove(from);
    let Some(at) = ids.iter().position(|id| *id == neighbour) else {
        ids.insert(from.min(ids.len()), moving);
        return;
    };
    ids.insert(if after { at + 1 } else { at }, moving);
}

/// The tasks the strip draws: the branch the active task is in, in tab order.
///
/// One initiative's worth of tabs and nothing else. The tree beside the strip
/// already lists every other branch, so a strip that carried them too spent the
/// widest surface in the window repeating the one fact that was never in doubt
/// — and it is the reason the strip kept running out of room.
///
/// A task in no initiative sits with the other loose ones, which makes this the
/// identity function for a session that has never been organised: every tab,
/// exactly as before.
///
/// Kept beside [`rows`] on purpose: the strip and the tree answer the same
/// question about the same session and must never disagree about which tasks
/// exist.
pub fn family(places: &[Place], active: usize) -> Vec<usize> {
    let home = places.get(active).and_then(|p| p.initiative);
    places
        .iter()
        .enumerate()
        .filter(|(_, place)| place.initiative == home)
        .map(|(i, _)| i)
        .collect()
}

/// What the strip is NOT showing, summed — the price of narrowing it.
///
/// The tree shows this branch by branch, but the tree can be closed, and a
/// narrowed strip with the tree closed is the one arrangement where an agent
/// could stop and ask a question with nothing on screen to say so. This is what
/// the mother bar's own out-of-branch chip reads.
pub fn roll_outside(tasks: &[TaskRef], shown: &[usize]) -> Roll {
    let mut roll = Roll::default();
    for (i, t) in tasks.iter().enumerate() {
        if !shown.contains(&i) {
            roll.fold(&t.roll);
        }
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
    fn the_strip_carries_the_branch_you_are_in_and_no_other() {
        let places: Vec<Place> = [
            task(Some(1), Some(10)),
            task(Some(1), Some(10)),
            task(Some(1), Some(11)),
            task(Some(2), None),
            task(None, None),
        ]
        .iter()
        .map(|t| t.place)
        .collect();
        // in an initiative: its members, and not the sibling initiative that
        // happens to share a project
        assert_eq!(family(&places, 0), vec![0, 1]);
        assert_eq!(family(&places, 2), vec![2]);
        // loose tasks keep each other company whatever project they hang from
        assert_eq!(family(&places, 3), vec![3, 4]);
        assert_eq!(family(&places, 4), vec![3, 4]);
    }

    #[test]
    fn a_session_that_never_organised_anything_still_sees_every_tab() {
        // The identity case, and the one that decides whether this is safe to
        // ship to somebody who has never made a group.
        let places: Vec<Place> = (0..4).map(|_| task(None, None).place).collect();
        assert_eq!(family(&places, 0), vec![0, 1, 2, 3]);
    }

    #[test]
    fn an_active_index_off_the_end_narrows_to_the_loose_tabs_not_to_nothing() {
        // A strip with nothing on it is the one state there is no way back from,
        // so an impossible index lands somewhere rather than nowhere.
        let places: Vec<Place> = [task(Some(1), Some(10)), task(None, None)]
            .iter()
            .map(|t| t.place)
            .collect();
        assert_eq!(family(&places, 99), vec![1]);
    }

    #[test]
    fn what_the_strip_hides_is_counted_so_the_strip_can_say_so() {
        // The safety catch on narrowing: an agent that stops to ask a question
        // in a branch you are not looking at must still be able to interrupt
        // you.
        let tasks = vec![
            loud(Some(1), None, 0),
            loud(Some(2), None, 1),
            loud(None, None, 2),
        ];
        let hidden = roll_outside(&tasks, &[0]);
        assert_eq!(hidden.needs_input, 3);
        assert_eq!(hidden.tasks, 2);
        // nothing is hidden when nothing is narrowed
        assert_eq!(roll_outside(&tasks, &[0, 1, 2]), Roll::default());
        assert!(roll_outside(&tasks, &[0, 1, 2]).quiet());
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
    fn dropping_a_task_on_a_task_takes_its_branch_and_the_seat_you_pointed_at() {
        // The gesture people try first, and the one bb taught them: drag a row
        // onto another row and it lands exactly there, not "somewhere in that
        // group".
        let places = vec![
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place::default(),
        ];
        let inis = [initiative(10, Some(1), false)];
        let before = land_task(&places, &inis, Drop::Before(RowId::Task(1))).expect("lands");
        assert_eq!(before.place, places[1]);
        assert_eq!(before.slot, Some(1));
        let after = land_task(&places, &inis, Drop::After(RowId::Task(1))).expect("lands");
        assert_eq!(after.place, places[1]);
        assert_eq!(after.slot, Some(2));
    }

    #[test]
    fn dropping_a_task_on_a_branch_header_files_it_at_the_end_of_that_branch() {
        // Not "keeps its old index and appears in the middle of the branch" —
        // which is what happens if the slot is left alone, and reads as the
        // drop having landed somewhere else.
        let places = vec![
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place::default(),
        ];
        let inis = [initiative(10, Some(1), false)];
        let landing = land_task(&places, &inis, Drop::Into(RowId::Initiative(10))).expect("lands");
        assert_eq!(
            landing.place,
            Place {
                project: Some(1),
                initiative: Some(10)
            }
        );
        assert_eq!(landing.slot, Some(2), "after the branch's last task");

        // Into an EMPTY branch there is no seat to take, so the tab keeps its
        // place in the order and only its branch changes.
        let empty = land_task(&places, &inis, Drop::Into(RowId::Project(9))).expect("lands");
        assert_eq!(empty.place.project, Some(9));
        assert_eq!(empty.slot, None);
    }

    #[test]
    fn a_task_cannot_be_seated_between_two_branches() {
        // The edges of a branch header mean the same as its middle for a task.
        // Anything else would invent a place — "between projects" — that no
        // task can occupy.
        let places = vec![Place::default()];
        let inis = [initiative(10, None, false)];
        let edge = land_task(&places, &inis, Drop::Before(RowId::Project(3))).expect("lands");
        assert_eq!(edge.place.project, Some(3));
        assert_eq!(edge.place.initiative, None);
    }

    #[test]
    fn an_initiative_dropped_beside_another_adopts_its_project_and_its_side() {
        let places = vec![
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place {
                project: Some(2),
                initiative: Some(20),
            },
            Place {
                project: Some(2),
                initiative: Some(20),
            },
        ];
        let inis = [
            initiative(10, Some(1), false),
            initiative(20, Some(2), false),
        ];
        let before = land_initiative(&places, &inis, 10, Drop::Before(RowId::Initiative(20)))
            .expect("lands");
        assert_eq!(before.project, Some(2));
        assert_eq!(before.slot, Some(1), "the first seat of 20's run");
        let after =
            land_initiative(&places, &inis, 10, Drop::After(RowId::Initiative(20))).expect("lands");
        assert_eq!(after.project, Some(2));
        assert_eq!(after.slot, Some(3), "one past 20's last task");
    }

    #[test]
    fn the_tree_refuses_the_third_layer() {
        // An initiative inside an initiative, or beside a task, is the depth
        // this tree deliberately does not have. It must resolve to nothing
        // rather than to something surprising.
        let places = vec![Place {
            project: Some(1),
            initiative: Some(10),
        }];
        let inis = [initiative(10, Some(1), false)];
        assert_eq!(
            land_initiative(&places, &inis, 10, Drop::Into(RowId::Initiative(10))),
            None
        );
        assert_eq!(
            land_initiative(&places, &inis, 10, Drop::Before(RowId::Task(0))),
            None
        );
        // and it cannot be dropped onto itself
        assert_eq!(
            land_initiative(&places, &inis, 10, Drop::Before(RowId::Initiative(10))),
            None
        );
    }

    /// Build a deterministic spread of trees — every combination of two
    /// projects, three initiatives (one dangling, one unfiled) and folds on and
    /// off — and hold the three invariants over all of them.
    ///
    /// Written as a sweep rather than as more hand-picked cases because the
    /// invariants are what the feature IS: a tree that loses a task has lost
    /// somebody's work from the only complete index of the session, and the
    /// shapes that do it are the ones nobody thought to write a case for.
    fn sweep(mut check: impl FnMut(&[Row], &[TaskRef], Option<usize>)) {
        let places = [
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place {
                project: Some(1),
                initiative: None,
            },
            Place {
                project: Some(2),
                initiative: Some(11),
            },
            Place {
                project: Some(2),
                initiative: None,
            },
            Place {
                project: None,
                initiative: Some(12),
            },
            Place {
                project: None,
                initiative: None,
            },
            // a task pointing at a project that does not exist: the restore
            // path filters these, but the row builder must not depend on that
            Place {
                project: Some(99),
                initiative: None,
            },
        ];
        for mask in 0u32..(1 << 7) {
            let tasks: Vec<TaskRef> = places
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(i, place)| TaskRef {
                    place: *place,
                    roll: Roll::task(i % 2, 0, 0, 0, i % 3, 1),
                })
                .collect();
            if tasks.is_empty() {
                continue;
            }
            for folds in 0u32..8 {
                let projects = [project(1, folds & 1 != 0), project(2, folds & 2 != 0)];
                let inis = [
                    initiative(10, Some(1), folds & 4 != 0),
                    initiative(11, Some(2), false),
                    // an initiative whose project is not in the list
                    initiative(12, Some(77), false),
                ];
                for active in [None, Some(0), Some(tasks.len() - 1)] {
                    let rows = rows(&projects, &inis, &tasks, active);
                    check(&rows, &tasks, active);
                }
            }
        }
    }

    #[test]
    fn across_every_shape_each_task_is_listed_once_or_folded_away() {
        sweep(|rows, tasks, _| {
            let mut seen: Vec<usize> = rows
                .iter()
                .filter_map(|r| match r {
                    Row::Task { index, .. } => Some(*index),
                    _ => None,
                })
                .collect();
            let before = seen.len();
            seen.sort_unstable();
            seen.dedup();
            assert_eq!(before, seen.len(), "a task was listed twice: {rows:?}");
            assert!(
                seen.len() <= tasks.len(),
                "more task rows than tasks: {rows:?}"
            );
            // Anything missing is missing because a branch above it is folded —
            // never because the builder dropped it.
            for (i, _) in tasks.iter().enumerate() {
                if !seen.contains(&i) {
                    let folded = rows.iter().any(|r| match r {
                        Row::Project { collapsed, .. } | Row::Initiative { collapsed, .. } => {
                            *collapsed
                        }
                        _ => false,
                    });
                    assert!(folded, "task {i} vanished with nothing folded: {rows:?}");
                }
            }
        });
    }

    #[test]
    fn across_every_shape_the_active_task_is_always_on_screen() {
        sweep(|rows, _, active| {
            let Some(active) = active else { return };
            let shown = rows
                .iter()
                .any(|r| matches!(r, Row::Task { index, .. } if *index == active));
            assert!(
                shown,
                "the active task {active} was folded away — the window has lost your place: {rows:?}"
            );
        });
    }

    #[test]
    fn across_every_shape_the_indentation_reads_as_a_tree() {
        // A row deeper than its parent by more than one step, or a task at the
        // depth of a project it does not belong to, is a tree that lies about
        // what contains what.
        sweep(|rows, _, _| {
            let mut last_branch_depth = 0u8;
            for row in rows {
                match row {
                    Row::Project { depth, .. } => {
                        assert_eq!(*depth, 0, "a project is always a root: {rows:?}");
                        last_branch_depth = 0;
                    }
                    Row::Initiative { depth, .. } => {
                        assert!(
                            *depth <= 1,
                            "an initiative is never deeper than one: {rows:?}"
                        );
                        last_branch_depth = *depth;
                    }
                    Row::Task { depth, .. } => {
                        assert!(*depth <= 2, "a task is never deeper than two: {rows:?}");
                        assert!(
                            *depth <= last_branch_depth + 1,
                            "a task indented past its branch: {rows:?}"
                        );
                    }
                    Row::Unfiled { depth } => {
                        assert_eq!(*depth, 0, "the loose divider is a root: {rows:?}");
                        last_branch_depth = 0;
                    }
                }
            }
        });
    }

    #[test]
    fn widening_always_produces_a_scope_that_shows_the_task() {
        // The strip's half of the same promise. Whatever place a task is in,
        // and whatever the scope was, one widening is enough — never two, and
        // never a scope that still hides it.
        let places = [
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place {
                project: Some(2),
                initiative: None,
            },
            Place {
                project: None,
                initiative: Some(12),
            },
            Place::default(),
        ];
        let scopes = [
            Scope::All,
            Scope::Project(1),
            Scope::Project(2),
            Scope::Project(99),
            Scope::Initiative(10),
            Scope::Initiative(12),
        ];
        for scope in scopes {
            for place in places {
                let settled = scope.widened_for(&place).unwrap_or(scope);
                assert!(
                    settled.shows(&place),
                    "{scope:?} widened for {place:?} to {settled:?}, which still hides it"
                );
                assert_eq!(
                    settled.widened_for(&place),
                    None,
                    "widening twice: {scope:?} -> {settled:?} for {place:?}"
                );
            }
        }
    }

    #[test]
    fn every_landing_puts_a_task_somewhere_the_tree_can_draw_it() {
        // A drop that resolved to "in initiative 10, but in project 2" would
        // render under a branch it does not belong to. The landing's place must
        // agree with the initiative's own project, always.
        let places = vec![
            Place {
                project: Some(1),
                initiative: Some(10),
            },
            Place {
                project: Some(2),
                initiative: Some(20),
            },
            Place::default(),
        ];
        let inis = [
            initiative(10, Some(1), false),
            initiative(20, Some(2), false),
        ];
        let targets = [
            RowId::Project(1),
            RowId::Project(2),
            RowId::Initiative(10),
            RowId::Initiative(20),
            RowId::Task(0),
            RowId::Task(1),
            RowId::Task(2),
            RowId::Unfiled,
        ];
        for target in targets {
            for drop in [
                Drop::Into(target),
                Drop::Before(target),
                Drop::After(target),
            ] {
                let Some(landing) = land_task(&places, &inis, drop) else {
                    continue;
                };
                if let Some(g) = landing.place.initiative {
                    assert_eq!(
                        landing.place.project,
                        project_of(&inis, g),
                        "{drop:?} landed a task in initiative {g} under the wrong project"
                    );
                }
                if let Some(slot) = landing.slot {
                    assert!(
                        slot <= places.len(),
                        "{drop:?} produced slot {slot}, past the end of the tab list"
                    );
                }
            }
        }
    }

    #[test]
    fn reordering_ids_moves_one_and_disturbs_nothing_else() {
        let mut ids = vec![1, 2, 3, 4];
        reorder(&mut ids, 4, 2, false);
        assert_eq!(ids, vec![1, 4, 2, 3]);
        reorder(&mut ids, 1, 3, true);
        assert_eq!(ids, vec![4, 2, 3, 1]);
        // a no-op is a no-op, not a shuffle
        reorder(&mut ids, 2, 2, false);
        assert_eq!(ids, vec![4, 2, 3, 1]);
        // an id that is not in the list leaves the list alone
        reorder(&mut ids, 99, 3, false);
        assert_eq!(ids, vec![4, 2, 3, 1]);
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
