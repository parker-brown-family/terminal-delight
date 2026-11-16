//! High trust agentic, as the window runs it: a coordinator's request for a
//! team, checked, then started in a new tab on the next pass that has a
//! window to build panes in.
//!
//! The rules are in `team`. Seating an agent and drawing a line stay where
//! they have always been, in `threshing_floor_view.rs`, which is the only
//! file allowed to do either (`only_the_threshing_floor_and_the_rail_draw_lines`).
//! This file decides, queues, builds the panes and says what happened.

use super::*;
use crate::team;

/// A team that has been allowed and not yet started.
pub(crate) struct PendingTeam {
    id: u64,
    coordinator: gpui::WeakEntity<TerminalView>,
    spec: team::Spec,
    /// Each member's launch line, prepared when the request was allowed so
    /// that a program this machine cannot start is refused to the caller
    /// rather than found out in a pane.
    lines: Vec<String>,
}

/// The window's state for teams.
#[derive(Default)]
pub(crate) struct TeamState {
    pub(crate) teams: team::Teams,
    /// The seats a person promoted to orchestrator: the only panes that may
    /// start a team. Saved with the session. Empty until a person promotes
    /// one, whatever the switch says.
    pub(crate) orchestrators: std::collections::BTreeSet<threshing_floor::SeatId>,
    pending: Vec<PendingTeam>,
}

impl TeamState {
    pub(crate) fn from_saved(
        saved: Vec<team::Record>,
        orchestrators: Vec<threshing_floor::SeatId>,
    ) -> Self {
        Self {
            teams: team::Teams::from_saved(saved),
            orchestrators: orchestrators.into_iter().collect(),
            pending: vec![],
        }
    }
}

thread_local! {
    /// The window's switch and its promoted seats, as last drawn: what a
    /// pane's own right-click menu reads to mark its Orchestrator row. The
    /// window writes it as it draws and a pane reads it as it draws, the way
    /// the tab hotkeys are shared (`keylayer::TabKeys`). It is a picture for
    /// the menu and decides nothing: `team::decide` reads the window's own
    /// record.
    static PUBLISHED: std::cell::RefCell<(bool, std::collections::BTreeSet<threshing_floor::SeatId>)> =
        const { std::cell::RefCell::new((false, std::collections::BTreeSet::new())) };
}

/// The window says what is so, once a frame. Written only when it changed.
pub(crate) fn publish(
    switch_on: bool,
    promoted: &std::collections::BTreeSet<threshing_floor::SeatId>,
) {
    PUBLISHED.with(|p| {
        let same = {
            let p = p.borrow();
            p.0 == switch_on && &p.1 == promoted
        };
        if !same {
            *p.borrow_mut() = (switch_on, promoted.clone());
        }
    });
}

/// Whether High trust agentic is on in this window, and whether the pane
/// wearing `seat` is promoted. A pane with no seat is not promoted.
pub(crate) fn published(seat: Option<threshing_floor::SeatId>) -> (bool, bool) {
    PUBLISHED.with(|p| {
        let p = p.borrow();
        (p.0, seat.is_some_and(|s| p.1.contains(&s)))
    })
}

fn said(text: String, data: serde_json::Value) -> mcp::AskReply {
    mcp::AskReply::Said(mcp::tool_ok(text, data))
}

fn refused(why: &str) -> mcp::AskReply {
    mcp::AskReply::Said(mcp::tool_err(why))
}

impl Workspace {
    /// Every seat a pane in this window is wearing.
    fn team_worn_seats(&self, cx: &App) -> std::collections::HashSet<threshing_floor::SeatId> {
        let mut worn = std::collections::HashSet::new();
        for tab in &self.tabs {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            worn.extend(leaves.into_iter().filter_map(|l| l.read(cx).seat));
        }
        worn
    }

    /// Agent-started agents still running in this window: a pane a team
    /// started (marked on the pane itself, so a person unseating it does not
    /// free its room while it runs), or a pane wearing a seat a saved team
    /// names (which is how a restarted window still counts them). Counting
    /// seats alone let eight agents be unseated and eight more started
    /// (cross-provider review, 2026-11-16).
    fn team_live(&self, cx: &App) -> usize {
        let mut n = 0;
        for tab in &self.tabs {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            n += leaves
                .iter()
                .filter(|l| {
                    let v = l.read(cx);
                    v.team_member || v.seat.is_some_and(|s| self.team.teams.is_member(s))
                })
                .count();
        }
        n
    }

