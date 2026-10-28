//! The Threshing Floor panel: seats a person placed, the lines between them, what
//! each seat is doing, and the window's MCP switches.
//!
//! The rules it draws live in `threshing_floor`; this file only gathers what the
//! window already knows about each seat (the agent counter's state, the size,
//! age and model vitals read from the transcript) and turns a person's gestures
//! into membership and directed lines. Seating
//! and drawing happen here and nowhere else — see
//! `only_the_threshing_floor_and_the_rail_draw_lines`.

use super::*;
use crate::surfacefeed::now_ms;
use gpui::{anchored, deferred, PathBuilder};

/// A seat's box at scale 1: place, title, latest message, program and model,
/// state and role, age and tokens, and the newest artifact.
const SEAT_W: f32 = 248.;
const SEAT_H: f32 = 156.;
const COL_GAP: f32 = 30.;
const ROW_GAP: f32 = 56.;
const PAD: f32 = 14.;

/// Everything one seat shows, gathered from facts the window already has.
struct PaneFacts {
    leaf: Entity<TerminalView>,
    seat: Option<threshing_floor::SeatId>,
    name: threshing_floor::PaneName,
    project: String,
    program: String,
    model: Option<String>,
    /// The effort the agent runs at, verbatim from its transcript (`MAX`, `xhigh`).
    effort: Option<String>,
    /// The agent's own latest message on its workbench: its brief, or the
    /// start of its plain reply when it wrote no brief.
    message: Option<String>,
    /// The pane's conversation is only guessed at, so its bench may hold
    /// another conversation's words: the card says so instead of quoting them.
    message_unproven: bool,
    /// Its newest artifact, how many more it has handed over, and whether a
    /// person has left feedback on the newest. `None`: nothing handed over yet.
    artifact: Option<(String, usize, crate::feedback::Feedback)>,
    size: Option<u64>,
    started_ms: Option<i64>,
    state: hud::AgentState,
    access: threshing_floor::AgentAccess,
}

/// A wire the canvas is ready to stroke: its polyline, whether it bypasses a
/// copy's press limit (drawn dashed amber), and whether its target is off-limits
/// (drawn faint and dashed, granting nothing).
type WireStroke = (Vec<(f32, f32)>, bool, bool);

/// One drawn connection: a polyline in chart coordinates, how it is inked,
/// and where its label chip sits.
struct Wire {
    from: threshing_floor::SeatId,
    to: threshing_floor::SeatId,
    points: Vec<(f32, f32)>,
    bypass: bool,
    /// The rung this line grants, when this build can read it — what the label
    /// chip is inked by, so the clearance reads at a glance.
    rung: Option<threshing_floor::Rung>,
    label_at: (f32, f32),
    label: String,
}

#[derive(Clone)]
struct PaneDrag {
    leaf: Entity<TerminalView>,
    title: String,
    ink: Hsla,
    bg: Hsla,
}

#[derive(Clone)]
struct RelationDrag {
    from: threshing_floor::SeatId,
    title: String,
    ink: Hsla,
    bg: Hsla,
}

/// An inexpensive drag representation. The original card keeps its dimensions.
struct DragPreview {
    title: String,
    offset: Point<Pixels>,
    ink: Hsla,
    bg: Hsla,
}

impl Render for DragPreview {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mouse = window.mouse_position();
        let viewport = window.viewport_size();
        // Flip beside the pointer near the panel edges so the preview remains
        // readable over the bin instead of being clipped or frosted outside it.
        let dx = if mouse.x + px(196.) > viewport.width * 0.975 - px(16.) {
            px(-196.)
        } else {
            px(16.)
        };
        let dy = if mouse.y + px(52.) > viewport.height * 0.95 - px(16.) {
            px(-52.)
        } else {
            px(8.)
        };
        div().child(
            div()
                .id("floor-drag-preview")
                .debug_selector(|| "floor-drag-preview".to_string())
                .relative()
                .left(self.offset.x + dx)
                .top(self.offset.y + dy)
                .w(px(180.))
                .h(px(44.))
                .p_2()
                .border_1()
                .border_color(self.ink)
                .rounded(px(5.))
                .bg(self.bg)
                .text_color(self.ink)
                .text_size(px(11.))
                .overflow_hidden()
                .whitespace_nowrap()
                .child(SharedString::from(self.title.clone())),
        )
    }
}

/// A restrained phosphor halo: stronger only over the destination under the pointer.
fn drop_glow(ink: Hsla, hovered: bool) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: ink.alpha(if hovered { 0.26 } else { 0.12 }),
        offset: point(px(0.), px(0.)),
        blur_radius: px(if hovered { 16. } else { 10. }),
        spread_radius: px(1.),
        inset: true,
    }]
}

