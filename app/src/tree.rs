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

/// Which branch a task hangs from most immediately.
///
/// Two levels can be a task's parent and only one of them ever is, so this is an
/// either rather than a pair — a caller that has to ask "initiative, or project
/// if that is None" at each site is a caller that will eventually forget at one
/// of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nearest {
    Initiative(u32),
    Project(u32),
}

impl Place {
    pub fn unfiled(&self) -> bool {
        self.project.is_none() && self.initiative.is_none()
    }

    /// The NEAREST branch above this task: its initiative, or its project when
    /// it belongs to no initiative, or nothing when it is unfiled.
    ///
    /// Nearest rather than a fixed rung, because a task may hang from either
    /// level and asking for a specific one prints a dash whenever the task does
    /// not use it. That is what the attention rail did — it read the project
    /// slot, which a GROUPED task always leaves empty, and so labelled every
    /// grouped task with a dash for a project it really was in.
    pub fn nearest(&self) -> Option<Nearest> {
        self.initiative
            .map(Nearest::Initiative)
            .or(self.project.map(Nearest::Project))
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
    /// The divider above the loose tasks. Drawn whenever there is something for
    /// a task to be loose FROM, whether or not any task currently is — it is
    /// the target that means "out of every branch" (see [`rows`]).
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
/// - **A fold is a fold.** Every `collapsed` flag is obeyed, including on the
///   branch holding the task you are in. This module used to force-expand the
///   active task's branches on every frame, which kept the window from losing
///   your place but made the fold gesture DEAD on the one branch a person is
///   most likely to want shut — the one they are working in. The promise is
///   kept in `Workspace::reveal_active_branch` instead: opened when the active
///   task CHANGES, not while you are looking at it.
/// - **The unfiled divider is drawn whenever anything above it is filed, empty
///   below or not.** It is the only row that means "out of every branch", so
///   the drag that takes a tab OUT of a group needs it on screen. It used to
///   wait until some other tab was already loose, which made leaving a group
///   the one gesture a fully-filed session could not perform: the target only
///   existed once you no longer needed it. The single case that still
///   suppresses it is a tree with nothing else in it — a session that has never
///   been organised is a flat list of tasks, and labelling the whole window
///   "unfiled" says nothing while costing a line.
pub fn rows(projects: &[ProjectRef], initiatives: &[InitiativeRef], tasks: &[TaskRef]) -> Vec<Row> {
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
        let collapsed = ini.collapsed;
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
        let collapsed = p.collapsed;
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
    // The divider separates what is filed from what is not, and the half that
    // earns it is the one ABOVE: with nothing filed there is nothing to be
    // loose from, and with something filed the line is where a drag goes to
    // leave a branch. A target that appears only once it is no longer needed is
    // not a target.
    if !out.is_empty() {
        out.push(Row::Unfiled { depth: 0 });
    }
    for i in loose {
        out.push(Row::Task { index: i, depth: 0 });
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
/// Where the active tab ends up once `removed` indices are lifted out.
///
/// `removed` is ascending and indexes the list as it was BEFORE anything was
/// taken; `left` is how many tabs remain after.
///
/// Clamping alone is not enough and looks like it is, which is why this is a
/// function with tests rather than three lines at each call site. Take
/// `[A,B,C,D,E]` with `D` active at 3, delete `B` and `C`, and the list becomes
/// `[A,D,E]` — `D` is now at 1, while `min(3, 2)` says 2, which is `E`. The
/// window would come back focused on a terminal nobody asked for, in a session
/// where the one you were working in is still right there. Every index above a
/// removal shifts down by one per removal below it.
///
/// When the active tab was itself removed there is no right answer, only a
/// sensible one: the position the deleted run occupied, which is where the eye
/// already is.
pub fn active_after_removal(active: usize, removed: &[usize], left: usize) -> usize {
    if left == 0 {
        return 0;
    }
    let last = left - 1;
    if removed.contains(&active) {
        return removed.first().copied().unwrap_or(0).min(last);
    }
    let below = removed.iter().filter(|&&i| i < active).count();
    active.saturating_sub(below).min(last)
}

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

/// The sibling to land on when Up/Down walks a carried item at one depth:
/// next/previous in `siblings`, wrapping — or the first/last (by direction)
/// when nothing is currently selected there, the same seed-by-direction
/// convention `bar_leave`'s first press already uses.
///
/// `None` also when `current` is the only element `siblings` has: wrapping
/// from a singleton lands back on itself, which is "nowhere else to go",
/// not a move. Callers that read `None` as "this layer is exhausted,
/// escalate" (`Workspace::carry_to_sibling_initiative`/`_project`) depend on
/// that — a self-wrap read as a successful move was a real, confirmed bug
/// (a project with exactly one initiative, or a workspace with exactly one
/// project, silently "moved" a task to the far end of its own unchanged
/// branch instead of escalating past it).
pub fn sibling_landing(siblings: &[u32], current: Option<u32>, down: bool) -> Option<u32> {
    if siblings.is_empty() {
        return None;
    }
    let at = current.and_then(|id| siblings.iter().position(|&s| s == id));
    let next = match at {
        Some(p) if down => (p + 1) % siblings.len(),
        Some(p) => (p + siblings.len() - 1) % siblings.len(),
        None if down => 0,
        None => siblings.len() - 1,
    };
    if at == Some(next) {
        return None;
    }
    Some(siblings[next])
}

/// The tasks the strip draws, in tab order: the active task's own branch —
/// its group's tabs, or its project's loose ones — and nothing else.
///
/// Parker, on a strip carrying every tab in the window: *"the top area should
/// only have tabs for the GROUP — not even sibling groups"*; and on the shut
/// strip, *"PROJECT > GROUPS … > showing sibling tabs is proper"*. The sibling
/// groups appear in the strip's head as chips, never as their tabs.
///
/// There is no scope any more. The strip used to obey a scope chip and then
/// branch-row PINS, and every stage of that produced a complaint: a chip that
/// said ALL over three tabs, a default of the whole session, a second press on
/// your own group that put 31 tabs across the top, a pin lit over a strip that
/// had stopped obeying it. Pins were retired in #757 — a click on a branch now
/// GOES there — so the strip is what it was always meant to be, and there is
/// no state left that can put a stranger's tabs on it.
///
/// Never empty while `active` names a task: that task is always in its own
/// branch. An `active` that indexes nothing reads as a loose unfiled tab, the
/// same as `Place::default()`, and the answer is simply whatever else is loose
/// and unfiled — possibly nothing, never a loop.
///
/// Kept beside [`rows`] on purpose: the strip and the tree answer the same
/// question about the same session and must never disagree about which tasks
/// exist.
pub fn family(places: &[Place], active: usize) -> Vec<usize> {
    let home = places.get(active).copied().unwrap_or_default();
    places
        .iter()
        .enumerate()
        .filter(|(_, place)| same_branch(place, &home))
        .map(|(i, _)| i)
        .collect()
}

/// Do these two tasks hang from the SAME branch of the tree?
///
/// The one definition of the strip's key, and it has to be one definition:
/// the filter, the heading over it and the slot a new tab lands in all have to
/// agree, or the strip draws a set of tabs under a name that does not describe
/// them. An initiative is a branch. A task in no initiative belongs to its
/// PROJECT's loose bucket, not to a single global one — two projects' loose
/// tasks are no more siblings than two projects' initiatives are, and drawing
/// them together under one project's name was the bug that made this a
/// function instead of a field comparison.
pub fn same_branch(a: &Place, b: &Place) -> bool {
    match (a.initiative, b.initiative) {
        (Some(x), Some(y)) => x == y,
        (None, None) => a.project == b.project,
        _ => false,
    }
}

/// Where the drag caret goes: how many of the strip's tabs are drawn BEFORE it.
///
/// The caret marks a gap, not an index. Drop slots are resolved in full-list
/// index space while the strip draws one branch, so a family whose tabs are not
/// adjacent — any session organised before a branch became contiguous — has
/// landing slots belonging to tabs nobody can see. Every such slot collapses
/// onto the visible gap it falls in, which is exactly where the drop lands.
/// Matching the slot against a visible index instead drew no caret at all for
/// those slots, and a drag with no feedback reads as a drag that will not work.
///
/// Returns `family.len()` for a slot past the last visible tab: the caret goes
/// at the end of the strip.
pub fn caret_gap(family: &[usize], slot: usize) -> usize {
    family.iter().take_while(|&&i| i < slot).count()
}

// `roll_outside` and `roll_glyphs` used to live here: the sum of every branch
// the strip is not carrying, rendered as one line of glyphs for the mother
// bar's out-of-branch chip. The chip is gone — the tree states the same rollup
// branch by branch, and `Workspace::roll_badges` draws it there with the
// loudest state still animated, which the flattened string could not do. Two
// implementations of one summary, and the surviving one is the richer.

// ── the keyboard cursor ────────────────────────────────────────────────────
//
// Ctrl+Alt+↑/↓ walk the bar; Ctrl+Alt+→/← open and close what they are over.
// Everything below is a pure function over the output of [`rows`], and that is
// the whole trick: `rows` has ALREADY dropped the children of a folded branch,
// so stepping past a folded project skips its entire subtree without a single
// line here knowing that a subtree exists. Walking the projects/initiatives/
// tasks structures directly instead would have needed its own fold rules, kept
// in step with `rows` by hand, and the first divergence would have been a
// cursor that landed on a row nobody can see.
//
// The consequence to hold on to: the cursor moves over what is DRAWN. Fold a
// branch and the same keypress travels further, which is the behaviour asked
// for — visual state is maintained rather than overridden by the keyboard.

/// Where the cursor sits. `None` on the unfiled divider, which is a hairline
/// rather than a destination — arriving there would give the eye nothing to
/// look at and the arrows nothing to do.
pub fn row_id(row: &Row) -> Option<RowId> {
    Some(match *row {
        Row::Project { id, .. } => RowId::Project(id),
        Row::Initiative { id, .. } => RowId::Initiative(id),
        Row::Task { index, .. } => RowId::Task(index),
        Row::Unfiled { .. } => return None,
    })
}

/// How far in a row is indented. The divider has a depth like anything else;
/// it is simply never a stop.
fn depth_of(row: &Row) -> u8 {
    match *row {
        Row::Project { depth, .. }
        | Row::Initiative { depth, .. }
        | Row::Task { depth, .. }
        | Row::Unfiled { depth } => depth,
    }
}

/// Is this branch folded? `None` for a task or the divider — not `false`: a
/// task is not an expanded branch, and → must not try to open one.
pub fn folded(rows: &[Row], of: RowId) -> Option<bool> {
    rows.iter().find_map(|r| match *r {
        Row::Project { id, collapsed, .. } if RowId::Project(id) == of => Some(collapsed),
        Row::Initiative { id, collapsed, .. } if RowId::Initiative(id) == of => Some(collapsed),
        _ => None,
    })
}

/// Every row the cursor may land on, in draw order.
pub fn stops(rows: &[Row]) -> Vec<RowId> {
    rows.iter().filter_map(row_id).collect()
}

/// Where a walk with no live cursor BEGINS: the row of the active task.
///
/// The bar is a map of the session, and the one place a person is certain to
/// already be is the tab they are working in. Entering the tree anywhere else
/// meant every walk began by travelling back to where you already were, past
/// rows you had no business highlighting.
///
/// This is an ORIGIN, and for the arrows it is not a destination — [`walk`]
/// steps off it, so the first press moves. →/← do land on it, because there
/// the seed is the thing being acted on rather than a place to leave.
///
/// `None` when that task is not DRAWN — its branch is folded over it — and the
/// caller falls back to entering at the near end. The cursor never lands on a
/// row nobody can see; that is the same rule [`step`] holds.
pub fn seed(rows: &[Row], active: usize) -> Option<RowId> {
    let at = RowId::Task(active);
    stops(rows).contains(&at).then_some(at)
}

/// Where Ctrl+Alt+↑/↓ LANDS: one row on from wherever the person already is.
///
/// A first press that merely lit up the active task spent itself announcing
/// something its reader was already looking at, so ↑ cost two presses to reach
/// the row above and the first of them was indistinguishable from a dead key.
/// The seed is what the walk starts FROM, never what it arrives at.
///
/// `cursor` is the LIVE cursor — a stale one is not a place to step off. With
/// no cursor and an active task that is not drawn there is nothing to step off
/// at all, and [`step`] enters at the end the press came from, which is still a
/// move.
pub fn walk(rows: &[Row], cursor: Option<RowId>, active: usize, down: bool) -> Option<RowId> {
    step(rows, cursor.or_else(|| seed(rows, active)), down)
}

/// One step of the cursor. The list is a RING: ↓ from the last row lands on the
/// first, ↑ from the first lands on the last, and `None` now means only that
/// there was nothing to land on at all.
///
/// The walls this replaced were borrowed from Alt+arrows over PANES, where they
/// are right — panes have a geometry, so "up from the top pane" names a
/// direction with nothing in it. The left bar is a menu, and a menu closes. With
/// a wall at each end, the cheapest row to reach from the bottom of a twenty-row
/// tree is the one at the very top and the dearest is the one directly above it,
/// which is the opposite of how the bar is read.
///
/// A cursor sitting on a row that no longer exists (its branch was folded away
/// under it, or its tab closed) re-enters at the near end rather than vanishing.
pub fn step(rows: &[Row], from: Option<RowId>, down: bool) -> Option<RowId> {
    let stops = stops(rows);
    let n = stops.len();
    if n == 0 {
        return None;
    }
    let at = from.and_then(|f| stops.iter().position(|s| *s == f));
    Some(match (at, down) {
        (Some(i), true) => stops[(i + 1) % n],
        (Some(i), false) => stops[(i + n - 1) % n],
        // No cursor yet (or a stale one): land at the end the press came from.
        (None, true) => stops[0],
        (None, false) => stops[n - 1],
    })
}

/// The top-level branches, in draw order — the rows the number keys address.
///
/// Depth zero and a branch: every project, then every initiative that hangs
/// from no project, which is the order [`rows`] lays them out and therefore the
/// order the eye counts them in. The loose tasks that also sit at depth zero
/// are NOT included: a number is a jump to a section of the session, and a
/// task's number would move every time a neighbour was filed. The divider is
/// never a stop anywhere.
pub fn top_branches(rows: &[Row]) -> Vec<RowId> {
    rows.iter()
        .filter(|r| depth_of(r) == 0)
        .filter_map(|r| match *r {
            Row::Project { id, .. } => Some(RowId::Project(id)),
            Row::Initiative { id, .. } => Some(RowId::Initiative(id)),
            Row::Task { .. } | Row::Unfiled { .. } => None,
        })
        .collect()
}

/// The branch the digit `n` addresses, counting from one.
///
/// Nine and no further. Zero is not a tenth row, and a two-digit number cannot
/// be typed without the chord pausing on every press to see whether a second
/// digit is coming — a delay on all nine to buy a shortcut to the tenth. A
/// tree with a tenth top-level branch is walked to with the arrows, which reach
/// every row there is.
pub fn nth_top_branch(rows: &[Row], n: usize) -> Option<RowId> {
    if !(1..=9).contains(&n) {
        return None;
    }
    top_branches(rows).get(n - 1).copied()
}

/// The branch a row hangs from: the nearest row above it drawn shallower.
///
/// Read off the drawn list rather than from `Place`, so it answers for an
/// initiative (whose parent is a project) and a task with the same code, and so
/// it can never name a parent that is not on screen.
pub fn parent(rows: &[Row], of: RowId) -> Option<RowId> {
    let i = rows.iter().position(|r| row_id(r) == Some(of))?;
    let mine = depth_of(&rows[i]);
    rows[..i]
        .iter()
        .rev()
        .find(|r| depth_of(r) < mine)
        .and_then(row_id)
}

/// The first row drawn under a branch, if it has one and is showing it.
pub fn first_child(rows: &[Row], of: RowId) -> Option<RowId> {
    let i = rows.iter().position(|r| row_id(r) == Some(of))?;
    let mine = depth_of(&rows[i]);
    let next = rows.get(i + 1)?;
    (depth_of(next) > mine).then(|| row_id(next)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sibling_landing_wraps_and_seeds_by_direction() {
        let ids = [10, 20, 30];
        assert_eq!(sibling_landing(&ids, Some(10), true), Some(20));
        assert_eq!(
            sibling_landing(&ids, Some(30), true),
            Some(10),
            "wraps forward"
        );
        assert_eq!(
            sibling_landing(&ids, Some(10), false),
            Some(30),
            "wraps backward"
        );
        assert_eq!(
            sibling_landing(&ids, None, true),
            Some(10),
            "seeds first going down"
        );
        assert_eq!(
            sibling_landing(&ids, None, false),
            Some(30),
            "seeds last going up"
        );
        assert_eq!(
            sibling_landing(&ids, Some(999), true),
            Some(10),
            "an id not among the siblings seeds fresh, same as no current id"
        );
        assert_eq!(
            sibling_landing(&[], Some(10), true),
            None,
            "no siblings, nowhere to land"
        );
        assert_eq!(
            sibling_landing(&[10], Some(10), true),
            None,
            "a singleton wraps back onto itself, which is not a move"
        );
        assert_eq!(
            sibling_landing(&[10], Some(10), false),
            None,
            "singleton self-wrap is direction-independent"
        );
        assert_eq!(
            sibling_landing(&[10], None, true),
            Some(10),
            "a singleton still seeds fresh when nothing is currently selected"
        );
    }

    /// The nearest parent, in all four shapes a task can be filed in.
    ///
    /// The grouped case is the one that was wrong on screen: a task under the
    /// JOB initiative read as `—:JOB`, a dash standing in for a project the task
    /// really is in, because the label was taken from the project slot that a
    /// grouped task always leaves empty. Its nearest parent is the initiative,
    /// and the project is a rung further up.
    #[test]
    fn the_nearest_parent_is_the_initiative_when_there_is_one() {
        let grouped = Place {
            project: Some(7),
            initiative: Some(3),
        };
        let loose_in_project = Place {
            project: Some(7),
            initiative: None,
        };
        let unfiled = Place::default();

        assert_eq!(grouped.nearest(), Some(Nearest::Initiative(3)));
        assert_eq!(
            loose_in_project.nearest(),
            Some(Nearest::Project(7)),
            "a task with no initiative hangs from its project directly"
        );
        assert_eq!(unfiled.nearest(), None, "unfiled hangs from nothing");
        assert!(unfiled.unfiled());
    }

    /// An initiative with no project still answers. The rung above a parent has
    /// nothing to do with whether the parent exists, and a task filed under a
    /// group that nobody has put in a project yet must still say which group.
    #[test]
    fn an_initiative_without_a_project_is_still_the_nearest_parent() {
        let p = Place {
            project: None,
            initiative: Some(3),
        };
        assert_eq!(p.nearest(), Some(Nearest::Initiative(3)));
        assert!(!p.unfiled());
    }

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
        let rows = rows(&[], &[], &tasks);
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
        let rows = rows(&[], &[initiative(7, None, false)], &tasks);
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
                // Nothing is loose here, and the line is drawn anyway: it is
                // where a task goes to STOP being filed.
                ("unfiled", 0),
            ]
        );
    }

    #[test]
    fn a_fully_filed_session_still_draws_the_line_you_drag_out_to() {
        // The bug this pins: with every tab in a group the divider was
        // suppressed, and the only drop target meaning "out of the group" was
        // the one row that only existed once a tab was already out. Leaving a
        // group needed a tab that had already left.
        let tasks = vec![task(None, Some(7)), task(None, Some(7))];
        let rows = rows(&[], &[initiative(7, None, false)], &tasks);
        assert_eq!(
            rows.last(),
            Some(&Row::Unfiled { depth: 0 }),
            "the line is the bottom of the tree: {rows:?}"
        );
        // ...and it is still scenery. An empty loose section must not park the
        // keyboard cursor on a hairline with nothing under it.
        assert_eq!(
            stops(&rows),
            vec![RowId::Initiative(7), RowId::Task(0), RowId::Task(1)]
        );
        // The walk stays a ring over the stops, so ↑ from nowhere enters on the
        // last TASK rather than falling off the divider.
        assert_eq!(walk(&rows, None, 9, false), Some(RowId::Task(1)));
        // Dropping on it is the ungroup: no project, no initiative.
        let places: Vec<Place> = tasks.iter().map(|t| t.place).collect();
        let inis = [initiative(7, None, false)];
        assert_eq!(
            land_task(&places, &inis, Drop::Into(RowId::Unfiled))
                .expect("the divider is a landing")
                .place,
            Place::default(),
            "a release on the line files under nothing"
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
        );
        assert_eq!(
            rows,
            vec![
                Row::Project {
                    id: 1,
                    depth: 0,
                    roll: Roll {
                        needs_input: 3,
                        panes: 2,
                        tasks: 2,
                        ..Default::default()
                    },
                    collapsed: true,
                },
                // The fold hides the tasks, not the place a task goes to leave.
                Row::Unfiled { depth: 0 },
            ]
        );
    }

    #[test]
    fn the_branch_holding_the_active_task_folds_like_any_other() {
        // The behaviour Parker asked for by name: Ctrl+Alt+← on a collapsible
        // row collapses it, including the branch he is working in. This module
        // used to overrule that fold on every frame — the flag was written and
        // the row was drawn open anyway — so the gesture was dead on exactly
        // the two rows nearest the work. The active task is kept reachable by
        // `Workspace::reveal_active_branch` when it is ACTIVATED, and by the
        // mother bar, which is still showing it while the tree is shut.
        let tasks = vec![task(Some(1), Some(10))];
        let rows = rows(
            &[project(1, true)],
            &[initiative(10, Some(1), true)],
            &tasks,
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
                    collapsed: true,
                },
                Row::Unfiled { depth: 0 },
            ],
            "a folded project draws one branch row whoever is active inside it, \
             over the line every branch can be left by"
        );
    }

