//! The War Room panel: seats a person placed, the lines between them, what
//! each seat is doing, and the window's MCP switches.
//!
//! The rules it draws live in `warroom`; this file only gathers what the
//! window already knows about each seat (the agent counter's state, the size,
//! age and model vitals read from the transcript) and turns a person's gestures
//! into membership and directed lines. Seating
//! and drawing happen here and nowhere else — see
//! `only_the_war_room_and_the_rail_draw_lines`.

use super::*;
use crate::surfacefeed::now_ms;
use gpui::{anchored, deferred, PathBuilder};

/// A seat's box at scale 1: place, title, program, state, age and tokens.
const SEAT_W: f32 = 248.;
const SEAT_H: f32 = 126.;
const COL_GAP: f32 = 30.;
const ROW_GAP: f32 = 56.;
const PAD: f32 = 14.;

/// Everything one seat shows, gathered from facts the window already has.
struct PaneFacts {
    leaf: Entity<TerminalView>,
    seat: Option<warroom::SeatId>,
    name: warroom::PaneName,
    project: String,
    program: String,
    model: Option<String>,
    size: Option<u64>,
    started_ms: Option<i64>,
    state: hud::AgentState,
    access: warroom::AgentAccess,
}

/// A wire the canvas is ready to stroke: its polyline, whether it bypasses a
/// copy's press limit (drawn dashed amber), and whether its target is off-limits
/// (drawn faint and dashed, granting nothing).
type WireStroke = (Vec<(f32, f32)>, bool, bool);

