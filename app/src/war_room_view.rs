//! The War Room panel: seats a person placed, the lines between them, what
//! each seat is doing, and the window's MCP switches.
//!
//! The rules it draws live in `warroom`; this file only gathers what the
//! window already knows about each seat (the agent counter's state, the size
//! and model vitals read from the transcript, the agent's own latest summary,
//! its sticky note) and turns a person's clicks into seats and lines. Seating
//! and drawing happen here and nowhere else — see
//! `only_the_war_room_and_the_rail_draw_lines`.

use super::*;
use gpui::PathBuilder;

/// A seat's box, at scale 1.
const SEAT_W: f32 = 196.;
const SEAT_H: f32 = 66.;
const COL_GAP: f32 = 30.;
const ROW_GAP: f32 = 56.;
const PAD: f32 = 14.;
/// Room on the right for the picked seat's card.
const CARD_W: f32 = 300.;

/// Everything one seat shows, gathered from facts the window already has.
struct SeatFacts {
    leaf: Entity<TerminalView>,
    seat: warroom::SeatId,
    title: String,
    project: String,
    program: String,
    model: Option<String>,
    size: Option<u64>,
    state: hud::AgentState,
    access: warroom::AgentAccess,
    note: Option<String>,
    summary: Option<(Option<String>, String)>,
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