    #[test]
    fn folds_apply_one_branch_at_a_time() {
        let tasks = vec![task(Some(1), None), task(Some(2), None)];
        let rows = rows(&[project(1, true), project(2, false)], &[], &tasks);
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
        let rows = rows(&[], &[initiative(10, Some(99), false)], &tasks);
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
        assert_eq!(rows.get(1), Some(&Row::Task { index: 0, depth: 1 }));
        assert_eq!(rows.last(), Some(&Row::Unfiled { depth: 0 }));
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
    fn the_strip_is_the_active_task_s_group_and_no_sibling() {
        // Parker, on a strip carrying every tab in the window: *"the top area
        // should only have tabs for the GROUP — not even sibling groups"*.
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
        // in initiative 10: its two tabs. Initiative 11 is a SIBLING under the
        // same project and stays off the strip; so does everything else.
        assert_eq!(family(&places, 0), vec![0, 1]);
        assert_eq!(family(&places, 1), vec![0, 1]);
        // the sibling, from the other side
        assert_eq!(family(&places, 2), vec![2]);
    }

    #[test]
    fn activating_a_task_in_another_branch_moves_the_strip_with_it() {
        // Following is a narrowing as well as a widening, and it costs nothing
        // to undo because nothing is stored: the strip is re-asked of the
        // active task every frame.
        let places: Vec<Place> = [
            task(Some(1), Some(10)),
            task(Some(1), Some(11)),
            task(Some(2), Some(12)),
        ]
        .iter()
        .map(|t| t.place)
        .collect();
        assert_eq!(family(&places, 0), vec![0]);
        assert_eq!(family(&places, 1), vec![1]);
        assert_eq!(family(&places, 2), vec![2]);
        // and the strip is never empty: whatever is active is on it
        for active in 0..places.len() {
            assert!(
                family(&places, active).contains(&active),
                "the strip dropped the task it is standing on"
            );
        }
    }

    #[test]
    fn a_loose_task_s_branch_is_its_project_s_loose_bucket_not_the_project() {
        // A loose task's siblings are its project's other loose tasks — never
        // that project's GROUPS, which it is not in.
        let places: Vec<Place> = [
            task(Some(1), Some(10)),
            task(Some(1), None),
            task(Some(1), None),
            task(Some(2), None),
        ]
        .iter()
        .map(|t| t.place)
        .collect();
        assert_eq!(family(&places, 1), vec![1, 2]);
    }

    #[test]
    fn a_task_in_no_project_is_its_own_bucket_and_not_everyones() {
        // One branch at a time. Loose in project 1, loose in project 2 and
        // filed nowhere are three buckets, and a strip that mixed them would
        // draw tabs under a head that does not describe them.
        let places: Vec<Place> = [
            task(Some(1), None),
            task(Some(2), None),
            task(Some(1), None),
            task(None, None),
        ]
        .iter()
        .map(|t| t.place)
        .collect();
        assert_eq!(family(&places, 3), vec![3]);
        assert_eq!(family(&places, 0), vec![0, 2]);
        assert_eq!(family(&places, 1), vec![1]);
    }

    #[test]
    fn an_initiative_ignores_the_project_when_deciding_who_is_in_it() {
        // A group carries its own project, and its members leave theirs unset
        // (see `Workspace::place_of`), so initiative identity alone decides —
        // and a project disagreeing must never split a group in half.
        let a = Place {
            project: Some(1),
            initiative: Some(10),
        };
        let b = Place {
            project: Some(2),
            initiative: Some(10),
        };
        assert!(same_branch(&a, &b));
        assert!(!same_branch(
            &a,
            &Place {
                project: Some(1),
                initiative: None,
            }
        ));
    }

    #[test]
    fn a_session_that_never_organised_anything_still_sees_every_tab() {
        // The identity case, and the one that decides whether this is safe to
        // ship to somebody who has never made a group: one bucket holding
        // everything, so the strip hides nothing from them.
        let places: Vec<Place> = (0..4).map(|_| task(None, None).place).collect();
        assert_eq!(family(&places, 0), vec![0, 1, 2, 3]);
    }

    #[test]
    fn an_active_index_that_names_nothing_reads_as_unfiled_and_terminates() {
        // An `active` past the end has no place of its own, so it reads as a
        // loose unfiled tab: the answer is whatever else is loose and unfiled.
        let places: Vec<Place> = [task(Some(1), Some(10)), task(None, None)]
            .iter()
            .map(|t| t.place)
            .collect();
        assert_eq!(family(&places, 9), vec![1]);
        // With no tabs at all the answer is empty, not an invented row.
        assert_eq!(family(&[], 0), Vec::<usize>::new());
        // And where nothing is unfiled either, empty too — never a search for
        // a branch that is not there.
        let filed: Vec<Place> = [task(Some(1), Some(10))].iter().map(|t| t.place).collect();
        assert_eq!(family(&filed, 9), Vec::<usize>::new());
    }

    #[test]
    fn the_drag_caret_finds_a_gap_even_when_the_branch_is_not_contiguous() {
        // Underlying order G1 H1 G2 H2 with the G branch showing: the slot
        // between G1 and H1 is index 1, which is a tab the strip does not draw.
        // It has to land in the only gap it can mean — after G1.
        let family = [0usize, 2];
        assert_eq!(caret_gap(&family, 0), 0); // before the first
        assert_eq!(caret_gap(&family, 1), 1); // the invisible slot, one gap in
        assert_eq!(caret_gap(&family, 2), 1); // same gap, said the visible way
        assert_eq!(caret_gap(&family, 3), 2); // after the last
        assert_eq!(caret_gap(&family, 4), 2); // past the end of the whole list
    }

    #[test]
    fn every_slot_in_a_contiguous_branch_still_maps_to_its_own_gap() {
        // The ordinary case must not have been traded away for the odd one:
        // with a contiguous family each slot keeps its own caret position.
        let family = [3usize, 4, 5];
        assert_eq!(caret_gap(&family, 3), 0);
        assert_eq!(caret_gap(&family, 4), 1);
        assert_eq!(caret_gap(&family, 5), 2);
        assert_eq!(caret_gap(&family, 6), 3);
        // an empty strip has exactly one gap, and it is the end
        assert_eq!(caret_gap(&[], 7), 0);
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
                    // What `Workspace::reveal_active_branch` does when a task
                    // is activated, modelled on the row builder's own inputs:
                    // open the branches it hangs from, then draw. Drawing
                    // without it is the other half of the sweep, one line up in
                    // `across_every_shape_each_task_is_listed_once_or_folded_away`.
                    let mut opened = projects;
                    let mut opened_inis = inis;
                    if let Some(place) = active.and_then(|i| tasks.get(i)).map(|t| t.place) {
                        for p in opened.iter_mut().filter(|p| Some(p.id) == place.project) {
                            p.collapsed = false;
                        }
                        for g in opened_inis
                            .iter_mut()
                            .filter(|g| Some(g.id) == place.initiative)
                        {
                            g.collapsed = false;
                        }
                    }
                    let rows = rows(&opened, &opened_inis, &tasks);
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
    fn across_every_shape_activating_a_task_puts_it_on_screen() {
        // The promise, restated where it now lives: a fold may hide the task
        // you are in, but ACTIVATING one always opens its way down to it. The
        // sweep applies the reveal before drawing (see `sweep`), so this fails
        // if opening a task's project and initiative is ever not enough — a
        // third layer, say, or a task filed under a branch it does not name.
        sweep(|rows, _, active| {
            let Some(active) = active else { return };
            let shown = rows
                .iter()
                .any(|r| matches!(r, Row::Task { index, .. } if *index == active));
            assert!(
                shown,
                "activating task {active} left it folded away: {rows:?}"
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
    fn a_branch_with_nothing_to_say_is_quiet_and_one_with_anything_is_not() {
        // `quiet` is the gate on drawing a folded branch's badge cluster at
        // all. Every counted state has to open it — a branch holding only a
        // pinned note is still saying something.
        assert!(Roll::default().quiet());
        for roll in [
            Roll::task(1, 0, 0, 0, 0, 1),
            Roll::task(0, 1, 0, 0, 0, 1),
            Roll::task(0, 0, 1, 0, 0, 1),
            Roll::task(0, 0, 0, 1, 0, 1),
            Roll::task(0, 0, 0, 0, 1, 1),
        ] {
            assert!(!roll.quiet(), "{roll:?} has something to say");
        }
        // panes and tasks are size, not news: a branch of nine silent
        // terminals draws no badges
        assert!(Roll::task(0, 0, 0, 0, 0, 9).quiet());
    }

    /// Deleting a branch must not move you to a terminal you never chose.
    ///
    /// The case that made this a function: five tabs, the fourth active, the
    /// second and third deleted. Clamping says index 2 and the honest answer is
    /// index 1 — one is the tab you were working in and the other is the one
    /// after it. Both are in range, both look fine, and only one is right.
    #[test]
    fn the_active_tab_survives_a_deletion_above_it() {
        // [A,B,C,D,E], D active, delete B and C -> [A,D,E], D is at 1
        assert_eq!(active_after_removal(3, &[1, 2], 3), 1);
        // the clamp-only answer, for contrast: clamping the old index into the
        // shorter list gives 2, where the honest answer is 1. Written through a
        // binding because clippy refuses a literal `3.min(2)` as having no
        // effect — true of the arithmetic, and beside the point of the line.
        let len_after = 3usize;
        let clamp_only = 3usize.min(len_after - 1);
        assert_eq!(clamp_only, 2);
        assert_ne!(clamp_only, active_after_removal(3, &[1, 2], 3));
    }

    #[test]
    fn a_deletion_below_the_active_tab_leaves_it_alone() {
        // [A,B,C,D,E], B active, delete D -> [A,B,C,E], B still at 1
        assert_eq!(active_after_removal(1, &[3], 4), 1);
        // [A,B,C], C active, delete nothing
        assert_eq!(active_after_removal(2, &[], 3), 2);
    }

    /// Removing the active tab has no right answer, only a sensible one.
    #[test]
    fn removing_the_active_tab_lands_where_the_branch_was() {
        // [A,B,C,D,E], C active, delete B,C,D -> [A,E]; land at 1, which is
        // where the run was, clamped into the shorter list
        assert_eq!(active_after_removal(2, &[1, 2, 3], 2), 1);
        // the run was at the end: clamp back onto the last survivor
        assert_eq!(active_after_removal(3, &[2, 3], 2), 1);
        // the run was the whole front of the list
        assert_eq!(active_after_removal(0, &[0, 1], 1), 0);
    }

    /// Nothing indexes into an empty list, whatever it was told.
    #[test]
    fn an_emptied_list_answers_zero_rather_than_indexing_into_nothing() {
        assert_eq!(active_after_removal(4, &[0, 1, 2, 3, 4], 0), 0);
    }

    /// A single removal is the common case and must not need its own reasoning.
    #[test]
    fn one_removal_shifts_everything_above_it_down_by_one() {
        for active in 0..5usize {
            for gone in 0..5usize {
                let got = active_after_removal(active, &[gone], 4);
                let want = if gone == active {
                    gone.min(3)
                } else if gone < active {
                    active - 1
                } else {
                    active
                };
                assert_eq!(got, want, "active={active} removed={gone}");
            }
        }
    }
    // ── the keyboard cursor ────────────────────────────────────────────────

    /// The session in the screenshot, near enough: two projects, the first
    /// carrying an initiative with two tasks, the second a loose task.
    fn bar() -> (Vec<ProjectRef>, Vec<InitiativeRef>, Vec<TaskRef>) {
        (
            vec![project(1, false), project(2, false)],
            vec![initiative(10, Some(1), false)],
            vec![
                task(Some(1), Some(10)),
                task(Some(1), Some(10)),
                task(Some(2), None),
            ],
        )
    }

    #[test]
    fn the_cursor_walks_every_drawn_row_top_to_bottom_and_comes_back_round() {
        let (p, i, t) = bar();
        let rows = rows(&p, &i, &t);
        let order = vec![
            RowId::Project(1),
            RowId::Initiative(10),
            RowId::Task(0),
            RowId::Task(1),
            RowId::Project(2),
            RowId::Task(2),
        ];
        // Walk one row at a time for exactly as many presses as there are rows,
        // then assert the last press landed back at the start — a bounded loop,
        // because the unbounded one this replaced would now never end.
        let mut seen = vec![];
        let mut at = step(&rows, None, true);
        for _ in 0..order.len() {
            let id = at.expect("the ring always has somewhere to go");
            seen.push(id);
            at = step(&rows, Some(id), true);
        }
        assert_eq!(seen, order);
        assert_eq!(
            at,
            Some(RowId::Project(1)),
            "↓ off the bottom wraps to the top"
        );
        assert_eq!(
            step(&rows, Some(RowId::Project(1)), false),
            Some(RowId::Task(2)),
            "↑ off the top wraps to the bottom"
        );
    }

    /// A ring of one is still a ring, and a ring of none is not one at all.
    /// Both ends of that are reachable in a running window: a session with a
    /// single tab and nothing filed draws one row, and a window mid-teardown
    /// draws none.
    #[test]
    fn a_ring_of_one_row_stays_put_and_a_ring_of_none_refuses() {
        let one = rows(&[], &[], &[task(None, None)]);
        assert_eq!(stops(&one), vec![RowId::Task(0)]);
        assert_eq!(step(&one, Some(RowId::Task(0)), true), Some(RowId::Task(0)));
        assert_eq!(
            step(&one, Some(RowId::Task(0)), false),
            Some(RowId::Task(0))
        );
        assert_eq!(step(&[], None, true), None);
        assert_eq!(step(&[], Some(RowId::Task(0)), false), None);
    }

    /// The numbers count the SECTIONS of the bar, top to bottom: the projects
    /// in their order, then the initiatives nobody has filed under one. This is
    /// the shape of Parker's own session — four projects and then a loose
    /// group — and the count he read off the screen when he asked for it.
    #[test]
    fn the_number_keys_address_top_level_branches_in_draw_order() {
        let projects = vec![project(1, false), project(3, true), project(4, true)];
        let initiatives = vec![
            initiative(10, Some(1), false),
            initiative(20, None, false),
            initiative(30, None, true),
        ];
        let tasks = vec![
            task(Some(1), Some(10)),
            task(None, Some(20)),
            task(None, Some(30)),
            task(None, None),
        ];
        let drawn = rows(&projects, &initiatives, &tasks);
        assert_eq!(
            top_branches(&drawn),
            vec![
                RowId::Project(1),
                RowId::Project(3),
                RowId::Project(4),
                RowId::Initiative(20),
                RowId::Initiative(30),
            ],
            "projects first, then the initiatives that hang from none of them"
        );
        assert_eq!(nth_top_branch(&drawn, 1), Some(RowId::Project(1)));
        assert_eq!(nth_top_branch(&drawn, 4), Some(RowId::Initiative(20)));
        // Past the end, and past nine, are both nothing rather than a clamp: a
        // press with no branch under it must not move the cursor somewhere the
        // person did not aim.
        assert_eq!(nth_top_branch(&drawn, 6), None);
        assert_eq!(nth_top_branch(&drawn, 0), None);
        assert_eq!(nth_top_branch(&drawn, 10), None);
    }

    /// A folded project is still a section, and a numbered one. Folding the
    /// tree down to its headings is the state the numbers are MOST useful in,
    /// so a rule that counted only what is expanded would have withdrawn them
    /// exactly when they were wanted.
    #[test]
    fn folding_changes_no_branch_number() {
        let (p, i, t) = bar();
        let open = top_branches(&rows(&p, &i, &t));
        let shut = top_branches(&rows(&[project(1, true), project(2, true)], &i, &t));
        assert_eq!(open, shut);
        assert_eq!(open.first(), Some(&RowId::Project(1)));
    }

    /// A task sitting loose at the bottom is at depth zero like a project, and
    /// is not a section. If it were counted, filing one tab would renumber
    /// every branch below it.
    #[test]
    fn a_loose_task_never_takes_a_number() {
        let tasks = vec![task(None, Some(7)), task(None, None)];
        let drawn = rows(&[], &[initiative(7, None, false)], &tasks);
        assert!(drawn.contains(&Row::Task { index: 1, depth: 0 }));
        assert_eq!(top_branches(&drawn), vec![RowId::Initiative(7)]);
    }

    /// The behaviour the whole feature was asked for: a folded branch is ONE
    /// row to the keyboard. Down from it lands on the next sibling, not on the
    /// children it is hiding.
    #[test]
    fn a_folded_branch_is_stepped_over_whole_not_walked_into() {
        let (mut p, i, t) = bar();
        p[0] = project(1, true);
        let drawn = rows(&p, &i, &t);
        assert_eq!(
            step(&drawn, Some(RowId::Project(1)), true),
            Some(RowId::Project(2)),
            "down from a folded project must clear its whole subtree"
        );
        // ...and the rows it skipped really are the ones that would otherwise
        // have been there — otherwise this test passes against an empty tree.
        let open = rows(&[project(1, false), project(2, false)], &i, &t);
        assert_eq!(
            step(&open, Some(RowId::Project(1)), true),
            Some(RowId::Initiative(10))
        );
    }

    /// Folding a branch the cursor is standing inside leaves the cursor
    /// pointing at a row nobody draws. It re-enters the list rather than
    /// jamming: an arrow key that does nothing for ever is indistinguishable
    /// from a dead binding.
    #[test]
    fn a_cursor_on_a_row_that_is_no_longer_drawn_re_enters_the_list() {
        let (mut p, i, t) = bar();
        p[0] = project(1, true);
        let rows = rows(&p, &i, &t);
        assert_eq!(
            step(&rows, Some(RowId::Task(0)), true),
            Some(RowId::Project(1))
        );
        assert_eq!(
            step(&rows, Some(RowId::Task(0)), false),
            Some(RowId::Task(2))
        );
    }

    /// A walk that starts from nothing starts where the person already is.
    /// Landing at the top of the list instead made every fresh Ctrl+Alt+↑/↓
    /// begin with a journey back to the active task.
    #[test]
    fn a_fresh_walk_enters_the_tree_on_the_active_task() {
        let (p, i, t) = bar();
        let rows = rows(&p, &i, &t);
        assert!(stops(&rows).contains(&RowId::Task(1)));
        assert_eq!(seed(&rows, 1), Some(RowId::Task(1)));
    }

    /// ...and STEPS OFF it. The seed is where the walk begins, not where it
    /// ends: a first press that landed on the active task told its reader the
    /// one thing they could already see, and cost a press to do it.
    ///
    /// Both directions, because a rule that only holds going up is a typo that
    /// passes. Task(1) sits between Task(0) above it and Project(2) below.
    #[test]
    fn the_first_press_moves_off_the_active_task_rather_than_onto_it() {
        let (p, i, t) = bar();
        let rows = rows(&p, &i, &t);
        assert_eq!(walk(&rows, None, 1, false), Some(RowId::Task(0)));
        assert_eq!(walk(&rows, None, 1, true), Some(RowId::Project(2)));
        // The point of the whole change, said plainly.
        assert_ne!(walk(&rows, None, 1, false), Some(RowId::Task(1)));
        assert_ne!(walk(&rows, None, 1, true), Some(RowId::Task(1)));
    }

    /// A live cursor outranks the active task, or the walk would snap back to
    /// the tab you are in on every press instead of carrying on from where the
    /// highlight got to.
    #[test]
    fn a_walk_already_under_way_carries_on_from_the_cursor() {
        let (p, i, t) = bar();
        let rows = rows(&p, &i, &t);
        assert_eq!(
            walk(&rows, Some(RowId::Project(1)), 1, true),
            Some(RowId::Initiative(10))
        );
    }

    /// ...unless that task is folded away under its own branch, where landing
    /// on it would put the highlight on a row nobody can see. The caller falls
    /// back to the end of the list, which is the old behaviour.
    #[test]
    fn a_fresh_walk_declines_to_start_on_a_task_that_is_not_drawn() {
        let (mut p, i, t) = bar();
        p[0] = project(1, true);
        let rows = rows(&p, &i, &t);
        assert!(!stops(&rows).contains(&RowId::Task(0)));
        assert_eq!(seed(&rows, 0), None);
        // Nothing to step off, so the press enters at the end it came from —
        // still a move, and still not a row that is folded away.
        assert_eq!(walk(&rows, None, 0, true), Some(RowId::Project(1)));
        assert_eq!(walk(&rows, None, 0, false), Some(RowId::Task(2)));
    }

    #[test]
    fn the_unfiled_hairline_is_scenery_and_never_takes_the_cursor() {
        let tasks = vec![task(None, Some(7)), task(None, None)];
        let rows = rows(&[], &[initiative(7, None, false)], &tasks);
        assert!(rows.contains(&Row::Unfiled { depth: 0 }));
        assert_eq!(
            stops(&rows),
            vec![RowId::Initiative(7), RowId::Task(0), RowId::Task(1)]
        );
    }

    /// → asks "can this be opened?", and the answer for a task is not "no" —
    /// it is "that is not a question about me". A task reported as unfolded
    /// would make → try to expand it and silently do nothing.
    #[test]
    fn only_a_branch_answers_whether_it_is_folded() {
        let (mut p, i, t) = bar();
        p[1] = project(2, true);
        let rows = rows(&p, &i, &t);
        assert_eq!(folded(&rows, RowId::Project(1)), Some(false));
        assert_eq!(folded(&rows, RowId::Project(2)), Some(true));
        assert_eq!(folded(&rows, RowId::Initiative(10)), Some(false));
        assert_eq!(folded(&rows, RowId::Task(0)), None);
    }

    #[test]
    fn parent_and_first_child_read_off_what_is_drawn() {
        let (p, i, t) = bar();
        let drawn = rows(&p, &i, &t);
        assert_eq!(parent(&drawn, RowId::Task(0)), Some(RowId::Initiative(10)));
        assert_eq!(
            parent(&drawn, RowId::Initiative(10)),
            Some(RowId::Project(1))
        );
        assert_eq!(parent(&drawn, RowId::Project(1)), None);
        assert_eq!(parent(&drawn, RowId::Task(2)), Some(RowId::Project(2)));

        assert_eq!(
            first_child(&drawn, RowId::Project(1)),
            Some(RowId::Initiative(10))
        );
        assert_eq!(
            first_child(&drawn, RowId::Initiative(10)),
            Some(RowId::Task(0))
        );
        // A task has nothing under it, and an EMPTY branch must not adopt the
        // sibling drawn after it.
        assert_eq!(first_child(&drawn, RowId::Task(0)), None);
        assert_eq!(first_child(&drawn, RowId::Project(2)), Some(RowId::Task(2)));
        let empty = rows(&[project(1, false), project(2, false)], &[], &[]);
        assert_eq!(first_child(&empty, RowId::Project(1)), None);
    }

    /// A folded branch hides its children from `first_child` too, which is why
    /// → opens first and only steps in on the press after — the sequence
    /// Parker described.
    #[test]
    fn a_folded_branch_offers_no_child_to_step_into() {
        let (mut p, i, t) = bar();
        p[0] = project(1, true);
        let rows = rows(&p, &i, &t);
        assert_eq!(first_child(&rows, RowId::Project(1)), None);
    }
}