/// One drawn connection: a polyline in chart coordinates, how it is inked,
/// and where its label chip sits.
struct Wire {
    from: warroom::SeatId,
    to: warroom::SeatId,
    points: Vec<(f32, f32)>,
    bypass: bool,
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
    from: warroom::SeatId,
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
                .id("war-drag-preview")
                .debug_selector(|| "war-drag-preview".to_string())
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

#[derive(Clone, Copy)]
enum RelationEnd {
    From,
    To,
}

pub(crate) struct RelationEditor {
    draft: warroom::RelationDraft,
    choosing: Option<RelationEnd>,
}

/// Spaces match independent words anywhere in the name. Colons address
/// project:tab:pane; empty segments leave that field unrestricted.
fn pane_search_score(project: &str, name: &warroom::PaneName, query: &str) -> Option<i64> {
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
    /// The War Room, when it is open. Drawn flat over the panes (see the
    /// warped-surface table), closed by a click outside it or by Esc.
    pub(crate) fn render_war_room(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        if !self.war_room {
            return None;
        }
        let sk = skin::for_theme(cx, th, th.grade.scale);
        let ui = self.lang.strings().ui;
        let lang = self.lang;
        let dragging = self.war_drag.as_ref().filter(|_| cx.has_active_drag());
        let removing = dragging.is_some_and(|leaf| leaf.read(cx).seat.is_some());
        let drag_active = dragging.is_some();
        let chart_active = drag_active && self.war_org.is_some();

        // ---- gather: seated panes, and every pane off the chart ----
        let mut seats: Vec<PaneFacts> = Vec::new();
        let mut loose: Vec<PaneFacts> = Vec::new();
        let query = self.war_room_query.text();
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
                let name = warroom::PaneName::new(&spine, &v.header_title());
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
                    size: vit.and_then(|x| x.tokens),
                    started_ms: vit.and_then(|x| x.started_ms),
                    state: v.agent_status().state,
                    access: v.agent_access,
                };
                let org = facts.seat.and_then(|seat| self.war_orgs.owner(seat));
                if org.is_some() && org == self.war_org {
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
            .war_org
            .and_then(|id| self.war_orgs.get(id))
            .into_iter()
            .flat_map(|o| o.seats.iter())
            .copied()
            .filter(|seat| query.trim().is_empty() && self.war_room_seat_leaf(*seat, cx).is_none())
            .collect();
        let mut keys: Vec<(warroom::SeatId, String, String)> = seats
            .iter()
            .filter_map(|f| f.seat.map(|seat| (seat, f.project.clone(), f.name.full())))
            .collect();
        keys.extend(
            missing
                .iter()
                .map(|seat| (*seat, String::new(), ui.wr_unavailable.to_string())),
        );
        let placed = warroom::layout(&keys, &self.chart);
        let at = |seat: warroom::SeatId| -> Option<(f32, f32)> {
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

        let rung_word = |r: warroom::Rung| match r {
            warroom::Rung::See => ui.wr_rung_see,
            warroom::Rung::Read => ui.wr_rung_read,
            warroom::Rung::Ask => ui.wr_rung_ask,
        };
        let wires: Vec<Wire> = self
            .chart
            .saved()
            .iter()
            .filter_map(|l| {
                let (fx, fy) = at(l.from)?;
                let (tx, ty) = at(l.to)?;
                let label = match (l.rung.known(), l.prompts_left) {
                    (Some(r), None) => rung_word(r).to_string(),
                    (Some(r), Some(n)) => format!(
                        "{} · {}",
                        rung_word(r),
                        lang::fill(ui.wr_bypass, &[("n", &n.to_string())])
                    ),
                    // A word this build cannot read: drawn, unknown, granting nothing.
                    (None, _) => format!("? · {}", ui.wr_unknown),
                };
                let (points, label_at) = wire_route((fx, fy), (tx, ty), sw, sh, cg, rg);
                Some(Wire {
                    from: l.from,
                    to: l.to,
                    points,
                    bypass: l.is_bypass(),
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
            ui.wr_counts,
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
        let off_limits: std::collections::HashSet<warroom::SeatId> = seats
            .iter()
            .filter(|f| matches!(f.access, warroom::AgentAccess::OffLimits))
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

        let pick = self.war_room_pick;
        let mut chart = div()
            .relative()
            .flex_none()
            .w(px(chart_w))
            .h(px(chart_h))
            .child(canvas);
        if seats.is_empty() && query.trim().is_empty() {
            chart = chart.child(
                div()
                    .absolute()
                    .left(px(pad))
                    .top(px(pad))
                    .w(px(chart_w - pad * 2.))
                    .text_color(th.text.alpha(0.6))
                    .child(SharedString::from(
                        if self.war_org.is_some() {
                            ui.wr_org_drop
                        } else {
                            ui.wr_org_empty
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
                warroom::Role::Orchestrator => ui.wr_role_orchestrator,
                warroom::Role::Manager => ui.wr_role_manager,
                warroom::Role::Worker => ui.wr_role_worker,
                warroom::Role::Alone => ui.wr_role_alone,
            };
            let second = match (&f.model, f.access) {
                // Off-limits is the strongest position and must show on the card,
                // not fall through to a plain model/role line that reads normal:
                // the person set it to hide the pane from every agent, line or no.
                (_, warroom::AgentAccess::OffLimits) => format!(
                    "{} · {}",
                    f.program,
                    self.lang.strings().parts.pane_off_limits_badge
                ),
                (_, warroom::AgentAccess::ReadOnly) => format!(
                    "{} · {}",
                    f.program,
                    self.lang.strings().parts.pane_read_only_badge
                ),
                (Some(m), _) => format!("{} · {m}", f.program),
                (None, _) => format!("{} · {role}", f.program),
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
                    .id(SharedString::from(format!("war-seat-{}", seat.0)))
                    .debug_selector(move || format!("war-seat-{}", seat.0))
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(sw))
                    .h(px(sh))
                    .px(px(10. * s))
                    .py(px(6. * s))
                    .rounded(sk.rad_raw(7.))
                    .border_1()
                    .border_color(if picked {
                        th.complement
                    } else {
                        th.text.alpha(0.28)
                    })
                    .bg(darken(th.surface, 0.35))
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .cursor(gpui::CursorStyle::OpenHand)
                    .when(dragging == Some(&f.leaf), |d| d.opacity(0.35))
                    .hover(|d| d.border_color(th.accent))
                    .on_drag(pane_drag, {
                        let ws = cx.entity().downgrade();
                        move |d: &PaneDrag, position, window: &mut Window, cx: &mut App| {
                            let _ =
                                ws.update(cx, |ws, cx| ws.war_room_begin_drag(&d.leaf, window, cx));
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
                                ws.war_context = Some((leaf.clone(), ev.position));
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
                        ws.war_room_begin_relation(Some(d.from), Some(seat), cx);
                    }))
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
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(second)),
                    )
                    .child(
                        div()
                            .text_size(sk.text_px(8.5))
                            .text_color(state_ink)
                            .child(SharedString::from(format!(
                                "\u{25cf} {}",
                                f.state.label_in(lang)
                            ))),
                    )
                    .child(self.war_room_metrics(f, &sk, th))
                    .child(
                        div()
                            .id(SharedString::from(format!("war-connect-{}", seat.0)))
                            .debug_selector(move || format!("war-connect-{}", seat.0))
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
                                ws.war_room_begin_relation(Some(seat), None, cx);
                            })),
                    )
                    .on_click(cx.listener(move |ws, _, _w, cx| {
                        cx.stop_propagation();
                        ws.war_room_click_seat(seat, cx);
                    })),
            );
        }
        for seat in missing {
            let Some((x, y)) = at(seat) else {
                continue;
            };
            chart = chart.child(
                div()
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .w(px(sw))
                    .h(px(sh))
                    .p_2()
                    .border_1()
                    .border_color(th.text.alpha(0.3))
                    .rounded(sk.rad_raw(7.))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .text_color(th.text.alpha(0.6))
                    .child(SharedString::from(ui.wr_unavailable.to_string()))
                    .child(SharedString::from(format!(
                        "{} {} · {} {}",
                        ui.wr_age, ui.wr_unknown, ui.wr_tokens, ui.wr_unknown
                    )))
                    .child(
                        Self::bezel_btn(&sk, ui.wr_unseat_btn, false)
                            .id(SharedString::from(format!("war-forget-{}", seat.0)))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.chart.forget_seat(seat);
                                ws.war_orgs.remove(seat);
                                ws.save(cx);
                                cx.notify();
                            })),
                    ),
            );
        }
        for w in &wires {
            let (from, to) = (w.from, w.to);
            // Sized to its words, and kept inside the chart's own edges.
            let lw = (w.label.chars().count() as f32 * 6.4 + 16.) * s;
            let lx = (w.label_at.0 - lw / 2.).clamp(2., (chart_w - lw - 2.).max(2.));
            chart = chart.child(
                div()
                    .id(SharedString::from(format!("war-line-{}-{}", from.0, to.0)))
                    .debug_selector(move || format!("war-line-{}-{}", from.0, to.0))
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
                            .border_color(if w.bypass {
                                bypass_ink.alpha(0.8)
                            } else {
                                line_ink.alpha(0.6)
                            })
                            .text_size(sk.text_px(8.))
                            .text_color(if w.bypass { bypass_ink } else { line_ink })
                            .cursor_pointer()
                            .child(SharedString::from(w.label.clone())),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.war_room_click_line(from, to, cx);
                        }),
                    ),
            );
        }

        // ---- the pane pool ----
        let mut off = div().w(px(sw)).flex_none().flex().flex_col().gap_2();
        for f in &loose {
            let leaf = f.leaf.clone();
            let is_agent = leaf.read(cx).mode.is_agent();
            let owner = f.seat.and_then(|s| self.war_orgs.owner(s));
            let pane_drag = PaneDrag {
                leaf: leaf.clone(),
                title: f.name.tab.clone(),
                ink: th.accent,
                bg: th.surface,
            };
            off = off.child(
                div()
                    .id(SharedString::from(format!(
                        "war-pane-{}",
                        leaf.entity_id().as_u64()
                    )))
                    .debug_selector({
                        let id = leaf.entity_id().as_u64();
                        move || format!("war-pane-{id}")
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
                                        ws.war_room_begin_drag(&d.leaf, window, cx)
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
                                ws.war_context = Some((leaf.clone(), ev.position));
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
                    .child(self.war_room_metrics(f, &sk, th))
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
                                .child(SharedString::from(ui.wr_org_unknown.to_string())),
                        )
                    })
                    .when_some(owner, |d, org| {
                        d.child(
                            div()
                                .text_size(sk.text_px(8.5))
                                .text_color(th.accent)
                                .child(SharedString::from(self.war_room_org_label(org))),
                        )
                    })
                    .when(is_agent && owner.is_some(), |d| {
                        let org = owner.unwrap();
                        d.child(
                            Self::bezel_btn(&sk, ui.wr_org_open, false)
                                .id(SharedString::from(format!(
                                    "war-open-org-{}",
                                    leaf.entity_id().as_u64()
                                )))
                                .debug_selector(move || format!("war-open-org-{}", org.0))
                                .on_click(cx.listener(move |ws, _, _, cx| {
                                    cx.stop_propagation();
                                    ws.war_room_set_org(org, cx);
                                })),
                        )
                    })
                    .when(is_agent && owner.is_none() && self.war_org.is_some(), |d| {
                        d.child(
                            Self::bezel_btn(&sk, ui.wr_org_add, false)
                                .id(SharedString::from(format!(
                                    "war-seat-btn-{}",
                                    leaf.entity_id().as_u64()
                                )))
                                .hover(|d| d.border_color(th.accent))
                                .on_click(cx.listener(move |ws, _, _w, cx| {
                                    cx.stop_propagation();
                                    ws.war_room_seat(&leaf, cx);
                                })),
                        )
                    }),
            );
        }

        let switches = self.mcp_switch_buttons(&sk, th, "war", cx);
        // Headers are siblings of scroll bodies. Zero minimum height on every
        // flex ancestor prevents the contents from stretching the frame.
        let header = div()
            .id("war-room-header")
            .debug_selector(|| "war-room-header".to_string())
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
                            .child(SharedString::from(ui.wr_hdr.to_string())),
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
            .child(
                div()
                    .id("war-room-search")
                    .debug_selector(|| "war-room-search".to_string())
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
                        &self.war_room_query,
                        s,
                        th.text,
                        th.accent,
                        th.accent.alpha(0.3),
                    ))
                    .when(query.is_empty(), |d| {
                        d.child(
                            div()
                                .text_color(th.text.alpha(0.55))
                                .child(SharedString::from(ui.wr_search.to_string())),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        Self::bezel_btn(&sk, "×", false)
                            .id("war-room-clear")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.war_room_query = EditBuffer::seeded("");
                                    ws.war_room_pick = None;
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
                        ui.wr_results,
                        &[("shown", &shown.to_string()), ("total", &total.to_string())],
                    ))),
            )
            .when(shown == 0 && !query.trim().is_empty(), |d| {
                d.child(
                    div()
                        .p_2()
                        .child(SharedString::from(ui.wr_no_matches.to_string())),
                )
            });
        let org_header = div()
            .id("war-org-header")
            .debug_selector(|| "war-org-header".to_string())
            .flex_none()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap_2()
            .child(self.war_room_org_controls(&sk, th, cx))
            .child(
                div()
                    .text_size(sk.text_px(8.5))
                    .text_color(th.accent.alpha(0.85))
                    .child(SharedString::from(
                        if drag_active {
                            ui.wr_drag_hint
                        } else {
                            ui.wr_hint
                        }
                        .to_string(),
                    )),
            )
            .child(self.war_room_relationship_controls(&sk, th, cx))
            .children(self.ask_numbers.as_ref().map(|n| {
                div()
                    .text_size(sk.text_px(8.5))
                    .text_color(if n.unlined_answered > 0 {
                        th.ansi[1]
                    } else {
                        th.text.alpha(0.6)
                    })
                    .child(SharedString::from(lang::fill(
                        ui.wr_asks,
                        &[
                            ("cheap", &n.cheap_answers.to_string()),
                            ("copies", &n.copy_answers.to_string()),
                            ("unlined", &n.unlined_answered.to_string()),
                        ],
                    )))
            }));
        let drop_chart_ink = th.accent;
        let chart_tile = div()
            .id("war-org-tile")
            .debug_selector(|| "war-org-tile".to_string())
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
            .children(self.war_org.map(|id| {
                div()
                    .id("war-org-title")
                    .debug_selector(|| "war-org-title".to_string())
                    .flex_none()
                    .pb_1()
                    .border_b_1()
                    .border_color(th.ansi[6].alpha(0.35))
                    .text_size(sk.text_px(13.))
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_color(th.ansi[6])
                    .child(SharedString::from(self.war_room_org_label(id)))
            }))
            .child(
                div()
                    .id("war-room-chart")
                    .debug_selector(|| "war-room-chart".to_string())
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
                        let has_org = self.war_org.is_some();
                        move |_, _, _| has_org
                    })
                    .on_drop(cx.listener(|ws, d: &PaneDrag, _, cx| {
                        if let Some(org) = ws.war_org {
                            ws.war_room_assign(&d.leaf, org, false, cx);
                        }
                    }))
                    .child(chart),
            );
        let pool_tile = div()
            .id("war-pool-tile")
            .debug_selector(|| "war-pool-tile".to_string())
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
            .on_drop(cx.listener(|ws, d: &PaneDrag, _, cx| ws.war_room_unseat(&d.leaf, cx)))
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .id("war-pool-header")
                    .debug_selector(|| "war-pool-header".to_string())
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(th.text.alpha(0.85))
                            .child(SharedString::from(ui.wr_off_hdr.to_string())),
                    )
                    .child(
                        div()
                            .text_size(sk.text_px(8.5))
                            .text_color(th.text.alpha(0.6))
                            .child(SharedString::from(
                                if removing {
                                    ui.wr_drop_remove
                                } else {
                                    ui.wr_off_blurb
                                }
                                .to_string(),
                            )),
                    ),
            )
            .child(
                div()
                    .id("war-room-pool")
                    .debug_selector(|| "war-room-pool".to_string())
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .overflow_x_scroll()
                    .overflow_y_scroll()
                    .child(off),
            );
        let panel = div()
            .id("war-room-panel")
            .debug_selector(|| "war-room-panel".to_string())
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
                    if ws.war_room_pick.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .map(|d| sk.chrome(sk.finish(d)))
            .child(header)
            .child(
                div()
                    .id("war-room-tiles")
                    .debug_selector(|| "war-room-tiles".to_string())
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
            .child(self.war_room_drag_footer(removing, drag_active, &sk, th, cx));

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
                    if ws.war_drag.take().is_some() {
                        cx.notify();
                    }
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.war_context = None;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        ws.close_war_room();
                        ws.focus_active(window, cx);
                        cx.notify();
                    }),
                )
                .child(panel)
                .children(self.war_room_context_menu(&sk, th, cx)),
        )
    }

    fn war_room_begin_drag(
        &mut self,
        leaf: &Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.war_drag = Some(leaf.clone());
        self.war_context = None;
        self.war_feedback = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    fn war_room_drag_footer(
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
                        ui.wr_drag_hint.to_string()
                    } else {
                        self.war_feedback.clone().unwrap_or_default()
                    })),
            )
            .when(removing, |d| {
                d.child(
                    div()
                        .id("war-remove-drop")
                        .debug_selector(|| "war-remove-drop".to_string())
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
                        .on_drop(
                            cx.listener(|ws, d: &PaneDrag, _, cx| ws.war_room_unseat(&d.leaf, cx)),
                        )
                        .child(trash_icon(ink))
                        .child(
                            div()
                                .min_w(px(0.))
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(div().text_color(ink).child(ui.wr_remove))
                                .child(
                                    div()
                                        .text_size(sk.text_px(8.))
                                        .text_color(th.text.alpha(0.7))
                                        .child(ui.wr_access_off),
                                ),
                        ),
                )
            })
    }

    fn war_room_context_menu(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (leaf, pos) = self.war_context.as_ref()?;
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
            .id("war-context-menu")
            .debug_selector(|| "war-context-menu".to_string())
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
                ws.war_context = None;
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
                row(ui.wr_remove.to_string())
                    .id("war-context-remove")
                    .debug_selector(|| "war-context-remove".to_string())
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.war_room_unseat(&leaf, cx);
                    })),
            );
        }
        for (access, label, id) in [
            (
                warroom::AgentAccess::Open,
                parts.pane_ctx_let_agents_see,
                "war-context-open",
            ),
            (
                warroom::AgentAccess::ReadOnly,
                parts.pane_ctx_read_only,
                "war-context-read-only",
            ),
            (
                warroom::AgentAccess::OffLimits,
                parts.pane_ctx_off_limits,
                "war-context-off-limits",
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
                        ws.war_context = None;
                        ws.save(cx);
                        cx.notify();
                    })),
            );
        }
        Some(deferred(anchored().position(*pos).snap_to_window().child(menu)).into_any_element())
    }

    /// Modal keyboard capture keeps all editing away from the underlying PTY.
    pub(crate) fn war_room_capture_key(
        &mut self,
        key: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.war_room {
            return false;
        }
        if key.key == "escape" {
            if cx.stop_active_drag(window) {
                self.war_drag = None;
                cx.notify();
                return true;
            }
            if self.war_context.take().is_some() {
                cx.notify();
                return true;
            }
            if self.war_move.take().is_some()
                || self.war_org_edit.take().is_some()
                || self.war_relation.take().is_some()
            {
            } else if !self.war_room_query.text().is_empty() {
                self.war_room_query = EditBuffer::seeded("");
                self.war_room_pick = None;
            } else {
                self.close_war_room();
                self.focus_active(window, cx);
            }
            cx.notify();
            return true;
        }
        if cx.has_active_drag() {
            return true;
        }
        if self.war_context.is_some() {
            return true;
        }
        if key.key == "delete" && self.war_relation.is_none() && self.war_org_edit.is_none() {
            if let Some(leaf) = self
                .war_room_pick
                .and_then(|seat| self.war_room_seat_leaf(seat, cx))
            {
                self.war_room_unseat(&leaf, cx);
                return true;
            }
        }
        if key.key == "enter" {
            if self.war_org_edit.is_some() {
                self.war_room_save_org(cx);
            } else if self.war_relation.is_some() {
                self.war_room_save_relation(cx);
            }
            return true;
        }
        if self.war_relation.is_some() || self.war_move.is_some() {
            return true;
        }
        let editing_org = self.war_org_edit.is_some();
        let buffer = match &mut self.war_org_edit {
            Some((_, text)) => text,
            None => &mut self.war_room_query,
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
            self.war_room_pick = None;
        }
        cx.notify();
        true
    }

    fn war_room_metrics(&self, f: &PaneFacts, sk: &skin::Skin, th: &theme::Theme) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let age =
            session_age(f.started_ms, now_ms() as i64).unwrap_or_else(|| ui.wr_unknown.to_string());
        let tokens = f
            .size
            .map(hud::fmt_tokens)
            .unwrap_or_else(|| ui.wr_unknown.to_string());
        div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap_2()
            .text_size(sk.text_px(8.5))
            .text_color(th.text.alpha(0.85))
            .child(SharedString::from(format!("{} {age}", ui.wr_age)))
            .child(SharedString::from(format!("{} {tokens}", ui.wr_tokens)))
    }

    fn war_room_org_label(&self, id: warroom::OrgId) -> String {
        self.war_orgs
            .get(id)
            .and_then(|o| o.name.clone())
            .unwrap_or_else(|| self.lang.strings().ui.wr_org_unnamed.to_string())
    }

    fn war_room_set_org(&mut self, id: warroom::OrgId, cx: &mut Context<Self>) {
        if self.war_orgs.get(id).is_none() {
            return;
        }
        self.war_org = Some(id);
        // The pane filter is the person's, not the view's: seating or moving a
        // pane (which lands here) must not wipe what they typed. Clearing is
        // theirs to do — the × button and Esc. See also begin_relation/unseat.
        self.war_room_pick = None;
        self.war_relation = None;
        self.war_org_edit = None;
        self.save(cx);
        cx.notify();
    }

    fn war_room_save_org(&mut self, cx: &mut Context<Self>) {
        let Some((id, buffer)) = &self.war_org_edit else {
            return;
        };
        let text = buffer.text();
        let org = match id {
            Some(id) => self.war_orgs.rename(*id, &text).then_some(*id),
            None => self.war_orgs.create(&text),
        };
        if let Some(org) = org {
            self.war_room_set_org(org, cx);
        }
    }

    fn war_room_org_controls(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let dragging = self.war_drag.is_some() && cx.has_active_drag();
        let mut choices = div()
            .flex()
            .flex_row()
            .flex_none()
            .items_center()
            .gap_1()
            .child(SharedString::from(ui.wr_org.to_string()));
        for org in self.war_orgs.saved() {
            let id = org.id;
            let ink = th.accent;
            choices = choices.child(
                Self::bezel_btn(sk, &self.war_room_org_label(id), self.war_org == Some(id))
                    .id(SharedString::from(format!("war-org-{}", id.0)))
                    .debug_selector(move || format!("war-org-{}", id.0))
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
                        ws.war_room_assign(&drag.leaf, id, false, cx);
                    }))
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.war_room_set_org(id, cx);
                    })),
            );
        }
        let mut actions = div().flex().flex_row().flex_none().gap_1().child(
            Self::bezel_btn(sk, ui.wr_org_new, false)
                .id("war-org-new")
                .debug_selector(|| "war-org-new".to_string())
                .on_click(cx.listener(|ws, _, w, cx| {
                    cx.stop_propagation();
                    ws.war_org_edit = Some((None, EditBuffer::seeded("")));
                    w.focus(&ws.focus_handle, cx);
                    cx.notify();
                })),
        );
        if let Some(id) = self.war_org {
            actions = actions.child(
                Self::bezel_btn(sk, ui.wr_org_rename, false)
                    .id("war-org-rename")
                    .debug_selector(|| "war-org-rename".to_string())
                    .on_click(cx.listener(move |ws, _, w, cx| {
                        cx.stop_propagation();
                        let name = ws
                            .war_orgs
                            .get(id)
                            .and_then(|o| o.name.as_deref())
                            .unwrap_or("");
                        ws.war_org_edit = Some((Some(id), EditBuffer::seeded(name)));
                        w.focus(&ws.focus_handle, cx);
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
                        .id("war-org-choices")
                        .min_w(px(0.))
                        .flex_1()
                        .overflow_x_scroll()
                        .child(choices),
                )
                .child(actions.when(dragging, |d| d.opacity(0.3))),
        );
        if let Some((_, buffer)) = &self.war_org_edit {
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
                        Self::bezel_btn(sk, ui.wr_org_save, false)
                            .id("war-org-save")
                            .debug_selector(|| "war-org-save".to_string())
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.war_room_save_org(cx);
                            })),
                    )
                    .child(
                        Self::bezel_btn(sk, ui.dlg_cancel, false)
                            .id("war-org-cancel")
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.war_org_edit = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        controls
    }

    fn war_room_assign(
        &mut self,
        leaf: &Entity<TerminalView>,
        org: warroom::OrgId,
        confirmed: bool,
        cx: &mut Context<Self>,
    ) {
        if self.war_orgs.get(org).is_none() || !leaf.read(cx).mode.is_agent() {
            return;
        }
        let old = leaf.read(cx).seat;
        if old.is_some_and(|seat| self.war_orgs.owner(seat) == Some(org)) {
            self.war_room_set_org(org, cx);
            return;
        }
        let connected = old.is_some_and(|seat| {
            self.chart
                .saved()
                .iter()
                .any(|l| l.from == seat || l.to == seat)
        });
        if connected && !confirmed {
            self.war_move = Some((leaf.clone(), org));
            cx.notify();
            return;
        }
        let seat = old.unwrap_or_else(warroom::SeatId::mint);
        if old.is_some() {
            self.chart.forget_seat(seat);
            self.war_room_forget_pending(leaf, seat, cx);
        }
        self.war_orgs.assign(seat, org);
        leaf.update(cx, |v, cx| v.set_seat(Some(seat), cx));
        self.war_move = None;
        self.war_room_set_org(org, cx);
    }

    fn war_room_seat_leaf(&self, seat: warroom::SeatId, cx: &App) -> Option<Entity<TerminalView>> {
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

    fn war_room_seat_label(&self, seat: warroom::SeatId, cx: &App) -> String {
        self.war_room_seat_leaf(seat, cx)
            .as_ref()
            .map(|leaf| self.pane_name(leaf, cx).full())
            .unwrap_or_else(|| self.lang.strings().ui.wr_unknown.to_string())
    }

    fn war_room_begin_relation(
        &mut self,
        from: Option<warroom::SeatId>,
        to: Option<warroom::SeatId>,
        cx: &mut Context<Self>,
    ) {
        if from.is_some_and(|s| self.war_orgs.owner(s) != self.war_org) {
            return;
        }
        if to.is_some_and(|s| self.war_orgs.owner(s) != self.war_org || Some(s) == from) {
            return;
        }
        let rung = from
            .zip(to)
            .and_then(|(f, t)| self.chart.line(f, t))
            .and_then(|l| l.rung.known());
        self.war_relation = Some(RelationEditor {
            draft: warroom::RelationDraft { from, to, rung },
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
        self.war_room_pick = None;
        cx.notify();
    }

    fn war_room_choose_endpoint(&mut self, seat: warroom::SeatId, cx: &mut Context<Self>) {
        if self.war_orgs.owner(seat) != self.war_org {
            return;
        }
        if let Some(editor) = &mut self.war_relation {
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

    fn war_room_save_relation(&mut self, cx: &mut Context<Self>) {
        let ready = self
            .war_relation
            .as_ref()
            .and_then(|e| e.draft.ready(&self.war_orgs));
        if let Some((from, to, rung)) = ready {
            if self.war_orgs.owner(from) != self.war_org
                || self.war_room_seat_leaf(from, cx).is_none()
                || self.war_room_seat_leaf(to, cx).is_none()
            {
                return;
            }
            self.chart.draw(from, to, rung);
            self.war_room_forget_pair(from, to, cx);
            self.war_relation = None;
            self.save(cx);
            cx.notify();
        }
    }

    fn war_room_relationship_controls(
        &self,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let mut controls = div().flex().flex_col().gap_2();
        if let Some((leaf, org)) = &self.war_move {
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
                ui.wr_move_hint,
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
                        self.war_room_org_label(org)
                    )))
                    .child(
                        Self::bezel_btn(sk, ui.wr_move_confirm, false)
                            .id("war-move-confirm")
                            .debug_selector(|| "war-move-confirm".to_string())
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.war_room_assign(&leaf, org, true, cx);
                            })),
                    )
                    .child(
                        Self::bezel_btn(sk, ui.dlg_cancel, false)
                            .id("war-move-cancel")
                            .debug_selector(|| "war-move-cancel".to_string())
                            .on_click(cx.listener(|ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.war_move = None;
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
                Self::bezel_btn(sk, ui.wr_rel_connect, self.war_relation.is_some())
                    .id("war-connect")
                    .debug_selector(|| "war-connect".to_string())
                    .on_click(cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.war_room_begin_relation(ws.war_room_pick, None, cx);
                    })),
            );
        if let Some(seat) = self.war_room_pick {
            if let Some(leaf) = self.war_room_seat_leaf(seat, cx) {
                let n = self
                    .chart
                    .saved()
                    .iter()
                    .filter(|l| l.from == seat || l.to == seat)
                    .count();
                let label = lang::fill(ui.wr_org_remove, &[("n", &n.to_string())]);
                row = row
                    .child(SharedString::from(self.war_room_seat_label(seat, cx)))
                    .child(
                        Self::bezel_btn(sk, &label, false)
                            .id("war-unseat")
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.war_room_unseat(&leaf, cx);
                            })),
                    );
            }
        }
        controls = controls.child(row);
        let Some(editor) = &self.war_relation else {
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
            .child(SharedString::from(ui.wr_rel.to_string()));
        let mut ends = div().flex().flex_row().flex_wrap().gap_2();
        for (end, seat, label, id) in [
            (
                RelationEnd::From,
                editor.draft.from,
                ui.wr_rel_from,
                "war-rel-from",
            ),
            (RelationEnd::To, editor.draft.to, ui.wr_rel_to, "war-rel-to"),
        ] {
            let value = seat
                .map(|s| self.war_room_seat_label(s, cx))
                .unwrap_or_else(|| ui.wr_unknown.to_string());
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
                        if let Some(e) = &mut ws.war_relation {
                            e.choosing = Some(end);
                        }
                        cx.notify();
                    })),
            );
        }
        form = form.child(ends);
        if let Some(end) = editor.choosing {
            let mut list = div()
                .id("war-rel-endpoints")
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
                    if self.war_orgs.owner(seat) != self.war_org
                        || matches!(end, RelationEnd::To) && editor.draft.from == Some(seat)
                    {
                        continue;
                    }
                    list = list.child(
                        Self::bezel_btn(sk, &self.war_room_seat_label(seat, cx), false)
                            .id(SharedString::from(format!("war-endpoint-{}", seat.0)))
                            .on_click(cx.listener(move |ws, _, _, cx| {
                                cx.stop_propagation();
                                ws.war_room_choose_endpoint(seat, cx);
                            })),
                    );
                }
            }
            form = form.child(list);
        }
        let mut levels = div().flex().flex_row().flex_wrap().items_center().gap_2();
        for (rung, label) in [
            (warroom::Rung::See, ui.wr_rung_see),
            (warroom::Rung::Read, ui.wr_rung_read),
            (warroom::Rung::Ask, ui.wr_rung_ask),
        ] {
            let selected = editor.draft.rung == Some(rung);
            // The rungs escalate see < read < ask. `ask` is the top clearance —
            // the ONLY rung that lets one agent prompt another (fork its
            // session) — so it wears the action colour (the same amber a bypass
            // line is drawn in) and a ▲ mark, while the lower rungs are dimmed,
            // so the powerful one reads first. A person should never grant it by
            // reflex.
            let is_ask = matches!(rung, warroom::Rung::Ask);
            let shown = if is_ask {
                format!("\u{25B2} {label}")
            } else {
                label.to_string()
            };
            let mut btn = Self::bezel_btn(sk, &shown, selected)
                .id(SharedString::from(format!("war-rung-{rung:?}")))
                .debug_selector(move || format!("war-rung-{rung:?}"));
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
                if let Some(e) = &mut ws.war_relation {
                    e.draft.rung = Some(rung);
                }
                cx.notify();
            })));
        }
        let sentence = if let Some((from, to, rung)) = editor.draft.ready(&self.war_orgs) {
            let word = match rung {
                warroom::Rung::See => ui.wr_rung_see,
                warroom::Rung::Read => ui.wr_rung_read,
                warroom::Rung::Ask => ui.wr_rung_ask,
            };
            lang::fill(
                ui.wr_rel_sentence,
                &[
                    ("from", &self.war_room_seat_label(from, cx)),
                    ("to", &self.war_room_seat_label(to, cx)),
                    ("rung", word),
                ],
            )
        } else {
            ui.wr_rel_choose.to_string()
        };
        let ready = editor.draft.ready(&self.war_orgs).is_some_and(|(f, t, _)| {
            self.war_room_seat_leaf(f, cx).is_some() && self.war_room_seat_leaf(t, cx).is_some()
        });
        levels = levels
            .child(
                Self::bezel_btn(sk, ui.wr_rel_save, ready)
                    .id("war-rel-save")
                    .debug_selector(|| "war-rel-save".to_string())
                    .opacity(if ready { 1. } else { 0.4 })
                    .on_click(cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.war_room_save_relation(cx);
                    })),
            )
            .child(
                Self::bezel_btn(sk, ui.dlg_cancel, false)
                    .id("war-rel-cancel")
                    .debug_selector(|| "war-rel-cancel".to_string())
                    .on_click(cx.listener(|ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.war_relation = None;
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
                Self::bezel_btn(sk, ui.wr_erase, false)
                    .id("war-rel-remove")
                    .debug_selector(|| "war-rel-remove".to_string())
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        ws.chart.erase(from, to);
                        ws.war_room_forget_pair(from, to, cx);
                        ws.war_relation = None;
                        ws.save(cx);
                        cx.notify();
                    })),
            );
        }
        controls.child(form.child(SharedString::from(sentence)).child(levels))
    }

    /// What a person calls `leaf`: where its tab sits on the left bar, then
    /// its header's title (see [`warroom::PaneName`]). Every place the War
    /// Room and its rail name a pane goes through here.
    pub(crate) fn pane_name(&self, leaf: &Entity<TerminalView>, cx: &App) -> warroom::PaneName {
        let spine = self
            .tab_with_leaf(leaf.entity_id())
            .map(|ti| self.spine_path(ti))
            .unwrap_or_default();
        warroom::PaneName::new(&spine, &leaf.read(cx).header_title())
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
                    mcp::AskReply::Copy(warroom::CopyStatus {
                        ask_id: *ask_id,
                        reads: a.reads,
                        press_limit: self.press_limit(),
                        stage: a.stage.clone(),
                    })
                }
                None => mcp::AskReply::Refused(warroom::Refusal::NoSuchAsk),
            },
        }
    }

    /// A copy's admission is not permission to deliver after revocation. A
    /// spent bypass remains a receipt for this one already-admitted question;
    /// explicit relationship edits and membership moves erase those receipts.
    fn copy_refusal(&self, a: &warroom::CopyAsk, cx: &App) -> Option<warroom::Refusal> {
        let Some(caller) = self.leaf_by_pane_id(a.asker_pane, cx) else {
            return Some(warroom::Refusal::Gone);
        };
        let Some(target) = self.leaf_by_shell_pid(a.target_pid, cx) else {
            return Some(warroom::Refusal::Gone);
        };
        let (c, t) = (caller.read(cx), target.read(cx));
        if a.from.is_none() || a.to.is_none() || c.seat != a.from || t.seat != a.to {
            return Some(warroom::Refusal::Gone);
        }
        if !c.mode.is_agent() || askcopy::Program::of_mode(t.mode.label()) != Some(a.program) {
            return Some(warroom::Refusal::NotAnAgent);
        }
        let mut receipt = warroom::Chart::default();
        let chart = if a.via == "bypass" {
            // This transient chart checks the already-spent prompt against
            // today's global and target ceilings without spending it twice.
            receipt.grant_bypass(a.from.unwrap(), a.to.unwrap(), 1);
            &receipt
        } else {
            &self.chart
        };
        warroom::decide_ask(
            &self.mcp,
            chart,
            &c.party(),
            &t.party(),
            false,
            a.question_len,
        )
        .err()
    }

    fn forget_copy(&mut self, id: warroom::AskId, cx: &mut Context<Self>) {
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

    fn war_room_forget_pair(
        &mut self,
        from: warroom::SeatId,
        to: warroom::SeatId,
        cx: &mut Context<Self>,
    ) {
        self.asks.forget_pair(from, to);
        if let Some(asker) = self.war_room_seat_leaf(from, cx) {
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
        self.ask_press_tokens.unwrap_or(warroom::ASK_PRESS_TOKENS)
    }

    /// A line from the asker's seat must reach as far as ask
    /// (`warroom::decide_ask`). A cheap ask is answered from what the window
    /// can already show; a copy is priced, and starts at once when it reads no
    /// more than the person's press limit, or goes to the rail for their Yes.
    /// Every ask — refused or answered — leaves one line in the record,
    /// without its words.
    fn ask_question(&mut self, req: &mcp::AskRequest, cx: &mut Context<Self>) -> mcp::AskReply {
        let now_ms = now_ms();
        self.ask_seq = (self.ask_seq + 1).max(now_ms);
        let id = warroom::AskId(self.ask_seq);
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
            _ => mcp::AskReply::Refused(warroom::Refusal::Gone),
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
                warroom::CopyStage::AwaitingPress => self.raise_copy_card(id, cx),
                warroom::CopyStage::Running => self.start_copy(id, "copy_started", cx),
                _ => {}
            }
        }
        reply
    }

    fn decide_question(
        &mut self,
        req: &mcp::AskRequest,
        id: warroom::AskId,
        caller: &Entity<TerminalView>,
        target: &Entity<TerminalView>,
        entry: &mut askrecord::Entry,
        cx: &mut Context<Self>,
    ) -> mcp::AskReply {
        let (c, t) = (caller.read(cx), target.read(cx));
        let q = req.question.chars().count();
        let same = caller.entity_id() == target.entity_id();
        let via = match warroom::decide_ask(&self.mcp, &self.chart, &c.party(), &t.party(), same, q)
        {
            Ok(via) => via,
            Err(r) => return mcp::AskReply::Refused(r),
        };
        let vit = t.shell_pid().and_then(|pid| self.agent_vitals.get(&pid));
        entry.program = Some(t.mode.label().to_string());
        if req.copy {
            let Some(program) = askcopy::Program::of_mode(t.mode.label()) else {
                return mcp::AskReply::Refused(warroom::Refusal::CannotCopy);
            };
            // One at a time: a copy running or waiting, or any ask of this
            // agent's still on the person's rail.
            if self.asks.busy(req.caller_pane) || c.access_ask.is_some() {
                return mcp::AskReply::Refused(warroom::Refusal::Busy);
            }
            // Priced by the transport, off this thread, from the transcript
            // the copy would fork. No price is no copy.
            let Some(price) = &req.price else {
                return mcp::AskReply::Refused(warroom::Refusal::ConversationUnnamed);
            };
            let conversation = match &price.conversation {
                Ok(c) => c.clone(),
                Err(r) => return mcp::AskReply::Refused(r.clone()),
            };
            let (reads, model) = (price.reads, price.model.clone());
            let limit = self.press_limit();
            let stage = if warroom::copy_waits_for_press(reads, limit) {
                warroom::CopyStage::AwaitingPress
            } else {
                warroom::CopyStage::Running
            };
            entry.program = Some(program.word().to_string());
            entry.model = model.clone();
            entry.reads_tokens = reads;
            entry.stage = "priced".into();
            self.asks.open(
                id,
                warroom::CopyAsk {
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
            return mcp::AskReply::Copy(warroom::CopyStatus {
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
        let seat = warroom::SeatView {
            title: seat_title(t),
            project,
            role: t.seat.map(|s| self.chart.role_of(s)),
            state: t.agent_status().state.label_in(self.lang).to_string(),
            model: vit.and_then(|v| v.model.clone()),
            size: vit.and_then(|v| v.tokens),
        };
        entry.model = seat.model.clone();
        entry.stage = "cheap".into();
        mcp::AskReply::Cheap(warroom::CheapAnswer {
            ask_id: id,
            seat,
            summary: t.bench.latest_response().map(|r| warroom::Summary {
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
    /// back for the War Room.
    fn record(&mut self, entry: &askrecord::Entry) {
        let record = askrecord::path_for(crate::instance::key());
        if let Err(e) = askrecord::append(&record, entry) {
            eprintln!("[war-room] the ask record could not be written: {e}");
        }
        self.ask_numbers = askrecord::numbers(&record).ok();
    }

    /// One stage of a copy ask, for the record.
    fn record_copy(
        &mut self,
        id: warroom::AskId,
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
    fn raise_copy_card(&mut self, id: warroom::AskId, cx: &mut Context<Self>) {
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
            risk: Some(warroom::risk(&warroom::RiskFacts::Copy { reads: a.reads })),
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
    fn start_copy(&mut self, id: warroom::AskId, stage: &str, cx: &mut Context<Self>) {
        let Some(a) = self.asks.get(id).cloned() else {
            return;
        };
        if self.copy_refusal(&a, cx).is_some() {
            self.forget_copy(id, cx);
            return;
        }
        self.asks.set(id, warroom::CopyStage::Running);
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
        id: warroom::AskId,
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
                warroom::CopyStage::Answered {
                    answer: a.text,
                    tokens_read: a.tokens_read,
                    cost_usd: a.cost_usd,
                },
                "answered",
            ),
            Err(askcopy::End::Refused(refusal)) => {
                (warroom::CopyStage::Refused { refusal }, "refused")
            }
            Err(askcopy::End::Failed(why)) => (warroom::CopyStage::Failed { why }, "failed"),
        };
        let (tokens_read, cost_usd, refusal) = match &stage {
            warroom::CopyStage::Answered {
                tokens_read,
                cost_usd,
                ..
            } => (*tokens_read, *cost_usd, None),
            warroom::CopyStage::Refused { refusal } => (None, None, refusal_code(refusal)),
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
        id: Option<warroom::AskId>,
        answer: access::Answer,
        cx: &mut Context<Self>,
    ) -> access::Outcome {
        let Some(id) = id.filter(|id| {
            self.asks
                .get(*id)
                .is_some_and(|a| a.stage == warroom::CopyStage::AwaitingPress)
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
                        warroom::CopyStage::Failed {
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
                self.asks.set(id, warroom::CopyStage::Declined);
                self.record_copy(id, "declined", |_| {});
                access::Outcome::Kept
            }
        }
    }
    /// Close the War Room and let go of any half-drawn line.
    pub(crate) fn close_war_room(&mut self) {
        self.war_room = false;
        self.war_room_pick = None;
        self.war_relation = None;
        self.war_org_edit = None;
        self.war_move = None;
        self.war_drag = None;
        self.war_context = None;
        self.war_feedback = None;
        self.war_room_query = EditBuffer::seeded("");
    }

    /// A person seated a pane: mint its seat and save it with the pane.
    fn war_room_seat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        if let Some(org) = self.war_org {
            self.war_room_assign(leaf, org, false, cx);
        }
    }

    /// Both removal targets and the click alternative revoke the same authority.
    fn war_room_unseat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        let Some(seat) = leaf.read(cx).seat else {
            return;
        };
        self.chart.forget_seat(seat);
        self.war_orgs.remove(seat);
        self.war_room_forget_pending(leaf, seat, cx);
        self.war_relation = None;
        self.war_move = None;
        self.war_context = None;
        leaf.update(cx, |v, cx| {
            v.set_agent_access(warroom::AgentAccess::OffLimits, cx);
            v.set_seat(None, cx);
        });
        self.war_feedback = Some(lang::fill(
            self.lang.strings().ui.wr_removed,
            &[("pane", &self.pane_name(leaf, cx).tab)],
        ));
        if self.war_room_pick == Some(seat) {
            self.war_room_pick = None;
        }
        self.save(cx);
        cx.notify();
    }

    /// Membership changes invalidate both directions and every waiting card.
    fn war_room_forget_pending(
        &mut self,
        leaf: &Entity<TerminalView>,
        seat: warroom::SeatId,
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
    fn war_room_click_seat(&mut self, seat: warroom::SeatId, cx: &mut Context<Self>) {
        if self
            .war_relation
            .as_ref()
            .is_some_and(|e| e.choosing.is_some())
        {
            self.war_room_choose_endpoint(seat, cx);
        } else {
            self.war_room_pick = if self.war_room_pick == Some(seat) {
                None
            } else {
                Some(seat)
            };
            cx.notify();
        }
    }

    /// The edge label opens an explicit editor; it never cycles a permission.
    fn war_room_click_line(
        &mut self,
        from: warroom::SeatId,
        to: warroom::SeatId,
        cx: &mut Context<Self>,
    ) {
        self.war_room_begin_relation(Some(from), Some(to), cx);
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
fn refusal_code(r: &warroom::Refusal) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(
        cx: &mut gpui::TestAppContext,
    ) -> (
        Entity<Workspace>,
        &'static mut gpui::VisualTestContext,
        warroom::OrgId,
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
                let org = ws.war_orgs.create("Cinema crew").unwrap();
                let other = ws.war_orgs.create("Launch crew").unwrap();
                for (i, tab) in ws.tabs.iter_mut().enumerate() {
                    tab.name = Some(if i == 0 { "Lead" } else { "Worker" }.into());
                    let mut leaves = Vec::new();
                    tab.root.leaves(&mut leaves);
                    let pane = leaves[0].clone();
                    let seat = warroom::SeatId(100 + i as u64);
                    pane.update(cx, |v, cx| {
                        v.mode = pane::PaneMode::Codex;
                        v.seat = Some(seat);
                        v.fixture_set_pane_id(10 + i as u64);
                        cx.notify();
                    });
                    ws.war_orgs.assign(seat, org);
                }
                ws.war_org = Some(org);
                ws.war_room = true;
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
            target.update(cx, |v, _| v.agent_access = warroom::AgentAccess::OffLimits);
            let hidden = propose_open(ws, cx, pid);
            assert_ne!(hidden["isError"], true, "{hidden}");
            let pending_hidden = propose_open(ws, cx, pid);
            assert_eq!(pending_hidden["isError"], true);
            assert_eq!(
                target.read(cx).agent_access,
                warroom::AgentAccess::OffLimits
            );
            let proposal = asker.read(cx).access_ask.as_ref().unwrap();
            assert_eq!(proposal.target_key, Some(target.entity_id().as_u64()));
            assert!(proposal.title.as_ref().unwrap().contains("Worker"));
            assert!(ws.rail_answer(key, access::Answer::No, cx));
            assert_eq!(
                target.read(cx).agent_access,
                warroom::AgentAccess::OffLimits
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
                warroom::AgentAccess::OffLimits
            );

            assert_eq!(propose_open(ws, cx, pid), hidden);
            // Model a stale/replaced entity while retaining its claimed PID.
            asker.update(cx, |v, _| {
                v.access_ask.as_mut().unwrap().target_key = Some(key)
            });
            assert!(ws.rail_answer(key, access::Answer::Yes, cx));
            assert_eq!(
                target.read(cx).agent_access,
                warroom::AgentAccess::OffLimits
            );

            assert_eq!(propose_open(ws, cx, pid), hidden);
            assert!(ws.rail_answer(key, access::Answer::Yes, cx));
            assert_eq!(target.read(cx).agent_access, warroom::AgentAccess::Open);
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
            ws.war_room_seat_leaf(warroom::SeatId(100), cx).unwrap()
        });
        ws.update(cx, |ws, cx| {
            ws.mcp.enabled = true;
            ws.chart.draw(
                warroom::SeatId(100),
                warroom::SeatId(101),
                warroom::Rung::Read,
            );
            ws.chart
                .grant_bypass(warroom::SeatId(101), warroom::SeatId(100), 3);
            ws.chart.draw(
                warroom::SeatId(300),
                warroom::SeatId(301),
                warroom::Rung::See,
            );
            let other = ws.war_room_seat_leaf(warroom::SeatId(101), cx).unwrap();
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
        drag(cx, "war-seat-100", target);
        ws.read_with(cx, |ws, cx| {
            assert_eq!(ws.war_orgs.owner(warroom::SeatId(100)), None);
            let v = leaf.read(cx);
            assert_eq!(v.seat, None);
            assert_eq!(v.agent_access, warroom::AgentAccess::OffLimits);
            assert_eq!(ws.chart.saved().len(), 1, "unrelated relationships survive");
            let party = warroom::Party {
                seat: v.seat,
                access: v.agent_access,
                is_agent: true,
            };
            assert_eq!(
                warroom::reach(&ws.mcp, &ws.chart, None, &party, false),
                warroom::Reach::Hidden
            );
            assert!(!warroom::may_write_live(
                &ws.mcp,
                &ws.chart,
                Some(warroom::SeatId(101)),
                &party,
                false
            ));
            assert!(ws
                .war_room_seat_leaf(warroom::SeatId(101), cx)
                .unwrap()
                .read(cx)
                .access_ask
                .is_none());
            assert!(ws.war_feedback.as_ref().unwrap().contains("Lead"));
            assert!(ws.war_drag.is_none());
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
                        warroom::AgentAccess::from_saved(*off_limits, agents.as_deref()),
                        warroom::AgentAccess::OffLimits
                    );
                }
                _ => panic!("fixture is one leaf"),
            }
            assert_eq!(
                warroom::Orgs::restore(saved.war_orgs, &[]).owner(warroom::SeatId(100)),
                None
            );
        });
        assert!(
            cx.debug_bounds("war-remove-drop").is_none(),
            "bin stayed after release"
        );
        let pool_id = format!("war-pane-{}", leaf.entity_id().as_u64());
        drag(cx, &pool_id, "war-room-chart");
        ws.read_with(cx, |ws, cx| {
            let v = leaf.read(cx);
            assert!(
                v.seat.is_some() && v.seat != Some(warroom::SeatId(100)),
                "reseating gets a fresh identity"
            );
            assert_eq!(
                v.agent_access,
                warroom::AgentAccess::OffLimits,
                "reseating must not reopen access"
            );
            assert_eq!(ws.chart.saved().len(), 1);
        });
    }

    fn pending_copy(ws: &Workspace, cx: &App, stage: warroom::CopyStage) -> warroom::CopyAsk {
        let target = ws.war_room_seat_leaf(warroom::SeatId(101), cx).unwrap();
        warroom::CopyAsk {
            asker_pane: 10,
            from: Some(warroom::SeatId(100)),
            to: Some(warroom::SeatId(101)),
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
            ws.chart.draw(warroom::SeatId(100), warroom::SeatId(101), warroom::Rung::Read);
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
            ws.chart.erase(warroom::SeatId(100), warroom::SeatId(101));
            assert!(!contains_target(ws), "queued grep used a revoked snapshot grant to read the live grid");
            ws.chart.draw(warroom::SeatId(100), warroom::SeatId(101), warroom::Rung::See);
            assert!(!contains_target(ws), "a census-only line disclosed scrollback");
            ws.chart.draw(warroom::SeatId(100), warroom::SeatId(101), warroom::Rung::Read);
            assert!(contains_target(ws), "restoring read must restore the positive control");
            ws.mcp.enabled = false;
            assert!(!contains_target(ws), "disabling MCP did not revoke the queued read");
            ws.mcp.enabled = true;
            assert!(!ws.mcp_search("QA_GRID_CANARY", 2000, None, cx).iter().any(|p| p.pid == pid));
            assert!(!ws.mcp_search("QA_GRID_CANARY", 2000, Some(999), cx).iter().any(|p| p.pid == pid));
        });
    }

    #[gpui::test]
    fn org_transfer_revokes_pending_copies_and_permission_cards(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, other) = fixture(cx);
        for moving in [100, 101] {
            ws.update(cx, |ws, cx| {
                let original = ws.war_orgs.owner(warroom::SeatId(100)).unwrap();
                let leaf = ws.war_room_seat_leaf(warroom::SeatId(moving), cx).unwrap();
                let destination = if original == other {
                    ws.war_orgs.create("Third crew").unwrap()
                } else {
                    other
                };
                ws.chart.draw(
                    warroom::SeatId(100),
                    warroom::SeatId(101),
                    warroom::Rung::Ask,
                );
                for (i, stage) in [
                    warroom::CopyStage::AwaitingPress,
                    warroom::CopyStage::Running,
                    warroom::CopyStage::Answered {
                        answer: "PRIVATE_CANARY".into(),
                        tokens_read: None,
                        cost_usd: None,
                    },
                ]
                .into_iter()
                .enumerate()
                {
                    ws.asks
                        .open(warroom::AskId(i as u64 + 1), pending_copy(ws, cx, stage));
                }
                let mut unrelated = pending_copy(ws, cx, warroom::CopyStage::Running);
                unrelated.from = Some(warroom::SeatId(300));
                unrelated.to = Some(warroom::SeatId(301));
                ws.asks.open(warroom::AskId(99), unrelated);
                let asker = ws.leaf_by_pane_id(10, cx).unwrap();
                let target_pid = ws.asks.get(warroom::AskId(1)).unwrap().target_pid;
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
                ws.war_room_assign(&leaf, destination, false, cx);
                assert!(
                    ws.asks.get(warroom::AskId(1)).is_some(),
                    "unconfirmed move changed authority"
                );
                ws.war_room_assign(&leaf, destination, true, cx);
                for id in 1..=3 {
                    assert!(
                        ws.asks.get(warroom::AskId(id)).is_none(),
                        "org transfer retained copy {id}"
                    );
                }
                assert!(
                    asker.read(cx).access_ask.is_none(),
                    "old bypass prompt survived transfer"
                );
                ws.copy_finished(
                    warroom::AskId(2),
                    Ok(askcopy::Answer {
                        text: "LATE_PRIVATE_CANARY".into(),
                        tokens_read: None,
                        cost_usd: None,
                    }),
                    cx,
                );
                assert!(
                    ws.asks.get(warroom::AskId(2)).is_none(),
                    "late completion restored a revoked answer"
                );
                assert!(
                    ws.asks.get(warroom::AskId(99)).is_some(),
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
                warroom::SeatId(100),
                warroom::SeatId(101),
                warroom::Rung::Read,
            );
            assert!(
                ws.apply_mcp_config(&[(mcp::Target::Pane(pid), patch.clone())], cx)[0]
                    .1
                    .is_ok()
            );
            ws.chart.erase(warroom::SeatId(100), warroom::SeatId(101));
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
                asker.update(cx, |v, _| v.seat = Some(warroom::SeatId(100)));
                target.update(cx, |v, _| {
                    v.seat = Some(warroom::SeatId(101));
                    v.agent_access = warroom::AgentAccess::Open;
                });
                ws.chart.draw(
                    warroom::SeatId(100),
                    warroom::SeatId(101),
                    warroom::Rung::Ask,
                );
                let mut ask = pending_copy(
                    ws,
                    cx,
                    warroom::CopyStage::Answered {
                        answer: "SECRET_ANSWER".into(),
                        tokens_read: None,
                        cost_usd: None,
                    },
                );
                match case {
                    "line-removed" => ws.chart.erase(warroom::SeatId(100), warroom::SeatId(101)),
                    "off-limits" => {
                        target.update(cx, |v, _| v.agent_access = warroom::AgentAccess::OffLimits)
                    }
                    "read-only" => {
                        target.update(cx, |v, _| v.agent_access = warroom::AgentAccess::ReadOnly)
                    }
                    "mcp-off" => ws.mcp.enabled = false,
                    "asker-reseated" => {
                        asker.update(cx, |v, _| v.seat = Some(warroom::SeatId(900)))
                    }
                    "target-reseated" => {
                        target.update(cx, |v, _| v.seat = Some(warroom::SeatId(901)))
                    }
                    "spent-bypass" => {
                        ask.via = "bypass";
                        ws.chart.erase(warroom::SeatId(100), warroom::SeatId(101));
                    }
                    _ => {}
                }
                let id = warroom::AskId(500);
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
                ws.asks.forget_seat(warroom::SeatId(100));
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
                warroom::SeatId(100),
                warroom::SeatId(101),
                warroom::Rung::Ask,
            );
            ws.asks.open(
                warroom::AskId(1),
                pending_copy(ws, cx, warroom::CopyStage::AwaitingPress),
            );
            target.update(cx, |v, _| v.agent_access = warroom::AgentAccess::OffLimits);
            assert_eq!(
                ws.answer_copy(Some(warroom::AskId(1)), access::Answer::Yes, cx),
                access::Outcome::Gone
            );
            assert!(ws.asks.get(warroom::AskId(1)).is_none());
            target.update(cx, |v, _| v.agent_access = warroom::AgentAccess::Open);
            ws.asks.open(
                warroom::AskId(2),
                pending_copy(ws, cx, warroom::CopyStage::Running),
            );
            ws.chart.erase(warroom::SeatId(100), warroom::SeatId(101));
            ws.copy_finished(
                warroom::AskId(2),
                Ok(askcopy::Answer {
                    text: "REVOKED_CANARY".into(),
                    tokens_read: None,
                    cost_usd: None,
                }),
                cx,
            );
            assert!(ws.asks.get(warroom::AskId(2)).is_none());
            let mut bypass = pending_copy(ws, cx, warroom::CopyStage::Running);
            bypass.via = "bypass";
            ws.asks.open(warroom::AskId(3), bypass.clone());
            std::mem::swap(&mut bypass.from, &mut bypass.to);
            ws.asks.open(warroom::AskId(4), bypass);
            ws.war_relation = Some(RelationEditor {
                draft: warroom::RelationDraft {
                    from: Some(warroom::SeatId(100)),
                    to: Some(warroom::SeatId(101)),
                    rung: Some(warroom::Rung::See),
                },
                choosing: None,
            });
            ws.war_room_save_relation(cx);
            assert!(
                ws.asks.get(warroom::AskId(3)).is_none(),
                "relationship edit preserved a bypass receipt"
            );
            assert!(
                ws.asks.get(warroom::AskId(4)).is_some(),
                "reverse direction was cancelled"
            );
        });
    }

    #[gpui::test]
    fn pool_drop_revokes_access_and_saved_membership(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        removal_check(cx, "war-room-pool");
    }

    #[gpui::test]
    fn bin_drop_revokes_access_and_saved_membership(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        removal_check(cx, "war-remove-drop");
    }

    #[gpui::test]
    fn drag_cancel_and_invalid_release_preserve_membership_and_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let source = bounds(cx, "war-seat-100");
        let chart = bounds(cx, "war-room-chart");
        let pool = bounds(cx, "war-pool-tile");
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
            assert!(cx.debug_bounds("war-remove-drop").is_some());
            assert_eq!(
                bounds(cx, "war-seat-100"),
                source,
                "pickup moved its source"
            );
            assert_eq!(bounds(cx, "war-room-chart"), chart);
            assert_eq!(bounds(cx, "war-pool-tile"), pool);
            let bin = bounds(cx, "war-remove-drop");
            cx.simulate_mouse_move(
                bin.bottom_right() - point(px(3.), px(3.)),
                MouseButton::Left,
                gpui::Modifiers::default(),
            );
            cx.run_until_parked();
            let preview = bounds(cx, "war-drag-preview");
            let panel = bounds(cx, "war-room-panel");
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
                bounds(cx, "war-room-header").center()
            };
            cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::default());
            cx.simulate_mouse_up(end, MouseButton::Left, gpui::Modifiers::default());
            cx.run_until_parked();
            ws.read_with(cx, |ws, cx| {
                assert!(ws.war_room);
                assert!(ws.war_room_seat_leaf(warroom::SeatId(100), cx).is_some());
                assert!(ws.war_feedback.is_none());
            });
            assert!(cx.debug_bounds("war-remove-drop").is_none());
        }
    }

    #[gpui::test]
    fn card_menu_dismisses_on_press_and_changes_only_its_card(cx: &mut gpui::TestAppContext) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        let at = bounds(cx, "war-seat-101").center();
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("war-context-menu").is_some());
        let outside = bounds(cx, "war-room-search").center();
        cx.simulate_mouse_down(outside, MouseButton::Left, gpui::Modifiers::default());
        assert!(
            ws.read_with(cx, |ws, _| ws.war_context.is_none()),
            "waited for release"
        );
        cx.simulate_mouse_up(outside, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.run_until_parked();
        click(cx, "war-context-off-limits");
        ws.read_with(cx, |ws, cx| {
            assert!(ws.war_context.is_none());
            assert_eq!(
                ws.war_room_seat_leaf(warroom::SeatId(101), cx)
                    .unwrap()
                    .read(cx)
                    .agent_access,
                warroom::AgentAccess::OffLimits
            );
            assert_eq!(
                ws.war_room_seat_leaf(warroom::SeatId(100), cx)
                    .unwrap()
                    .read(cx)
                    .agent_access,
                warroom::AgentAccess::Open
            );
        });
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_keystrokes("escape");
        ws.read_with(cx, |ws, _| {
            assert!(ws.war_room);
            assert!(ws.war_context.is_none());
        });
    }

    #[gpui::test]
    fn terminal_menu_closes_even_when_the_war_room_swallows_the_press(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        ws.update(cx, |ws, cx| {
            ws.close_war_room();
            cx.notify();
        });
        cx.run_until_parked();
        let at = point(px(700.), px(450.));
        cx.simulate_mouse_down(at, MouseButton::Right, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Right, gpui::Modifiers::default());
        cx.run_until_parked();
        assert!(cx.debug_bounds("pane-context-menu").is_some());
        ws.update(cx, |ws, cx| {
            ws.war_room = true;
            cx.notify();
        });
        cx.run_until_parked();
        let outside = bounds(cx, "war-room-search").center();
        cx.simulate_mouse_down(outside, MouseButton::Left, gpui::Modifiers::default());
        ws.read_with(cx, |ws, cx| {
            assert!(!ws
                .war_room_seat_leaf(warroom::SeatId(100), cx)
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
        let original = bounds(cx, "war-seat-100").size;
        click(cx, "war-seat-100");
        assert_eq!(
            bounds(cx, "war-seat-100").size,
            original,
            "selection transformed the card"
        );
        assert!(ws.read_with(cx, |ws, _| ws.chart.saved().is_empty()));
        drag(cx, "war-connect-100", "war-seat-101");
        ws.read_with(cx, |ws, _| {
            let editor = ws
                .war_relation
                .as_ref()
                .expect("connector drop opened the builder");
            assert_eq!(editor.draft.from, Some(warroom::SeatId(100)));
            assert_eq!(editor.draft.to, Some(warroom::SeatId(101)));
            assert_eq!(editor.draft.rung, None);
            assert!(
                ws.chart.saved().is_empty(),
                "dropping a connector granted access"
            );
        });
        click(cx, "war-rel-save");
        assert!(ws.read_with(cx, |ws, _| ws.chart.saved().is_empty()));
        click(cx, "war-rung-Read");
        click(cx, "war-rel-save");
        ws.read_with(cx, |ws, _| {
            assert_eq!(
                ws.chart
                    .line(warroom::SeatId(100), warroom::SeatId(101))
                    .unwrap()
                    .rung
                    .known(),
                Some(warroom::Rung::Read)
            )
        });
        assert_eq!(bounds(cx, "war-seat-100").size, original);
        click(cx, "war-line-100-101");
        click(cx, "war-rung-Ask");
        click(cx, "war-rel-cancel");
        ws.read_with(cx, |ws, _| {
            assert_eq!(
                ws.chart
                    .line(warroom::SeatId(100), warroom::SeatId(101))
                    .unwrap()
                    .rung
                    .known(),
                Some(warroom::Rung::Read)
            )
        });
        click(cx, "war-line-100-101");
        click(cx, "war-rel-remove");
        assert!(ws.read_with(cx, |ws, _| ws.chart.saved().is_empty()));
    }

    #[gpui::test]
    fn starting_a_relationship_keeps_the_pane_filter_and_the_top_rung_reads_prompt(
        cx: &mut gpui::TestAppContext,
    ) {
        let _warp = crate::warp::WARP_SERIAL
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (ws, cx, _) = fixture(cx);
        // The top rung is the one that lets an agent drive another — it must
        // read "prompt", not the underwhelming "ask", while the saved word is
        // still "ask" so older charts keep loading.
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.lang.strings().ui.wr_rung_ask, "prompt");
            assert_eq!(
                warroom::RungWord::Known(warroom::Rung::Ask).known(),
                Some(warroom::Rung::Ask)
            );
        });
        cx.simulate_keystrokes("l e a d");
        cx.run_until_parked();
        assert_eq!(ws.read_with(cx, |ws, _| ws.war_room_query.text()), "lead");
        // Opening the relationship builder (Connect, or a connector drop) used
        // to wipe the filter mid-wiring. It must survive.
        click(cx, "war-connect");
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.war_relation.is_some(),
                "Connect did not open the relationship builder"
            );
            assert_eq!(
                ws.war_room_query.text(),
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
        let original = bounds(cx, "war-seat-100").size;
        ws.update(cx, |ws, cx| {
            ws.chart.draw(
                warroom::SeatId(100),
                warroom::SeatId(101),
                warroom::Rung::Read,
            );
            cx.notify();
        });
        cx.run_until_parked();
        drag(cx, "war-seat-100", &format!("war-org-{}", other.0));
        ws.read_with(cx, |ws, _| {
            assert!(
                ws.war_move.is_some(),
                "a connected move needs a visible choice"
            );
            assert_ne!(ws.war_orgs.owner(warroom::SeatId(100)), Some(other));
            assert_eq!(
                ws.chart.saved().len(),
                1,
                "dragging silently erased a connection"
            );
        });
        click(cx, "war-move-cancel");
        assert_eq!(ws.read_with(cx, |ws, _| ws.chart.saved().len()), 1);
        drag(cx, "war-seat-100", &format!("war-org-{}", other.0));
        click(cx, "war-move-confirm");
        assert_eq!(bounds(cx, "war-seat-100").size, original);
        ws.read_with(cx, |ws, cx| {
            assert_eq!(ws.war_orgs.owner(warroom::SeatId(100)), Some(other));
            assert_eq!(ws.war_org, Some(other));
            assert!(ws.chart.saved().is_empty());
            let state = ws.build_state(cx);
            let saved: StateFile = toml::from_str(&toml::to_string(&state).unwrap()).unwrap();
            let orgs = warroom::Orgs::restore(saved.war_orgs, &[]);
            assert_eq!(orgs.owner(warroom::SeatId(100)), Some(other));
            assert_eq!(
                orgs.get(other).unwrap().name.as_deref(),
                Some("Launch crew")
            );
        });
        let first = ws.read_with(cx, |ws, _| ws.war_orgs.owner(warroom::SeatId(101)).unwrap());
        cx.simulate_keystrokes("w o r k e r");
        cx.run_until_parked();
        click(cx, &format!("war-open-org-{}", first.0));
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.war_org, Some(first));
            // The filter is the person's. Opening the org it found keeps what
            // they typed instead of wiping it — only ×, Esc and closing clear.
            assert_eq!(ws.war_room_query.text(), "worker");
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
        click(cx, "war-org-new");
        cx.simulate_keystrokes("c r e w enter");
        cx.run_until_parked();
        let created = ws.read_with(cx, |ws, _| {
            let id = ws.war_org.unwrap();
            assert_eq!(ws.war_orgs.get(id).unwrap().name.as_deref(), Some("crew"));
            assert!(ws.war_orgs.get(id).unwrap().seats.is_empty());
            assert!(ws.chart.saved().is_empty());
            id
        });
        click(cx, "war-org-rename");
        cx.simulate_keystrokes("t e a m enter");
        cx.run_until_parked();
        ws.read_with(cx, |ws, _| {
            assert_eq!(ws.war_org, Some(created));
            assert_eq!(
                ws.war_orgs.get(created).unwrap().name.as_deref(),
                Some("team")
            );
            assert!(ws.chart.saved().is_empty());
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        cx.update(|window, cx| {
            ws.read_with(cx, |ws, cx| {
                assert!(!ws.war_room);
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
                let worker = ws.war_room_seat_leaf(warroom::SeatId(101), cx).unwrap();
                ws.war_room_unseat(&worker, cx);
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
                let mut from = warroom::SeatId(100);
                for i in 200..208 {
                    let to = warroom::SeatId(i);
                    ws.war_orgs.assign(to, ws.war_org.unwrap());
                    ws.chart.draw(from, to, warroom::Rung::See);
                    from = to;
                }
                for i in 0..12 {
                    ws.war_orgs.create(&format!("Other crew {i}"));
                }
                cx.notify();
                (worker, extras)
            })
        });
        for pane in extras {
            let stop = pane.read_with(cx, |v, _| v.fixture_shutdown());
            cx.on_quit(stop);
        }
        let first = format!("war-pane-{}", worker.entity_id().as_u64());
        for dimensions in [size(px(900.), px(560.)), size(px(1200.), px(1000.))] {
            cx.simulate_resize(dimensions);
            cx.run_until_parked();
            let frame = bounds(cx, "war-room-panel");
            let chart = bounds(cx, "war-room-chart");
            let pool = bounds(cx, "war-room-pool");
            assert!(frame.origin.x >= px(0.) && frame.right() <= dimensions.width);
            assert!(frame.origin.y >= px(0.) && frame.bottom() <= dimensions.height);
            assert!(chart.bottom() <= frame.bottom() && pool.bottom() <= frame.bottom());
            assert!(chart.size.height > px(0.) && pool.size.height > px(0.));
            assert!(
                chart.right() < pool.origin.x,
                "the pool belongs on the right"
            );
            let headers = ["war-room-header", "war-org-header", "war-pool-header"];
            let fixed = headers.map(|id| bounds(cx, id));
            let card = bounds(cx, &first);
            let member = bounds(cx, "war-seat-100");
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
                bounds(cx, "war-seat-100"),
                member,
                "pool wheel moved the chart"
            );
            let pool_after = bounds(cx, &first);
            wheel(cx, chart.center());
            assert!(
                bounds(cx, "war-seat-100").origin.y < member.origin.y,
                "chart contents scroll"
            );
            assert_eq!(bounds(cx, &first), pool_after, "chart wheel moved the pool");
            assert_eq!(
                bounds(cx, &first).size,
                card.size,
                "scrolling reshaped a card"
            );
            assert_eq!(bounds(cx, "war-room-panel"), frame);
            assert_eq!(
                headers.map(|id| bounds(cx, id)),
                fixed,
                "a header scrolled away"
            );
        }
    }

    fn name() -> warroom::PaneName {
        warroom::PaneName::new(
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
        let repeated = warroom::PaneName::new(&["same".into()], "same");
        assert!(pane_search_score("", &repeated, ":same:same").is_some());
        let unicode = warroom::PaneName::new(&["ÜBER".into()], "修复权限");
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
}
