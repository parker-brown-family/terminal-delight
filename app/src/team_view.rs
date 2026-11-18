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

fn refused(code: &str, why: &str) -> mcp::AskReply {
    mcp::AskReply::Said(mcp::tool_err(code, why))
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
            return refused(
                "caller_unverified",
                "the pane you are calling from is not in this window.",
            );
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
            return refused("team_policy_refused", &why);
        }
        if !spec.project.is_dir() {
            return refused(
                "invalid_project",
                &format!(
                    "`project` {} is not a folder on this machine.",
                    spec.project.display()
                ),
            );
        }
        // Each member's launch line, made the way the launcher makes one.
        let Some(dir) = surfacefeed::session_dir() else {
            return refused(
                "session_unavailable",
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
                Err(why) => {
                    return refused(
                        "launch_unavailable",
                        &format!("{} cannot be started: {why}", m.role),
                    )
                }
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
        // Making a pane gives it the keyboard. Whatever the person was typing
        // into (the composer, a pane) gets it back below, and again after the
        // panes' own deferred work, so a team starting behind them never
        // closes what they are writing.
        let held = window.focused(cx);
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
        if let Some(h) = held {
            window.focus(&h, cx);
            window.defer(cx, move |w, cx| w.focus(&h, cx));
        }
        self.save(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threshing_floor::SeatId;

    /// The Floor's own fixture: a window with two seated agent panes, "Lead"
    /// (seat 100, pane 10) and "Worker" (seat 101, pane 11).
    fn window(
        cx: &mut gpui::TestAppContext,
    ) -> (Entity<Workspace>, &'static mut gpui::VisualTestContext) {
        let (ws, vcx, _other) = crate::threshing_floor_view::floor_test_fixture(cx);
        (ws, vcx)
    }

    fn leaf_of(ws: &Workspace, seat: SeatId, cx: &App) -> Entity<TerminalView> {
        ws.threshing_floor_seat_leaf(seat, cx)
            .expect("a seated pane")
    }

    fn spec(dir: &std::path::Path) -> team::Spec {
        team::parse(
            &serde_json::json!({
                "name": "review crew",
                "project": dir,
                "agents": [{ "role": "review", "harness": "codex" }]
            }),
            None,
        )
        .expect("a well-formed team")
    }

    fn said_text(reply: &mcp::AskReply) -> (bool, String) {
        let mcp::AskReply::Said(v) = reply else {
            panic!("start_team answers in words")
        };
        let text = v["content"][0]["text"].as_str().unwrap_or("").to_string();
        (v["isError"] == true, text)
    }

    /// The pane's own right-click row reaches the window as an event, and
    /// the window keeps the promotion by seat. Taking it back removes it.
    /// This is the wiring a person's click goes through, end to end short of
    /// the pixel.
    #[gpui::test]
    fn the_pane_menus_row_promotes_and_demotes_the_pane_by_its_seat(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = window(cx);
        let lead = ws.update(cx, |ws, cx| leaf_of(ws, SeatId(100), cx));
        lead.update(cx, |_, cx| cx.emit(pane::PromoteOrchestrator(true)));
        cx.run_until_parked();
        ws.update(cx, |ws, _| {
            assert!(ws.team.orchestrators.contains(&SeatId(100)));
            assert!(
                !ws.team.orchestrators.contains(&SeatId(101)),
                "only the pane clicked"
            );
        });
        lead.update(cx, |_, cx| cx.emit(pane::PromoteOrchestrator(false)));
        cx.run_until_parked();
        ws.update(cx, |ws, _| assert!(ws.team.orchestrators.is_empty()));
    }

    /// A pane nobody seated is given a seat when a person promotes it,
    /// because a promotion is kept by seat and must outlive the pane's
    /// number. Demoting a pane with no seat does nothing.
    #[gpui::test]
    fn promoting_an_unseated_pane_gives_it_a_seat_and_demoting_one_is_harmless(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = window(cx);
        ws.update(cx, |ws, cx| {
            let worker = leaf_of(ws, SeatId(101), cx);
            worker.update(cx, |v, cx| v.set_seat(None, cx));
            ws.team_promote(&worker, false, cx);
            assert!(worker.read(cx).seat.is_none(), "a demotion seats nobody");
            ws.team_promote(&worker, true, cx);
            let seat = worker.read(cx).seat.expect("seated by the promotion");
            assert!(ws.team.orchestrators.contains(&seat));
        });
    }

    /// The two things a person grants, in the order the refusal names them:
    /// the window's switch, then the pane's promotion. With both, the request
    /// passes the decision and goes on to building launch lines.
    #[gpui::test]
    fn start_team_needs_the_switch_and_then_the_promotion(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = window(cx);
        let dir = std::env::temp_dir();
        ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            ws.mcp.team = false;
            let (err, text) = said_text(&ws.team_request(10, &spec(&dir), cx));
            assert!(err && text.contains("High trust agentic is off"), "{text}");

            ws.mcp.team = true;
            let (err, text) = said_text(&ws.team_request(10, &spec(&dir), cx));
            assert!(err && text.contains("not been promoted"), "{text}");

            // The worker is promoted, not the lead: still the lead's refusal.
            let worker = leaf_of(ws, SeatId(101), cx);
            ws.team_promote(&worker, true, cx);
            let (err, text) = said_text(&ws.team_request(10, &spec(&dir), cx));
            assert!(err && text.contains("not been promoted"), "{text}");

            // A pane that is not in the window is answered, not panicked on.
            let (err, text) = said_text(&ws.team_request(999, &spec(&dir), cx));
            assert!(err && text.contains("not in this window"), "{text}");
        });
    }

    /// A team allowed and queued does not start if a person took the
    /// promotion back before the window got to it (cross-provider review,
    /// round one). And it does start, as the control, when they did not.
    #[gpui::test]
    fn a_queued_team_whose_coordinator_was_demoted_does_not_start(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = window(cx);
        let dir = std::env::temp_dir();
        let queue = |ws: &mut Workspace, cx: &mut Context<Workspace>| {
            let lead = leaf_of(ws, SeatId(100), cx);
            ws.team.pending.push(PendingTeam {
                id: 1,
                coordinator: lead.downgrade(),
                spec: spec(&dir),
                lines: vec!["exec /bin/cat".into()],
            });
        };
        let tabs_before = ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            ws.mcp.team = true;
            let lead = leaf_of(ws, SeatId(100), cx);
            ws.team_promote(&lead, true, cx);
            queue(ws, cx);
            ws.team_promote(&lead, false, cx);
            ws.tabs.len()
        });
        cx.update(|w, cx| ws.update(cx, |ws, cx| ws.team_drain(w, cx)));
        ws.update(cx, |ws, _| {
            assert!(ws.team.pending.is_empty(), "taken off the queue");
            assert_eq!(ws.tabs.len(), tabs_before, "and nothing started");
        });
        // The control: promoted all the way through, the same queue starts a tab.
        ws.update(cx, |ws, cx| {
            let lead = leaf_of(ws, SeatId(100), cx);
            ws.team_promote(&lead, true, cx);
            queue(ws, cx);
        });
        // The person is typing in the lead's pane while the team starts.
        let lead_focus = ws.update(cx, |ws, cx| leaf_of(ws, SeatId(100), cx).focus_handle(cx));
        cx.update(|w, cx| w.focus(&lead_focus, cx));
        {
            let _fork = crate::testsync::forks_and_locks();
            cx.update(|w, cx| ws.update(cx, |ws, cx| ws.team_drain(w, cx)));
        }
        cx.run_until_parked();
        assert!(
            cx.update(|w, _| lead_focus.is_focused(w)),
            "a team starting behind the person leaves their keyboard where it was"
        );
        ws.update(cx, |ws, _cx| {
            assert_eq!(
                ws.tabs.len(),
                tabs_before + 1,
                "the control started its tab"
            );
            assert_eq!(ws.team.teams.saved().len(), 1, "and recorded the team");
            let member = ws.team.teams.saved()[0].members[0];
            assert!(
                ws.chart.line(SeatId(100), member).is_some_and(|l| l.team),
                "with a marked line from the coordinator"
            );
        });
    }

    /// An agent a team started counts under the window's limit while it
    /// runs, whatever a person does to its seat. Unseating it used to free
    /// its room (cross-provider review, round one); a wake then lost the
    /// mark again (round two).
    #[gpui::test]
    fn a_team_member_keeps_counting_after_an_unseat_and_through_a_wake(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx) = window(cx);
        ws.update(cx, |ws, cx| {
            assert_eq!(ws.team_live(cx), 0);
            ws.team.teams.add(team::Record {
                id: 7,
                name: "crew".into(),
                by: SeatId(100),
                members: vec![SeatId(101)],
                started_ms: 0,
            });
            assert_eq!(ws.team_live(cx), 1, "counted by its seat");
            let worker = leaf_of(ws, SeatId(101), cx);
            ws.threshing_floor_unseat(&worker, cx);
            assert!(worker.read(cx).seat.is_none());
            assert!(worker.read(cx).team_member, "the mark moved to the pane");
            assert_eq!(ws.team_live(cx), 1, "and it still counts");

            // A wake builds a fresh pane and hands it the old one's
            // presentation: the mark must come across with it.
            let lead = leaf_of(ws, SeatId(100), cx);
            let carried = worker.read(cx).presentation();
            lead.update(cx, |v, cx| {
                v.team_member = false;
                v.adopt_presentation(carried, cx);
            });
            assert!(lead.read(cx).team_member, "carried through the hand-over");
        });
    }
}