/// Reuse the reader's GPU frosting; no overlay intercepts any drop target.
fn drag_focus() -> impl IntoElement {
    gpui::canvas(
        |bounds, window, _| {
            let sf = window.scale_factor();
            // Include the reserved drag footer below the work area.
            warp::set_focus_blur(
                [
                    f32::from(bounds.origin.x) * sf,
                    f32::from(bounds.origin.y) * sf,
                    f32::from(bounds.size.width) * sf,
                    (f32::from(bounds.size.height) + 68.) * sf,
                ],
                12. * sf,
                6. * sf,
                0.45,
                6. * sf,
            );
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

fn trash_icon(ink: Hsla) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| {},
        move |b, _, window, _| {
            let mut path = PathBuilder::stroke(px(1.6));
            let p = |x, y| b.origin + point(px(x), px(y));
            path.move_to(p(4., 6.));
            path.line_to(p(20., 6.));
            path.move_to(p(9., 6.));
            path.line_to(p(9., 3.));
            path.line_to(p(15., 3.));
            path.line_to(p(15., 6.));
            path.move_to(p(6., 8.));
            path.line_to(p(7., 21.));
            path.line_to(p(17., 21.));
            path.line_to(p(18., 8.));
            path.move_to(p(10., 10.));
            path.line_to(p(10., 18.));
            path.move_to(p(14., 10.));
            path.line_to(p(14., 18.));
            if let Ok(path) = path.build() {
                window.paint_path(path, ink);
            }
        },
    )
    .size(px(24.))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RelationEnd {
    From,
    To,
}

pub(crate) struct RelationEditor {
    draft: threshing_floor::RelationDraft,
    choosing: Option<RelationEnd>,
}

/// The System Map's own relationship draft. Unlike [`RelationEditor`], each end
/// is a whole-org or a single agent ([`threshing_floor::End`]), and the two may sit in
/// different orgs — this is the surface where cross-org and whole-org
/// relationships are drawn. `Some` means the map is open.
#[derive(Default)]
pub(crate) struct SystemMap {
    from: Option<threshing_floor::End>,
    to: Option<threshing_floor::End>,
    rung: Option<threshing_floor::Rung>,
    choosing: Option<RelationEnd>,
}

/// What a pane card dropped onto a *different* org can become. Dropping used to
/// move the agent silently; this opens a choice at the drop so the same gesture
/// can build a relationship instead. `seat` is the dragged agent's own seat (it
/// must be seated to relate — a loose pane just moves in); `org` is where it
/// landed. `Some` means the popup is open. Routing only: every route below
/// reuses an existing mutation.
pub(crate) struct DropChoice {
    leaf: Entity<TerminalView>,
    seat: threshing_floor::SeatId,
    org: threshing_floor::OrgId,
    stage: DropStage,
}

/// The drop popup walks three routes. `Choosing` is the fork; the two relate
/// routes each gather a rung — and, for a member, which member of the target
/// org — before the line is drawn.
enum DropStage {
    Choosing,
    RelateOrg {
        rung: Option<threshing_floor::Rung>,
    },
    RelateMember {
        member: Option<threshing_floor::SeatId>,
        rung: Option<threshing_floor::Rung>,
    },
}

/// Spaces match independent words anywhere in the name. Colons address
/// project:tab:pane; empty segments leave that field unrestricted.
fn pane_search_score(project: &str, name: &threshing_floor::PaneName, query: &str) -> Option<i64> {
    let query = query.trim().to_lowercase();
    let score = |hay: &str, query: &str| -> Option<i64> {
        let hay = hay.to_lowercase();
        query.split_whitespace().try_fold(0, |sum, word| {
            pane::fuzzy_match(&hay, word).map(|(n, _)| sum + n)
        })
    };
    if query.contains(':') {
        let header = if name.header.is_empty() {
            &name.tab
        } else {
            &name.header
        };
        let fields = [project, &name.tab, header];
        let parts: Vec<_> = query.split(':').collect();
        if parts.len() > fields.len() {
            return None;
        }
        parts
            .iter()
            .zip(fields)
            .try_fold(0, |sum, (query, hay)| score(hay, query).map(|n| sum + n))
    } else {
        score(
            &format!("{project} {} {} {}", name.outer, name.tab, name.header),
            &query,
        )
    }
}

/// Wall-clock age since the conversation's first recorded timestamp. No
/// timestamp (or one in the future) remains unknown, including after resume.
fn session_age(started_ms: Option<i64>, now_ms: i64) -> Option<String> {
    let seconds = u64::try_from(now_ms.checked_sub(started_ms?)?).ok()? / 1000;
    Some(match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h {}m", seconds / 3600, seconds % 3600 / 60),
        _ => format!("{}d {}h", seconds / 86400, seconds % 86400 / 3600),
    })
}

impl Workspace {
    /// The Threshing Floor, when it is open. Drawn flat over the panes (see the
    /// warped-surface table), closed by a click outside it or by Esc.
    pub(crate) fn render_threshing_floor(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        if !self.threshing_floor {
            return None;
        }
        let sk = skin::for_theme(cx, th, th.grade.scale);
        let ui = self.lang.strings().ui;
        let lang = self.lang;
        let dragging = self.floor_drag.as_ref().filter(|_| cx.has_active_drag());
        let removing = dragging.is_some_and(|leaf| leaf.read(cx).seat.is_some());
        let drag_active = dragging.is_some();
        let chart_active = drag_active && self.floor_org.is_some();

        // ---- gather: seated panes, and every pane off the chart ----
        let mut seats: Vec<PaneFacts> = Vec::new();
        let mut loose: Vec<PaneFacts> = Vec::new();
        let query = self.threshing_floor_query.text();
        let mut total = 0;
        for (ti, tab) in self.tabs.iter().enumerate() {
            let project = self
                .place_of(ti)
                .project
                .and_then(|id| self.project_at(id))
                .and_then(|p| p.name.clone())
                .unwrap_or_default();
            let spine = self.spine_path(ti);
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for leaf in leaves {
                total += 1;
                let v = leaf.read(cx);
                let name = threshing_floor::PaneName::new(&spine, &v.header_title());
                if pane_search_score(&project, &name, &query).is_none() {
                    continue;
                }
                let vit = v
                    .mode
                    .is_agent()
                    .then(|| v.shell_pid().and_then(|pid| self.agent_vitals.get(&pid)))
                    .flatten();
                let facts = PaneFacts {
                    leaf: leaf.clone(),
                    seat: v.seat,
                    name,
                    project: project.clone(),
                    program: v.mode.label().to_string(),
                    model: vit.and_then(|x| x.model.clone()),
                    effort: vit.and_then(|x| x.effort.clone()),
                    message: v
                        .bench
                        .latest_response()
                        .map(|r| r.brief.clone().unwrap_or_else(|| r.layman.clone())),
                    message_unproven: !v.bench_is_its_own(),
                    artifact: v.handovers().newest().map(|h| {
                        (
                            h.label.clone(),
                            v.handovers().len().saturating_sub(1),
                            crate::feedback::of(&h.href),
                        )
                    }),
                    size: vit.and_then(|x| x.tokens),
                    started_ms: vit.and_then(|x| x.started_ms),
                    state: v.agent_status().state,
                    access: v.agent_access,
                };
                let org = facts.seat.and_then(|seat| self.floor_orgs.owner(seat));
                if org.is_some() && org == self.floor_org {
                    seats.push(facts);
                } else if org.is_none() || !query.trim().is_empty() {
                    loose.push(facts);
                }
            }
        }
        loose.sort_by_key(|f| std::cmp::Reverse(pane_search_score(&f.project, &f.name, &query)));
        let shown = seats.len() + loose.len();

        // ---- lay the chart out ----
        let s = th.grade.scale.clamp(0.7, 1.6);
        let (sw, sh, cg, rg, pad) = (SEAT_W * s, SEAT_H * s, COL_GAP * s, ROW_GAP * s, PAD * s);
        let missing: Vec<_> = self
            .floor_org
            .and_then(|id| self.floor_orgs.get(id))
            .into_iter()
            .flat_map(|o| o.seats.iter())
            .copied()
            .filter(|seat| {
                query.trim().is_empty() && self.threshing_floor_seat_leaf(*seat, cx).is_none()
            })
            .collect();
        let mut keys: Vec<(threshing_floor::SeatId, String, String)> = seats
            .iter()
            .filter_map(|f| f.seat.map(|seat| (seat, f.project.clone(), f.name.full())))
            .collect();
        keys.extend(
            missing
                .iter()
                .map(|seat| (*seat, String::new(), ui.floor_unavailable.to_string())),
        );
        let placed = threshing_floor::layout(&keys, &self.chart);
        let at = |seat: threshing_floor::SeatId| -> Option<(f32, f32)> {
            placed.iter().find(|p| p.seat == seat).map(|p| {
                (
                    pad + p.col as f32 * (sw + cg),
                    pad + p.row as f32 * (sh + rg),
                )
            })
        };
        let cols = placed.iter().map(|p| p.col + 1).max().unwrap_or(0);
        let rows = placed.iter().map(|p| p.row + 1).max().unwrap_or(0);
        let chart_w = (pad * 2. + cols as f32 * (sw + cg) - cg).max(420. * s);
        let chart_h = (pad * 2. + rows as f32 * (sh + rg) - rg).max(sh + pad * 2.);

        let rung_word = |r: threshing_floor::Rung| match r {
            threshing_floor::Rung::See => ui.floor_rung_see,
            threshing_floor::Rung::Read => ui.floor_rung_read,
            threshing_floor::Rung::Ask => ui.floor_rung_ask,
            threshing_floor::Rung::Prompt => ui.floor_rung_prompt,
        };
        let wires: Vec<Wire> = self
            .chart
            .saved()
            .iter()
            .filter_map(|l| {
                let (fx, fy) = at(l.from)?;
                let (tx, ty) = at(l.to)?;
                // The wire reads as a sentence fragment with a direction — "may
                // prompt \u{2192}" — so the relationship, not just a bare word,
                // shows on the line. A bypass keeps its remaining-count; a word
                // this build cannot read is drawn as "?", granting nothing.
                let label = match (l.rung.known(), l.prompts_left) {
                    (Some(r), None) => lang::fill(ui.floor_rel_wire, &[("rung", rung_word(r))]),
                    (Some(r), Some(n)) => format!(
                        "{} · {}",
                        lang::fill(ui.floor_rel_wire, &[("rung", rung_word(r))]),
                        lang::fill(ui.floor_bypass, &[("n", &n.to_string())])
                    ),
                    (None, _) => lang::fill(ui.floor_rel_wire, &[("rung", ui.floor_unknown)]),
                };
                let (points, label_at) = wire_route((fx, fy), (tx, ty), sw, sh, cg, rg);
                Some(Wire {
                    from: l.from,
                    to: l.to,
                    points,
                    bypass: l.is_bypass(),
                    rung: l.rung.known(),
                    label_at,
                    label,
                })
            })
            .collect();

        // ---- counts ----
        let working = seats
            .iter()
            .filter(|f| f.state == hud::AgentState::Working)
            .count();
        let needs = seats.iter().filter(|f| f.state.needs_you()).count();
        let counts = lang::fill(
            ui.floor_counts,
            &[
                ("seats", &seats.len().to_string()),
                ("working", &working.to_string()),
                ("needs", &needs.to_string()),
                ("off", &loose.len().to_string()),
            ],
        );

        // ---- the chart: wires on a canvas, seats and labels over it ----
        let line_ink = th.accent;
        let bypass_ink = th.ansi[3];
        // Off-limits beats every line, so a line INTO an off-limits seat grants
        // nothing — draw it faint and dashed rather than live accent, or the chart
        // reads as a working Read/Ask into a pane the person has walled off.
        let hidden_ink = th.faint;
        let off_limits: std::collections::HashSet<threshing_floor::SeatId> = seats
            .iter()
            .filter(|f| matches!(f.access, threshing_floor::AgentAccess::OffLimits))
            .filter_map(|f| f.seat)
            .collect();
        let paths: Vec<WireStroke> = wires
            .iter()
            .map(|w| (w.points.clone(), w.bypass, off_limits.contains(&w.to)))
            .collect();
        let canvas = gpui::canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                for (pts, bypass, ineffective) in &paths {
                    let ink = if *ineffective {
                        hidden_ink
                    } else if *bypass {
                        bypass_ink
                    } else {
                        line_ink
                    };
                    // A line to a walled-off pane is drawn dashed, like a bypass,
                    // to read as "not a live grant".
                    stroke_wire(bounds, pts, ink, *bypass || *ineffective, window);
                }
            },
        )
        .absolute()
        .inset_0();

        let pick = self.threshing_floor_pick;
        let mut chart = div()
            .relative()
            .flex_none()
            .w(px(chart_w))
            .h(px(chart_h))
            .child(canvas);
        if seats.is_empty() && missing.is_empty() && query.trim().is_empty() {
            chart = chart.child(
                div()
                    .debug_selector(|| "floor-org-empty-hint".to_string())
                    .absolute()
                    .left(px(pad))
                    .top(px(pad))
                    .w(px(chart_w - pad * 2.))
                    .text_color(th.text.alpha(0.6))
                    .child(SharedString::from(
                        if self.floor_org.is_some() {
                            ui.floor_org_drop
                        } else {
                            ui.floor_org_empty
                        }
                        .to_string(),
                    )),
            );
        }
        for f in &seats {
            let Some(seat) = f.seat else { continue };
            let Some((x, y)) = at(seat) else { continue };
            let picked = pick == Some(seat);
            let state_ink = agent_state_glow(th, th.accent, f.state);
            let role = match self.chart.role_of(seat) {
                threshing_floor::Role::Orchestrator => ui.floor_role_orchestrator,
                threshing_floor::Role::Manager => ui.floor_role_manager,
                threshing_floor::Role::Worker => ui.floor_role_worker,
                threshing_floor::Role::Alone => ui.floor_role_alone,
            };
            let second = model_line(
                &f.program,
                f.model.as_deref(),
                f.effort.as_deref(),
                ui.floor_model_unknown,
            );
            // Off-limits is the strongest position and must show on the card,
            // beside the state where the eye already goes: the person set it to
            // hide the pane from every agent, line or no.
            let badge = match f.access {
                threshing_floor::AgentAccess::OffLimits => {
                    Some(self.lang.strings().parts.pane_off_limits_badge)
                }
                threshing_floor::AgentAccess::ReadOnly => {
                    Some(self.lang.strings().parts.pane_read_only_badge)
                }
                _ => None,
            };
            let state_line = match badge {
                Some(b) => format!("\u{25cf} {} · {role} · {b}", f.state.label_in(lang)),
                None => format!("\u{25cf} {} · {role}", f.state.label_in(lang)),
            };
            let produced = f.artifact.is_some();
            // A header that only says "shell", or repeats the tab, adds a line
            // and no information.
            let header = {
                let h = f.name.header.trim();
                (!h.is_empty() && !h.eq_ignore_ascii_case("shell") && h != f.name.tab.trim())
                    .then(|| h.to_string())
            };
            // A guessed binding is said as such, never quoted (Bond::Guess).
            let message = if f.message_unproven {
                Some(ui.floor_conversation_unknown.to_string())
            } else {
                f.message
                    .as_deref()
                    .and_then(|m| m.lines().map(str::trim).find(|l| !l.is_empty()))
                    .map(str::to_string)
            };
            let message_ink = if f.message_unproven {
                th.text.alpha(0.45)
            } else {
                th.text.alpha(0.85)
            };
            let pane_drag = PaneDrag {
                leaf: f.leaf.clone(),
                title: f.name.tab.clone(),
                ink: th.accent,
                bg: th.surface,
            };
            let rel_drag = RelationDrag {
                from: seat,
                title: format!("{} →", f.name.tab),
                ink: th.accent,
                bg: th.surface,
            };
            let drop_ink = th.accent;
            chart = chart.child(
                div()
                    .id(SharedString::from(format!("floor-seat-{}", seat.0)))
                    .debug_selector(move || format!("floor-seat-{}", seat.0))
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(sw))
                    .h(px(sh))
                    .px(px(12. * s))
                    .py(px(9. * s))
                    .rounded(sk.rad_raw(7.))
                    .border_1()
                    // Nothing handed over yet wears the theme's accent: a fact
                    // about the work, drawn as an accent and never as an alarm.
                    .border_color(if picked {
                        th.complement
                    } else if produced {
                        th.text.alpha(0.28)
                    } else {
                        th.accent.alpha(0.8)
                    })
                    .bg(darken(th.surface, 0.35))
                    .flex()
                    .flex_col()
                    .gap(px(8. * s))
                    .cursor(gpui::CursorStyle::OpenHand)
                    .when(dragging == Some(&f.leaf), |d| d.opacity(0.35))
                    .hover(|d| d.border_color(th.accent))
                    .on_drag(pane_drag, {
                        let ws = cx.entity().downgrade();
                        move |d: &PaneDrag, position, window: &mut Window, cx: &mut App| {
                            let _ = ws.update(cx, |ws, cx| {
                                ws.threshing_floor_begin_drag(&d.leaf, window, cx)
                            });
                            cx.new(|_| DragPreview {
                                title: d.title.clone(),
                                offset: position,
                                ink: d.ink,
                                bg: d.bg,
                            })
                        }
                    })
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener({
                            let leaf = f.leaf.clone();
                            move |ws, ev: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                ws.floor_context = Some((leaf.clone(), ev.position));
                                window.focus(&ws.focus_handle, cx);
                                cx.notify();
                            }
                        }),
                    )
                    .drag_over::<RelationDrag>(move |style, d, _, _| {
                        if d.from != seat {
                            style.border_color(drop_ink)
                        } else {
                            style
                        }
                    })
                    .on_drop(cx.listener(move |ws, d: &RelationDrag, _, cx| {
                        ws.threshing_floor_begin_relation(Some(d.from), Some(seat), cx);
                    }))
                    // Four groups, read top to bottom: who it is, what it is doing
                    // now, what it runs on, and what it has produced. Lines in a
                    // group sit close and a wider gap falls between groups, so a
                    // sweep of the floor sorts each card before it is read. The
                    // produced row sits at the foot of every card, so it lines up
                    // from card to card.
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2. * s))
                            // Where it sits: the tab's own name is kept whole, and the
                            // levels above it are the part that clips on a narrow box.
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(sk.text_px(10.5))
                                    .when(!f.name.outer.is_empty(), |d| {
                                        // The `›` sits outside the part that clips, so
                                        // a cut group never runs into the tab's name.
                                        d.child(
                                            div()
                                                .min_w(px(0.))
                                                .truncate()
                                                .text_color(th.text.alpha(0.6))
                                                .child(SharedString::from(f.name.outer.clone())),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_color(th.text.alpha(0.6))
                                                .child("\u{a0}\u{203a}\u{a0}"),
                                        )
                                    })
                                    .child(
                                        div()
                                            .flex_none()
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(th.text)
                                            .child(SharedString::from(f.name.tab.clone())),
                                    ),
                            )
                            .when_some(header, |d, h| {
                                d.child(
                                    div()
                                        .text_size(sk.text_px(8.5))
                                        .text_color(th.text.alpha(0.55))
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .child(SharedString::from(h)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2. * s))
                            .when_some(message, |d, m| {
                                d.child(
                                    div()
                                        .id(SharedString::from(format!("floor-message-{}", seat.0)))
                                        .debug_selector(move || format!("floor-message-{}", seat.0))
                                        .text_size(sk.text_px(8.5))
                                        .text_color(message_ink)
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .child(SharedString::from(m)),
                                )
                            })
                            .child(
                                div()
                                    .text_size(sk.text_px(8.5))
                                    .text_color(state_ink)
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(SharedString::from(state_line)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2. * s))
                            .child(
                                div()
                                    .text_size(sk.text_px(8.5))
                                    .text_color(th.text.alpha(0.6))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(SharedString::from(second)),
                            )
                            .child(self.threshing_floor_metrics(f, &sk, th)),
                    )
                    .child(
                        div()
                            .mt_auto()
                            .pt(px(6. * s))
                            .mr(px(30. * s))
                            .border_t_1()
                            .border_color(th.text.alpha(0.12))
                            .child(self.threshing_floor_artifact_row(f, seat, &sk, th, cx)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("floor-connect-{}", seat.0)))
                            .debug_selector(move || format!("floor-connect-{}", seat.0))
                            .absolute()
                            .right(px(7. * s))
                            .bottom(px(5. * s))
                            .w(px(24. * s))
                            .h(px(20. * s))
                            .rounded(sk.rad_raw(4.))
                            .border_1()
                            .border_color(th.accent.alpha(0.7))
                            .text_color(th.accent)
                            .text_size(sk.text_px(12.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("○")
                            .cursor_crosshair()
                            .on_drag(rel_drag, |d, position, _, cx| {
                                cx.new(|_| DragPreview {
                                    title: d.title.clone(),
                                    offset: position,
                                    ink: d.ink,
                                    bg: d.bg,
                                })
                            })
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.threshing_floor_begin_relation(Some(seat), None, cx);
                            })),
                    )
                    .on_click(cx.listener(move |ws, _, _w, cx| {
                        cx.stop_propagation();
                        ws.threshing_floor_click_seat(seat, cx);
                    })),
            );
        }
        for seat in missing {
            let Some((x, y)) = at(seat) else {
                continue;
            };
            chart = chart.child(
                div()
                    .debug_selector(move || format!("floor-unavailable-{}", seat.0))
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(sw))
                    .h(px(sh))
                    .px(px(12. * s))
                    .py(px(9. * s))
                    .border_1()
                    .border_color(th.text.alpha(0.3))
                    .rounded(sk.rad_raw(7.))
                    .flex()
                    .flex_col()
                    .gap(px(8. * s))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2. * s))
                            .child(
                                div()
                                    .text_size(sk.text_px(10.5))
                                    .text_color(th.text.alpha(0.75))
                                    .child(SharedString::from(ui.floor_unavailable.to_string())),
                            )
                            .child(
                                div()
                                    .text_size(sk.text_px(8.5))
                                    .text_color(th.text.alpha(0.5))
                                    .child(SharedString::from(
                                        ui.floor_unavailable_why.to_string(),
                                    )),
                            )
                            // Its pane is gone, so what it produced is not known
                            // here: said plainly, never drawn as "nothing yet".
                            .child(
                                div()
                                    .text_size(sk.text_px(8.5))
                                    .text_color(th.text.alpha(0.45))
                                    .child(SharedString::from(
                                        ui.floor_artifacts_unknown.to_string(),
                                    )),
                            ),
                    )
                    // The one thing to do with it, alone at the foot.
                    .child(
                        div().mt_auto().flex().flex_row().child(
                            Self::bezel_btn(&sk, ui.floor_unseat_btn, false)
                                .id(SharedString::from(format!("floor-forget-{}", seat.0)))
                                .on_click(cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.chart.forget_seat(seat);
                                    ws.floor_orgs.remove(seat);
                                    ws.save(cx);
                                    cx.notify();
                                })),
                        ),
                    ),
            );
        }
        for w in &wires {
            let (from, to) = (w.from, w.to);
            // The chip is inked by the rung, so the clearance reads at a glance:
            // prompt, which writes into the worker, in the action amber; ask in
            // full accent; the quieter rungs fainter; an unreadable word faint.
            // A bypass keeps its amber.
            let rink = if w.bypass {
                bypass_ink
            } else {
                match w.rung {
                    Some(threshing_floor::Rung::Prompt) => th.ansi[3],
                    Some(threshing_floor::Rung::Ask) => line_ink,
                    Some(threshing_floor::Rung::Read) => line_ink,
                    Some(threshing_floor::Rung::See) => line_ink.alpha(0.75),
                    None => hidden_ink,
                }
            };
            // Sized to its words, and kept inside the chart's own edges.
            let lw = (w.label.chars().count() as f32 * 6.4 + 16.) * s;
            let lx = (w.label_at.0 - lw / 2.).clamp(2., (chart_w - lw - 2.).max(2.));
            chart = chart.child(
                div()
                    .id(SharedString::from(format!(
                        "floor-line-{}-{}",
                        from.0, to.0
                    )))
                    .debug_selector(move || format!("floor-line-{}-{}", from.0, to.0))
                    .absolute()
                    .left(px(lx))
                    .top(px(w.label_at.1 - 9. * s))
                    .w(px(lw))
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .px(px(6. * s))
                            .rounded(sk.rad_raw(5.))
                            .bg(darken(th.surface, 0.55))
                            .border_1()
                            .border_color(rink.alpha(0.7))
                            .text_size(sk.text_px(8.))
                            .text_color(rink)
                            .cursor_pointer()
                            .child(SharedString::from(w.label.clone())),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.threshing_floor_click_line(from, to, cx);
                        }),
                    ),
            );
        }

        // ---- the pane pool ----
        let mut off = div().w(px(sw)).flex_none().flex().flex_col().gap_2();
        for f in &loose {
            let leaf = f.leaf.clone();
            let is_agent = leaf.read(cx).mode.is_agent();
            let owner = f.seat.and_then(|s| self.floor_orgs.owner(s));
            let pane_drag = PaneDrag {
                leaf: leaf.clone(),
                title: f.name.tab.clone(),
                ink: th.accent,
                bg: th.surface,
            };
            off = off.child(
                div()
                    .id(SharedString::from(format!(
                        "floor-pane-{}",
                        leaf.entity_id().as_u64()
                    )))
                    .debug_selector({
                        let id = leaf.entity_id().as_u64();
                        move || format!("floor-pane-{id}")
                    })
                    .w(px(sw))
                    .h(px(sh))
                    .flex_none()
                    .overflow_hidden()
                    .p_2()
                    .rounded(sk.rad_raw(7.))
                    .border_1()
                    .border_color(th.text.alpha(0.28))
                    .bg(darken(th.surface, 0.35))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .when(is_agent, |d| {
                        d.cursor(gpui::CursorStyle::OpenHand)
                            .when(dragging == Some(&f.leaf), |d| d.opacity(0.35))
                            .on_drag(pane_drag, {
                                let ws = cx.entity().downgrade();
                                move |d: &PaneDrag, position, window: &mut Window, cx: &mut App| {
                                    let _ = ws.update(cx, |ws, cx| {
                                        ws.threshing_floor_begin_drag(&d.leaf, window, cx)
                                    });
                                    cx.new(|_| DragPreview {
                                        title: d.title.clone(),
                                        offset: position,
                                        ink: d.ink,
                                        bg: d.bg,
                                    })
                                }
                            })
                    })
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener({
                            let leaf = f.leaf.clone();
                            move |ws, ev: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                ws.floor_context = Some((leaf.clone(), ev.position));
                                window.focus(&ws.focus_handle, cx);
                                cx.notify();
                            }
                        }),
                    )
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(th.text.alpha(0.85))
                            .child(SharedString::from(f.name.place())),
                    )
                    .child(
                        div()
                            .text_size(sk.text_px(8.5))
                            .text_color(th.text.alpha(0.8))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(f.name.header.clone())),
                    )
                    .child(
                        div()
                            .text_size(sk.text_px(8.5))
                            .text_color(th.text.alpha(0.6))
                            .child(SharedString::from(f.program.clone())),
                    )
                    .child(self.threshing_floor_metrics(f, &sk, th))
                    .when(f.access.is_off_limits(), |d| {
                        d.child(
                            div()
                                .text_size(sk.text_px(8.5))
                                .text_color(th.faint)
                                .child(self.lang.strings().parts.pane_off_limits_badge),
                        )
                    })
                    .when(f.seat.is_some() && owner.is_none(), |d| {
                        d.child(
                            div()
                                .text_size(sk.text_px(8.5))
                                .text_color(th.text.alpha(0.6))
                                .child(SharedString::from(ui.floor_org_unknown.to_string())),
                        )
                    })
                    .when_some(owner, |d, org| {
                        d.child(
                            div()
                                .text_size(sk.text_px(8.5))
                                .text_color(th.accent)
                                .child(SharedString::from(self.threshing_floor_org_label(org))),
                        )
                    })
                    .when(is_agent && owner.is_some(), |d| {
                        let org = owner.unwrap();
                        d.child(
                            Self::bezel_btn(&sk, ui.floor_org_open, false)
                                .id(SharedString::from(format!(
                                    "floor-open-org-{}",
                                    leaf.entity_id().as_u64()
                                )))
                                .debug_selector(move || format!("floor-open-org-{}", org.0))
                                .on_click(cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.threshing_floor_set_org(org, cx);
                                })),
                        )
                    })
                    .when(
                        is_agent && owner.is_none() && self.floor_org.is_some(),
                        |d| {
                            d.child(
                                Self::bezel_btn(&sk, ui.floor_org_add, false)
                                    .id(SharedString::from(format!(
                                        "floor-seat-btn-{}",
                                        leaf.entity_id().as_u64()
                                    )))
                                    .hover(|d| d.border_color(th.accent))
                                    .on_click(cx.listener(move |ws, _, _w, cx| {
                                        cx.stop_propagation();
                                        ws.threshing_floor_seat(&leaf, cx);
                                    })),
                            )
                        },
                    ),
            );
        }

        let switches = self.mcp_switch_buttons(&sk, th, "war", cx);
        // Headers are siblings of scroll bodies. Zero minimum height on every
        // flex ancestor prevents the contents from stretching the frame.
        let header = div()
            .id("threshing-floor-header")
            .debug_selector(|| "threshing-floor-header".to_string())
            .flex_none()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .when(drag_active, |d| d.opacity(0.3))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_size(sk.text_px(13.))
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.complement)
                            .child(SharedString::from(ui.floor_hdr.to_string())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_size(sk.text_px(9.))
                            .text_color(th.text.alpha(0.6))
                            .child(SharedString::from(counts)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap_1()
                    .children(switches),
            )
            // Finding a pane is a different job from the window's switches above
            // it, so it starts a group of its own: a wider gap here than inside.
            .child(
                div()
                    .id("threshing-floor-search")
                    .debug_selector(|| "threshing-floor-search".to_string())
                    .mt_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .rounded(sk.rad_raw(6.))
                    .border_1()
                    .border_color(th.accent.alpha(0.6))
                    .bg(darken(th.surface, 0.35))
                    .cursor(gpui::CursorStyle::IBeam)
                    .child(render_edit_buffer(
                        &self.threshing_floor_query,
                        s,
                        th.text,
                        th.accent,
                        th.accent.alpha(0.3),
                    ))
                    .when(query.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(th.text.alpha(0.55))
                                .child(SharedString::from(ui.floor_search.to_string())),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Self::bezel_btn(&sk, "×", false)
                            .id("threshing-floor-clear")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.threshing_floor_query = EditBuffer::seeded("");
                                    ws.threshing_floor_pick = None;
                                    cx.notify();
                                }),
                            ),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, w, cx| {
                            cx.stop_propagation();
                            w.focus(&ws.focus_handle, cx);
                        }),
                    ),
            )
            .child(
                div()
                    .text_color(th.text.alpha(0.6))
                    .child(SharedString::from(lang::fill(
                        ui.floor_results,
                        &[("shown", &shown.to_string()), ("total", &total.to_string())],
                    ))),
            )
            .when(shown == 0 && !query.trim().is_empty(), |d| {
                d.child(
                    div()
                        .p_2()
                        .child(SharedString::from(ui.floor_no_matches.to_string())),
                )
            });
        let org_header = div()
            .id("floor-org-header")
            .debug_selector(|| "floor-org-header".to_string())
            .flex_none()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .child(self.threshing_floor_org_controls(&sk, th, cx))
            .child(
                div()
                    .text_size(sk.text_px(8.5))
                    .text_color(th.accent.alpha(0.85))
                    .child(SharedString::from(
                        if drag_active {
                            ui.floor_drag_hint
                        } else {
                            ui.floor_hint
                        }
                        .to_string(),
                    )),
            )
            .children(
                self.floor_map
                    .is_none()
                    .then(|| self.threshing_floor_relationship_controls(&sk, th, cx)),
            )
            // The session's numbers report on the floor rather than act on it:
            // set a little apart from the org controls.
            .children(self.ask_numbers.as_ref().map(|n| {
                div()
                    .mt_2()
                    .text_size(sk.text_px(8.5))
                    .text_color(if n.unlined_answered > 0 {
                        th.ansi[1]
                    } else {
                        th.text.alpha(0.6)
                    })
                    .child(SharedString::from(lang::fill(
                        ui.floor_asks,
                        &[
                            ("cheap", &n.cheap_answers.to_string()),
                            ("copies", &n.copy_answers.to_string()),
                            ("unlined", &n.unlined_answered.to_string()),
                        ],
                    )))
            }));
        let drop_chart_ink = th.accent;
        let chart_tile = div()
            .id("floor-org-tile")
            .debug_selector(|| "floor-org-tile".to_string())
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_2()
            .child(org_header)
            // The active org titles its own chart, in its own ink — a cool
            // heading distinct from the room's pink header and the green field,
            // so the board reads as "this org's chart" instead of cards adrift
            // in empty space.
            .children(self.floor_org.map(|id| {
                div()
                    .id("floor-org-title")
                    .debug_selector(|| "floor-org-title".to_string())
                    .flex_none()
                    .pb_1()
                    .border_b_1()
                    .border_color(th.ansi[6].alpha(0.35))
                    .text_size(sk.text_px(13.))
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_color(th.ansi[6])
                    .child(SharedString::from(self.threshing_floor_org_label(id)))
            }))
            .child(
                div()
                    .id("threshing-floor-chart")
                    .debug_selector(|| "threshing-floor-chart".to_string())
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .overflow_x_scroll()
                    .overflow_y_scroll()
                    .rounded(sk.rad_raw(8.))
                    .border_1()
                    .border_color(if chart_active {
                        th.accent.alpha(0.65)
                    } else {
                        th.text.alpha(0.14)
                    })
                    .when(chart_active, |d| {
                        d.bg(th.accent.alpha(0.035))
                            .shadow(drop_glow(th.accent, false))
                    })
                    .drag_over::<PaneDrag>(move |d, _, _, _| {
                        d.border_color(drop_chart_ink)
                            .bg(drop_chart_ink.alpha(0.10))
                            .shadow(drop_glow(drop_chart_ink, true))
                    })
                    .can_drop({
                        let has_org = self.floor_org.is_some();
                        move |_, _, _| has_org
                    })
                    .on_drop(cx.listener(|ws, d: &PaneDrag, _, cx| {
                        if let Some(org) = ws.floor_org {
                            ws.threshing_floor_assign(&d.leaf, org, false, cx);
                        }
                    }))
                    .child(if self.floor_map.is_some() {
                        self.floor_system_map(&sk, th, cx).into_any_element()
                    } else {
                        chart.into_any_element()
                    }),
            );
        let pool_tile = div()
            .id("floor-pool-tile")
            .debug_selector(|| "floor-pool-tile".to_string())
            .w(px(sw + 18.))
            .max_w(gpui::relative(0.4))
            .flex_none()
            .min_w(px(0.))
            .min_h(px(0.))
            .p_2()
            .rounded(sk.rad_raw(8.))
            .border_1()
            .border_color(if removing {
                th.accent.alpha(0.65)
            } else {
                th.text.alpha(0.14)
            })
            .when(removing, |d| {
                d.bg(th.accent.alpha(0.035))
                    .shadow(drop_glow(th.accent, false))
            })
            .drag_over::<PaneDrag>(move |style, d, _, cx| {
                if d.leaf.read(cx).seat.is_some() {
                    style
                        .border_color(drop_chart_ink)
                        .bg(drop_chart_ink.alpha(0.10))
                        .shadow(drop_glow(drop_chart_ink, true))
                } else {
                    style
                }
            })
            .on_drop(cx.listener(|ws, d: &PaneDrag, _, cx| ws.threshing_floor_unseat(&d.leaf, cx)))
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("floor-pool-header")
                    .debug_selector(|| "floor-pool-header".to_string())
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(th.text.alpha(0.85))
                            .child(SharedString::from(ui.floor_off_hdr.to_string())),
                    )
                    .child(
                        div()
                            .text_size(sk.text_px(8.5))
                            .text_color(th.text.alpha(0.6))
                            .child(SharedString::from(
                                if removing {
                                    ui.floor_drop_remove
                                } else {
                                    ui.floor_off_blurb
                                }
                                .to_string(),
                            )),
                    ),
            )
            .child(
                div()
                    .id("threshing-floor-pool")
                    .debug_selector(|| "threshing-floor-pool".to_string())
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .overflow_x_scroll()
                    .overflow_y_scroll()
                    .child(off),
            );
        let panel = div()
            .id("threshing-floor-panel")
            .debug_selector(|| "threshing-floor-panel".to_string())
            .w(gpui::relative(0.95))
            .h(gpui::relative(0.9))
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_hidden()
            .p_3()
            .rounded(sk.radius_lg())
            .border_2()
            .border_color(th.accent.alpha(0.85))
            .bg(darken(th.surface, 0.6))
            .shadow(float_shadows(th.accent, th.grade.phosphor))
            .flex()
            .flex_col()
            .gap_2()
            .text_size(sk.text_px(10.))
            .text_color(th.text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    if ws.threshing_floor_pick.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .map(|d| sk.chrome(sk.finish(d)))
            .child(header)
            .child(
                div()
                    .id("threshing-floor-tiles")
                    .debug_selector(|| "threshing-floor-tiles".to_string())
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .overflow_hidden()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .relative()
                    .when(drag_active, |d| d.child(drag_focus()))
                    .child(chart_tile)
                    .child(pool_tile),
            )
            .child(self.threshing_floor_drag_footer(removing, drag_active, &sk, th, cx));

        Some(
            div()
                .occlude()
                .absolute()
                .inset_0()
                .bg(th.bg.alpha(0.70))
                .flex()
                .items_center()
                .justify_center()
                .on_drag_move::<PaneDrag>(|_, window, cx| {
                    cx.set_active_drag_cursor_style(gpui::CursorStyle::ClosedHand, window);
                })
                .capture_any_mouse_up(cx.listener(|ws, _, _, cx| {
                    if ws.floor_drag.take().is_some() {
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.floor_context = None;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        ws.close_threshing_floor();
                        ws.focus_active(window, cx);
                        cx.notify();
                    }),
                )
                .child(panel)
                .children(self.threshing_floor_context_menu(&sk, th, cx)),
        )
    }

    fn threshing_floor_begin_drag(
        &mut self,
        leaf: &Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.floor_drag = Some(leaf.clone());
        self.floor_context = None;
        self.floor_feedback = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn threshing_floor_drag_footer(
        &self,
        removing: bool,
        dragging: bool,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let ink = th.accent;
        div()
            .h(px(60.))
            .flex_none()
            .min_w(px(0.))
            .flex()
            .items_center()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .text_size(sk.text_px(9.))
                    .text_color(th.text.alpha(0.8))
                    .child(SharedString::from(if dragging {
                        ui.floor_drag_hint.to_string()
                    } else {
                        self.floor_feedback.clone().unwrap_or_default()
                    })),
            )
            .when(removing, |d| {
                d.child(
                    div()
                        .id("floor-remove-drop")
                        .debug_selector(|| "floor-remove-drop".to_string())
                        .h_full()
                        .w(px(236.))
                        .flex_none()
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(sk.rad_raw(8.))
                        .border_1()
                        .border_color(ink.alpha(0.65))
                        .bg(ink.alpha(0.035))
                        .shadow(drop_glow(ink, false))
                        .drag_over::<PaneDrag>(move |d, _, _, _| {
                            d.border_color(ink)
                                .bg(ink.alpha(0.14))
                                .shadow(drop_glow(ink, true))
                        })
                        .on_drop(cx.listener(|ws, d: &PaneDrag, _, cx| {
                            ws.threshing_floor_unseat(&d.leaf, cx)
                        }))
                        .child(trash_icon(ink))
                        .child(
                            div()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_color(ink).child(ui.floor_remove))
                                .child(
                                    div()
                                        .text_size(sk.text_px(8.))
                                        .text_color(th.text.alpha(0.7))
                                        .child(ui.floor_access_off),
                                ),
                        ),
                )
            })
    }

    fn threshing_floor_context_menu(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (leaf, pos) = self.floor_context.as_ref()?;
        let ui = self.lang.strings().ui;
        let parts = self.lang.strings().parts;
        let row = |label: String| {
            div()
                .px_3()
                .py_2()
                .cursor_pointer()
                .child(label)
                .hover(|d| d.bg(th.accent.alpha(0.16)))
        };
        let mut menu = div()
            .id("floor-context-menu")
            .debug_selector(|| "floor-context-menu".to_string())
            .min_w(px(220.))
            .max_w(px(360.))
            .py_1()
            .flex()
            .flex_col()
            .occlude()
            .bg(th.surface)
            .border_1()
            .border_color(th.accent.alpha(0.7))
            .rounded(sk.rad_raw(7.))
            .text_size(sk.text_px(11.))
            .text_color(th.text)
            .shadow_md()
            .on_mouse_down_out(cx.listener(|ws, _, _, cx| {
                ws.floor_context = None;
                cx.notify();
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px_3()
                    .py_1()
                    .truncate()
                    .text_color(th.faint)
                    .child(SharedString::from(self.pane_name(leaf, cx).place())),
            );
        if leaf.read(cx).seat.is_some() {
            let leaf = leaf.clone();
            menu = menu.child(
                row(ui.floor_remove.to_string())
                    .id("floor-context-remove")
                    .debug_selector(|| "floor-context-remove".to_string())
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.threshing_floor_unseat(&leaf, cx);
                    })),
            );
        }
        for (access, label, id) in [
            (
                threshing_floor::AgentAccess::Open,
                parts.pane_ctx_let_agents_see,
                "floor-context-open",
            ),
            (
                threshing_floor::AgentAccess::ReadOnly,
                parts.pane_ctx_read_only,
                "floor-context-read-only",
            ),
            (
                threshing_floor::AgentAccess::OffLimits,
                parts.pane_ctx_off_limits,
                "floor-context-off-limits",
            ),
        ] {
            let mark = if leaf.read(cx).agent_access == access {
                "●"
            } else {
                "○"
            };
            let leaf = leaf.clone();
            menu = menu.child(
                row(format!("{mark} {label}"))
                    .id(id)
                    .debug_selector(move || id.to_string())
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        leaf.update(cx, |v, cx| v.set_agent_access(access, cx));
                        ws.floor_context = None;
                        ws.save(cx);
                        cx.notify();
                    })),
            );
        }
        Some(deferred(anchored().position(*pos).snap_to_window().child(menu)).into_any_element())
    }

    /// Modal keyboard capture keeps all editing away from the underlying PTY.
    pub(crate) fn threshing_floor_capture_key(
        &mut self,
        key: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.threshing_floor {
            return false;
        }
        if key.key == "escape" {
            if cx.stop_active_drag(window) {
                self.floor_drag = None;
                cx.notify();
                return true;
            }
            if self.floor_context.take().is_some() {
                cx.notify();
                return true;
            }
            if self.floor_drop.take().is_some()
                || self.floor_move.take().is_some()
                || self.floor_org_edit.take().is_some()
                || self.floor_relation.take().is_some()
            {
            } else if !self.threshing_floor_query.text().is_empty() {
                self.threshing_floor_query = EditBuffer::seeded("");
                self.threshing_floor_pick = None;
            } else {
                self.close_threshing_floor();
                self.focus_active(window, cx);
            }
            cx.notify();
            return true;
        }
        if cx.has_active_drag() {
            return true;
        }
        if self.floor_context.is_some() {
            return true;
        }
        if key.key == "delete" && self.floor_relation.is_none() && self.floor_org_edit.is_none() {
            if let Some(leaf) = self
                .threshing_floor_pick
                .and_then(|seat| self.threshing_floor_seat_leaf(seat, cx))
            {
                self.threshing_floor_unseat(&leaf, cx);
                return true;
            }
        }
        if key.key == "enter" {
            if self.floor_org_edit.is_some() {
                self.threshing_floor_save_org(cx);
            } else if self.floor_relation.is_some() {
                self.threshing_floor_save_relation(cx);
            }
            return true;
        }
        if self.floor_relation.is_some() || self.floor_move.is_some() {
            return true;
        }
        let editing_org = self.floor_org_edit.is_some();
        let buffer = match &mut self.floor_org_edit {
            Some((_, text)) => text,
            None => &mut self.threshing_floor_query,
        };
        let before = buffer.text();
        let cap = if editing_org { 80 } else { 256 };
        if key.key == "v" && (key.modifiers.control || key.modifiers.platform) {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                let text: String = text
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                buffer.insert_capped(&text, cap);
            }
        } else {
            buffer.apply(&key.key, &key.modifiers, key.key_char.as_deref(), cap);
        }
        if !editing_org && buffer.text() != before {
            self.threshing_floor_pick = None;
        }
        cx.notify();
        true
    }

    /// The card's last row: the agent's newest artifact by name, how many more
    /// it has handed over, and whether a person has left feedback on it. A click
    /// opens it in that agent's pane, the way the artifact rail does. An agent
    /// that has handed nothing over says so in the accent.
    fn threshing_floor_artifact_row(
        &self,
        f: &PaneFacts,
        seat: threshing_floor::SeatId,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let ui = self.lang.strings().ui;
        let s = th.grade.scale;
        let Some((label, more, feedback)) = &f.artifact else {
            return div()
                .id(SharedString::from(format!("floor-no-artifact-{}", seat.0)))
                .debug_selector(move || format!("floor-no-artifact-{}", seat.0))
                .text_size(sk.text_px(8.5))
                .text_color(th.accent)
                .child(SharedString::from(ui.floor_no_artifact.to_string()))
                .into_any_element();
        };
        let pill = |word: &str, ink: Hsla, ground: Hsla| {
            div()
                .flex_none()
                .px(px(6. * s))
                .py(px(1. * s))
                .rounded(sk.rad_raw(8.))
                .bg(ground)
                .text_color(ink)
                .text_size(sk.text_px(7.5))
                .font_weight(gpui::FontWeight::BOLD)
                .child(SharedString::from(word.to_string()))
        };
        let pill = match feedback {
            crate::feedback::Feedback::Annotated => Some(
                pill(ui.floor_annotated, th.human, th.human.alpha(0.16))
                    .debug_selector(move || format!("floor-annotated-{}", seat.0)),
            ),
            crate::feedback::Feedback::None => Some(
                pill(
                    ui.floor_no_feedback,
                    th.text.alpha(0.6),
                    th.text.alpha(0.08),
                )
                .debug_selector(move || format!("floor-no-feedback-{}", seat.0)),
            ),
            // Nothing is claimed for an artifact that takes no notes here.
            crate::feedback::Feedback::Unknown => None,
        };
        let leaf = f.leaf.clone();
        div()
            .id(SharedString::from(format!("floor-artifact-{}", seat.0)))
            .debug_selector(move || format!("floor-artifact-{}", seat.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(5. * s))
            .min_w(px(0.))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(sk.text_px(8.5))
            .cursor_pointer()
            .child(
                div()
                    .min_w(px(0.))
                    .truncate()
                    .text_color(th.text)
                    .child(SharedString::from(label.clone())),
            )
            .when(*more > 0, |d| {
                d.child(
                    div()
                        .flex_none()
                        .text_color(th.text.alpha(0.55))
                        .child(SharedString::from(format!("+{more}"))),
                )
            })
            .when_some(pill, |d, p| d.child(p))
            .on_click(cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.close_threshing_floor();
                leaf.update(cx, |v, cx| v.open_newest_handover(cx));
                ws.pending_jump = Some(leaf.entity_id());
                cx.notify();
            }))
            .into_any_element()
    }

    fn threshing_floor_metrics(
        &self,
        f: &PaneFacts,
        sk: &skin::Skin,
        th: &theme::Theme,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let age = session_age(f.started_ms, now_ms() as i64)
            .unwrap_or_else(|| ui.floor_unknown.to_string());
        let tokens = f
            .size
            .map(hud::fmt_tokens)
            .unwrap_or_else(|| ui.floor_unknown.to_string());
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .text_size(sk.text_px(8.5))
            .text_color(th.text.alpha(0.85))
            .child(SharedString::from(format!("{} {age}", ui.floor_age)))
            .child(SharedString::from(format!("{} {tokens}", ui.floor_tokens)))
    }

    fn threshing_floor_org_label(&self, id: threshing_floor::OrgId) -> String {
        self.floor_orgs
            .get(id)
            .and_then(|o| o.name.clone())
            .unwrap_or_else(|| self.lang.strings().ui.floor_org_unnamed.to_string())
    }

    fn threshing_floor_set_org(&mut self, id: threshing_floor::OrgId, cx: &mut Context<Self>) {
        if self.floor_orgs.get(id).is_none() {
            return;
        }
        self.floor_org = Some(id);
        // The pane filter is the person's, not the view's: seating or moving a
        // pane (which lands here) must not wipe what they typed. Clearing is
        // theirs to do — the × button and Esc. See also begin_relation/unseat.
        self.threshing_floor_pick = None;
        self.floor_relation = None;
        self.floor_org_edit = None;
        self.floor_delete_org = None;
        self.floor_drop = None;
        self.save(cx);
        cx.notify();
    }

    fn threshing_floor_save_org(&mut self, cx: &mut Context<Self>) {
        let Some((id, buffer)) = &self.floor_org_edit else {
            return;
        };
        let text = buffer.text();
        let org = match id {
            Some(id) => self.floor_orgs.rename(*id, &text).then_some(*id),
            None => self.floor_orgs.create(&text),
        };
        if let Some(org) = org {
            self.threshing_floor_set_org(org, cx);
        }
    }

    /// How many relationships deleting `org` would erase: every seat-to-seat
    /// line touching a member, plus every org line that names the org. This is
    /// the count the confirm shows before anything is removed.
    fn threshing_floor_org_relationship_count(&self, org: threshing_floor::OrgId) -> usize {
        let members: std::collections::HashSet<threshing_floor::SeatId> = self
            .floor_orgs
            .get(org)
            .map(|o| o.seats.iter().copied().collect())
            .unwrap_or_default();
        let seat_lines = self
            .chart
            .saved()
            .iter()
            .filter(|l| members.contains(&l.from) || members.contains(&l.to))
            .count();
        seat_lines + self.chart.org_lines_touching(org)
    }

    /// Delete an org once the person has confirmed the count. Its members become
    /// loose — un-orged but still seated, so the panes stay available — while
    /// every relationship touching a member and every org line naming the org
    /// are erased. Nothing is kept dormant: a deleted org's grants are gone, and
    /// the view falls back to another org (or none).
    fn threshing_floor_delete_org(&mut self, org: threshing_floor::OrgId, cx: &mut Context<Self>) {
        let members = self.floor_orgs.delete(org);
        for seat in members {
            self.chart.forget_seat(seat);
            if let Some(leaf) = self.threshing_floor_seat_leaf(seat, cx) {
                self.threshing_floor_forget_pending(&leaf, seat, cx);
            }
        }
        self.chart.forget_org(org);
        self.floor_delete_org = None;
        if self.floor_org == Some(org) {
            self.floor_org = self.floor_orgs.saved().first().map(|o| o.id);
        }
        self.floor_relation = None;
        self.floor_move = None;
        self.floor_drop = None;
        self.threshing_floor_pick = None;
        self.save(cx);
        cx.notify();
    }

    /// What one System Map endpoint reads as: an agent by its pane name, or a
    /// whole org by its name with the "whole org" tag so the two never blur.
    fn floor_map_end_label(&self, end: threshing_floor::End, cx: &App) -> String {
        let ui = self.lang.strings().ui;
        match end {
            threshing_floor::End::Seat(seat) => self.threshing_floor_seat_label(seat, cx),
            threshing_floor::End::Org { org } => {
                format!(
                    "{} \u{00b7} {}",
                    self.threshing_floor_org_label(org),
                    ui.floor_map_whole_org
                )
            }
        }
    }

    /// Pick one end of the System Map's relationship. From is chosen first, then
    /// To; choosing the same endpoint twice is refused so a line never points at
    /// itself.
    fn floor_map_choose(&mut self, end: threshing_floor::End, cx: &mut Context<Self>) {
        if let Some(m) = &mut self.floor_map {
            match m.choosing {
                Some(RelationEnd::From) => {
                    m.from = Some(end);
                    if m.to == Some(end) {
                        m.to = None;
                    }
                    m.choosing = Some(RelationEnd::To);
                }
                Some(RelationEnd::To) if m.from != Some(end) => {
                    m.to = Some(end);
                    m.choosing = None;
                }
                _ => return,
            }
        }
        cx.notify();
    }

    /// Draw the System Map's relationship: a seat-to-seat line (even across
    /// orgs) goes on the chart as a plain line; anything naming a whole org is a
    /// whole-org line. The draft clears so the next one starts fresh.
    fn floor_map_draw(&mut self, cx: &mut Context<Self>) {
        let Some(m) = self.floor_map.as_ref() else {
            return;
        };
        let (Some(from), Some(to), Some(rung)) = (m.from, m.to, m.rung) else {
            return;
        };
        if from == to {
            return;
        }
        match (from, to) {
            (threshing_floor::End::Seat(f), threshing_floor::End::Seat(t)) => {
                self.chart.draw(f, t, rung)
            }
            _ => self.chart.draw_org(from, to, rung),
        }
        if let Some(m) = &mut self.floor_map {
            m.from = None;
            m.to = None;
            m.rung = None;
            m.choosing = None;
        }
        self.save(cx);
        cx.notify();
    }

    fn threshing_floor_org_controls(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let dragging = self.floor_drag.is_some() && cx.has_active_drag();
        let mut choices = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap_1()
            .child(SharedString::from(ui.floor_org.to_string()));
        for org in self.floor_orgs.saved() {
            let id = org.id;
            let ink = th.accent;
            choices = choices.child(
                Self::bezel_btn(
                    sk,
                    &self.threshing_floor_org_label(id),
                    self.floor_org == Some(id),
                )
                .id(SharedString::from(format!("floor-org-{}", id.0)))
                .debug_selector(move || format!("floor-org-{}", id.0))
                .flex_none()
                .max_w(px(240.))
                .truncate()
                .when(dragging, |d| {
                    d.border_color(ink.alpha(0.65))
                        .shadow(drop_glow(ink, false))
                })
                .drag_over::<PaneDrag>(move |s, _, _, _| {
                    s.border_color(ink)
                        .bg(ink.alpha(0.14))
                        .shadow(drop_glow(ink, true))
                })
                .on_drop(cx.listener(move |ws, drag: &PaneDrag, _, cx| {
                    ws.threshing_floor_drop_on_org(&drag.leaf, id, cx);
                }))
                .on_click(cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.threshing_floor_set_org(id, cx);
                })),
            );
        }
        let mut actions = div()
            .flex()
            .flex_row()
            .flex_none()
            .gap_1()
            .child(
                Self::bezel_btn(sk, ui.floor_org_new, false)
                    .id("floor-org-new")
                    .debug_selector(|| "floor-org-new".to_string())
                    .on_click(cx.listener(|ws, _, w, cx| {
                        cx.stop_propagation();
                        ws.floor_org_edit = Some((None, EditBuffer::seeded("")));
                        w.focus(&ws.focus_handle, cx);
                        cx.notify();
                    })),
            )
            .child(
                Self::bezel_btn(sk, ui.floor_map, self.floor_map.is_some())
                    .id("floor-system-map-toggle")
                    .debug_selector(|| "floor-system-map-toggle".to_string())
                    .on_click(cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        // Toggle the higher-level view; its draft lives only
                        // while it is open.
                        ws.floor_map = match ws.floor_map {
                            Some(_) => None,
                            None => Some(SystemMap::default()),
                        };
                        cx.notify();
                    })),
            );
        if let Some(id) = self.floor_org {
            actions = actions.child(
                Self::bezel_btn(sk, ui.floor_org_rename, false)
                    .id("floor-org-rename")
                    .debug_selector(|| "floor-org-rename".to_string())
                    .on_click(cx.listener(move |ws, _, w, cx| {
                        cx.stop_propagation();
                        let name = ws
                            .floor_orgs
                            .get(id)
                            .and_then(|o| o.name.as_deref())
                            .unwrap_or("");
                        ws.floor_org_edit = Some((Some(id), EditBuffer::seeded(name)));
                        w.focus(&ws.focus_handle, cx);
                        cx.notify();
                    })),
            );
            actions = actions.child(
                Self::bezel_btn(sk, ui.floor_org_delete, false)
                    .id("floor-org-delete")
                    .debug_selector(|| "floor-org-delete".to_string())
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        // Hold the delete until the person confirms the count; the
                        // name editor steps aside so the two never stack.
                        ws.floor_delete_org = Some(id);
                        ws.floor_org_edit = None;
                        cx.notify();
                    })),
            );
        }
        let mut controls = div().min_w(px(0.)).flex().flex_col().gap_2().child(
            div()
                .min_w(px(0.))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .id("floor-org-choices")
                        .min_w(px(0.))
                        .flex_1()
                        .overflow_x_scroll()
                        .child(choices),
                )
                .child(actions.when(dragging, |d| d.opacity(0.3))),
        );
        if let Some(id) = self.floor_delete_org {
            let n = self.threshing_floor_org_relationship_count(id);
            let text = lang::fill(
                ui.floor_org_delete_confirm,
                &[
                    ("org", &self.threshing_floor_org_label(id)),
                    ("n", &n.to_string()),
                ],
            );
            controls = controls.child(
                div()
                    .p_2()
                    .border_1()
                    // Red, not the move hint's amber: this one erases, and the
                    // colour is the warning (unknown is not on this path).
                    .border_color(th.ansi[1])
                    .rounded(sk.rad_raw(5.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(SharedString::from(text))
                    .child(
                        Self::bezel_btn(sk, ui.floor_org_delete_go, false)
                            .id("floor-org-delete-go")
                            .debug_selector(|| "floor-org-delete-go".to_string())
                            .text_color(th.ansi[1])
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.threshing_floor_delete_org(id, cx);
                            })),
                    )
                    .child(
                        Self::bezel_btn(sk, ui.dlg_cancel, false)
                            .id("floor-org-delete-cancel")
                            .debug_selector(|| "floor-org-delete-cancel".to_string())
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.floor_delete_org = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        if let Some(drop) = &self.floor_drop {
            let org_label = self.threshing_floor_org_label(drop.org);
            let pane = self.pane_name(&drop.leaf, cx).full();
            let rung_word = |rung: threshing_floor::Rung| match rung {
                threshing_floor::Rung::See => ui.floor_rung_see,
                threshing_floor::Rung::Read => ui.floor_rung_read,
                threshing_floor::Rung::Ask => ui.floor_rung_ask,
                threshing_floor::Rung::Prompt => ui.floor_rung_prompt,
            };
            // Amber, like the move hint: these routes build a grant — the colour
            // of "yours to argue with", never the delete confirm's red.
            let mut panel = div()
                .id("floor-drop-choice")
                .debug_selector(|| "floor-drop-choice".to_string())
                .p_2()
                .border_1()
                .border_color(th.ansi[3])
                .rounded(sk.rad_raw(5.))
                .flex()
                .flex_col()
                .gap_1();
            let cancel = Self::bezel_btn(sk, ui.dlg_cancel, false)
                .id("floor-drop-cancel")
                .debug_selector(|| "floor-drop-cancel".to_string())
                .on_click(cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.floor_drop = None;
                    cx.notify();
                }));
            let back = Self::bezel_btn(sk, ui.floor_drop_back, false)
                .id("floor-drop-back")
                .debug_selector(|| "floor-drop-back".to_string())
                .on_click(cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.floor_drop_back(cx);
                }));
            match &drop.stage {
                DropStage::Choosing => {
                    let members = self.floor_drop_members(drop);
                    panel = panel
                        .child(SharedString::from(lang::fill(
                            ui.floor_drop_title,
                            &[("pane", &pane), ("org", &org_label)],
                        )))
                        .child(
                            Self::bezel_btn(
                                sk,
                                &lang::fill(ui.floor_drop_move, &[("org", &org_label)]),
                                false,
                            )
                            .id("floor-drop-move")
                            .debug_selector(|| "floor-drop-move".to_string())
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.floor_drop_move(cx);
                            })),
                        )
                        .child(
                            Self::bezel_btn(
                                sk,
                                &lang::fill(ui.floor_drop_relate_org, &[("org", &org_label)]),
                                false,
                            )
                            .id("floor-drop-relate-org")
                            .debug_selector(|| "floor-drop-relate-org".to_string())
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.floor_drop_relate_org(cx);
                            })),
                        );
                    if !members.is_empty() {
                        panel = panel.child(
                            Self::bezel_btn(sk, ui.floor_drop_relate_member, false)
                                .id("floor-drop-relate-member")
                                .debug_selector(|| "floor-drop-relate-member".to_string())
                                .on_click(cx.listener(|ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.floor_drop_relate_member(cx);
                                })),
                        );
                    }
                    panel = panel.child(cancel);
                }
                DropStage::RelateOrg { rung } => {
                    let to = format!("{} \u{00b7} {}", org_label, ui.floor_map_whole_org);
                    let sentence = match rung {
                        Some(rung) => lang::fill(
                            ui.floor_rel_sentence,
                            &[("from", &pane), ("to", &to), ("rung", rung_word(*rung))],
                        ),
                        None => ui.floor_rel_choose.to_string(),
                    };
                    let ready = rung.is_some();
                    panel = panel
                        .child(SharedString::from(sentence))
                        .child(self.floor_drop_rung_row(sk, th, *rung, cx))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    Self::bezel_btn(sk, ui.floor_drop_draw, ready)
                                        .id("floor-drop-draw")
                                        .debug_selector(|| "floor-drop-draw".to_string())
                                        .opacity(if ready { 1. } else { 0.4 })
                                        .on_click(cx.listener(|ws, _, _, cx| {
                                            cx.stop_propagation();
                                            ws.floor_drop_draw(cx);
                                        })),
                                )
                                .child(back)
                                .child(cancel),
                        );
                }
                DropStage::RelateMember { member, rung } => match member {
                    None => {
                        let mut list = div()
                            .id("floor-drop-members")
                            .max_h(px(130.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap_1();
                        for seat in self.floor_drop_members(drop) {
                            list = list.child(
                                Self::bezel_btn(
                                    sk,
                                    &self.threshing_floor_seat_label(seat, cx),
                                    false,
                                )
                                .id(SharedString::from(format!("floor-drop-member-{}", seat.0)))
                                .on_click(cx.listener(
                                    move |ws, _, _, cx| {
                                        cx.stop_propagation();
                                        ws.floor_drop_pick_member(seat, cx);
                                    },
                                )),
                            );
                        }
                        panel = panel
                            .child(SharedString::from(lang::fill(
                                ui.floor_drop_pick,
                                &[("org", &org_label)],
                            )))
                            .child(list)
                            .child(div().flex().flex_row().gap_2().child(back).child(cancel));
                    }
                    Some(to) => {
                        let to = *to;
                        let to_label = self.threshing_floor_seat_label(to, cx);
                        let sentence = match rung {
                            Some(rung) => lang::fill(
                                ui.floor_rel_sentence,
                                &[
                                    ("from", &pane),
                                    ("to", &to_label),
                                    ("rung", rung_word(*rung)),
                                ],
                            ),
                            None => ui.floor_rel_choose.to_string(),
                        };
                        let ready = rung.is_some();
                        panel = panel
                            .child(SharedString::from(sentence))
                            .child(self.floor_drop_rung_row(sk, th, *rung, cx))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(
                                        Self::bezel_btn(sk, ui.floor_drop_draw, ready)
                                            .id("floor-drop-draw")
                                            .debug_selector(|| "floor-drop-draw".to_string())
                                            .opacity(if ready { 1. } else { 0.4 })
                                            .on_click(cx.listener(|ws, _, _, cx| {
                                                cx.stop_propagation();
                                                ws.floor_drop_draw(cx);
                                            })),
                                    )
                                    .child(back)
                                    .child(cancel),
                            );
                    }
                },
            }
            controls = controls.child(panel);
        }
        if let Some((_, buffer)) = &self.floor_org_edit {
            controls = controls.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .p_2()
                    .border_1()
                    .border_color(th.accent.alpha(0.6))
                    .rounded(sk.rad_raw(5.))
                    .child(render_edit_buffer(
                        buffer,
                        1.,
                        th.text,
                        th.accent,
                        th.accent.alpha(0.3),
                    ))
                    .child(
                        Self::bezel_btn(sk, ui.floor_org_save, false)
                            .id("floor-org-save")
                            .debug_selector(|| "floor-org-save".to_string())
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.threshing_floor_save_org(cx);
                            })),
                    )
                    .child(
                        Self::bezel_btn(sk, ui.dlg_cancel, false)
                            .id("floor-org-cancel")
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.floor_org_edit = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        controls
    }

    fn threshing_floor_assign(
        &mut self,
        leaf: &Entity<TerminalView>,
        org: threshing_floor::OrgId,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        if self.floor_orgs.get(org).is_none() || !leaf.read(cx).mode.is_agent() {
            return;
        }
        let old = leaf.read(cx).seat;
        if old.is_some_and(|seat| self.floor_orgs.owner(seat) == Some(org)) {
            self.threshing_floor_set_org(org, cx);
            return;
        }
        let connected = old.is_some_and(|seat| {
            self.chart
                .saved()
                .iter()
                .any(|l| l.from == seat || l.to == seat)
        });
        if connected && !confirmed {
            self.floor_move = Some((leaf.clone(), org));
            cx.notify();
            return;
        }
        let seat = old.unwrap_or_else(threshing_floor::SeatId::mint);
        if old.is_some() {
            self.chart.forget_seat(seat);
            self.threshing_floor_forget_pending(leaf, seat, cx);
        }
        self.floor_orgs.assign(seat, org);
        leaf.update(cx, |v, cx| v.set_seat(Some(seat), cx));
        self.floor_move = None;
        self.threshing_floor_set_org(org, cx);
    }

    /// A pane card was dropped on an org button. If the pane is already in that
    /// org this just opens it; a loose (seatless) pane simply moves in; a seated
    /// pane from a *different* org opens the move/relate choice, so the drop can
    /// build a relationship instead of silently moving. Routing only.
    fn threshing_floor_drop_on_org(
        &mut self,
        leaf: &Entity<TerminalView>,
        org: threshing_floor::OrgId,
        cx: &mut Context<Self>,
    ) {
        if self.floor_orgs.get(org).is_none() || !leaf.read(cx).mode.is_agent() {
            return;
        }
        let seat = leaf.read(cx).seat;
        // Already a member of this org: nothing to move or relate — just select.
        if seat.is_some_and(|s| self.floor_orgs.owner(s) == Some(org)) {
            self.threshing_floor_set_org(org, cx);
            return;
        }
        // A loose pane has no seat to relate from; dropping it means "seat here".
        let Some(seat) = seat else {
            self.threshing_floor_assign(leaf, org, false, cx);
            return;
        };
        // Seated, and a different org: offer the choice. Clear the sibling
        // popups so only one is ever open at a time.
        self.floor_delete_org = None;
        self.floor_org_edit = None;
        self.floor_move = None;
        self.floor_relation = None;
        self.floor_drop = Some(DropChoice {
            leaf: leaf.clone(),
            seat,
            org,
            stage: DropStage::Choosing,
        });
        cx.notify();
    }

    /// The MOVE route: hand off to the same assign the drop used to do, which
    /// raises its own confirm when the pane would lose connections.
    fn floor_drop_move(&mut self, cx: &mut Context<Self>) {
        if let Some(drop) = self.floor_drop.take() {
            self.threshing_floor_assign(&drop.leaf, drop.org, false, cx);
        }
    }

    fn floor_drop_relate_org(&mut self, cx: &mut Context<Self>) {
        if let Some(drop) = &mut self.floor_drop {
            drop.stage = DropStage::RelateOrg { rung: None };
            cx.notify();
        }
    }

    fn floor_drop_relate_member(&mut self, cx: &mut Context<Self>) {
        if let Some(drop) = &mut self.floor_drop {
            drop.stage = DropStage::RelateMember {
                member: None,
                rung: None,
            };
            cx.notify();
        }
    }

    fn floor_drop_pick_member(&mut self, member: threshing_floor::SeatId, cx: &mut Context<Self>) {
        if let Some(drop) = &mut self.floor_drop {
            if let DropStage::RelateMember { member: slot, rung } = &mut drop.stage {
                // Re-picking clears a rung chosen for the previous member.
                if *slot != Some(member) {
                    *rung = None;
                }
                *slot = Some(member);
            }
            cx.notify();
        }
    }

    fn floor_drop_set_rung(&mut self, rung: threshing_floor::Rung, cx: &mut Context<Self>) {
        if let Some(drop) = &mut self.floor_drop {
            match &mut drop.stage {
                DropStage::RelateOrg { rung: slot } => *slot = Some(rung),
                DropStage::RelateMember { rung: slot, .. } => *slot = Some(rung),
                DropStage::Choosing => {}
            }
            cx.notify();
        }
    }

    fn floor_drop_back(&mut self, cx: &mut Context<Self>) {
        if let Some(drop) = &mut self.floor_drop {
            drop.stage = DropStage::Choosing;
            cx.notify();
        }
    }

    /// Members of the drop's target org the dragged agent could relate to: every
    /// seat in the org except itself. Empty means the member route is not
    /// offered (nothing to point at).
    fn floor_drop_members(&self, drop: &DropChoice) -> Vec<threshing_floor::SeatId> {
        self.floor_orgs
            .get(drop.org)
            .map(|o| {
                o.seats
                    .iter()
                    .copied()
                    .filter(|s| *s != drop.seat)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Draw the relationship the drop popup composed: a whole-org line from the
    /// agent to the target org, or a seat-to-seat line to a chosen member. The
    /// agent stays where it is — relating is not moving. Same two Chart calls
    /// the System Map makes, so the grant is identical however it was drawn.
    fn floor_drop_draw(&mut self, cx: &mut Context<Self>) {
        let Some(drop) = self.floor_drop.as_ref() else {
            return;
        };
        let from = drop.seat;
        match &drop.stage {
            DropStage::RelateOrg { rung: Some(rung) } => {
                self.chart.draw_org(
                    threshing_floor::End::Seat(from),
                    threshing_floor::End::Org { org: drop.org },
                    *rung,
                );
            }
            DropStage::RelateMember {
                member: Some(to),
                rung: Some(rung),
            } if *to != from => {
                self.chart.draw(from, *to, *rung);
            }
            _ => return,
        }
        self.floor_drop = None;
        self.save(cx);
        cx.notify();
    }

    /// The three rungs for the drop popup, drawn like the relationship editor's:
    /// see < read < ask, with `ask` — the only rung that lets one agent prompt
    /// another — in the action amber with a ▲, so the powerful one is never
    /// granted by reflex. Each sets the current stage's rung.
    fn floor_drop_rung_row(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        current: Option<threshing_floor::Rung>,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let mut levels = div().flex().flex_row().flex_wrap().items_center().gap_2();
        for (rung, label) in [
            (threshing_floor::Rung::See, ui.floor_rung_see),
            (threshing_floor::Rung::Read, ui.floor_rung_read),
            (threshing_floor::Rung::Ask, ui.floor_rung_ask),
            (threshing_floor::Rung::Prompt, ui.floor_rung_prompt),
        ] {
            let selected = current == Some(rung);
            let is_ask = matches!(rung, threshing_floor::Rung::Prompt);
            let shown = if is_ask {
                format!("\u{25B2} {label}")
            } else {
                label.to_string()
            };
            let mut btn = Self::bezel_btn(sk, &shown, selected)
                .id(SharedString::from(format!("floor-drop-rung-{rung:?}")))
                .debug_selector(move || format!("floor-drop-rung-{rung:?}"));
            if is_ask {
                btn = btn
                    .border_color(th.ansi[3])
                    .font_weight(gpui::FontWeight::BOLD);
                if !selected {
                    btn = btn.text_color(th.ansi[3]);
                }
            } else if !selected {
                btn = btn.opacity(0.7);
            }
            levels = levels.child(btn.on_click(cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                ws.floor_drop_set_rung(rung, cx);
            })));
        }
        levels
    }

    fn threshing_floor_seat_leaf(
        &self,
        seat: threshing_floor::SeatId,
        cx: &App,
    ) -> Option<Entity<TerminalView>> {
        self.tabs
            .iter()
            .flat_map(|t| {
                let mut leaves = Vec::new();
                t.root.leaves(&mut leaves);
                leaves
            })
            .find(|leaf| leaf.read(cx).seat == Some(seat))
            .cloned()
    }

    fn threshing_floor_seat_label(&self, seat: threshing_floor::SeatId, cx: &App) -> String {
        self.threshing_floor_seat_leaf(seat, cx)
            .as_ref()
            .map(|leaf| self.pane_name(leaf, cx).full())
            .unwrap_or_else(|| self.lang.strings().ui.floor_unknown.to_string())
    }

    fn threshing_floor_begin_relation(
        &mut self,
        from: Option<threshing_floor::SeatId>,
        to: Option<threshing_floor::SeatId>,
        cx: &mut Context<Self>,
    ) {
        if from.is_some_and(|s| self.floor_orgs.owner(s) != self.floor_org) {
            return;
        }
        if to.is_some_and(|s| self.floor_orgs.owner(s) != self.floor_org || Some(s) == from) {
            return;
        }
        let rung = from
            .zip(to)
            .and_then(|(f, t)| self.chart.line(f, t))
            .and_then(|l| l.rung.known());
        self.floor_relation = Some(RelationEditor {
            draft: threshing_floor::RelationDraft { from, to, rung },
            choosing: if from.is_none() {
                Some(RelationEnd::From)
            } else if to.is_none() {
                Some(RelationEnd::To)
            } else {
                None
            },
        });
        // Beginning a relationship (a drag onto a card, or Connect) keeps the
        // person's filter — losing it mid-wiring was the "clicking an agent
        // clears the filter" complaint.
        self.threshing_floor_pick = None;
        cx.notify();
    }

    fn threshing_floor_choose_endpoint(
        &mut self,
        seat: threshing_floor::SeatId,
        cx: &mut Context<Self>,
    ) {
        if self.floor_orgs.owner(seat) != self.floor_org {
            return;
        }
        if let Some(editor) = &mut self.floor_relation {
            match editor.choosing {
                Some(RelationEnd::From) => {
                    editor.draft.from = Some(seat);
                    if editor.draft.to == Some(seat) {
                        editor.draft.to = None;
                    }
                    editor.choosing = Some(RelationEnd::To);
                }
                Some(RelationEnd::To) if editor.draft.from != Some(seat) => {
                    editor.draft.to = Some(seat);
                    editor.choosing = None;
                }
                _ => return,
            }
            editor.draft.rung = None;
        }
        cx.notify();
    }

    fn threshing_floor_save_relation(&mut self, cx: &mut Context<Self>) {
        let ready = self
            .floor_relation
            .as_ref()
            .and_then(|e| e.draft.ready(&self.floor_orgs));
        if let Some((from, to, rung)) = ready {
            if self.floor_orgs.owner(from) != self.floor_org
                || self.threshing_floor_seat_leaf(from, cx).is_none()
                || self.threshing_floor_seat_leaf(to, cx).is_none()
            {
                return;
            }
            self.chart.draw(from, to, rung);
            self.threshing_floor_forget_pair(from, to, cx);
            self.floor_relation = None;
            self.save(cx);
            cx.notify();
        }
    }

    fn threshing_floor_relationship_controls(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let mut controls = div().flex().flex_col().gap_2();
        if let Some((leaf, org)) = &self.floor_move {
            let leaf = leaf.clone();
            let org = *org;
            let n = leaf
                .read(cx)
                .seat
                .map(|s| {
                    self.chart
                        .saved()
                        .iter()
                        .filter(|l| l.from == s || l.to == s)
                        .count()
                })
                .unwrap_or(0);
            let text = lang::fill(
                ui.floor_move_hint,
                &[
                    ("pane", &self.pane_name(&leaf, cx).full()),
                    ("n", &n.to_string()),
                ],
            );
            controls = controls.child(
                div()
                    .p_2()
                    .border_1()
                    .border_color(th.ansi[3])
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(SharedString::from(format!(
                        "{text} → {}",
                        self.threshing_floor_org_label(org)
                    )))
                    .child(
                        Self::bezel_btn(sk, ui.floor_move_confirm, false)
                            .id("floor-move-confirm")
                            .debug_selector(|| "floor-move-confirm".to_string())
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.threshing_floor_assign(&leaf, org, true, cx);
                            })),
                    )
                    .child(
                        Self::bezel_btn(sk, ui.dlg_cancel, false)
                            .id("floor-move-cancel")
                            .debug_selector(|| "floor-move-cancel".to_string())
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.floor_move = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(
                Self::bezel_btn(sk, ui.floor_rel_connect, self.floor_relation.is_some())
                    .id("floor-connect")
                    .debug_selector(|| "floor-connect".to_string())
                    .on_click(cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.threshing_floor_begin_relation(ws.threshing_floor_pick, None, cx);
                    })),
            );
        if let Some(seat) = self.threshing_floor_pick {
            if let Some(leaf) = self.threshing_floor_seat_leaf(seat, cx) {
                let n = self
                    .chart
                    .saved()
                    .iter()
                    .filter(|l| l.from == seat || l.to == seat)
                    .count();
                let label = lang::fill(ui.floor_org_remove, &[("n", &n.to_string())]);
                row = row
                    .child(SharedString::from(
                        self.threshing_floor_seat_label(seat, cx),
                    ))
                    .child(
                        Self::bezel_btn(sk, &label, false)
                            .id("floor-unseat")
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.threshing_floor_unseat(&leaf, cx);
                            })),
                    );
            }
        }
        controls = controls.child(row);
        let Some(editor) = &self.floor_relation else {
            return controls;
        };
        let mut form = div()
            .p_2()
            .border_1()
            .border_color(th.accent.alpha(0.7))
            .rounded(sk.rad_raw(6.))
            .flex()
            .flex_col()
            .gap_2()
            .child(SharedString::from(ui.floor_rel.to_string()));
        let mut ends = div().flex().flex_row().flex_wrap().gap_2();
        for (end, seat, label, id) in [
            (
                RelationEnd::From,
                editor.draft.from,
                ui.floor_rel_from,
                "floor-rel-from",
            ),
            (
                RelationEnd::To,
                editor.draft.to,
                ui.floor_rel_to,
                "floor-rel-to",
            ),
        ] {
            let value = seat
                .map(|s| self.threshing_floor_seat_label(s, cx))
                .unwrap_or_else(|| ui.floor_unknown.to_string());
            ends = ends.child(
                Self::bezel_btn(sk, &format!("{label}: {value}"), false)
                    .id(id)
                    .max_w(px(320.))
                    .overflow_hidden()
                    // A full pane name ("TERMINAL DELIGHT › MCP work · claude-…")
                    // truncates on one line instead of char-wrapping down the panel.
                    .text_ellipsis()
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(e) = &mut ws.floor_relation {
                            e.choosing = Some(end);
                        }
                        cx.notify();
                    })),
            );
        }
        form = form.child(ends);
        if let Some(end) = editor.choosing {
            let mut list = div()
                .id("floor-rel-endpoints")
                .max_h(px(130.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            for tab in &self.tabs {
                let mut leaves = Vec::new();
                tab.root.leaves(&mut leaves);
                for leaf in leaves {
                    let Some(seat) = leaf.read(cx).seat else {
                        continue;
                    };
                    if self.floor_orgs.owner(seat) != self.floor_org
                        || matches!(end, RelationEnd::To) && editor.draft.from == Some(seat)
                    {
                        continue;
                    }
                    list = list.child(
                        Self::bezel_btn(sk, &self.threshing_floor_seat_label(seat, cx), false)
                            .id(SharedString::from(format!("floor-endpoint-{}", seat.0)))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.threshing_floor_choose_endpoint(seat, cx);
                            })),
                    );
                }
            }
            form = form.child(list);
        }
        let mut levels = div().flex().flex_row().flex_wrap().items_center().gap_2();
        for (rung, label) in [
            (threshing_floor::Rung::See, ui.floor_rung_see),
            (threshing_floor::Rung::Read, ui.floor_rung_read),
            (threshing_floor::Rung::Ask, ui.floor_rung_ask),
            (threshing_floor::Rung::Prompt, ui.floor_rung_prompt),
        ] {
            let selected = editor.draft.rung == Some(rung);
            // The rungs escalate see < read < ask < prompt. `prompt` is the top
            // clearance, the only rung that writes into another agent's own
            // conversation, so it wears the action colour (the same amber a
            // bypass line is drawn in) and a ▲ mark, and the lower rungs are
            // dimmed so the powerful one reads first. A person should never
            // grant it by reflex. (`is_ask` names the top rung, now Prompt.)
            let is_ask = matches!(rung, threshing_floor::Rung::Prompt);
            let shown = if is_ask {
                format!("\u{25B2} {label}")
            } else {
                label.to_string()
            };
            let mut btn = Self::bezel_btn(sk, &shown, selected)
                .id(SharedString::from(format!("floor-rung-{rung:?}")))
                .debug_selector(move || format!("floor-rung-{rung:?}"));
            if is_ask {
                btn = btn
                    .border_color(th.ansi[3])
                    .font_weight(gpui::FontWeight::BOLD);
                if !selected {
                    btn = btn.text_color(th.ansi[3]);
                }
            } else if !selected {
                btn = btn.opacity(0.7);
            }
            levels = levels.child(btn.on_click(cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                if let Some(e) = &mut ws.floor_relation {
                    e.draft.rung = Some(rung);
                }
                cx.notify();
            })));
        }
        let sentence = if let Some((from, to, rung)) = editor.draft.ready(&self.floor_orgs) {
            let word = match rung {
                threshing_floor::Rung::See => ui.floor_rung_see,
                threshing_floor::Rung::Read => ui.floor_rung_read,
                threshing_floor::Rung::Ask => ui.floor_rung_ask,
                threshing_floor::Rung::Prompt => ui.floor_rung_prompt,
            };
            lang::fill(
                ui.floor_rel_sentence,
                &[
                    ("from", &self.threshing_floor_seat_label(from, cx)),
                    ("to", &self.threshing_floor_seat_label(to, cx)),
                    ("rung", word),
                ],
            )
        } else {
            ui.floor_rel_choose.to_string()
        };
        let ready = editor
            .draft
            .ready(&self.floor_orgs)
            .is_some_and(|(f, t, _)| {
                self.threshing_floor_seat_leaf(f, cx).is_some()
                    && self.threshing_floor_seat_leaf(t, cx).is_some()
            });
        levels = levels
            .child(
                Self::bezel_btn(sk, ui.floor_rel_save, ready)
                    .id("floor-rel-save")
                    .debug_selector(|| "floor-rel-save".to_string())
                    .opacity(if ready { 1. } else { 0.4 })
                    .on_click(cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.threshing_floor_save_relation(cx);
                    })),
            )
            .child(
                Self::bezel_btn(sk, ui.dlg_cancel, false)
                    .id("floor-rel-cancel")
                    .debug_selector(|| "floor-rel-cancel".to_string())
                    .on_click(cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.floor_relation = None;
                        cx.notify();
                    })),
            );
        if let Some((from, to)) = editor
            .draft
            .from
            .zip(editor.draft.to)
            .filter(|(f, t)| self.chart.line(*f, *t).is_some())
        {
            levels = levels.child(
                Self::bezel_btn(sk, ui.floor_erase, false)
                    .id("floor-rel-remove")
                    .debug_selector(|| "floor-rel-remove".to_string())
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.chart.erase(from, to);
                        ws.threshing_floor_forget_pair(from, to, cx);
                        ws.floor_relation = None;
                        ws.save(cx);
                        cx.notify();
                    })),
            );
        }
        controls.child(form.child(SharedString::from(sentence)).child(levels))
    }

    /// The System Map: the higher-level view across every org. It carries a
    /// builder whose two ends are each a whole org or a single agent — the one
    /// place cross-org and whole-org relationships are drawn — a list of the
    /// relationships that span orgs (the ones a single org's chart cannot show),
    /// and a reference of every org with its members.
    fn floor_system_map(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let ui = self.lang.strings().ui;
        let Some(map) = self.floor_map.as_ref() else {
            return div().id("floor-system-map");
        };
        let rung_word = |r: threshing_floor::Rung| match r {
            threshing_floor::Rung::See => ui.floor_rung_see,
            threshing_floor::Rung::Read => ui.floor_rung_read,
            threshing_floor::Rung::Ask => ui.floor_rung_ask,
            threshing_floor::Rung::Prompt => ui.floor_rung_prompt,
        };

        // ---- the builder: two ends, each an org or an agent ----
        let mut ends = div().flex().flex_row().flex_wrap().gap_2();
        for (which, end, label, id) in [
            (
                RelationEnd::From,
                map.from,
                ui.floor_rel_from,
                "floor-map-from",
            ),
            (RelationEnd::To, map.to, ui.floor_rel_to, "floor-map-to"),
        ] {
            let value = end
                .map(|e| self.floor_map_end_label(e, cx))
                .unwrap_or_else(|| ui.floor_unknown.to_string());
            ends = ends.child(
                Self::bezel_btn(
                    sk,
                    &format!("{label}: {value}"),
                    map.choosing == Some(which),
                )
                .id(id)
                .max_w(px(320.))
                .overflow_hidden()
                .text_ellipsis()
                .on_click(cx.listener(move |ws, _, _, cx| {
                    cx.stop_propagation();
                    if let Some(m) = &mut ws.floor_map {
                        m.choosing = Some(which);
                    }
                    cx.notify();
                })),
            );
        }
        let mut form = div()
            .p_2()
            .border_1()
            .border_color(th.accent.alpha(0.7))
            .rounded(sk.rad_raw(6.))
            .flex()
            .flex_col()
            .gap_2()
            .child(ends);
        if let Some(which) = map.choosing {
            let other = match which {
                RelationEnd::From => map.to,
                RelationEnd::To => map.from,
            };
            let mut list = div()
                .id("floor-map-endpoints")
                .max_h(px(180.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1();
            for org in self.floor_orgs.saved() {
                let end = threshing_floor::End::Org { org: org.id };
                if Some(end) == other {
                    continue;
                }
                list = list.child(
                    Self::bezel_btn(
                        sk,
                        &format!(
                            "{} \u{00b7} {}",
                            self.threshing_floor_org_label(org.id),
                            ui.floor_map_whole_org
                        ),
                        false,
                    )
                    .id(SharedString::from(format!(
                        "floor-map-end-org-{}",
                        org.id.0
                    )))
                    .text_color(th.ansi[6])
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.floor_map_choose(end, cx);
                    })),
                );
            }
            for tab in &self.tabs {
                let mut leaves = Vec::new();
                tab.root.leaves(&mut leaves);
                for leaf in leaves {
                    let Some(seat) = leaf.read(cx).seat else {
                        continue;
                    };
                    let end = threshing_floor::End::Seat(seat);
                    if Some(end) == other {
                        continue;
                    }
                    list = list.child(
                        Self::bezel_btn(sk, &self.threshing_floor_seat_label(seat, cx), false)
                            .id(SharedString::from(format!("floor-map-end-seat-{}", seat.0)))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.floor_map_choose(end, cx);
                            })),
                    );
                }
            }
            form = form.child(list);
        }
        let mut levels = div().flex().flex_row().flex_wrap().items_center().gap_2();
        for (rung, label) in [
            (threshing_floor::Rung::See, ui.floor_rung_see),
            (threshing_floor::Rung::Read, ui.floor_rung_read),
            (threshing_floor::Rung::Ask, ui.floor_rung_ask),
            (threshing_floor::Rung::Prompt, ui.floor_rung_prompt),
        ] {
            let selected = map.rung == Some(rung);
            let is_ask = matches!(rung, threshing_floor::Rung::Prompt);
            let shown = if is_ask {
                format!("\u{25B2} {label}")
            } else {
                label.to_string()
            };
            let mut btn = Self::bezel_btn(sk, &shown, selected)
                .id(SharedString::from(format!("floor-map-rung-{rung:?}")))
                .debug_selector(move || format!("floor-map-rung-{rung:?}"));
            if is_ask {
                btn = btn
                    .border_color(th.ansi[3])
                    .font_weight(gpui::FontWeight::BOLD);
                if !selected {
                    btn = btn.text_color(th.ansi[3]);
                }
            } else if !selected {
                btn = btn.opacity(0.7);
            }
            levels = levels.child(btn.on_click(cx.listener(move |ws, _, _, cx| {
                cx.stop_propagation();
                if let Some(m) = &mut ws.floor_map {
                    m.rung = Some(rung);
                }
                cx.notify();
            })));
        }
        let ready = matches!((map.from, map.to, map.rung), (Some(f), Some(t), Some(_)) if f != t);
        let sentence = match (map.from, map.to, map.rung) {
            (Some(f), Some(t), Some(r)) if f != t => lang::fill(
                ui.floor_rel_sentence,
                &[
                    ("from", &self.floor_map_end_label(f, cx)),
                    ("to", &self.floor_map_end_label(t, cx)),
                    ("rung", rung_word(r)),
                ],
            ),
            _ => ui.floor_rel_choose.to_string(),
        };
        levels = levels.child(
            Self::bezel_btn(sk, ui.floor_rel_save, ready)
                .id("floor-map-draw")
                .debug_selector(|| "floor-map-draw".to_string())
                .opacity(if ready { 1. } else { 0.4 })
                .on_click(cx.listener(|ws, _, _, cx| {
                    cx.stop_propagation();
                    ws.floor_map_draw(cx);
                })),
        );
        form = form.child(levels).child(SharedString::from(sentence));

        // ---- the relationships that span orgs ----
        let cross: Vec<threshing_floor::Line> = self
            .chart
            .saved()
            .iter()
            .filter(|l| {
                !l.is_bypass() && self.floor_orgs.owner(l.from) != self.floor_orgs.owner(l.to)
            })
            .cloned()
            .collect();
        let org_lines = self.chart.org_lines().to_vec();
        let mut rels = div().flex().flex_col().gap_1().child(
            div()
                .text_size(sk.text_px(9.5))
                .font_weight(gpui::FontWeight::BOLD)
                .child(SharedString::from(ui.floor_map_rels.to_string())),
        );
        if cross.is_empty() && org_lines.is_empty() {
            rels = rels.child(
                div()
                    .text_color(th.text.alpha(0.6))
                    .child(SharedString::from(ui.floor_map_empty.to_string())),
            );
        }
        for l in &cross {
            let (from, to) = (l.from, l.to);
            let word = l.rung.known().map(rung_word).unwrap_or("?");
            let sentence = lang::fill(
                ui.floor_rel_sentence,
                &[
                    ("from", &self.threshing_floor_seat_label(from, cx)),
                    ("to", &self.threshing_floor_seat_label(to, cx)),
                    ("rung", word),
                ],
            );
            rels = rels.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(SharedString::from(sentence))
                    .child(
                        Self::bezel_btn(sk, ui.floor_map_remove, false)
                            .id(SharedString::from(format!(
                                "floor-map-rm-seat-{}-{}",
                                from.0, to.0
                            )))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.chart.erase(from, to);
                                ws.threshing_floor_forget_pair(from, to, cx);
                                ws.save(cx);
                                cx.notify();
                            })),
                    ),
            );
        }
        for ol in &org_lines {
            let (from, to) = (ol.from, ol.to);
            let word = ol.rung.known().map(rung_word).unwrap_or("?");
            let sentence = lang::fill(
                ui.floor_rel_sentence,
                &[
                    ("from", &self.floor_map_end_label(from, cx)),
                    ("to", &self.floor_map_end_label(to, cx)),
                    ("rung", word),
                ],
            );
            rels = rels.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(SharedString::from(sentence))
                    .child(
                        Self::bezel_btn(sk, ui.floor_map_remove, false)
                            .id(SharedString::from(format!(
                                "floor-map-rm-org-{from:?}-{to:?}"
                            )))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.chart.erase_org(from, to);
                                ws.save(cx);
                                cx.notify();
                            })),
                    ),
            );
        }

        // ---- a reference of every org and its members ----
        let mut orgboxes = div().flex().flex_row().flex_wrap().gap_2();
        for org in self.floor_orgs.saved() {
            let id = org.id;
            let mut boxed = div()
                .p_2()
                .border_1()
                .border_color(th.ansi[6].alpha(0.4))
                .rounded(sk.rad_raw(6.))
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .text_color(th.ansi[6])
                        .child(SharedString::from(self.threshing_floor_org_label(id))),
                );
            for seat in &org.seats {
                boxed = boxed.child(
                    div()
                        .text_size(sk.text_px(9.))
                        .text_color(th.text.alpha(0.85))
                        .child(SharedString::from(
                            self.threshing_floor_seat_label(*seat, cx),
                        )),
                );
            }
            orgboxes = orgboxes.child(boxed);
        }

        div()
            .id("floor-system-map")
            .debug_selector(|| "floor-system-map".to_string())
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_2()
            .child(
                div()
                    .text_size(sk.text_px(12.))
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_color(th.complement)
                    .child(SharedString::from(ui.floor_map.to_string())),
            )
            .child(form)
            .child(rels)
            .child(orgboxes)
    }

    /// What a person calls `leaf`: where its tab sits on the left bar, then
    /// its header's title (see [`threshing_floor::PaneName`]). Every place the War
    /// Room and its rail name a pane goes through here.
    pub(crate) fn pane_name(
        &self,
        leaf: &Entity<TerminalView>,
        cx: &App,
    ) -> threshing_floor::PaneName {
        let spine = self
            .tab_with_leaf(leaf.entity_id())
            .map(|ti| self.spine_path(ti))
            .unwrap_or_default();
        threshing_floor::PaneName::new(&spine, &leaf.read(cx).header_title())
    }

    /// An agent's question for another (`ask_agent`), or its read-back of a
    /// copy (`ask_answer`), decided here because the chart, the seat facts
    /// and the copies are the window's.
    pub(crate) fn ask_bridge(
        &mut self,
        call: &mcp::AskCall,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        match call {
            mcp::AskCall::Question(req) => self.ask_question(req, cx),
            mcp::AskCall::Answer {
                caller_pane,
                ask_id,
            } => match self.asks.of_asker(*ask_id, *caller_pane).cloned() {
                Some(a) => {
                    if let Some(refusal) = self.copy_refusal(&a, cx) {
                        self.forget_copy(*ask_id, cx);
                        return mcp::AskReply::Refused(refusal);
                    }
                    mcp::AskReply::Copy(threshing_floor::CopyStatus {
                        ask_id: *ask_id,
                        reads: a.reads,
                        press_limit: self.press_limit(),
                        stage: a.stage.clone(),
                    })
                }
                None => mcp::AskReply::Refused(threshing_floor::Refusal::NoSuchAsk),
            },
            mcp::AskCall::Prompt(req) => self.prompt_send(req, cx),
            mcp::AskCall::PromptStatus {
                caller_pane,
                prompt_id,
            } => match self.prompts.of_sender(*prompt_id, *caller_pane) {
                Some(sent) => mcp::AskReply::Prompt(sent.status(crate::prompt::now_ms())),
                None => mcp::AskReply::Refused(threshing_floor::Refusal::NoSuchPrompt),
            },
        }
    }

    /// An agent's prompt for another (`prompt_agent`), decided here against
    /// the lines a person drew and handed to the book, which waits for the
    /// worker to come to rest. Nothing is typed in this call: typing is
    /// [`Self::prompt_tick`]'s, on the book's word.
    fn prompt_send(&mut self, req: &mcp::PromptRequest, cx: &mut Context<Self>) -> mcp::AskReply {
        let now = crate::prompt::now_ms();
        self.ask_seq = (self.ask_seq + 1).max(now);
        let id = threshing_floor::AskId(self.ask_seq);
        let caller = self.leaf_by_pane_id(req.caller_pane, cx);
        let target = self.leaf_by_shell_pid(req.target, cx);
        let from = caller.as_ref().and_then(|l| l.read(cx).seat);
        let to = target.as_ref().and_then(|l| l.read(cx).seat);
        let brief = crate::prompt::clean(&req.brief);
        let mut entry = askrecord::Entry {
            id,
            at_ms: now,
            from,
            to,
            via: "prompt".into(),
            stage: "refused".into(),
            refusal: None,
            question_len: brief.chars().count(),
            question_sha256: askrecord::fingerprint(&brief),
            program: None,
            model: None,
            reads_tokens: None,
            tokens_read: None,
            cost_usd: None,
        };
        let decided = match (caller.as_ref(), target.as_ref()) {
            (Some(c), Some(t)) => {
                let (cv, tv) = (c.read(cx), t.read(cx));
                entry.program = Some(tv.mode.label().to_string());
                threshing_floor::decide_prompt(
                    &self.mcp,
                    &self.chart,
                    &self.floor_orgs,
                    &cv.party(),
                    &prompt_party(tv),
                    c.entity_id() == t.entity_id(),
                    entry.question_len,
                )
            }
            _ => Err(threshing_floor::Refusal::Gone),
        };
        let (Ok(()), Some(c), Some(t)) = (decided.clone(), caller, target) else {
            let r = decided.err().unwrap_or(threshing_floor::Refusal::Gone);
            entry.refusal = refusal_code(&r);
            self.record(&entry);
            return mcp::AskReply::Refused(r);
        };
        let sender = self.pane_name(&c, cx).full();
        // Its last message can be read back from a Claude or Codex
        // conversation; the header says only what will happen.
        let returns = askcopy::Program::of_mode(t.read(cx).mode.label()).is_some();
        let header = crate::prompt::header(id, &sender, c.read(cx).shell_pid(), returns);
        let taken = self.prompts.send(crate::prompt::NewPrompt {
            id,
            sender_pane: req.caller_pane,
            from,
            to,
            target_pid: req.target,
            key: req.key.clone(),
            text: crate::prompt::compose(&header, &brief),
            brief_len: entry.question_len,
            brief_sha256: entry.question_sha256.clone(),
            now_ms: now,
        });
        match taken {
            Ok(crate::prompt::Taken::New(status)) => {
                entry.stage = "waiting".into();
                self.record(&entry);
                cx.notify();
                mcp::AskReply::Prompt(status)
            }
            // A retry by key: the first prompt as it stands, recorded once.
            Ok(crate::prompt::Taken::Again(status)) => mcp::AskReply::Prompt(status),
            Err(r) => {
                entry.refusal = refusal_code(&r);
                self.record(&entry);
                mcp::AskReply::Refused(r)
            }
        }
    }

    /// One look at every worker with an open prompt, and what the book
    /// decides, done. Called on a short clock while any prompt is open.
    pub(crate) fn prompt_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.prompts.open().next().is_none() {
            return;
        }
        let now = crate::prompt::now_ms();
        let senders_gone: Vec<u64> = self
            .prompts
            .open()
            .map(|s| s.sender_pane)
            .filter(|p| self.leaf_by_pane_id(*p, cx).is_none())
            .collect();
        let mut book = std::mem::take(&mut self.prompts);
        let mut ended = Vec::new();
        for pane in senders_gone {
            ended.extend(book.sender_gone(pane, now));
        }
        let acts = {
            let app: &App = cx;
            book.tick(now, |sent| self.prompt_look(sent, app))
        };
        self.prompts = book;
        for id in ended {
            self.record_prompt(id);
        }
        for act in acts {
            self.prompt_act(act, window, cx);
        }
    }

    /// What the book needs to know of a prompt's worker on this tick. The
    /// worker is found by its shell, or by its seat once a wake has given it
    /// a new one.
    fn prompt_look(&self, sent: &crate::prompt::Sent, cx: &App) -> crate::prompt::Worker {
        let leaf = self.leaf_by_shell_pid(sent.target_pid, cx).or_else(|| {
            sent.to
                .and_then(|seat| self.threshing_floor_seat_leaf(seat, cx))
        });
        let Some(leaf) = leaf else {
            return crate::prompt::Worker::gone();
        };
        let v = leaf.read(cx);
        // An asleep agent wakes in a fresh pane whose shell starts it again:
        // until it is an agent once more, it is waking, not gone.
        let waking = !v.mode.is_agent()
            && sent.woke_ms.is_some_and(|t| {
                crate::prompt::now_ms().saturating_sub(t) < crate::prompt::WAKE_GRACE_MS
            });
        // An agent the sleep signalled is on its way to asleep: a prompt in
        // that gap waits for a sleeper (`prompt_party` counts it an agent).
        let going_to_sleep = v.rest.stopping.is_some();
        let mut party = prompt_party(v);
        party.is_agent |= waking;
        let allowed = match self.leaf_by_pane_id(sent.sender_pane, cx) {
            Some(c) => threshing_floor::decide_prompt(
                &self.mcp,
                &self.chart,
                &self.floor_orgs,
                &c.read(cx).party(),
                &party,
                c.entity_id() == leaf.entity_id(),
                sent.brief_len.max(1),
            ),
            None => Err(threshing_floor::Refusal::Gone),
        };
        let (seen, label) = if going_to_sleep {
            (crate::prompt::Seen::Dormant, "going to sleep".to_string())
        } else if waking {
            (crate::prompt::Seen::Dormant, "starting again".to_string())
        } else {
            (
                v.prompt_seen(),
                v.agent_status().state.label_in(lang::Lang::En).to_string(),
            )
        };
        crate::prompt::Worker {
            seen,
            label,
            takes_paste: v.takes_bracketed_paste(),
            picker: v.dial_picker_up(),
            person_key_ms: v.person_key_ms(),
            turn_began_ms: v.turn_began_ms(),
            rows: v.recent_lines(crate::prompt::LOOK_ROWS),
            allowed,
            pid: v.shell_pid(),
        }
    }

    /// One of the book's acts, done. Paste and Enter go through the pane's
    /// one prompt write, `TerminalView::type_prompt`, which refuses again for
    /// a pane a person has closed to agents since.
    fn prompt_act(&mut self, act: crate::prompt::Act, window: &mut Window, cx: &mut Context<Self>) {
        use crate::prompt::{Act, Keys};
        let now = crate::prompt::now_ms();
        let id = match act {
            Act::Wake(id)
            | Act::Paste(id)
            | Act::Enter(id)
            | Act::ReadReply(id)
            | Act::Probe(id)
            | Act::Record(id) => id,
        };
        let Some(sent) = self.prompts.get(id).cloned() else {
            return;
        };
        let leaf = self.leaf_by_shell_pid(sent.target_pid, cx);
        match act {
            Act::Record(_) => self.record_prompt(id),
            Act::ReadReply(_) => self.prompt_read_reply(id, crate::prompt::REPLY_TRIES, cx),
            Act::Probe(_) => self.prompt_read_reply(id, 1, cx),
            Act::Wake(_) => {
                let Some(leaf) = leaf else { return };
                if leaf.read(cx).rest.stopping.is_some() {
                    // Not asleep yet: nothing to wake, and the wake is asked
                    // for again on the next tick rather than a minute later.
                    self.prompts.wake_not_taken(id);
                } else if leaf.read(cx).rest.dormancy.asleep().is_some() {
                    self.wake_sleeper_as(&leaf, false, window, cx);
                    self.prompts.restarted(id);
                } else {
                    leaf.update(cx, |v, cx| {
                        v.rest.touch(std::time::Instant::now());
                        cx.notify();
                    });
                }
            }
            Act::Paste(_) | Act::Enter(_) => {
                let pasted = crate::prompt::paste_text(&sent);
                let keys = match act {
                    Act::Paste(_) => Keys::Paste(&pasted),
                    _ => Keys::Enter,
                };
                let typed = leaf.is_some_and(|leaf| {
                    leaf.update(cx, |v, cx| {
                        let typed = v.type_prompt(keys);
                        cx.notify();
                        typed
                    })
                });
                if !typed
                    && self.prompts.fail(
                        id,
                        "the worker's pane refused the keys: it closed, is no longer an agent, was marked read-only or off-limits, or its prompt does not take a paste",
                        now,
                    )
                {
                    self.record_prompt(id);
                }
            }
        }
    }

    /// The worker's last message after a prompt, read off the window's
    /// thread from whichever conversation its pane is bound to. The prompt's
    /// own number in that conversation is what proves the binding, so a guess
    /// that is wrong reads as unknown, never as another agent's words.
    fn prompt_read_reply(
        &mut self,
        id: threshing_floor::AskId,
        tries: u32,
        cx: &mut Context<Self>,
    ) {
        let Some(pid) = self.prompts.get(id).map(|s| s.target_pid) else {
            return;
        };
        let facts = std::sync::Arc::new(self.pane_facts(cx));
        cx.spawn(async move |this, cx| {
            // The screen comes to rest a moment before the program has
            // written its reply into the transcript (measured on Claude Code
            // and Codex both, 2026-10-27), so a reply not there yet is looked
            // for again, for a few seconds, before it is called unknown.
            let mut reply = Err(String::new());
            for attempt in 0..tries {
                if attempt > 0 {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(
                            crate::prompt::REPLY_RETRY_MS,
                        ))
                        .await;
                }
                let facts = facts.clone();
                reply = cx
                    .background_executor()
                    .spawn(async move {
                        let home =
                            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default());
                        match paneident::resolve(&facts, &home).remove(&pid) {
                            Some(bound) => crate::prompt::read_reply(&bound.path, id),
                            None => Err(crate::prompt::NO_CONVERSATION.to_string()),
                        }
                    })
                    .await;
                if !reply
                    .as_ref()
                    .is_err_and(|why| crate::prompt::worth_another_look(why))
                {
                    break;
                }
            }
            let _ = this.update(cx, |ws, cx| {
                if ws.prompts.replied(id, reply, crate::prompt::now_ms()) {
                    ws.record_prompt(id);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// One stage of a prompt, for the record: its length and fingerprint,
    /// never its words.
    fn record_prompt(&mut self, id: threshing_floor::AskId) {
        let Some(s) = self.prompts.get(id) else {
            return;
        };
        let entry = askrecord::Entry {
            id,
            at_ms: crate::prompt::now_ms(),
            from: s.from,
            to: s.to,
            via: "prompt".into(),
            stage: s.stage.word().into(),
            refusal: None,
            question_len: s.brief_len,
            question_sha256: s.brief_sha256.clone(),
            program: None,
            model: None,
            reads_tokens: None,
            tokens_read: None,
            cost_usd: None,
        };
        self.record(&entry);
    }

    /// A copy's admission is not permission to deliver after revocation. A
    /// spent bypass remains a receipt for this one already-admitted question;
    /// explicit relationship edits and membership moves erase those receipts.
    fn copy_refusal(
        &self,
        a: &threshing_floor::CopyAsk,
        cx: &App,
    ) -> Option<threshing_floor::Refusal> {
        let Some(caller) = self.leaf_by_pane_id(a.asker_pane, cx) else {
            return Some(threshing_floor::Refusal::Gone);
        };
        let Some(target) = self.leaf_by_shell_pid(a.target_pid, cx) else {
            return Some(threshing_floor::Refusal::Gone);
        };
        let (c, t) = (caller.read(cx), target.read(cx));
        if a.from.is_none() || a.to.is_none() || c.seat != a.from || t.seat != a.to {
            return Some(threshing_floor::Refusal::Gone);
        }
        if !c.mode.is_agent() || askcopy::Program::of_mode(t.mode.label()) != Some(a.program) {
            return Some(threshing_floor::Refusal::NotAnAgent);
        }
        let mut receipt = threshing_floor::Chart::default();
        let chart = if a.via == "bypass" {
            // This transient chart checks the already-spent prompt against
            // today's global and target ceilings without spending it twice.
            receipt.grant_bypass(a.from.unwrap(), a.to.unwrap(), 1);
            &receipt
        } else {
            &self.chart
        };
        threshing_floor::decide_ask(
            &self.mcp,
            chart,
            &self.floor_orgs,
            &c.party(),
            &t.party(),
            false,
            a.question_len,
        )
        .err()
    }

    fn forget_copy(&mut self, id: threshing_floor::AskId, cx: &mut Context<Self>) {
        if let Some(a) = self.asks.remove(id) {
            if let Some(asker) = self.leaf_by_pane_id(a.asker_pane, cx) {
                asker.update(cx, |v, cx| {
                    if v.access_ask
                        .as_ref()
                        .is_some_and(|ask| ask.ask_id == Some(id))
                    {
                        v.access_ask = None;
                        cx.notify();
                    }
                });
            }
        }
    }

    fn threshing_floor_forget_pair(
        &mut self,
        from: threshing_floor::SeatId,
        to: threshing_floor::SeatId,
        cx: &mut Context<Self>,
    ) {
        self.asks.forget_pair(from, to);
        if let Some(asker) = self.threshing_floor_seat_leaf(from, cx) {
            asker.update(cx, |v, cx| {
                if v.access_ask
                    .as_ref()
                    .and_then(|a| a.ask_id)
                    .is_some_and(|id| self.asks.get(id).is_none())
                {
                    v.access_ask = None;
                    cx.notify();
                }
            });
        }
    }

    /// The person's press limit: a copy reading more waits for their Yes.
    pub(crate) fn press_limit(&self) -> u64 {
        self.ask_press_tokens
            .unwrap_or(threshing_floor::ASK_PRESS_TOKENS)
    }

    /// A line from the asker's seat must reach as far as ask
    /// (`threshing_floor::decide_ask`). A cheap ask is answered from what the window
    /// can already show; a copy is priced, and starts at once when it reads no
    /// more than the person's press limit, or goes to the rail for their Yes.
    /// Every ask — refused or answered — leaves one line in the record,
    /// without its words.
    fn ask_question(&mut self, req: &mcp::AskRequest, cx: &mut Context<Self>) -> mcp::AskReply {
        let now_ms = now_ms();
        self.ask_seq = (self.ask_seq + 1).max(now_ms);
        let id = threshing_floor::AskId(self.ask_seq);
        let caller = self.leaf_by_pane_id(req.caller_pane, cx);
        let target = self.leaf_by_shell_pid(req.target, cx);
        let from = caller.as_ref().and_then(|l| l.read(cx).seat);
        let to = target.as_ref().and_then(|l| l.read(cx).seat);
        let line = from
            .zip(to)
            .and_then(|(f, t)| self.chart.line(f, t).cloned());
        let via = match &line {
            Some(l) if l.is_bypass() => "bypass",
            Some(_) => "line",
            None => "none",
        };
        let mut entry = askrecord::Entry {
            id,
            at_ms: now_ms,
            from,
            to,
            via: via.into(),
            stage: "refused".into(),
            refusal: None,
            question_len: req.question.chars().count(),
            question_sha256: askrecord::fingerprint(&req.question),
            program: None,
            model: None,
            reads_tokens: None,
            tokens_read: None,
            cost_usd: None,
        };
        let reply = match (caller.as_ref(), target.as_ref()) {
            (Some(c), Some(t)) => self.decide_question(req, id, c, t, &mut entry, cx),
            _ => mcp::AskReply::Refused(threshing_floor::Refusal::Gone),
        };
        if let mcp::AskReply::Refused(r) = &reply {
            entry.refusal = refusal_code(r);
        }
        self.record(&entry);
        // A copy, once priced and recorded, goes on: to the rail for a press,
        // or straight to work under the limit. A bypass spends its prompt
        // here, once the copy is sure to be asked for.
        if let mcp::AskReply::Copy(status) = &reply {
            if let (Some(f), Some(t), "bypass") = (from, to, via) {
                self.chart.spend(f, t);
                self.save(cx);
            }
            match status.stage {
                threshing_floor::CopyStage::AwaitingPress => self.raise_copy_card(id, cx),
                threshing_floor::CopyStage::Running => self.start_copy(id, "copy_started", cx),
                _ => {}
            }
        }
        reply
    }

    fn decide_question(
        &mut self,
        req: &mcp::AskRequest,
        id: threshing_floor::AskId,
        caller: &Entity<TerminalView>,
        target: &Entity<TerminalView>,
        entry: &mut askrecord::Entry,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        let (c, t) = (caller.read(cx), target.read(cx));
        let q = req.question.chars().count();
        let same = caller.entity_id() == target.entity_id();
        let via = match threshing_floor::decide_ask(
            &self.mcp,
            &self.chart,
            &self.floor_orgs,
            &c.party(),
            &t.party(),
            same,
            q,
        ) {
            Ok(via) => via,
            Err(r) => return mcp::AskReply::Refused(r),
        };
        let vit = t.shell_pid().and_then(|pid| self.agent_vitals.get(&pid));
        entry.program = Some(t.mode.label().to_string());
        if req.copy {
            let Some(program) = askcopy::Program::of_mode(t.mode.label()) else {
                return mcp::AskReply::Refused(threshing_floor::Refusal::CannotCopy);
            };
            // One at a time: a copy running or waiting, or any ask of this
            // agent's still on the person's rail.
            if self.asks.busy(req.caller_pane) || c.access_ask.is_some() {
                return mcp::AskReply::Refused(threshing_floor::Refusal::Busy);
            }
            // Priced by the transport, off this thread, from the transcript
            // the copy would fork. No price is no copy.
            let Some(price) = &req.price else {
                return mcp::AskReply::Refused(threshing_floor::Refusal::ConversationUnnamed);
            };
            let conversation = match &price.conversation {
                Ok(c) => c.clone(),
                Err(r) => return mcp::AskReply::Refused(r.clone()),
            };
            let (reads, model) = (price.reads, price.model.clone());
            let limit = self.press_limit();
            let stage = if threshing_floor::copy_waits_for_press(reads, limit) {
                threshing_floor::CopyStage::AwaitingPress
            } else {
                threshing_floor::CopyStage::Running
            };
            entry.program = Some(program.word().to_string());
            entry.model = model.clone();
            entry.reads_tokens = reads;
            entry.stage = "priced".into();
            self.asks.open(
                id,
                threshing_floor::CopyAsk {
                    asker_pane: req.caller_pane,
                    from: entry.from,
                    to: entry.to,
                    target_pid: req.target,
                    via,
                    reads,
                    model,
                    question: req.question.clone(),
                    question_len: entry.question_len,
                    question_sha256: entry.question_sha256.clone(),
                    program,
                    conversation,
                    stage: stage.clone(),
                },
            );
            return mcp::AskReply::Copy(threshing_floor::CopyStatus {
                ask_id: id,
                reads,
                press_limit: limit,
                stage,
            });
        }
        // `tab_with_leaf`, not `tab_index_of`: the second matches a tab's
        // first pane only, and a seat in a split would answer with no project.
        let project = self
            .tab_with_leaf(target.entity_id())
            .and_then(|ti| self.place_of(ti).project)
            .and_then(|id| self.project_at(id))
            .and_then(|p| p.name.clone());
        let seat = threshing_floor::SeatView {
            title: seat_title(t),
            project,
            role: t.seat.map(|s| self.chart.role_of(s)),
            state: t.agent_status().state.label_in(self.lang).to_string(),
            model: vit.and_then(|v| v.model.clone()),
            size: vit.and_then(|v| v.tokens),
        };
        entry.model = seat.model.clone();
        entry.stage = "cheap".into();
        mcp::AskReply::Cheap(threshing_floor::CheapAnswer {
            ask_id: id,
            seat,
            // A guessed conversation may be another agent's: answer with no
            // summary rather than a stranger's words.
            summary: t
                .bench
                .latest_response()
                .filter(|_| t.bench_is_its_own())
                .map(|r| threshing_floor::Summary {
                    brief: r.brief.clone(),
                    layman: r.layman.clone(),
                }),
            note: t.saved_note().map(|n| match n.title {
                Some(title) if !n.text.is_empty() => format!("{title} — {}", n.text),
                Some(title) => title,
                None => n.text,
            }),
            via: via.into(),
        })
    }

    /// Append one stage to this window's ask record, and read the numbers
    /// back for the Threshing Floor.
    fn record(&mut self, entry: &askrecord::Entry) {
        let record = askrecord::path_for(crate::instance::key());
        if let Err(e) = askrecord::append(&record, entry) {
            eprintln!("[threshing-floor] the ask record could not be written: {e}");
        }
        self.ask_numbers = askrecord::numbers_for(crate::instance::key()).ok();
    }

    /// One stage of a copy ask, for the record.
    fn record_copy(
        &mut self,
        id: threshing_floor::AskId,
        stage: &str,
        tweak: impl FnOnce(&mut askrecord::Entry),
    ) {
        let Some(a) = self.asks.get(id) else {
            return;
        };
        let mut entry = askrecord::Entry {
            id,
            at_ms: now_ms(),
            from: a.from,
            to: a.to,
            via: a.via.into(),
            stage: stage.into(),
            refusal: None,
            question_len: a.question_len,
            question_sha256: a.question_sha256.clone(),
            program: Some(a.program.word().to_string()),
            model: a.model.clone(),
            reads_tokens: a.reads,
            tokens_read: None,
            cost_usd: None,
        };
        tweak(&mut entry);
        self.record(&entry);
    }

    /// Put a priced copy on the asker's rail row, for a person's Yes or No.
    fn raise_copy_card(&mut self, id: threshing_floor::AskId, cx: &mut Context<Self>) {
        let Some(a) = self.asks.get(id).cloned() else {
            return;
        };
        let title = self
            .leaf_by_shell_pid(a.target_pid, cx)
            .map(|t| self.pane_name(&t, cx).full());
        let ask = access::Ask {
            change: access::Change::Copy,
            pid: Some(a.target_pid),
            target_key: None,
            title,
            why: access::excerpt(&a.question),
            at: std::time::Instant::now(),
            risk: Some(threshing_floor::risk(&threshing_floor::RiskFacts::Copy {
                reads: a.reads,
            })),
            prompts: access::BYPASS_PROMPTS,
            reads: a.reads,
            ask_id: Some(id),
        };
        if let Some(asker) = self.leaf_by_pane_id(a.asker_pane, cx) {
            asker.update(cx, |v, cx| {
                v.access_ask = Some(ask);
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Start a copy ask's copy, on a thread of its own: resolve which
    /// conversation the asked pane holds (named by its agent, and by no other
    /// pane), read the agent's program, folder and environment, run the copy
    /// and hand the answer back here. Called under the press limit by the ask
    /// itself and over it by the rail's Yes — nowhere else.
    fn start_copy(&mut self, id: threshing_floor::AskId, stage: &str, cx: &mut Context<Self>) {
        let Some(a) = self.asks.get(id).cloned() else {
            return;
        };
        if self.copy_refusal(&a, cx).is_some() {
            self.forget_copy(id, cx);
            return;
        }
        self.asks.set(id, threshing_floor::CopyStage::Running);
        self.record_copy(id, stage, |_| {});
        let facts = self.pane_facts(cx);
        let (tx, rx) = futures::channel::oneshot::channel();
        let spawned = std::thread::Builder::new()
            .name("td-ask-copy".into())
            .spawn(move || {
                let out = (|| {
                    let home = crate::session::home_dir();
                    let bound = paneident::resolve(&facts, &home);
                    let conversation = askcopy::conversation_for(&bound, a.target_pid)?;
                    if conversation != a.conversation {
                        return Err(askcopy::End::Failed(
                            "the agent is in another conversation than the one priced; ask again"
                                .into(),
                        ));
                    }
                    let plan = askcopy::prepare(
                        a.program,
                        a.target_pid,
                        &conversation,
                        a.model.as_deref(),
                    )?;
                    askcopy::run(&plan, &a.question)
                })();
                let _ = tx.send(out);
            });
        if spawned.is_err() {
            self.copy_finished(
                id,
                Err(askcopy::End::Failed("no thread for the copy".into())),
                cx,
            );
            return;
        }
        cx.spawn(async move |this, cx| {
            let out = rx
                .await
                .unwrap_or_else(|_| Err(askcopy::End::Failed("the copy's thread stopped".into())));
            let _ = this.update(cx, |ws: &mut Workspace, cx| ws.copy_finished(id, out, cx));
        })
        .detach();
    }

    /// A copy's answer, or why there is none, into the book and the record.
    /// The asker reads it with `ask_answer`; nothing is typed into its
    /// terminal, because a line there is signed as its person's and a copy
    /// finishing is not something a person did.
    fn copy_finished(
        &mut self,
        id: threshing_floor::AskId,
        out: Result<askcopy::Answer, askcopy::End>,
        cx: &mut Context<Self>,
    ) {
        let Some(a) = self.asks.get(id) else {
            return;
        };
        if self.copy_refusal(a, cx).is_some() {
            self.forget_copy(id, cx);
            return;
        }
        let (stage, word) = match out {
            Ok(a) => (
                threshing_floor::CopyStage::Answered {
                    answer: a.text,
                    tokens_read: a.tokens_read,
                    cost_usd: a.cost_usd,
                },
                "answered",
            ),
            Err(askcopy::End::Refused(refusal)) => {
                (threshing_floor::CopyStage::Refused { refusal }, "refused")
            }
            Err(askcopy::End::Failed(why)) => {
                (threshing_floor::CopyStage::Failed { why }, "failed")
            }
        };
        let (tokens_read, cost_usd, refusal) = match &stage {
            threshing_floor::CopyStage::Answered {
                tokens_read,
                cost_usd,
                ..
            } => (*tokens_read, *cost_usd, None),
            threshing_floor::CopyStage::Refused { refusal } => (None, None, refusal_code(refusal)),
            _ => (None, None, None),
        };
        if self.asks.set(id, stage).is_none() {
            return;
        }
        self.record_copy(id, word, |e| {
            e.tokens_read = tokens_read;
            e.cost_usd = cost_usd;
            e.refusal = refusal;
        });
        cx.notify();
    }

    /// A person's answer to a copy card: Yes starts it, No declines it.
    /// Called by `rail_answer` only. Returns what came of it.
    pub(crate) fn answer_copy(
        &mut self,
        id: Option<threshing_floor::AskId>,
        answer: access::Answer,
        cx: &mut Context<Self>,
    ) -> access::Outcome {
        let Some(id) = id.filter(|id| {
            self.asks
                .get(*id)
                .is_some_and(|a| a.stage == threshing_floor::CopyStage::AwaitingPress)
        }) else {
            return access::Outcome::Gone;
        };
        if self
            .asks
            .get(id)
            .is_some_and(|a| self.copy_refusal(a, cx).is_some())
        {
            self.forget_copy(id, cx);
            return access::Outcome::Gone;
        }
        match answer {
            access::Answer::Yes => {
                let alive = self
                    .asks
                    .get(id)
                    .is_some_and(|a| self.leaf_by_shell_pid(a.target_pid, cx).is_some());
                if !alive {
                    self.asks.set(
                        id,
                        threshing_floor::CopyStage::Failed {
                            why: "the pane asked has closed".into(),
                        },
                    );
                    self.record_copy(id, "failed", |_| {});
                    return access::Outcome::Gone;
                }
                self.start_copy(id, "pressed", cx);
                access::Outcome::Done
            }
            access::Answer::No => {
                self.asks.set(id, threshing_floor::CopyStage::Declined);
                self.record_copy(id, "declined", |_| {});
                access::Outcome::Kept
            }
        }
    }
    /// Close the Threshing Floor and let go of any half-drawn line.
    pub(crate) fn close_threshing_floor(&mut self) {
        self.threshing_floor = false;
        self.threshing_floor_pick = None;
        self.floor_relation = None;
        self.floor_org_edit = None;
        self.floor_move = None;
        self.floor_delete_org = None;
        self.floor_drop = None;
        self.floor_drag = None;
        self.floor_context = None;
        self.floor_feedback = None;
        self.threshing_floor_query = EditBuffer::seeded("");
    }

    /// A person seated a pane: mint its seat and save it with the pane.
    fn threshing_floor_seat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        if let Some(org) = self.floor_org {
            self.threshing_floor_assign(leaf, org, false, cx);
        }
    }

    /// Both removal targets and the click alternative revoke the same authority.
    fn threshing_floor_unseat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        let Some(seat) = leaf.read(cx).seat else {
            return;
        };
        self.chart.forget_seat(seat);
        self.floor_orgs.remove(seat);
        self.threshing_floor_forget_pending(leaf, seat, cx);
        self.floor_relation = None;
        self.floor_move = None;
        self.floor_drop = None;
        self.floor_context = None;
        leaf.update(cx, |v, cx| {
            v.set_agent_access(threshing_floor::AgentAccess::OffLimits, cx);
            v.set_seat(None, cx);
        });
        self.floor_feedback = Some(lang::fill(
            self.lang.strings().ui.floor_removed,
            &[("pane", &self.pane_name(leaf, cx).tab)],
        ));
        if self.threshing_floor_pick == Some(seat) {
            self.threshing_floor_pick = None;
        }
        self.save(cx);
        cx.notify();
    }

    /// Membership changes invalidate both directions and every waiting card.
    fn threshing_floor_forget_pending(
        &mut self,
        leaf: &Entity<TerminalView>,
        seat: threshing_floor::SeatId,
        cx: &mut Context<Self>,
    ) {
        self.asks.forget_seat(seat);
        let pid = leaf.read(cx).shell_pid();
        for tab in &self.tabs {
            let mut leaves = Vec::new();
            tab.root.leaves(&mut leaves);
            for pane in leaves {
                let related = pane == leaf
                    || pane
                        .read(cx)
                        .access_ask
                        .as_ref()
                        .is_some_and(|ask| pid.is_some() && ask.pid == pid);
                if related {
                    pane.update(cx, |v, cx| {
                        v.access_ask = None;
                        cx.notify();
                    });
                }
            }
        }
    }

    /// Selection never grants a relationship or changes a card's dimensions.
    fn threshing_floor_click_seat(
        &mut self,
        seat: threshing_floor::SeatId,
        cx: &mut Context<Self>,
    ) {
        if self
            .floor_relation
            .as_ref()
            .is_some_and(|e| e.choosing.is_some())
        {
            self.threshing_floor_choose_endpoint(seat, cx);
        } else {
            self.threshing_floor_pick = if self.threshing_floor_pick == Some(seat) {
                None
            } else {
                Some(seat)
            };
            cx.notify();
        }
    }

    /// The edge label opens an explicit editor; it never cycles a permission.
    fn threshing_floor_click_line(
        &mut self,
        from: threshing_floor::SeatId,
        to: threshing_floor::SeatId,
        cx: &mut Context<Self>,
    ) {
        self.threshing_floor_begin_relation(Some(from), Some(to), cx);
    }
}

/// A line between two seats; a bypass is drawn dashed.
/// The dogleg a relationship line takes from one seat to another, in chart
/// pixels, plus the point its rung label sits on. Pure geometry so the routing
/// can be tested without a window: `(fx, fy)` and `(tx, ty)` are the seats'
/// top-left corners, each `sw`×`sh`, with `cg`/`rg` the column/row gaps that
/// form the gutters a line runs down to miss the boxes between its ends.
///
/// The row test is the load-bearing line. One row down is a full pitch
/// (`sh + rg`); two rows down is two pitches. The threshold sits halfway
/// between, so an adjacent card takes the straight next-row path — a
/// same-column pair collapses to one vertical line with the label between the
/// cards — and only a line that actually clears a box in between detours into a
/// side gutter. A smaller threshold (the earlier `sh + rg/2`) mistook every
/// adjacent stacked card for a multi-row jump and sent its line off the left
/// edge through the column-0 gutter.
fn wire_route(
    (fx, fy): (f32, f32),
    (tx, ty): (f32, f32),
    sw: f32,
    sh: f32,
    cg: f32,
    rg: f32,
) -> (Vec<(f32, f32)>, (f32, f32)) {
    if ty > fy + 1.5 * (sh + rg) {
        // Two or more rows down: leave through the gap under the sender, run
        // down a column gutter beside the target so no box in between is
        // crossed, and come in from the gap above it.
        let (x0, y0) = (fx + sw / 2., fy + sh);
        let (x1, y1) = (tx + sw / 2., ty);
        let (top, bottom) = (y0 + rg / 2., y1 - rg / 2.);
        // Prefer the gutter on the target's left; at the leftmost column that
        // gutter is off the canvas, so fall to the one on its right. Either
        // misses the same-column box the line is routing around.
        let left = tx - cg / 2.;
        let gx = if tx >= fx && left >= 0. {
            left
        } else {
            tx + sw + cg / 2.
        };
        (
            vec![
                (x0, y0),
                (x0, top),
                (gx, top),
                (gx, bottom),
                (x1, bottom),
                (x1, y1),
            ],
            // Labelled on its last stretch, by the seat it reaches, where it
            // cannot sit on a shorter line's label.
            ((gx + x1) / 2., bottom),
        )
    } else if ty > fy {
        // The next row: down from the sender, across, down into the seat. A
        // same-column pair collapses to a single vertical line, label centred.
        let (x0, y0) = (fx + sw / 2., fy + sh);
        let (x1, y1) = (tx + sw / 2., ty);
        let mid = y0 + (y1 - y0) / 2.;
        (
            vec![(x0, y0), (x0, mid), (x1, mid), (x1, y1)],
            ((x0 + x1) / 2., mid),
        )
    } else {
        // Same row or above: side to side, through the gap.
        let left_to_right = tx > fx;
        let x0 = if left_to_right { fx + sw } else { fx };
        let x1 = if left_to_right { tx } else { tx + sw };
        let (y0, y1) = (fy + sh / 2., ty + sh / 2.);
        (vec![(x0, y0), (x1, y1)], ((x0 + x1) / 2., (y0 + y1) / 2.))
    }
}

fn stroke_wire(
    bounds: Bounds<Pixels>,
    pts: &[(f32, f32)],
    ink: Hsla,
    dashed: bool,
    window: &mut Window,
) {
    let segment = |a: (f32, f32), b: (f32, f32), window: &mut Window| {
        let mut path = PathBuilder::stroke(px(1.6));
        path.move_to(bounds.origin + point(px(a.0), px(a.1)));
        path.line_to(bounds.origin + point(px(b.0), px(b.1)));
        if let Ok(path) = path.build() {
            window.paint_path(path, ink);
        }
    };
    for pair in pts.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if !dashed {
            segment(a, b, window);
            continue;
        }
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 0. {
            continue;
        }
        let (dash, gap) = (6., 4.);
        let mut t = 0.;
        while t < len {
            let e = (t + dash).min(len);
            segment(
                (a.0 + dx * t / len, a.1 + dy * t / len),
                (a.0 + dx * e / len, a.1 + dy * e / len),
                window,
            );
            t = e + gap;
        }
    }
    // A line is directed: the arrow reaches the target, including upward links.
    if let Some(pair) = pts.windows(2).rev().find(|p| p[0] != p[1]) {
        let (a, b) = (pair[0], pair[1]);
        let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let (dx, dy) = ((b.0 - a.0) / length, (b.1 - a.1) / length);
        segment(
            (b.0 - dx * 8. - dy * 4., b.1 - dy * 8. + dx * 4.),
            b,
            window,
        );
        segment(
            (b.0 - dx * 8. + dy * 4., b.1 - dy * 8. - dx * 4.),
            b,
            window,
        );
    }
}

/// A refusal's wire name, for the record.
/// A worker as a prompt's rules see it. An asleep agent's pane runs a shell
/// until it is woken, and is still the agent a line was drawn to.
/// A pane as a prompt sees it: an asleep agent is still an agent, and so is
/// one the sleep has signalled and not yet recorded (`rest.stopping`), whose
/// pane reads as a plain shell for up to one rest sweep after it exits.
fn prompt_party(v: &TerminalView) -> threshing_floor::Party {
    let mut party = v.party();
    party.is_agent |= v.rest.dormancy.asleep().is_some() || v.rest.stopping.is_some();
    party
}

fn refusal_code(r: &threshing_floor::Refusal) -> Option<String> {
    serde_json::to_value(r)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

fn seat_title(t: &TerminalView) -> String {
    t.name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| t.shown_title())
}

/// The card's harness-and-model line, as Parker spelled it: `CLAUDE · OPUS 5.5
/// MAX`, `CODEX · SOL 6.1 XHIGH`. An unknown model says so rather than leaving
/// the line short, and an effort the transcript never named is left off.
fn model_line(program: &str, model: Option<&str>, effort: Option<&str>, unknown: &str) -> String {
    match (model, effort.map(str::trim).filter(|e| !e.is_empty())) {
        (Some(m), Some(e)) => format!("{program} · {m} {}", e.to_uppercase()),
        (Some(m), None) => format!("{program} · {m}"),
        (None, _) => format!("{program} · {unknown}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(
        cx: &mut gpui::TestAppContext,
    ) -> (
        Entity<Workspace>,
        &'static mut gpui::VisualTestContext,
        threshing_floor::OrgId,
    ) {
        cx.executor().allow_parking();
        cx.update(|cx| {
            theme::init_embedded(cx, theme::house_outer());
            skin::init_embedded(cx);
        });
        let seed = || session::PaneRestore {
            program: Some((
                "/bin/sh".into(),
                vec![
                    "-c".into(),
                    "printf 'QA_GRID_CANARY\\n'; exec /bin/cat".into(),
                ],
            )),
            ..Default::default()
        };
        let window = {
            let _fork = crate::testsync::forks_and_locks();
            cx.open_window(size(px(1200.), px(1000.)), |w, cx| {
                Workspace::build(true, true, Some(seed()), None, w, cx)
            })
        };
        let ws = window.root(cx).unwrap();
        let vcx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
        let other = vcx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                let second = {
                    let _fork = crate::testsync::forks_and_locks();
                    make_pane_window_owned(seed(), w, cx)
                };
                ws.tabs
                    .push(Tab::new(Node::Leaf(second), Some("Worker".into())));
                let org = ws.floor_orgs.create("Cinema crew").unwrap();
                let other = ws.floor_orgs.create("Launch crew").unwrap();
                for (i, tab) in ws.tabs.iter_mut().enumerate() {
                    tab.name = Some(if i == 0 { "Lead" } else { "Worker" }.into());
                    let mut leaves = Vec::new();
                    tab.root.leaves(&mut leaves);
                    let pane = leaves[0].clone();
                    let seat = threshing_floor::SeatId(100 + i as u64);
                    pane.update(cx, |v, cx| {
                        v.mode = pane::PaneMode::Codex;
                        v.seat = Some(seat);
                        v.fixture_set_pane_id(10 + i as u64);
                        cx.notify();
                    });
                    ws.floor_orgs.assign(seat, org);
                }
                ws.floor_org = Some(org);
                ws.threshing_floor = true;
                ws.left_bar = false;
                w.focus(&ws.focus_handle, cx);
                cx.notify();
                other
            })
        });
        let cleanup = ws.read_with(vcx, |ws, cx| {
            let mut leaves = Vec::new();
            for tab in &ws.tabs {
                tab.root.leaves(&mut leaves);
            }
            leaves
                .iter()
                .map(|p| p.read(cx).fixture_shutdown())
                .collect::<Vec<_>>()
        });
        for stop in cleanup {
            vcx.on_quit(stop);
        }
        vcx.run_until_parked();
        vcx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear();
        });
        (ws, vcx, other)
    }

    fn bounds(cx: &mut gpui::VisualTestContext, id: &str) -> Bounds<Pixels> {
        cx.debug_bounds(Box::leak(id.to_string().into_boxed_str()))
            .unwrap_or_else(|| panic!("{id} not drawn"))
    }

    fn propose_open(
        ws: &mut Workspace,
        cx: &mut Context<Workspace>,
        pid: u32,
    ) -> serde_json::Value {
        let mut snapshot = mcp::Snapshot::empty();
        snapshot.panes = ws.mcp_snapshot(cx);
        snapshot.config = ws.mcp.clone();
        snapshot.chart = ws.chart.clone();
        snapshot.orgs = ws.floor_orgs.clone();
        snapshot.caller = Some(mcp::Caller {
            session: "fixture".into(),
            pane: Some(10),
        });
        let request = serde_json::json!({"id":1,"method":"tools/call",
            "params":{"name":"request_access","arguments":{
                "change":"open_pane","pid":pid,"why":"review the build"}}});
        let ui = std::cell::RefCell::new((ws, cx));
        let reply = mcp::handle_line_with(
            &request.to_string(),
            &snapshot,
            |_, _| vec![],
            |updates| {
                let mut ui = ui.borrow_mut();
                let (ws, cx) = &mut *ui;
                ws.apply_mcp_config(updates, cx)
            },
            |_, _| vec![],
        )
        .unwrap();
        serde_json::from_str::<serde_json::Value>(&reply).unwrap()["result"].clone()
    }

    #[gpui::test]
    fn opening_proposals_hide_existence_until_the_person_answers(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            ws.mcp.writable = false;
            let asker = ws.leaf_by_pane_id(10, cx).unwrap();
            let target = ws.leaf_by_pane_id(11, cx).unwrap();
            let pid = target.read(cx).shell_pid().unwrap();
            let key = asker.entity_id().as_u64();
            target.update(cx, |v, _| {
                v.agent_access = threshing_floor::AgentAccess::OffLimits
            });
            let hidden = propose_open(ws, cx, pid);
            assert_ne!(hidden["isError"], true, "{hidden}");
            let pending_hidden = propose_open(ws, cx, pid);
            assert_eq!(pending_hidden["isError"], true);
            assert_eq!(
                target.read(cx).agent_access,
                threshing_floor::AgentAccess::OffLimits
            );
            let proposal = asker.read(cx).access_ask.as_ref().unwrap();
            assert_eq!(proposal.target_key, Some(target.entity_id().as_u64()));
            assert!(proposal.title.as_ref().unwrap().contains("Worker"));
            assert!(ws.rail_answer(key, access::Answer::No, cx));
            assert_eq!(
                target.read(cx).agent_access,
                threshing_floor::AgentAccess::OffLimits
            );

            // Same claimed PID, same caller/chart, but no target in the window.
            let tab = ws.tabs.pop().unwrap();
            let missing = propose_open(ws, cx, pid);
            assert_eq!(missing, hidden, "the protocol must not disclose existence");
            assert_eq!(
                propose_open(ws, cx, pid),
                pending_hidden,
                "a second request must not disclose existence either"
            );
            let proposal = asker.read(cx).access_ask.as_ref().unwrap();
            assert!(proposal.target_key.is_none());
            assert!(proposal.title.as_ref().unwrap().contains("unavailable"));
            // Even the original PID appearing afterward cannot inherit a Yes.
            ws.tabs.push(tab);
            assert!(ws.rail_answer(key, access::Answer::Yes, cx));
            assert_eq!(
                target.read(cx).agent_access,
                threshing_floor::AgentAccess::OffLimits
            );

            assert_eq!(propose_open(ws, cx, pid), hidden);
            // Model a stale/replaced entity while retaining its claimed PID.
            asker.update(cx, |v, _| {
                v.access_ask.as_mut().unwrap().target_key = Some(key)
            });
            assert!(ws.rail_answer(key, access::Answer::Yes, cx));
            assert_eq!(
                target.read(cx).agent_access,
                threshing_floor::AgentAccess::OffLimits
            );

            assert_eq!(propose_open(ws, cx, pid), hidden);
            assert!(ws.rail_answer(key, access::Answer::Yes, cx));
            assert_eq!(
                target.read(cx).agent_access,
                threshing_floor::AgentAccess::Open
            );
            assert_eq!(
                propose_open(ws, cx, pid),
                hidden,
                "already-open is also indistinguishable"
            );
            assert!(ws.rail_answer(key, access::Answer::No, cx));
        });
    }

    fn click(cx: &mut gpui::VisualTestContext, id: &str) {
        let at = bounds(cx, id).center();
        cx.simulate_mouse_down(at, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
    }

    fn drag(cx: &mut gpui::VisualTestContext, from: &str, to: &str) {
        let start = bounds(cx, from).center();
        cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            start + point(px(9.), px(9.)),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        let end = bounds(cx, to).center();
        cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
    }

    fn removal_check(cx: &mut gpui::TestAppContext, target: &str) {
        let (ws, cx, _) = fixture(cx);
        let leaf = ws.read_with(cx, |ws, cx| {
            ws.threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap()
        });
        ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(101),
                threshing_floor::Rung::Read,
            );
            ws.chart.grant_bypass(
                threshing_floor::SeatId(101),
                threshing_floor::SeatId(100),
                3,
            );
            ws.chart.draw(
                threshing_floor::SeatId(300),
                threshing_floor::SeatId(301),
                threshing_floor::Rung::See,
            );
            let other = ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(101), cx)
                .unwrap();
            other.update(cx, |v, cx| {
                v.access_ask = Some(access::Ask {
                    change: access::Change::OpenPane,
                    pid: leaf.read(cx).shell_pid(),
                    target_key: Some(leaf.entity_id().as_u64()),
                    title: None,
                    why: "fixture".into(),
                    at: std::time::Instant::now(),
                    risk: None,
                    prompts: 3,
                    reads: None,
                    ask_id: None,
                });
                cx.notify();
            });
            cx.notify();
        });
        cx.run_until_parked();
        drag(cx, "floor-seat-100", target);
        ws.read_with(cx, |ws, cx| {
            assert_eq!(ws.floor_orgs.owner(threshing_floor::SeatId(100)), None);
            let v = leaf.read(cx);
            assert_eq!(v.seat, None);
            assert_eq!(v.agent_access, threshing_floor::AgentAccess::OffLimits);
            assert_eq!(ws.chart.saved().len(), 1, "unrelated relationships survive");
            let party = threshing_floor::Party {
                seat: v.seat,
                access: v.agent_access,
                is_agent: true,
            };
            assert_eq!(
                threshing_floor::reach(&ws.mcp, &ws.chart, &ws.floor_orgs, None, &party, false),
                threshing_floor::Reach::Hidden
            );
            assert!(!threshing_floor::may_write_live(
                &ws.mcp,
                &ws.chart,
                &ws.floor_orgs,
                Some(threshing_floor::SeatId(101)),
                &party,
                false
            ));
            assert!(ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(101), cx)
                .unwrap()
                .read(cx)
                .access_ask
                .is_none());
            assert!(ws.floor_feedback.as_ref().unwrap().contains("Lead"));
            assert!(ws.floor_drag.is_none());
            let saved: StateFile =
                toml::from_str(&toml::to_string(&ws.build_state(cx)).unwrap()).unwrap();
            match &saved.tabs[0].node {
                SavedNode::Leaf {
                    seat,
                    off_limits,
                    agents,
                    ..
                } => {
                    assert_eq!(*seat, None);
                    assert_eq!(
                        threshing_floor::AgentAccess::from_saved(*off_limits, agents.as_deref()),
                        threshing_floor::AgentAccess::OffLimits
                    );
                }
                _ => panic!("fixture is one leaf"),
            }
            assert_eq!(
                threshing_floor::Orgs::restore(saved.floor_orgs, &[])
                    .owner(threshing_floor::SeatId(100)),
                None
            );
        });
        assert!(
            cx.debug_bounds("floor-remove-drop").is_none(),
            "bin stayed after release"
        );
        let pool_id = format!("floor-pane-{}", leaf.entity_id().as_u64());
        drag(cx, &pool_id, "threshing-floor-chart");
        ws.read_with(cx, |ws, cx| {
            let v = leaf.read(cx);
            assert!(
                v.seat.is_some() && v.seat != Some(threshing_floor::SeatId(100)),
                "reseating gets a fresh identity"
            );
            assert_eq!(
                v.agent_access,
                threshing_floor::AgentAccess::OffLimits,
                "reseating must not reopen access"
            );
            assert_eq!(ws.chart.saved().len(), 1);
        });
    }

    fn pending_copy(
        ws: &Workspace,
        cx: &App,
        stage: threshing_floor::CopyStage,
    ) -> threshing_floor::CopyAsk {
        let target = ws
            .threshing_floor_seat_leaf(threshing_floor::SeatId(101), cx)
            .unwrap();
        threshing_floor::CopyAsk {
            asker_pane: 10,
            from: Some(threshing_floor::SeatId(100)),
            to: Some(threshing_floor::SeatId(101)),
            target_pid: target.read(cx).shell_pid().unwrap(),
            via: "line",
            reads: Some(1),
            model: None,
            question: "synthetic question".into(),
            question_len: 18,
            question_sha256: "fixture".into(),
            program: askcopy::Program::Codex,
            conversation: "synthetic-conversation".into(),
            stage,
        }
    }

    #[gpui::test]
    fn queued_grep_cannot_read_fresh_content_after_relationship_revocation(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let target = ws.read_with(cx, |ws, cx| ws.leaf_by_pane_id(11, cx).unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while target.read_with(cx, |v, _| v.grep_grid("QA_GRID_CANARY", 2000).is_empty()) {
            assert!(
                std::time::Instant::now() < deadline,
                "fixture never printed its positive-control canary"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            ws.chart.draw(threshing_floor::SeatId(100), threshing_floor::SeatId(101), threshing_floor::Rung::Read);
            let pid = target.read(cx).shell_pid().unwrap();
            let mut snap = mcp::Snapshot::empty();
            snap.panes = ws.mcp_snapshot(cx);
            snap.config = ws.mcp.clone();
            snap.chart = ws.chart.clone();
            snap.caller = Some(mcp::Caller { session: "fixture".into(), pane: Some(10) });
            let request = serde_json::json!({"id":1,"method":"tools/call","params":{"name":"grep","arguments":{"query":"QA_GRID_CANARY"}}}).to_string();
            let contains_target = |ws: &Workspace| {
                let search = |q: &str, cap: usize| ws.mcp_search(q, cap, Some(10), cx);
                let reply = mcp::handle_line_with(&request, &snap, |_: &mcp::PaneInfo, _: usize| vec![], mcp::no_apply, search).unwrap();
                let value: serde_json::Value = serde_json::from_str(&reply).unwrap();
                value["result"]["structuredContent"]["panes"].as_array().unwrap().iter().any(|p| p["pid"] == pid)
            };
            assert!(contains_target(ws), "unchanged grant failed its positive control");
            ws.chart.erase(threshing_floor::SeatId(100), threshing_floor::SeatId(101));
            assert!(!contains_target(ws), "queued grep used a revoked snapshot grant to read the live grid");
            ws.chart.draw(threshing_floor::SeatId(100), threshing_floor::SeatId(101), threshing_floor::Rung::See);
            assert!(!contains_target(ws), "a census-only line disclosed scrollback");
            ws.chart.draw(threshing_floor::SeatId(100), threshing_floor::SeatId(101), threshing_floor::Rung::Read);
            assert!(contains_target(ws), "restoring read must restore the positive control");
            ws.mcp.enabled = false;
            assert!(!contains_target(ws), "disabling MCP did not revoke the queued read");
            ws.mcp.enabled = true;
            assert!(!ws.mcp_search("QA_GRID_CANARY", 2000, None, cx).iter().any(|p| p.pid == pid));
            assert!(!ws.mcp_search("QA_GRID_CANARY", 2000, Some(999), cx).iter().any(|p| p.pid == pid));
        });
    }

    #[gpui::test]
    fn deleting_an_org_erases_its_members_relationships_and_the_org(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            let org = ws.floor_orgs.owner(threshing_floor::SeatId(100)).unwrap();
            // a relationship inside the org, which the confirm must count
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(101),
                threshing_floor::Rung::Ask,
            );
            assert_eq!(ws.threshing_floor_org_relationship_count(org), 1);
            ws.floor_delete_org = Some(org);
            ws.threshing_floor_delete_org(org, cx);
            // the org is gone, its line is gone, the confirm is cleared
            assert!(ws.floor_orgs.get(org).is_none());
            assert!(ws
                .chart
                .line(threshing_floor::SeatId(100), threshing_floor::SeatId(101))
                .is_none());
            assert_eq!(ws.floor_delete_org, None);
            // members are loose: no owner, but still seated and available
            assert_eq!(ws.floor_orgs.owner(threshing_floor::SeatId(100)), None);
            assert!(ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .is_some());
        });
    }

    #[gpui::test]
    fn the_system_map_draws_a_whole_org_line_that_reaches_every_member(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            // put a member in the other org so the whole-org line has someone to reach
            ws.floor_orgs.assign(threshing_floor::SeatId(101), other);
            ws.floor_map = Some(SystemMap::default());
            if let Some(m) = &mut ws.floor_map {
                m.from = Some(threshing_floor::End::Seat(threshing_floor::SeatId(100)));
                m.to = Some(threshing_floor::End::Org { org: other });
                m.rung = Some(threshing_floor::Rung::Ask);
            }
            ws.floor_map_draw(cx);
            assert_eq!(ws.chart.org_lines().len(), 1, "the whole-org line is drawn");
            // the draft cleared for the next one
            assert!(ws.floor_map.as_ref().is_some_and(|m| m.from.is_none()));
            // and seat 100 now reaches the other org's member at ask
            ws.mcp.enabled = true;
            let caller = threshing_floor::Party {
                seat: Some(threshing_floor::SeatId(100)),
                is_agent: true,
                access: threshing_floor::AgentAccess::Open,
            };
            let member = threshing_floor::Party {
                seat: Some(threshing_floor::SeatId(101)),
                is_agent: true,
                access: threshing_floor::AgentAccess::Open,
            };
            assert_eq!(
                threshing_floor::reach(
                    &ws.mcp,
                    &ws.chart,
                    &ws.floor_orgs,
                    Some(&caller),
                    &member,
                    false
                ),
                threshing_floor::Reach::Ask
            );
        });
    }

    #[gpui::test]
    fn the_system_map_and_delete_confirm_paint(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _other) = fixture(cx);
        // open the System Map with the From chooser active
        ws.update(cx, |ws, _| {
            ws.floor_map = Some(SystemMap {
                choosing: Some(RelationEnd::From),
                ..Default::default()
            });
        });
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear();
        });
        // the map, its builder and its rung picker all drew
        bounds(cx, "floor-system-map");
        bounds(cx, "floor-map-draw");
        bounds(cx, "floor-map-rung-Ask");
        // arm the destructive org delete: its confirm button draws
        let org = ws.read_with(cx, |ws, _| ws.floor_org.unwrap());
        ws.update(cx, |ws, _| {
            ws.floor_map = None;
            ws.floor_delete_org = Some(org);
        });
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear();
        });
        bounds(cx, "floor-org-delete-go");
    }

    #[gpui::test]
    fn org_transfer_revokes_pending_copies_and_permission_cards(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        for moving in [100, 101] {
            ws.update(cx, |ws, cx| {
                let original = ws.floor_orgs.owner(threshing_floor::SeatId(100)).unwrap();
                let leaf = ws
                    .threshing_floor_seat_leaf(threshing_floor::SeatId(moving), cx)
                    .unwrap();
                let destination = if original == other {
                    ws.floor_orgs.create("Third crew").unwrap()
                } else {
                    other
                };
                ws.chart.draw(
                    threshing_floor::SeatId(100),
                    threshing_floor::SeatId(101),
                    threshing_floor::Rung::Ask,
                );
                for (i, stage) in [
                    threshing_floor::CopyStage::AwaitingPress,
                    threshing_floor::CopyStage::Running,
                    threshing_floor::CopyStage::Answered {
                        answer: "PRIVATE_CANARY".into(),
                        tokens_read: None,
                        cost_usd: None,
                    },
                ]
                .into_iter()
                .enumerate()
                {
                    ws.asks.open(
                        threshing_floor::AskId(i as u64 + 1),
                        pending_copy(ws, cx, stage),
                    );
                }
                let mut unrelated = pending_copy(ws, cx, threshing_floor::CopyStage::Running);
                unrelated.from = Some(threshing_floor::SeatId(300));
                unrelated.to = Some(threshing_floor::SeatId(301));
                ws.asks.open(threshing_floor::AskId(99), unrelated);
                let asker = ws.leaf_by_pane_id(10, cx).unwrap();
                let target_pid = ws.asks.get(threshing_floor::AskId(1)).unwrap().target_pid;
                asker.update(cx, |v, _| {
                    v.access_ask = Some(access::Ask {
                        change: access::Change::AskLine,
                        pid: Some(target_pid),
                        target_key: None,
                        title: None,
                        why: "synthetic bypass request".into(),
                        at: std::time::Instant::now(),
                        risk: None,
                        prompts: 1,
                        reads: None,
                        ask_id: None,
                    })
                });
                ws.threshing_floor_assign(&leaf, destination, false, cx);
                assert!(
                    ws.asks.get(threshing_floor::AskId(1)).is_some(),
                    "unconfirmed move changed authority"
                );
                ws.threshing_floor_assign(&leaf, destination, true, cx);
                for id in 1..=3 {
                    assert!(
                        ws.asks.get(threshing_floor::AskId(id)).is_none(),
                        "org transfer retained copy {id}"
                    );
                }
                assert!(
                    asker.read(cx).access_ask.is_none(),
                    "old bypass prompt survived transfer"
                );
                ws.copy_finished(
                    threshing_floor::AskId(2),
                    Ok(askcopy::Answer {
                        text: "LATE_PRIVATE_CANARY".into(),
                        tokens_read: None,
                        cost_usd: None,
                    }),
                    cx,
                );
                assert!(
                    ws.asks.get(threshing_floor::AskId(2)).is_none(),
                    "late completion restored a revoked answer"
                );
                assert!(
                    ws.asks.get(threshing_floor::AskId(99)).is_some(),
                    "unrelated copy was cancelled"
                );
            });
        }
    }

    #[gpui::test]
    fn queued_write_rechecks_relationship_and_caller_identity(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            ws.mcp.writable = true;
            let target = ws.leaf_by_pane_id(11, cx).unwrap();
            let pid = target.read(cx).shell_pid().unwrap();
            let patch = mcp::ConfigPatch {
                caller_pane: Some(10),
                brightness: Some(37.),
                ..Default::default()
            };
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(101),
                threshing_floor::Rung::Read,
            );
            assert!(
                ws.apply_mcp_config(&[(mcp::Target::Pane(pid), patch.clone())], cx)[0]
                    .1
                    .is_ok()
            );
            ws.chart
                .erase(threshing_floor::SeatId(100), threshing_floor::SeatId(101));
            let before = target.read(cx).appearance.clone();
            assert!(
                ws.apply_mcp_config(&[(mcp::Target::Pane(pid), patch.clone())], cx)[0]
                    .1
                    .is_err()
            );
            assert!(
                before == target.read(cx).appearance,
                "denied write changed appearance"
            );
            // A request was once its caller's own pane. The old boolean must
            // not confer that exemption on a different live target.
            let stale_own = mcp::ConfigPatch {
                own_pane: true,
                ..patch
            };
            assert!(
                ws.apply_mcp_config(&[(mcp::Target::Pane(pid), stale_own)], cx)[0]
                    .1
                    .is_err(),
                "captured own flag bypassed live identity"
            );
        });
    }

    #[gpui::test]
    fn copy_collection_checks_live_policy_seats_and_the_original_asker(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        ws.update(cx, |ws, cx| {
            let asker = ws.leaf_by_pane_id(10, cx).unwrap();
            let target = ws.leaf_by_pane_id(11, cx).unwrap();
            for case in [
                "valid",
                "wrong-asker",
                "line-removed",
                "off-limits",
                "mcp-off",
                "asker-reseated",
                "target-reseated",
                "spent-bypass",
                "read-only",
            ] {
                ws.mcp.enabled = true;
                asker.update(cx, |v, _| v.seat = Some(threshing_floor::SeatId(100)));
                target.update(cx, |v, _| {
                    v.seat = Some(threshing_floor::SeatId(101));
                    v.agent_access = threshing_floor::AgentAccess::Open;
                });
                ws.chart.draw(
                    threshing_floor::SeatId(100),
                    threshing_floor::SeatId(101),
                    threshing_floor::Rung::Ask,
                );
                let mut ask = pending_copy(
                    ws,
                    cx,
                    threshing_floor::CopyStage::Answered {
                        answer: "SECRET_ANSWER".into(),
                        tokens_read: None,
                        cost_usd: None,
                    },
                );
                match case {
                    "line-removed" => ws
                        .chart
                        .erase(threshing_floor::SeatId(100), threshing_floor::SeatId(101)),
                    "off-limits" => target.update(cx, |v, _| {
                        v.agent_access = threshing_floor::AgentAccess::OffLimits
                    }),
                    "read-only" => target.update(cx, |v, _| {
                        v.agent_access = threshing_floor::AgentAccess::ReadOnly
                    }),
                    "mcp-off" => ws.mcp.enabled = false,
                    "asker-reseated" => {
                        asker.update(cx, |v, _| v.seat = Some(threshing_floor::SeatId(900)))
                    }
                    "target-reseated" => {
                        target.update(cx, |v, _| v.seat = Some(threshing_floor::SeatId(901)))
                    }
                    "spent-bypass" => {
                        ask.via = "bypass";
                        ws.chart
                            .erase(threshing_floor::SeatId(100), threshing_floor::SeatId(101));
                    }
                    _ => {}
                }
                let id = threshing_floor::AskId(500);
                ws.asks.open(id, ask);
                let reply = ws.ask_bridge(
                    &mcp::AskCall::Answer {
                        caller_pane: if case == "wrong-asker" { 11 } else { 10 },
                        ask_id: id,
                    },
                    cx,
                );
                let allowed = matches!(case, "valid" | "spent-bypass" | "read-only");
                assert_eq!(
                    matches!(reply, mcp::AskReply::Copy(_)),
                    allowed,
                    "{case}: {reply:?}"
                );
                ws.asks.forget_seat(threshing_floor::SeatId(100));
            }
        });
    }

    #[gpui::test]
    fn revoked_copy_cannot_start_finish_or_survive_a_relationship_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            let target = ws.leaf_by_pane_id(11, cx).unwrap();
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(101),
                threshing_floor::Rung::Ask,
            );
            ws.asks.open(
                threshing_floor::AskId(1),
                pending_copy(ws, cx, threshing_floor::CopyStage::AwaitingPress),
            );
            target.update(cx, |v, _| {
                v.agent_access = threshing_floor::AgentAccess::OffLimits
            });
            assert_eq!(
                ws.answer_copy(Some(threshing_floor::AskId(1)), access::Answer::Yes, cx),
                access::Outcome::Gone
            );
            assert!(ws.asks.get(threshing_floor::AskId(1)).is_none());
            target.update(cx, |v, _| {
                v.agent_access = threshing_floor::AgentAccess::Open
            });
            ws.asks.open(
                threshing_floor::AskId(2),
                pending_copy(ws, cx, threshing_floor::CopyStage::Running),
            );
            ws.chart
                .erase(threshing_floor::SeatId(100), threshing_floor::SeatId(101));
            ws.copy_finished(
                threshing_floor::AskId(2),
                Ok(askcopy::Answer {
                    text: "REVOKED_CANARY".into(),
                    tokens_read: None,
                    cost_usd: None,
                }),
                cx,
            );
            assert!(ws.asks.get(threshing_floor::AskId(2)).is_none());
            let mut bypass = pending_copy(ws, cx, threshing_floor::CopyStage::Running);
            bypass.via = "bypass";
            ws.asks.open(threshing_floor::AskId(3), bypass.clone());
            std::mem::swap(&mut bypass.from, &mut bypass.to);
            ws.asks.open(threshing_floor::AskId(4), bypass);
            ws.floor_relation = Some(RelationEditor {
                draft: threshing_floor::RelationDraft {
                    from: Some(threshing_floor::SeatId(100)),
                    to: Some(threshing_floor::SeatId(101)),
                    rung: Some(threshing_floor::Rung::See),
                },
                choosing: None,
            });
            ws.threshing_floor_save_relation(cx);
            assert!(
                ws.asks.get(threshing_floor::AskId(3)).is_none(),
                "relationship edit preserved a bypass receipt"
            );
            assert!(
                ws.asks.get(threshing_floor::AskId(4)).is_some(),
                "reverse direction was cancelled"
            );
        });
    }

    #[gpui::test]
    fn pool_drop_revokes_access_and_saved_membership(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        removal_check(cx, "threshing-floor-pool");
    }

    #[gpui::test]
    fn bin_drop_revokes_access_and_saved_membership(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        removal_check(cx, "floor-remove-drop");
    }

    #[gpui::test]
    fn drag_cancel_and_invalid_release_preserve_membership_and_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let source = bounds(cx, "floor-seat-100");
        let chart = bounds(cx, "threshing-floor-chart");
        let pool = bounds(cx, "floor-pool-tile");
        for escape in [true, false] {
            cx.simulate_mouse_down(
                source.center(),
                MouseButton::Left,
                gpui::Modifiers::default(),
            );
            cx.simulate_mouse_move(
                source.center() + point(px(10.), px(10.)),
                MouseButton::Left,
                gpui::Modifiers::default(),
            );
            cx.run_until_parked();
            assert!(cx.debug_bounds("floor-remove-drop").is_some());
            assert_eq!(
                bounds(cx, "floor-seat-100"),
                source,
                "pickup moved its source"
            );
            assert_eq!(bounds(cx, "threshing-floor-chart"), chart);
            assert_eq!(bounds(cx, "floor-pool-tile"), pool);
            let bin = bounds(cx, "floor-remove-drop");
            cx.simulate_mouse_move(
                bin.bottom_right() - point(px(3.), px(3.)),
                MouseButton::Left,
                gpui::Modifiers::default(),
            );
            cx.run_until_parked();
            let preview = bounds(cx, "floor-drag-preview");
            let panel = bounds(cx, "threshing-floor-panel");
            assert!(
                preview.right() <= panel.right(),
                "preview {preview:?}, panel {panel:?}, bin {bin:?}"
            );
            assert!(
                preview.bottom() <= panel.bottom(),
                "preview {preview:?}, panel {panel:?}, bin {bin:?}"
            );
            if escape {
                cx.simulate_keystrokes("escape");
            }
            let end = if escape {
                pool.center()
            } else {
                bounds(cx, "threshing-floor-header").center()
            };
            cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::default());
            cx.simulate_mouse_up(end, MouseButton::Left, gpui::Modifiers::default());
            cx.run_until_parked();
            ws.read_with(cx, |ws, cx| {
                assert!(ws.threshing_floor);
                assert!(ws
                    .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                    .is_some());
                assert!(ws.floor_feedback.is_none());
            });
            assert!(cx.debug_bounds("floor-remove-drop").is_none());
        }
    }

    /// Parker's spelling of the model line, 2026-10-27: harness, model with
    /// its version, then effort.
    #[test]
    fn the_model_line_reads_harness_model_version_and_effort() {
        let u = "model unknown";
        assert_eq!(
            model_line("CLAUDE", Some("OPUS 5.5"), Some("max"), u),
            "CLAUDE · OPUS 5.5 MAX"
        );
        assert_eq!(
            model_line("CODEX", Some("SOL 6.1"), Some("xhigh"), u),
            "CODEX · SOL 6.1 XHIGH"
        );
        assert_eq!(
            model_line("CLAUDE", Some("HAIKU 5.1"), Some("  "), u),
            "CLAUDE · HAIKU 5.1",
            "an effort the transcript never named is left off"
        );
        assert_eq!(
            model_line("CODEX", None, Some("high"), u),
            "CODEX · model unknown",
            "an unknown model says so, effort or no"
        );
    }

    /// A seat that has handed nothing over says so in the accent; once it hands
    /// a brief over, the card names the newest, counts the rest, and says a
    /// person annotated it only because a concur is saved in the file.
    #[gpui::test]
    fn a_card_names_its_newest_artifact_and_whether_a_person_annotated_it(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        assert!(cx.debug_bounds("floor-no-artifact-101").is_some());
        assert!(cx.debug_bounds("floor-artifact-101").is_none());
        let dir = std::env::temp_dir().join(format!("td-floor-card-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let older = dir.join("older.html");
        std::fs::write(&older, "<html><body>an earlier page</body></html>").unwrap();
        let brief = dir.join("brief.html");
        let islands = |concurs: &str| {
            format!(
                "<html><body><p id=\"x\">x</p>\
                 <script type=\"application/json\" id=\"report-notes\" data-format=\"1\">{{}}</script>\
                 <script type=\"application/json\" id=\"report-concurs\" data-format=\"1\">{concurs}</script>\
                 </body></html>"
            )
        };
        std::fs::write(&brief, islands("{}")).unwrap();
        cx.update(|_, cx| {
            let leaf = ws
                .read(cx)
                .threshing_floor_seat_leaf(threshing_floor::SeatId(101), cx)
                .unwrap();
            leaf.update(cx, |v, cx| {
                v.hand_over(
                    None,
                    older.display().to_string(),
                    crate::handover::Source::Declared,
                    cx,
                );
                v.hand_over(
                    Some("The brief".into()),
                    brief.display().to_string(),
                    crate::handover::Source::Declared,
                    cx,
                );
            });
            ws.update(cx, |_, cx| cx.notify());
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("floor-no-artifact-101").is_none());
        assert!(cx.debug_bounds("floor-artifact-101").is_some());
        assert!(
            cx.debug_bounds("floor-no-feedback-101").is_some(),
            "handed over and not yet annotated"
        );
        assert!(
            cx.debug_bounds("floor-no-artifact-100").is_some(),
            "the other seat has still handed nothing over"
        );
        std::fs::write(&brief, islands(r#"{"ask-1":"2026-10-27 12:00"}"#)).unwrap();
        ws.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("floor-annotated-101").is_some(),
            "a concur saved in the file is a person's feedback"
        );
        assert!(cx.debug_bounds("floor-no-feedback-101").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An org whose every seat has lost its pane still draws those seats, and
    /// the hint for an empty org is not laid over them (Parker's screenshot,
    /// 2026-10-27: "Drop panes here" printed across the first card).
    #[gpui::test]
    fn an_org_whose_panes_are_all_gone_shows_their_cards_and_no_empty_hint(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        cx.update(|_, cx| {
            ws.update(cx, |ws, cx| {
                let org = ws.floor_org.unwrap();
                ws.floor_orgs.remove(threshing_floor::SeatId(100));
                ws.floor_orgs.remove(threshing_floor::SeatId(101));
                ws.floor_orgs.assign(threshing_floor::SeatId(999), org);
                cx.notify();
            })
        });
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("floor-unavailable-999").is_some(),
            "the seat whose pane is gone keeps its card"
        );
        assert!(
            cx.debug_bounds("floor-org-empty-hint").is_none(),
            "the empty-org hint is only for an org with no seats at all"
        );
    }

    #[gpui::test]
    fn card_menu_dismisses_on_press_and_changes_only_its_card(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let at = bounds(cx, "floor-seat-101").center();
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("floor-context-menu").is_some());
        let outside = bounds(cx, "threshing-floor-search").center();
        cx.simulate_mouse_down(outside, MouseButton::Left, gpui::Modifiers::default());
        assert!(
            ws.read_with(cx, |ws, _| ws.floor_context.is_none()),
            "waited for release"
        );
        cx.simulate_mouse_up(outside, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.run_until_parked();
        click(cx, "floor-context-off-limits");
        ws.read_with(cx, |ws, cx| {
            assert!(ws.floor_context.is_none());
            assert_eq!(
                ws.threshing_floor_seat_leaf(threshing_floor::SeatId(101), cx)
                    .unwrap()
                    .read(cx)
                    .agent_access,
                threshing_floor::AgentAccess::OffLimits
            );
            assert_eq!(
                ws.threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                    .unwrap()
                    .read(cx)
                    .agent_access,
                threshing_floor::AgentAccess::Open
            );
        });
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_keystrokes("escape");
        ws.read_with(cx, |ws, _| {
            assert!(ws.threshing_floor);
            assert!(ws.floor_context.is_none());
        });
    }

    #[gpui::test]
    fn terminal_menu_closes_even_when_the_threshing_floor_swallows_the_press(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        ws.update(cx, |ws, cx| {
            ws.close_threshing_floor();
            cx.notify();
        });
        cx.run_until_parked();
        let at = point(px(700.), px(450.));
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("pane-context-menu").is_some());
        ws.update(cx, |ws, cx| {
            ws.threshing_floor = true;
            cx.notify();
        });
        cx.run_until_parked();
        let outside = bounds(cx, "threshing-floor-search").center();
        cx.simulate_mouse_down(outside, MouseButton::Left, gpui::Modifiers::default());
        ws.read_with(cx, |ws, cx| {
            assert!(!ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap()
                .read(cx)
                .popup_open())
        });
        cx.simulate_mouse_up(outside, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_down(outside, MouseButton::Right, gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("pane-context-menu").is_none(),
            "right click leaked into the covered terminal"
        );
    }

    #[gpui::test]
    fn connector_drag_requires_explicit_permission_and_keeps_card_shape(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let original = bounds(cx, "floor-seat-100").size;
        click(cx, "floor-seat-100");
        assert_eq!(
            bounds(cx, "floor-seat-100").size,
            original,
            "selection transformed the card"
        );
        assert!(ws.read_with(cx, |ws, _| ws.chart.saved().is_empty()));
        drag(cx, "floor-connect-100", "floor-seat-101");
        ws.read_with(cx, |ws, _| {
            let editor = ws
                .floor_relation
                .as_ref()
                .expect("connector drop opened the builder");
            assert_eq!(editor.draft.from, Some(threshing_floor::SeatId(100)));
            assert_eq!(editor.draft.to, Some(threshing_floor::SeatId(101)));
            assert_eq!(editor.draft.rung, None);
            assert!(
                ws.chart.saved().is_empty(),
                "dropping a connector granted access"
            );
        });
        click(cx, "floor-rel-save");
        assert!(ws.read_with(cx, |ws, _| ws.chart.saved().is_empty()));
        click(cx, "floor-rung-Read");
        click(cx, "floor-rel-save");
        ws.read_with(cx, |ws, _| {
            assert_eq!(
                ws.chart
                    .line(threshing_floor::SeatId(100), threshing_floor::SeatId(101))
                    .unwrap()
                    .rung
                    .known(),
                Some(threshing_floor::Rung::Read)
            )
        });
        assert_eq!(bounds(cx, "floor-seat-100").size, original);
        click(cx, "floor-line-100-101");
        click(cx, "floor-rung-Ask");
        click(cx, "floor-rel-cancel");
        ws.read_with(cx, |ws, _| {
            assert_eq!(
                ws.chart
                    .line(threshing_floor::SeatId(100), threshing_floor::SeatId(101))
                    .unwrap()
                    .rung
                    .known(),
                Some(threshing_floor::Rung::Read)
            )
        });
        click(cx, "floor-line-100-101");
        click(cx, "floor-rel-remove");
        assert!(ws.read_with(cx, |ws, _| ws.chart.saved().is_empty()));
    }

    #[gpui::test]
    fn starting_a_relationship_keeps_the_pane_filter_and_ask_and_prompt_read_as_themselves(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        // Ask was once labelled "prompt", because a copy fork was the
        // strongest thing a line allowed. Prompt is its own rung now, so each
        // reads as what it does, and the saved words are the rungs' own.
        ws.read_with(cx, |ws, _| {
            let ui = ws.lang.strings().ui;
            assert_eq!((ui.floor_rung_ask, ui.floor_rung_prompt), ("ask", "prompt"));
            assert_eq!(
                threshing_floor::RungWord::Known(threshing_floor::Rung::Ask).known(),
                Some(threshing_floor::Rung::Ask)
            );
        });
        cx.simulate_keystrokes("l e a d");
        cx.run_until_parked();
        assert_eq!(
            ws.read_with(cx, |ws, _| ws.threshing_floor_query.text()),
            "lead"
        );
        // Opening the relationship builder (Connect, or a connector drop) used
        // to wipe the filter mid-wiring. It must survive.
        click(cx, "floor-connect");
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.floor_relation.is_some(),
                "Connect did not open the relationship builder"
            );
            assert_eq!(
                ws.threshing_floor_query.text(),
                "lead",
                "beginning a relationship wiped the person's filter"
            );
        });
    }

    #[gpui::test]
    fn whole_card_drag_moves_membership_and_the_saved_record_keeps_identity(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        let original = bounds(cx, "floor-seat-100").size;
        ws.update(cx, |ws, cx| {
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(101),
                threshing_floor::Rung::Read,
            );
            cx.notify();
        });
        cx.run_until_parked();
        // Dropping on a different org now offers a choice rather than moving
        // silently: the drag opens the popup, and nothing has changed yet.
        drag(cx, "floor-seat-100", &format!("floor-org-{}", other.0));
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.floor_drop.is_some(),
                "the drop opens a choice, not a move"
            );
            assert_ne!(
                ws.floor_orgs.owner(threshing_floor::SeatId(100)),
                Some(other)
            );
            assert_eq!(
                ws.chart.saved().len(),
                1,
                "dragging silently erased a connection"
            );
        });
        // Choosing Move on a connected pane still raises the connection-loss
        // confirm before anything is lost.
        click(cx, "floor-drop-move");
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.floor_move.is_some(),
                "a connected move needs a visible confirm"
            );
            assert_ne!(
                ws.floor_orgs.owner(threshing_floor::SeatId(100)),
                Some(other)
            );
            assert_eq!(ws.chart.saved().len(), 1);
        });
        click(cx, "floor-move-cancel");
        assert_eq!(ws.read_with(cx, |ws, _| ws.chart.saved().len()), 1);
        // Drive it all the way through: choose Move, then confirm the loss.
        drag(cx, "floor-seat-100", &format!("floor-org-{}", other.0));
        click(cx, "floor-drop-move");
        click(cx, "floor-move-confirm");
        assert_eq!(bounds(cx, "floor-seat-100").size, original);
        ws.read_with(cx, |ws, cx| {
            assert_eq!(
                ws.floor_orgs.owner(threshing_floor::SeatId(100)),
                Some(other)
            );
            assert_eq!(ws.floor_org, Some(other));
            assert!(ws.chart.saved().is_empty());
            let state = ws.build_state(cx);
            let saved: StateFile = toml::from_str(&toml::to_string(&state).unwrap()).unwrap();
            let orgs = threshing_floor::Orgs::restore(saved.floor_orgs, &[]);
            assert_eq!(orgs.owner(threshing_floor::SeatId(100)), Some(other));
            assert_eq!(
                orgs.get(other).unwrap().name.as_deref(),
                Some("Launch crew")
            );
        });
        let first = ws.read_with(cx, |ws, _| {
            ws.floor_orgs.owner(threshing_floor::SeatId(101)).unwrap()
        });
        cx.simulate_keystrokes("w o r k e r");
        cx.run_until_parked();
        click(cx, &format!("floor-open-org-{}", first.0));
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.floor_org, Some(first));
            // The filter is the person's. Opening the org it found keeps what
            // they typed instead of wiping it — only ×, Esc and closing clear.
            assert_eq!(ws.threshing_floor_query.text(), "worker");
        });
    }

    #[gpui::test]
    fn names_are_typed_without_creating_relationships_and_close_restores_terminal_focus(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        click(cx, "floor-org-new");
        cx.simulate_keystrokes("c r e w enter");
        cx.run_until_parked();
        let created = ws.read_with(cx, |ws, _| {
            let id = ws.floor_org.unwrap();
            assert_eq!(ws.floor_orgs.get(id).unwrap().name.as_deref(), Some("crew"));
            assert!(ws.floor_orgs.get(id).unwrap().seats.is_empty());
            assert!(ws.chart.saved().is_empty());
            id
        });
        click(cx, "floor-org-rename");
        cx.simulate_keystrokes("t e a m enter");
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.floor_org, Some(created));
            assert_eq!(
                ws.floor_orgs.get(created).unwrap().name.as_deref(),
                Some("team")
            );
            assert!(ws.chart.saved().is_empty());
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        cx.update(|window, cx| {
            ws.read_with(cx, |ws, cx| {
                assert!(!ws.threshing_floor);
                let mut leaves = Vec::new();
                ws.tabs[ws.active].root.leaves(&mut leaves);
                assert!(
                    leaves[0].focus_handle(cx).is_focused(window),
                    "closing the editor stranded keyboard focus"
                );
            })
        });
    }

    #[gpui::test]
    fn chart_and_pool_scroll_inside_the_frame_while_their_headers_stay_put(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let (worker, extras) = cx.update(|window, cx| {
            ws.update(cx, |ws, cx| {
                let worker = ws
                    .threshing_floor_seat_leaf(threshing_floor::SeatId(101), cx)
                    .unwrap();
                ws.threshing_floor_unseat(&worker, cx);
                let mut extras = Vec::new();
                for i in 0..6 {
                    let pane = {
                        let _fork = crate::testsync::forks_and_locks();
                        make_pane_window_owned(
                            session::PaneRestore {
                                program: Some(("/bin/cat".into(), vec![])),
                                ..Default::default()
                            },
                            window,
                            cx,
                        )
                    };
                    pane.update(cx, |v, _| v.mode = pane::PaneMode::Codex);
                    ws.tabs.push(Tab::new(
                        Node::Leaf(pane.clone()),
                        Some(format!("Pool {i}")),
                    ));
                    extras.push(pane);
                }
                let mut from = threshing_floor::SeatId(100);
                for i in 200..208 {
                    let to = threshing_floor::SeatId(i);
                    ws.floor_orgs.assign(to, ws.floor_org.unwrap());
                    ws.chart.draw(from, to, threshing_floor::Rung::See);
                    from = to;
                }
                for i in 0..12 {
                    ws.floor_orgs.create(&format!("Other crew {i}"));
                }
                cx.notify();
                (worker, extras)
            })
        });
        for pane in extras {
            let stop = pane.read_with(cx, |v, _| v.fixture_shutdown());
            cx.on_quit(stop);
        }
        let first = format!("floor-pane-{}", worker.entity_id().as_u64());
        for dimensions in [size(px(900.), px(560.)), size(px(1200.), px(1000.))] {
            cx.simulate_resize(dimensions);
            cx.run_until_parked();
            let frame = bounds(cx, "threshing-floor-panel");
            let chart = bounds(cx, "threshing-floor-chart");
            let pool = bounds(cx, "threshing-floor-pool");
            assert!(frame.origin.x >= px(0.) && frame.right() <= dimensions.width);
            assert!(frame.origin.y >= px(0.) && frame.bottom() <= dimensions.height);
            assert!(chart.bottom() <= frame.bottom() && pool.bottom() <= frame.bottom());
            assert!(chart.size.height > px(0.) && pool.size.height > px(0.));
            assert!(
                chart.right() < pool.origin.x,
                "the pool belongs on the right"
            );
            let headers = [
                "threshing-floor-header",
                "floor-org-header",
                "floor-pool-header",
            ];
            let fixed = headers.map(|id| bounds(cx, id));
            let card = bounds(cx, &first);
            let member = bounds(cx, "floor-seat-100");
            let wheel = |cx: &mut gpui::VisualTestContext, at| {
                cx.simulate_event(gpui::ScrollWheelEvent {
                    position: at,
                    delta: gpui::ScrollDelta::Lines(point(0., -4.)),
                    modifiers: Default::default(),
                    touch_phase: gpui::TouchPhase::Moved,
                });
                cx.run_until_parked();
            };
            wheel(cx, pool.center());
            assert!(
                bounds(cx, &first).origin.y < card.origin.y,
                "pool contents scroll"
            );
            assert_eq!(
                bounds(cx, "floor-seat-100"),
                member,
                "pool wheel moved the chart"
            );
            let pool_after = bounds(cx, &first);
            wheel(cx, chart.center());
            assert!(
                bounds(cx, "floor-seat-100").origin.y < member.origin.y,
                "chart contents scroll"
            );
            assert_eq!(bounds(cx, &first), pool_after, "chart wheel moved the pool");
            assert_eq!(
                bounds(cx, &first).size,
                card.size,
                "scrolling reshaped a card"
            );
            assert_eq!(bounds(cx, "threshing-floor-panel"), frame);
            assert_eq!(
                headers.map(|id| bounds(cx, id)),
                fixed,
                "a header scrolled away"
            );
        }
    }

    fn name() -> threshing_floor::PaneName {
        threshing_floor::PaneName::new(
            &["Terminal Delight".into(), "MCP wiring".into()],
            "Fix permissions",
        )
    }

    #[test]
    fn fuzzy_words_find_project_tab_and_pane_in_any_word_order() {
        for query in [
            "",
            "  ",
            "trmdlt",
            "mcp",
            "fxprm",
            "prm mcp delight",
            "MCP FIX",
        ] {
            assert!(
                pane_search_score("Terminal Delight", &name(), query).is_some(),
                "{query}"
            );
        }
        assert!(pane_search_score("Terminal Delight", &name(), "zebra").is_none());
        assert!(pane_search_score("Terminal Delight", &name(), "mcp zebra").is_none());
    }

    #[test]
    fn colons_keep_fuzzy_matches_in_the_requested_field() {
        for query in [
            "trmdlt:mcp:fxprm",
            ":mcp:",
            "::prm",
            "term::",
            "::",
            " : : ",
        ] {
            assert!(
                pane_search_score("Terminal Delight", &name(), query).is_some(),
                "{query}"
            );
        }
        for query in ["mcp::", "term:permissions:", "::mcp", ":::"] {
            assert!(
                pane_search_score("Terminal Delight", &name(), query).is_none(),
                "{query}"
            );
        }
        let repeated = threshing_floor::PaneName::new(&["same".into()], "same");
        assert!(pane_search_score("", &repeated, ":same:same").is_some());
        let unicode = threshing_floor::PaneName::new(&["ÜBER".into()], "修复权限");
        assert!(pane_search_score("", &unicode, ":über:修权").is_some());
    }

    #[test]
    fn age_keeps_unmeasured_and_future_timestamps_distinct_from_zero() {
        assert_eq!(session_age(None, 1000), None);
        assert_eq!(session_age(Some(1001), 1000), None);
        assert_eq!(session_age(Some(1000), 1000).as_deref(), Some("0s"));
        for (secs, expected) in [
            (59, "59s"),
            (60, "1m"),
            (3599, "59m"),
            (3660, "1h 1m"),
            (90000, "1d 1h"),
        ] {
            assert_eq!(session_age(Some(0), secs * 1000).as_deref(), Some(expected));
        }
    }

    fn seat_xy(col: usize, row: usize) -> (f32, f32) {
        (
            PAD + col as f32 * (SEAT_W + COL_GAP),
            PAD + row as f32 * (SEAT_H + ROW_GAP),
        )
    }

    #[test]
    fn an_adjacent_card_below_draws_one_straight_on_canvas_line() {
        // The reported bug: claude-warrom stacked over astra-warroom, same
        // column, one row apart. The line must be the straight next-row
        // connector, never the multi-row gutter dogleg that ran off the left
        // edge through column 0.
        let (pts, label) = wire_route(
            seat_xy(0, 0),
            seat_xy(0, 1),
            SEAT_W,
            SEAT_H,
            COL_GAP,
            ROW_GAP,
        );
        // The next-row path is the 4-point shape; the gutter path has 6.
        assert_eq!(
            pts.len(),
            4,
            "adjacent card must take the next-row path: {pts:?}"
        );
        // Same column => a single vertical line: every point shares one x.
        let x = pts[0].0;
        assert!(
            pts.iter().all(|p| (p.0 - x).abs() < 0.01),
            "not vertical: {pts:?}"
        );
        // On canvas: nothing crosses the left or top edge.
        assert!(
            pts.iter().all(|p| p.0 >= 0. && p.1 >= 0.),
            "off canvas: {pts:?}"
        );
        // The rung label sits on the line, between the two cards.
        assert!((label.0 - x).abs() < 0.01, "label off the line: {label:?}");
        let (bottom_of_top, top_of_bottom) = (seat_xy(0, 0).1 + SEAT_H, seat_xy(0, 1).1);
        assert!(
            label.1 > bottom_of_top && label.1 < top_of_bottom,
            "label not between cards: {label:?}"
        );
    }

    #[test]
    fn a_multi_row_jump_keeps_its_gutter_on_canvas() {
        // Same column, two rows down: a box sits between, so the line detours
        // into a side gutter. At column 0 the left gutter is off the canvas, so
        // it must reroute to the right one rather than go negative.
        let (pts, label) = wire_route(
            seat_xy(0, 0),
            seat_xy(0, 2),
            SEAT_W,
            SEAT_H,
            COL_GAP,
            ROW_GAP,
        );
        assert_eq!(
            pts.len(),
            6,
            "multi-row jump takes the gutter path: {pts:?}"
        );
        assert!(
            pts.iter().all(|p| p.0 >= 0.),
            "gutter left the canvas: {pts:?}"
        );
        assert!(label.0 >= 0., "label left the canvas: {label:?}");
        // A target clear of the left edge keeps the natural left-hand gutter.
        let (pts2, _) = wire_route(
            seat_xy(0, 0),
            seat_xy(1, 2),
            SEAT_W,
            SEAT_H,
            COL_GAP,
            ROW_GAP,
        );
        let left_gutter = seat_xy(1, 2).0 - COL_GAP / 2.;
        assert!(
            pts2.iter().any(|p| (p.0 - left_gutter).abs() < 0.01),
            "did not use the left gutter: {pts2:?}"
        );
    }

    #[test]
    fn a_same_row_link_runs_side_to_side() {
        let (pts, _) = wire_route(
            seat_xy(0, 0),
            seat_xy(1, 0),
            SEAT_W,
            SEAT_H,
            COL_GAP,
            ROW_GAP,
        );
        assert_eq!(
            pts.len(),
            2,
            "same-row link is a single straight segment: {pts:?}"
        );
        // Leaves the sender's right edge, enters the target's left edge.
        assert_eq!(pts[0].0, seat_xy(0, 0).0 + SEAT_W);
        assert_eq!(pts[1].0, seat_xy(1, 0).0);
        assert!(pts.iter().all(|p| p.0 >= 0. && p.1 >= 0.));
    }

    #[gpui::test]
    fn dropping_a_pane_on_a_different_org_opens_the_move_relate_popup(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            let lead = ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap();
            ws.threshing_floor_drop_on_org(&lead, other, cx);
            let drop = ws
                .floor_drop
                .as_ref()
                .expect("a drop onto a different org opens a choice, not a silent move");
            assert_eq!(drop.seat, threshing_floor::SeatId(100));
            assert_eq!(drop.org, other);
            assert!(matches!(drop.stage, DropStage::Choosing));
            // one popup at a time: the siblings were cleared.
            assert!(ws.floor_move.is_none() && ws.floor_relation.is_none());
        });
    }

    #[gpui::test]
    fn the_move_route_in_the_drop_popup_reseats_the_pane(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            let lead = ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap();
            ws.threshing_floor_drop_on_org(&lead, other, cx);
            ws.floor_drop_move(cx);
            // An unconnected pane moves straight through; no confirm, popup closed.
            assert_eq!(
                ws.floor_orgs.owner(threshing_floor::SeatId(100)),
                Some(other)
            );
            assert!(ws.floor_drop.is_none());
            assert!(ws.floor_move.is_none());
        });
    }

    #[gpui::test]
    fn the_relate_to_org_route_draws_a_whole_org_line_and_reaches_a_member(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            let cinema = ws.floor_orgs.owner(threshing_floor::SeatId(100)).unwrap();
            // Give the target org a member so the whole-org grant reaches someone.
            ws.floor_orgs.assign(threshing_floor::SeatId(101), other);
            let lead = ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap();
            ws.threshing_floor_drop_on_org(&lead, other, cx);
            ws.floor_drop_relate_org(cx);
            ws.floor_drop_set_rung(threshing_floor::Rung::Ask, cx);
            ws.floor_drop_draw(cx);
            // A whole-org line from the agent to the org — and the agent did not move.
            let lines = ws.chart.org_lines();
            assert_eq!(lines.len(), 1);
            assert_eq!(
                lines[0].from,
                threshing_floor::End::Seat(threshing_floor::SeatId(100))
            );
            assert_eq!(lines[0].to, threshing_floor::End::Org { org: other });
            assert_eq!(lines[0].rung.known(), Some(threshing_floor::Rung::Ask));
            assert_eq!(
                ws.floor_orgs.owner(threshing_floor::SeatId(100)),
                Some(cinema)
            );
            assert!(ws.floor_drop.is_none());
            // The agent now reaches the other org's member at ask.
            ws.mcp.enabled = true;
            let caller = threshing_floor::Party {
                seat: Some(threshing_floor::SeatId(100)),
                is_agent: true,
                access: threshing_floor::AgentAccess::Open,
            };
            let member = threshing_floor::Party {
                seat: Some(threshing_floor::SeatId(101)),
                is_agent: true,
                access: threshing_floor::AgentAccess::Open,
            };
            assert_eq!(
                threshing_floor::reach(
                    &ws.mcp,
                    &ws.chart,
                    &ws.floor_orgs,
                    Some(&caller),
                    &member,
                    false
                ),
                threshing_floor::Reach::Ask
            );
        });
    }

    #[gpui::test]
    fn the_relate_to_a_member_route_draws_a_cross_org_seat_line(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            // Seat 101 into the other org, so relating to it is a cross-org line.
            ws.floor_orgs.assign(threshing_floor::SeatId(101), other);
            let lead = ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap();
            ws.threshing_floor_drop_on_org(&lead, other, cx);
            ws.floor_drop_relate_member(cx);
            ws.floor_drop_pick_member(threshing_floor::SeatId(101), cx);
            ws.floor_drop_set_rung(threshing_floor::Rung::Read, cx);
            ws.floor_drop_draw(cx);
            let line = ws
                .chart
                .line(threshing_floor::SeatId(100), threshing_floor::SeatId(101))
                .expect("a cross-org seat-to-seat line");
            assert_eq!(line.rung.known(), Some(threshing_floor::Rung::Read));
            // Relating is not moving: the agent stayed in its own org.
            assert_ne!(
                ws.floor_orgs.owner(threshing_floor::SeatId(100)),
                Some(other)
            );
            assert!(ws.floor_drop.is_none());
        });
    }

    #[gpui::test]
    fn the_member_route_is_hidden_until_the_target_org_has_a_member(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            let lead = ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap();
            // Empty target org: nobody to relate an individual to.
            ws.threshing_floor_drop_on_org(&lead, other, cx);
            let empty = {
                let drop = ws.floor_drop.as_ref().unwrap();
                ws.floor_drop_members(drop).is_empty()
            };
            assert!(empty, "no member route for an empty org");
            // Give it a member and reopen: the route is offered.
            ws.floor_drop = None;
            ws.floor_orgs.assign(threshing_floor::SeatId(101), other);
            ws.threshing_floor_drop_on_org(&lead, other, cx);
            let members = {
                let drop = ws.floor_drop.as_ref().unwrap();
                ws.floor_drop_members(drop)
            };
            assert!(members == vec![threshing_floor::SeatId(101)]);
        });
    }

    #[gpui::test]
    fn the_drop_popup_paints_its_choices_and_the_draw_step(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        ws.update(cx, |ws, cx| {
            let lead = ws
                .threshing_floor_seat_leaf(threshing_floor::SeatId(100), cx)
                .unwrap();
            ws.threshing_floor_drop_on_org(&lead, other, cx);
        });
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear();
        });
        // The choice popup and its two always-present routes drew.
        bounds(cx, "floor-drop-choice");
        bounds(cx, "floor-drop-move");
        bounds(cx, "floor-drop-relate-org");
        bounds(cx, "floor-drop-cancel");
        // Step into the whole-org route: the rung picker and Draw appear.
        ws.update(cx, |ws, cx| ws.floor_drop_relate_org(cx));
        cx.update(|w, cx| {
            w.refresh();
            w.draw(cx).clear();
        });
        bounds(cx, "floor-drop-rung-Ask");
        bounds(cx, "floor-drop-draw");
        bounds(cx, "floor-drop-back");
    }

    /// A stand-in agent for the prompt tests: it asks for bracketed paste,
    /// draws Claude Code's idle footer, writes every line it is handed to
    /// `log`, and for a line carrying GO shows a working turn for a second and
    /// comes back to rest. `first` is what it draws instead of the footer, to
    /// stand in for a screen that must not be typed into.
    fn prompt_worker(
        ws: &Entity<Workspace>,
        cx: &mut gpui::VisualTestContext,
        log: &std::path::Path,
        first: &str,
    ) -> Entity<TerminalView> {
        let script = format!(
            "printf '\\033[?2004h\\033[2J\\033[H{first}\\n'; \
             while IFS= read -r l; do printf '%s\\n' \"$l\" >> '{log}'; \
             case \"$l\" in *GO*) printf '\\033[2J\\033[Hesc to interrupt\\n'; sleep 1; \
             printf '\\033[2J\\033[HFAKE_DONE\\n? for shortcuts\\n';; esac; done",
            log = log.display()
        );
        let seed = session::PaneRestore {
            program: Some(("/bin/sh".into(), vec!["-c".into(), script])),
            ..Default::default()
        };
        let worker = cx.update(|w, cx| {
            ws.update(cx, |ws, cx| {
                let pane = {
                    let _fork = crate::testsync::forks_and_locks();
                    make_pane_window_owned(seed, w, cx)
                };
                ws.tabs
                    .push(Tab::new(Node::Leaf(pane.clone()), Some("Prompted".into())));
                pane.update(cx, |v, cx| {
                    v.mode = pane::PaneMode::Claude;
                    v.seat = Some(threshing_floor::SeatId(102));
                    v.fixture_set_pane_id(12);
                    cx.notify();
                });
                ws.mcp.enabled = true;
                pane
            })
        });
        let stop = worker.read_with(cx, |v, _| v.fixture_shutdown());
        cx.on_quit(stop);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while worker.read_with(cx, |v, _| {
            !v.takes_bracketed_paste() || v.grep_grid(first, 50).is_empty()
        }) {
            assert!(
                std::time::Instant::now() < deadline,
                "the stand-in agent never drew its screen"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        }
        worker
    }

    /// A fresh, empty directory for one prompt test's log.
    fn prompt_scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("td-prompt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn prompt_ticks_until(
        ws: &Entity<Workspace>,
        cx: &mut gpui::VisualTestContext,
        id: threshing_floor::AskId,
        secs: u64,
        done: impl Fn(&crate::prompt::PromptStage) -> bool,
    ) -> crate::prompt::PromptStage {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        loop {
            cx.update(|w, cx| ws.update(cx, |ws, cx| ws.prompt_tick(w, cx)));
            cx.run_until_parked();
            let stage = ws.read_with(cx, |ws, _| ws.prompts.get(id).unwrap().stage.clone());
            if done(&stage) || std::time::Instant::now() >= deadline {
                return stage;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    fn send_prompt(
        ws: &Entity<Workspace>,
        cx: &mut gpui::VisualTestContext,
        worker: &Entity<TerminalView>,
        brief: &str,
    ) -> mcp::AskReply {
        let pid = worker.read_with(cx, |v, _| v.shell_pid().unwrap());
        ws.update(cx, |ws, cx| {
            ws.ask_bridge(
                &mcp::AskCall::Prompt(mcp::PromptRequest {
                    caller_pane: 10,
                    target: pid,
                    brief: brief.into(),
                    key: None,
                }),
                cx,
            )
        })
    }

    /// A worker the sleep has signalled but not yet recorded asleep is waited
    /// for, not refused: once its agent exits, its pane reads as a plain shell
    /// for up to one rest sweep, and a prompt sent then is a prompt for a
    /// sleeper (Codex review of the late-sleep change, 2026-10-28).
    ///
    /// Mutation-tested: dropping `rest.stopping` from `prompt_party` fails
    /// this at the send.
    #[gpui::test]
    fn a_worker_going_to_sleep_is_waited_for_not_refused(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let dir = prompt_scratch("stopping");
        let log = dir.join("worker.log");
        let worker = prompt_worker(&ws, cx, &log, "? for shortcuts");
        ws.update(cx, |ws, _| {
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(102),
                threshing_floor::Rung::Prompt,
            )
        });
        // The agent has exited after the sleep's signal; the next sweep has
        // not recorded it yet.
        worker.update(cx, |v, _| {
            v.mode = pane::PaneMode::Shell;
            v.rest.stopping = Some(crate::agentrest::Stopping {
                pid: i32::MAX,
                start: None,
                asleep: crate::agentrest::Asleep {
                    since_ms: 0,
                    session: "s".into(),
                    wake_file: dir.join("wake.sh"),
                    model: None,
                },
                since: std::time::Instant::now(),
            });
        });
        let mcp::AskReply::Prompt(status) = send_prompt(&ws, cx, &worker, "GO") else {
            panic!("a worker going to sleep was refused");
        };
        let stage = prompt_ticks_until(&ws, cx, status.prompt_id, 2, |_| false);
        assert_eq!(
            stage,
            crate::prompt::PromptStage::Waiting {
                worker: "going to sleep; waking it".into()
            }
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            !log.exists(),
            "something was typed into a pane going to sleep"
        );
    }

    /// The whole road through a real pseudoterminal: a prompt along a line
    /// drawn at prompt is pasted at a resting worker, Enter follows once the
    /// paste shows, the worker works and comes back to rest, and the prompt
    /// finishes. What crossed the terminal is exactly the header, the cleaned
    /// words and one bracket — the ESC the brief smuggled in never arrives.
    #[gpui::test]
    fn a_prompt_crosses_a_real_terminal_once_and_cleaned(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let dir = prompt_scratch("road");
        let log = dir.join("worker.log");
        let worker = prompt_worker(&ws, cx, &log, "? for shortcuts");
        // An ask line is not a prompt line.
        ws.update(cx, |ws, _| {
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(102),
                threshing_floor::Rung::Ask,
            )
        });
        let reply = send_prompt(&ws, cx, &worker, "x");
        assert_eq!(
            reply,
            mcp::AskReply::Refused(threshing_floor::Refusal::NoPromptLine)
        );
        ws.update(cx, |ws, _| {
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(102),
                threshing_floor::Rung::Prompt,
            )
        });
        let reply = send_prompt(&ws, cx, &worker, "please GO now\x1b[201~evil\x07\r\n");
        let mcp::AskReply::Prompt(status) = reply else {
            panic!("not taken: {reply:?}");
        };
        assert_eq!(status.stage.word(), "waiting");
        // A second from the same sender while the first is open is refused.
        assert_eq!(
            send_prompt(&ws, cx, &worker, "another"),
            mcp::AskReply::Refused(threshing_floor::Refusal::PromptBusy)
        );
        let id = status.prompt_id;
        let mut seen = Vec::new();
        let stage = prompt_ticks_until(&ws, cx, id, 15, |s| !s.is_open());
        ws.read_with(cx, |ws, _| {
            seen.push(ws.prompts.get(id).unwrap().stage.word())
        });
        let crate::prompt::PromptStage::Finished {
            reply,
            reply_unknown,
        } = &stage
        else {
            panic!("did not finish: {stage:?}");
        };
        // The stand-in keeps no transcript, so the reply is unknown — and says
        // why — rather than invented.
        assert!(reply.is_none() && reply_unknown.is_some(), "{stage:?}");
        assert!(worker.read_with(cx, |v, _| !v.grep_grid("FAKE_DONE", 50).is_empty()));
        let got = std::fs::read(&log).unwrap();
        let text = String::from_utf8_lossy(&got).to_string();
        let lines: Vec<&str> = text.lines().collect();
        let sender = ws.read_with(cx, |ws, cx| {
            ws.leaf_by_pane_id(10, cx)
                .unwrap()
                .read(cx)
                .shell_pid()
                .unwrap()
        });
        assert!(
            lines[0].starts_with(&format!("\x1b[200~[Threshing Floor prompt {},", id.0))
                && lines[0].contains(&format!("(pid {sender})")),
            "{text:?}"
        );
        assert_eq!(lines.get(1), Some(&""), "{text:?}");
        assert_eq!(
            lines.get(2),
            Some(&"please GO now[201~evil\x1b[201~"),
            "{text:?}"
        );
        assert_eq!(lines.len(), 3, "one prompt, one Enter: {text:?}");
        assert_eq!(text.matches("\x1b[201~").count(), 1, "{text:?}");
        assert_eq!(text.matches("\x1b[200~").count(), 1, "{text:?}");
        assert!(!text.contains('\x07'), "{text:?}");
    }

    /// A worker showing a question for its person is never typed into, and a
    /// line erased while the prompt waits ends it with nothing typed. A pane
    /// made read-only is refused at the send and again at the write.
    #[gpui::test]
    fn a_prompt_never_types_into_a_dialog_or_after_its_line_is_erased(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let dir = prompt_scratch("dialog");
        let log = dir.join("worker.log");
        let worker = prompt_worker(
            &ws,
            cx,
            &log,
            "Do you trust the files in this folder? \u{276f} 1. Yes",
        );
        assert_eq!(
            worker.read_with(cx, |v, _| v.prompt_seen()),
            crate::prompt::Seen::Blocked
        );
        ws.update(cx, |ws, _| {
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(102),
                threshing_floor::Rung::Prompt,
            )
        });
        let mcp::AskReply::Prompt(status) = send_prompt(&ws, cx, &worker, "GO") else {
            panic!("not taken");
        };
        let id = status.prompt_id;
        let stage = prompt_ticks_until(&ws, cx, id, 2, |_| false);
        assert_eq!(stage.word(), "waiting", "{stage:?}");
        ws.update(cx, |ws, _| {
            ws.chart
                .erase(threshing_floor::SeatId(100), threshing_floor::SeatId(102))
        });
        let stage = prompt_ticks_until(&ws, cx, id, 2, |s| !s.is_open());
        let crate::prompt::PromptStage::Failed { why } = &stage else {
            panic!("not failed: {stage:?}");
        };
        assert!(why.contains("prompt"), "{why}");
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!log.exists(), "something was typed into the dialog");

        // Read-only: refused when sent, and the pane's own write refuses too.
        ws.update(cx, |ws, cx| {
            ws.chart.draw(
                threshing_floor::SeatId(100),
                threshing_floor::SeatId(102),
                threshing_floor::Rung::Prompt,
            );
            worker.update(cx, |v, _| {
                v.agent_access = threshing_floor::AgentAccess::ReadOnly
            });
        });
        assert_eq!(
            send_prompt(&ws, cx, &worker, "GO"),
            mcp::AskReply::Refused(threshing_floor::Refusal::ReadOnly)
        );
        let typed = worker.update(cx, |v, _| {
            (
                v.type_prompt(crate::prompt::Keys::Paste("GO")),
                v.type_prompt(crate::prompt::Keys::Enter),
            )
        });
        assert_eq!(typed, (false, false));
        // And a pane that is not an agent takes nothing, Open or not.
        let typed = worker.update(cx, |v, _| {
            v.agent_access = threshing_floor::AgentAccess::Open;
            v.mode = pane::PaneMode::Shell;
            v.type_prompt(crate::prompt::Keys::Enter)
        });
        assert!(!typed);
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert!(!log.exists(), "a refused write reached the terminal");
    }

    /// A pane rebuilt from a snapshot, or an agent woken into a fresh shell,
    /// keeps the seat a person gave it and the mark a person set. Before,
    /// both came back empty: unseated, every line gone, and off-limits open.
    #[gpui::test]
    fn a_rebuilt_pane_keeps_its_seat_and_its_mark(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        for access in [
            threshing_floor::AgentAccess::OffLimits,
            threshing_floor::AgentAccess::ReadOnly,
        ] {
            let (old, fresh) = cx.update(|w, cx| {
                ws.update(cx, |ws, cx| {
                    let old = ws.leaf_by_pane_id(11, cx).unwrap();
                    old.update(cx, |v, _| v.agent_access = access);
                    let fresh = {
                        let _fork = crate::testsync::forks_and_locks();
                        make_pane_window_owned(session::PaneRestore::default(), w, cx)
                    };
                    (old, fresh)
                })
            });
            let stop = fresh.read_with(cx, |v, _| v.fixture_shutdown());
            cx.on_quit(stop);
            assert_eq!(
                fresh.read_with(cx, |v, _| (v.seat, v.agent_access)),
                (None, threshing_floor::AgentAccess::Open),
                "the control: a fresh pane starts unseated and open"
            );
            let carried = old.read_with(cx, |v, _| v.presentation());
            fresh.update(cx, |v, cx| v.adopt_presentation(carried, cx));
            assert_eq!(
                fresh.read_with(cx, |v, _| (v.seat, v.agent_access)),
                (Some(threshing_floor::SeatId(101)), access)
            );
        }
    }
}
