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
use gpui::PathBuilder;

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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.offset.x - px(90.))
            .pt(self.offset.y - px(22.))
            .child(
                div()
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
                let (points, label_at) = if ty > fy + sh + rg / 2. {
                    // Two or more rows down: leave through the gap under the
                    // sender, run down the column gap beside the seat so no box
                    // in between is crossed, and come in from the gap above it.
                    let (x0, y0) = (fx + sw / 2., fy + sh);
                    let (x1, y1) = (tx + sw / 2., ty);
                    let (top, bottom) = (y0 + rg / 2., y1 - rg / 2.);
                    let gx = if tx >= fx {
                        tx - cg / 2.
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
                        // Labelled on its last stretch, by the seat it reaches,
                        // where it cannot sit on a shorter line's label.
                        ((gx + x1) / 2., bottom),
                    )
                } else if ty > fy {
                    // The next row: down from the sender, across, down into the seat.
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
                };
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
        let paths: Vec<(Vec<(f32, f32)>, bool)> =
            wires.iter().map(|w| (w.points.clone(), w.bypass)).collect();
        let canvas = gpui::canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                for (pts, bypass) in &paths {
                    let ink = if *bypass { bypass_ink } else { line_ink };
                    stroke_wire(bounds, pts, ink, *bypass, window);
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
                    .cursor_pointer()
                    .hover(|d| d.border_color(th.accent))
                    .on_drag(pane_drag, |d, position, _, cx| {
                        cx.new(|_| DragPreview {
                            title: d.title.clone(),
                            offset: position,
                            ink: d.ink,
                            bg: d.bg,
                        })
                    })
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
                        d.on_drag(pane_drag, |d, position, _, cx| {
                            cx.new(|_| DragPreview {
                                title: d.title.clone(),
                                offset: position,
                                ink: d.ink,
                                bg: d.bg,
                            })
                        })
                    })
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
                    .child(SharedString::from(ui.wr_hint.to_string())),
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
                    .border_color(th.text.alpha(0.14))
                    .drag_over::<PaneDrag>(move |d, _, _, _| d.border_color(drop_chart_ink))
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
            .border_color(th.text.alpha(0.14))
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
                            .child(SharedString::from(ui.wr_off_blurb.to_string())),
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
                    .child(chart_tile)
                    .child(pool_tile),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .bg(th.bg.alpha(0.70))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        ws.close_war_room();
                        ws.focus_active(window, cx);
                        cx.notify();
                    }),
                )
                .child(panel),
        )
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
        self.war_room_query = EditBuffer::seeded("");
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
                    .drag_over::<PaneDrag>(move |s, _, _, _| s.border_color(ink))
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
                .child(actions),
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
        self.war_room_query = EditBuffer::seeded("");
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
        let mut levels = div().flex().flex_row().flex_wrap().gap_2();
        for (rung, label) in [
            (warroom::Rung::See, ui.wr_rung_see),
            (warroom::Rung::Read, ui.wr_rung_read),
            (warroom::Rung::Ask, ui.wr_rung_ask),
        ] {
            levels = levels.child(
                Self::bezel_btn(sk, label, editor.draft.rung == Some(rung))
                    .id(SharedString::from(format!("war-rung-{rung:?}")))
                    .debug_selector(move || format!("war-rung-{rung:?}"))
                    .on_click(cx.listener(move |ws, _, _, cx| {
                        cx.stop_propagation();
                        if let Some(e) = &mut ws.war_relation {
                            e.draft.rung = Some(rung);
                        }
                        cx.notify();
                    })),
            );
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
            } => match self.asks.of_asker(*ask_id, *caller_pane) {
                Some(a) => mcp::AskReply::Copy(warroom::CopyStatus {
                    ask_id: *ask_id,
                    reads: a.reads,
                    press_limit: self.press_limit(),
                    stage: a.stage.clone(),
                }),
                None => mcp::AskReply::Refused(warroom::Refusal::NoSuchAsk),
            },
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
        self.war_room_query = EditBuffer::seeded("");
    }

    /// A person seated a pane: mint its seat and save it with the pane.
    fn war_room_seat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        if let Some(org) = self.war_org {
            self.war_room_assign(leaf, org, false, cx);
        }
    }

    /// A person took a seat off the chart: its lines go with it, and the pane
    /// goes back to its own position — watched, never prompted.
    fn war_room_unseat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        let Some(seat) = leaf.read(cx).seat else {
            return;
        };
        self.chart.forget_seat(seat);
        self.war_orgs.remove(seat);
        self.war_relation = None;
        leaf.update(cx, |v, cx| v.set_seat(None, cx));
        if self.war_room_pick == Some(seat) {
            self.war_room_pick = None;
        }
        self.save(cx);
        cx.notify();
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
            program: Some(("/bin/cat".into(), vec![])),
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

    fn click(cx: &mut gpui::VisualTestContext, id: &str) {
        let at = bounds(cx, id).center();
        cx.simulate_mouse_down(at, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_up(at, MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
    }

    fn drag(cx: &mut gpui::VisualTestContext, from: &str, to: &str) {
        let start = bounds(cx, from).center();
        let end = bounds(cx, to).center();
        cx.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_move(
            start + point(px(9.), px(9.)),
            MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_move(end, MouseButton::Left, gpui::Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, gpui::Modifiers::default());
        cx.run_until_parked();
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
            assert!(ws.war_room_query.text().is_empty());
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
}