        // ---- gather: seated panes, and agent panes nobody seated ----
        let mut seats: Vec<SeatFacts> = Vec::new();
        let mut loose: Vec<(Entity<TerminalView>, String, String)> = Vec::new();
        for (ti, tab) in self.tabs.iter().enumerate() {
            let project = self
                .place_of(ti)
                .project
                .and_then(|id| self.project_at(id))
                .and_then(|p| p.name.clone())
                .unwrap_or_default();
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for leaf in leaves {
                let v = leaf.read(cx);
                if !v.mode.is_agent() && v.seat.is_none() {
                    continue;
                }
                let title = v
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| (!v.title.is_empty()).then(|| v.shown_title()))
                    .unwrap_or_else(|| v.mode.label().to_string());
                let program = v.mode.label().to_string();
                let Some(seat) = v.seat else {
                    loose.push((leaf.clone(), title, program));
                    continue;
                };
                let vit = v.shell_pid().and_then(|pid| self.agent_vitals.get(&pid));
                seats.push(SeatFacts {
                    leaf: leaf.clone(),
                    seat,
                    title,
                    project: project.clone(),
                    program,
                    model: vit.and_then(|x| x.model.clone()),
                    size: vit.and_then(|x| x.tokens),
                    state: v.agent_status().state,
                    access: v.agent_access,
                    note: v.saved_note().map(|n| match n.title {
                        Some(t) if !n.text.is_empty() => format!("{t} — {}", n.text),
                        Some(t) => t,
                        None => n.text,
                    }),
                    summary: v
                        .bench
                        .latest_response()
                        .map(|r| (r.brief.clone(), r.layman.clone())),
                });
            }
        }

        // ---- lay the chart out ----
        let s = th.grade.scale.clamp(0.7, 1.6);
        let (sw, sh, cg, rg, pad) = (SEAT_W * s, SEAT_H * s, COL_GAP * s, ROW_GAP * s, PAD * s);
        let keys: Vec<(warroom::SeatId, String, String)> = seats
            .iter()
            .map(|f| (f.seat, f.project.clone(), f.title.clone()))
            .collect();
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
        let mut chart = div().relative().w(px(chart_w)).h(px(chart_h)).child(canvas);
        if seats.is_empty() {
            chart = chart.child(
                div()
                    .absolute()
                    .left(px(pad))
                    .top(px(pad))
                    .w(px(chart_w - pad * 2.))
                    .text_color(th.text.alpha(0.6))
                    .child(SharedString::from(ui.wr_empty.to_string())),
            );
        }
        for f in &seats {
            let Some((x, y)) = at(f.seat) else { continue };
            let picked = pick == Some(f.seat);
            let state_ink = agent_state_glow(th, th.accent, f.state);
            let size = f
                .size
                .map(hud::fmt_tokens)
                .unwrap_or_else(|| ui.wr_unknown.to_string());
            let role = match self.chart.role_of(f.seat) {
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
            let seat = f.seat;
            chart = chart.child(
                div()
                    .id(SharedString::from(format!("war-seat-{}", seat.0)))
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
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(sk.text_px(10.5))
                            .text_color(th.text)
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(SharedString::from(f.title.clone())),
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
                                "\u{25cf} {} · {size}",
                                f.state.label_in(lang)
                            ))),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.war_room_click_seat(seat, cx);
                        }),
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

        // ---- the picked seat's card ----
        let card = pick
            .and_then(|p| seats.iter().find(|f| f.seat == p))
            .map(|f| self.war_room_card(f, &sk, th, cx));

        // ---- off the chart ----
        let mut off = div().flex().flex_col().gap_1();
        for (leaf, title, program) in &loose {
            let leaf = leaf.clone();
            off = off.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_color(th.text.alpha(0.85))
                            .child(SharedString::from(format!("{title} · {program}"))),
                    )
                    .child(
                        Self::bezel_btn(&sk, ui.wr_seat_btn, false)
                            .id(SharedString::from(format!(
                                "war-seat-btn-{}",
                                leaf.entity_id().as_u64()
                            )))
                            .hover(|d| d.border_color(th.accent))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.war_room_seat(&leaf, cx);
                                }),
                            ),
                    ),
            );
        }

        let switches = self.mcp_switch_buttons(&sk, th, "war", cx);
        let panel = div()
            .id("war-room-panel")
            .max_w(px(chart_w + CARD_W * s + pad * 4.).max(px(640.)))
            .max_h(px(self.tray_card_h()))
            .overflow_x_hidden()
            .overflow_y_scroll()
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
                    .text_size(sk.text_px(8.5))
                    .text_color(th.accent.alpha(0.85))
                    .child(SharedString::from(ui.wr_hint.to_string())),
            )
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
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .items_start()
                    .child(
                        div()
                            .id("war-room-chart")
                            .flex_none()
                            .overflow_x_scroll()
                            .rounded(sk.rad_raw(8.))
                            .border_1()
                            .border_color(th.text.alpha(0.14))
                            .child(chart),
                    )
                    .children(card),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .pt_2()
                    .border_t_1()
                    .border_color(th.text.alpha(0.14))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_size(sk.text_px(8.5))
                                    .text_color(th.text.alpha(0.7))
                                    .child(SharedString::from(ui.wr_off_hdr.to_string())),
                            )
                            .child(
                                div()
                                    .text_size(sk.text_px(8.5))
                                    .text_color(th.text.alpha(0.5))
                                    .child(SharedString::from(ui.wr_off_blurb.to_string())),
                            ),
                    )
                    .child(off),
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
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.close_war_room();
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }

    /// One seat, opened: the agent's own latest summary first, then its
    /// state, model, size, note and lines — the cheap answers, before anyone
    /// asks a copy anything.
    fn war_room_card(
        &self,
        f: &SeatFacts,
        sk: &skin::Skin,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let ui = self.lang.strings().ui;
        let unknown = || ui.wr_unknown.to_string();
        let title_of = |seat: warroom::SeatId| -> String {
            self.tabs
                .iter()
                .flat_map(|t| {
                    let mut l = vec![];
                    t.root.leaves(&mut l);
                    l
                })
                .find(|leaf| leaf.read(cx).seat == Some(seat))
                .map(|leaf| {
                    let v = leaf.read(cx);
                    v.name
                        .clone()
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| v.shown_title())
                })
                .unwrap_or_else(unknown)
        };
        let rung = |l: &warroom::Line| match l.rung.known() {
            Some(warroom::Rung::See) => ui.wr_rung_see.to_string(),
            Some(warroom::Rung::Read) => ui.wr_rung_read.to_string(),
            Some(warroom::Rung::Ask) => ui.wr_rung_ask.to_string(),
            None => ui.wr_unknown.to_string(),
        };
        let lines_in: Vec<String> = self
            .chart
            .saved()
            .iter()
            .filter(|l| l.to == f.seat)
            .map(|l| format!("{} · {}", title_of(l.from), rung(l)))
            .collect();
        let lines_out: Vec<String> = self
            .chart
            .saved()
            .iter()
            .filter(|l| l.from == f.seat)
            .map(|l| format!("{} · {}", title_of(l.to), rung(l)))
            .collect();
        let size = f
            .size
            .map(|n| lang::fill(ui.wr_size, &[("tokens", &grouped(n))]))
            .unwrap_or_else(unknown);
        let row = |label: &str, value: String| {
            div()
                .flex()
                .flex_row()
                .gap_2()
                .child(
                    div()
                        .w(px(88.))
                        .flex_none()
                        .text_size(sk.text_px(8.))
                        .text_color(th.text.alpha(0.55))
                        .child(SharedString::from(label.to_string())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_color(th.text)
                        .child(SharedString::from(value)),
                )
        };
        let join = |v: &[String]| {
            if v.is_empty() {
                ui.wr_none.to_string()
            } else {
                v.join("\n")
            }
        };
        let leaf = f.leaf.clone();
        let mut card = div()
            .w(px(CARD_W))
            .flex_none()
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .rounded(sk.rad_raw(8.))
            .border_1()
            .border_color(th.complement.alpha(0.6))
            .bg(darken(th.surface, 0.45))
            .child(
                div()
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_size(sk.text_px(11.))
                    .child(SharedString::from(f.title.clone())),
            )
            .child(
                div()
                    .text_size(sk.text_px(8.5))
                    .text_color(th.text.alpha(0.6))
                    .child(SharedString::from(f.project.clone())),
            );
        if let Some((brief, layman)) = &f.summary {
            card = card.child(
                div()
                    .mt_1()
                    .p_1()
                    .border_l_2()
                    .border_color(th.accent)
                    .child(
                        div()
                            .text_size(sk.text_px(7.5))
                            .text_color(th.text.alpha(0.5))
                            .child(SharedString::from(ui.wr_card_summary.to_string())),
                    )
                    .child(SharedString::from(
                        brief.clone().unwrap_or_else(|| layman.clone()),
                    )),
            );
        }
        card.child(row(
            ui.wr_card_state,
            f.state.label_in(self.lang).to_string(),
        ))
        .child(row(
            ui.wr_card_model,
            f.model.clone().unwrap_or_else(unknown),
        ))
        .child(row(ui.wr_card_size, size))
        .child(row(
            ui.wr_card_note,
            f.note.clone().unwrap_or_else(|| ui.wr_none.to_string()),
        ))
        .child(row(ui.wr_card_in, join(&lines_in)))
        .child(row(ui.wr_card_out, join(&lines_out)))
        .child(
            Self::bezel_btn(sk, ui.wr_unseat_btn, false)
                .id("war-unseat")
                .mt_1()
                .hover(|d| d.border_color(th.accent))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.war_room_unseat(&leaf, cx);
                    }),
                ),
        )
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
        let project = self
            .tab_index_of(target.entity_id())
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
            .map(|t| seat_title(t.read(cx)));
        let ask = access::Ask {
            change: access::Change::Copy,
            pid: Some(a.target_pid),
            title,
            why: access::excerpt(&a.question),
            at: std::time::Instant::now(),
            risk: Some(warroom::risk(&warroom::RiskFacts::Copy {
                reads: a.reads,
                limit: self.press_limit(),
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
    }

    /// A person seated a pane: mint its seat and save it with the pane.
    fn war_room_seat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        let seat = warroom::SeatId::mint();
        leaf.update(cx, |v, cx| v.set_seat(Some(seat), cx));
        self.save(cx);
        cx.notify();
    }

    /// A person took a seat off the chart: its lines go with it, and the pane
    /// goes back to its own position — watched, never prompted.
    fn war_room_unseat(&mut self, leaf: &Entity<TerminalView>, cx: &mut Context<Self>) {
        let Some(seat) = leaf.read(cx).seat else {
            return;
        };
        self.chart.forget_seat(seat);
        leaf.update(cx, |v, cx| v.set_seat(None, cx));
        if self.war_room_pick == Some(seat) {
            self.war_room_pick = None;
        }
        self.save(cx);
        cx.notify();
    }

    /// Click a seat, then another, to draw a line from the first to the
    /// second (Gate 3, question 3). Clicking the picked seat again lets go.
    fn war_room_click_seat(&mut self, seat: warroom::SeatId, cx: &mut Context<Self>) {
        match self.war_room_pick {
            Some(p) if p == seat => self.war_room_pick = None,
            Some(p) => {
                self.chart.draw(p, seat, warroom::Rung::Ask);
                self.war_room_pick = None;
                self.save(cx);
            }
            None => self.war_room_pick = Some(seat),
        }
        cx.notify();
    }

    /// Click a line to change how far it reaches: ask, see, read, then gone.
    /// A bypass is erased by its click, since a person granted it for its
    /// prompts and nothing else.
    fn war_room_click_line(
        &mut self,
        from: warroom::SeatId,
        to: warroom::SeatId,
        cx: &mut Context<Self>,
    ) {
        let Some(line) = self.chart.line(from, to) else {
            return;
        };
        match (line.is_bypass(), line.rung.known()) {
            (false, Some(warroom::Rung::Ask)) => self.chart.draw(from, to, warroom::Rung::See),
            (false, Some(warroom::Rung::See)) => self.chart.draw(from, to, warroom::Rung::Read),
            _ => self.chart.erase(from, to),
        }
        self.save(cx);
        cx.notify();
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
}

/// A count with its thousands grouped, as the briefs print them: 360,090.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A refusal's wire name, for the record.
fn refusal_code(r: &warroom::Refusal) -> Option<String> {
    serde_json::to_value(r)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
}

/// What a seat is called: the name a person gave it, else its title.
fn seat_title(t: &TerminalView) -> String {
    t.name
        .clone()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| t.shown_title())
}