    /// An agent's `start_team`, decided. Nothing is started in this call: a
    /// pane needs a window to be built in, and this answers a connection that
    /// has none. What is allowed here starts on the next pass
    /// ([`Self::team_drain`]), within a second.
    pub(crate) fn team_request(
        &mut self,
        caller_pane: u64,
        spec: &team::Spec,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        let Some(coordinator) = self.leaf_by_pane_id(caller_pane, cx) else {
            return refused("the pane you are calling from is not in this window.");
        };
        // Seats whose panes have closed stop counting before the count is read.
        let worn = self.team_worn_seats(cx);
        self.team.teams.retain_seats(&|s| worn.contains(&s));
        let (is_agent, seat) = {
            let v = coordinator.read(cx);
            (v.party().is_agent, v.seat)
        };
        let queued: usize = self.team.pending.iter().map(|p| p.spec.members.len()).sum();
        let asker = team::Asker {
            mcp: self.mcp.enabled,
            trusted: self.mcp.team,
            is_agent,
            promoted: seat.is_some_and(|s| self.team.orchestrators.contains(&s)),
            is_member: seat.is_some_and(|s| self.team.teams.is_member(s)),
            live: self.team_live(cx) + queued,
        };
        if let Err(why) = team::decide(&asker, spec.members.len()) {
            eprintln!(
                "terminal-delight: start_team refused for pane {caller_pane}: {}",
                why.split('.').next().unwrap_or(&why)
            );
            return refused(&why);
        }
        if !spec.project.is_dir() {
            return refused(&format!(
                "`project` {} is not a folder on this machine.",
                spec.project.display()
            ));
        }
        // Each member's launch line, made the way the launcher makes one.
        let Some(dir) = surfacefeed::session_dir() else {
            return refused(
                "Terminal Delight could not find its session folder to write a briefing in.",
            );
        };
        let kit = skills::briefing_root();
        let mut lines = Vec::with_capacity(spec.members.len());
        for m in &spec.members {
            let recipe = launcher::Recipe {
                harness: m.harness,
                model: m.model.clone(),
                effort: m.effort.clone(),
                reach: m.permissions,
                cwd: spec.project.clone(),
                opener: m.prompt.clone(),
            };
            let line = (|| {
                let briefing = recipe.prepare_briefing(&dir, kit.as_deref())?;
                let adapter = if m.harness == launcher::Harness::Local {
                    Some(launcher::LocalAdapter::load()?)
                } else {
                    None
                };
                recipe.command_line_with_adapter(Some(&briefing), adapter.as_ref())
            })();
            match line {
                Ok(line) => lines.push(line),
                Err(why) => return refused(&format!("{} cannot be started: {why}", m.role)),
            }
        }
        self.ask_seq = (self.ask_seq + 1).max(crate::prompt::now_ms());
        let id = self.ask_seq;
        let who = self.pane_name(&coordinator, cx).full();
        let members: Vec<serde_json::Value> = spec
            .members
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "harness": m.harness.label(),
                    "model": m.model,
                    "effort": m.effort,
                    "line": m.line,
                })
            })
            .collect();
        let roles: Vec<&str> = spec.members.iter().map(|m| m.role.as_str()).collect();
        eprintln!(
            "terminal-delight: {who} is starting team {:?}: {}",
            spec.name,
            roles.join(", ")
        );
        self.team.pending.push(PendingTeam {
            id,
            coordinator: coordinator.downgrade(),
            spec: spec.clone(),
            lines,
        });
        cx.notify();
        said(
            format!(
                "team {:?} is starting in a new tab: {}. Each is a pane your person can see. Your line to each is drawn as it starts. Read list_panes in a few seconds for each agent's `agent` id, then use prompt_agent, ask_agent and prompt_status along your lines.",
                spec.name,
                roles.join(", ")
            ),
            serde_json::json!({
                "team_id": id,
                "name": spec.name,
                "project": spec.project,
                "agents": members,
            }),
        )
    }

    /// Start the teams that were allowed. Called on a short clock with a
    /// window to build panes in.
    pub(crate) fn team_drain(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.team.pending.is_empty() {
            return;
        }
        for pending in std::mem::take(&mut self.team.pending) {
            // The switch is read again here: a person who turned it off in
            // the second between the answer and the start meant it.
            if !self.mcp.team || !self.mcp.enabled {
                eprintln!(
                    "terminal-delight: team {:?} was not started: High trust agentic was switched off",
                    pending.spec.name
                );
                continue;
            }
            let Some(coordinator) = pending.coordinator.upgrade() else {
                continue; // its pane closed first
            };
            // And the promotion: a person who took it back in that second
            // meant it too (cross-provider review, 2026-11-16).
            let promoted = coordinator
                .read(cx)
                .seat
                .is_some_and(|s| self.team.orchestrators.contains(&s));
            if !promoted {
                eprintln!(
                    "terminal-delight: team {:?} was not started: its coordinator is no longer promoted",
                    pending.spec.name
                );
                continue;
            }
            self.team_start(pending, coordinator, window, cx);
        }
    }

    fn team_start(
        &mut self,
        pending: PendingTeam,
        coordinator: Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let PendingTeam {
            id, spec, lines, ..
        } = pending;
        let cwd = spec.project.to_string_lossy().to_string();
        // Beside the coordinator's own tab, in its branch of the left bar, so
        // the team is filed where the work is. The person's own tab stays the
        // one on screen: an agent starting agents does not move anybody.
        let home = self.tab_with_leaf(coordinator.entity_id());
        let place = home.map(|i| self.place_of(i));
        let mut panes: Vec<Entity<TerminalView>> = Vec::with_capacity(spec.members.len());
        for (member, line) in spec.members.iter().zip(lines) {
            let pane = self.make_pane_in_mode(
                session::PaneRestore {
                    program: None,
                    mark: None,
                    cwd: Some(cwd.clone()),
                    resume: Some(line.clone()),
                    logo: None,
                    note: None,
                },
                window,
                cx,
            );
            pane.update(cx, |v, _| {
                v.name = Some(member.role.clone());
                v.team_member = true;
            });
            if let Some(dir) = surfacefeed::session_dir() {
                let _ = surfacefeed::journal_event(
                    &dir.join("launches.jsonl"),
                    &serde_json::json!({
                        "td": crate::surface::TDSP_VERSION,
                        "type": "launch",
                        "at_ms": surfacefeed::now_ms(),
                        "by": "start_team",
                        "team": spec.name,
                        "team_id": id,
                        "role": member.role,
                        "harness": member.harness.binary(),
                        "model": member.model,
                        "effort": member.effort,
                        "reach": member.permissions.id(),
                        "cwd": cwd,
                        "command": line,
                    }),
                );
            }
            panes.push(pane);
        }
        let Some(first) = panes.first().cloned() else {
            return;
        };
        let identity = match place {
            Some(place) => TabIdentity {
                name: Some(spec.name.clone()),
                group: place.initiative,
                project: place
                    .initiative
                    .is_none()
                    .then_some(place.project)
                    .flatten(),
                ..Default::default()
            },
            None => TabIdentity {
                name: Some(spec.name.clone()),
                ..Default::default()
            },
        };
        let mut tab = identity.onto(Node::Leaf(first.clone()));
        // Two across, then two down: four panes read as a square, the shape a
        // person would have split them into.
        for (i, pane) in panes.iter().enumerate().skip(1) {
            // `Row` sets two panes side by side; `Col` stacks them.
            let (beside, dir) = match i {
                1 => (0, SplitDir::Row),
                2 => (0, SplitDir::Col),
                _ => (1, SplitDir::Col),
            };
            let target = panes[beside].entity_id();
            tab.root.split_leaf(
                &|e: &Entity<TerminalView>| e.entity_id() == target,
                dir,
                pane.clone(),
            );
        }
        let at = match place {
            Some(place) => self.branch_end(place),
            None => self.tabs.len(),
        };
        self.tabs.insert(at, tab);
        // Inserting before the tab on screen moves its index, not the person.
        if at <= self.active {
            self.active += 1;
        }
        let seats = self.team_seat_and_line(&coordinator, &panes, &spec, cx);
        if let Some(by) = coordinator.read(cx).seat {
            self.team.teams.add(team::Record {
                id,
                name: spec.name.clone(),
                by,
                members: seats,
                started_ms: crate::prompt::now_ms(),
            });
        }
        let who = self.pane_name(&coordinator, cx).full();
        self.marquee_says = Some((
            lang::fill(
                self.lang.strings().team.started,
                &[
                    ("who", &who),
                    ("team", &spec.name),
                    ("count", &panes.len().to_string()),
                ],
            ),
            Instant::now(),
        ));
        self.save(cx);
        cx.notify();
    }

    /// Run [`Self::team_drain`] on a short clock. The clock only looks; with
    /// no team waiting a pass reads one empty list.
    pub(crate) fn watch_teams(window: &Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            if this
                .update_in(cx, |ws: &mut Workspace, window, cx| {
                    ws.team_drain(window, cx)
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
    }
}
