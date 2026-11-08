//! The relate dialog: drop a pane or a tab onto another tab, and say how the
//! two agents work together.
//!
//! Parker, 2026-11-07: *"dragging a tab OR dragging the PANE ONTO another tab
//! will ASK HOW YOU WOULD LIKE THE RELATIONSHIP ASSIGNED like the threshing
//! floor, but in the focus dialogue box kind of way."* The line it draws is
//! the Threshing Floor's own: the dialog only gathers who, how far and whom,
//! and hands them to [`Workspace::floor_relate`], which runs the panel's draft
//! and save. Nothing here touches the chart.
//!
//! A pane dropped on a tab used to join that tab. That landing is kept as one
//! choice in the dialog, so nothing that worked before is lost, and a drop
//! with no agent on one side skips the dialog and joins as it always did.

use super::*;

/// The four lines, nearest first, the order the Floor lists them.
const RUNGS: [threshing_floor::Rung; 4] = [
    threshing_floor::Rung::See,
    threshing_floor::Rung::Read,
    threshing_floor::Rung::Ask,
    threshing_floor::Rung::Prompt,
];

/// The dialog's state while it is up.
pub(crate) struct Relate {
    /// The agent the line comes FROM.
    pub(crate) actor: gpui::WeakEntity<TerminalView>,
    /// The agents it points AT: every agent pane in the tab dropped onto, or
    /// the one agent whose INBOX row it was.
    pub(crate) targets: Vec<gpui::WeakEntity<TerminalView>>,
    rung: Option<threshing_floor::Rung>,
    /// Set when a PANE was dropped: the pane and where it landed, so moving
    /// it into the tab stays one choice.
    pub(crate) join: Option<(EntityId, DropTarget)>,
    error: Option<String>,
}

impl Workspace {
    /// The agent panes of tab `i`, the one last focused first.
    pub(crate) fn tab_agents(&self, i: usize, cx: &App) -> Vec<Entity<TerminalView>> {
        let Some(tab) = self.tabs.get(i) else {
            return Vec::new();
        };
        let mut leaves = Vec::new();
        tab.root.leaves(&mut leaves);
        let mut agents: Vec<Entity<TerminalView>> = leaves
            .into_iter()
            .filter(|l| l.read(cx).hosts_agent())
            .cloned()
            .collect();
        if let Some(at) = tab
            .focused
            .and_then(|f| agents.iter().position(|a| a.entity_id() == f))
        {
            let first = agents.remove(at);
            agents.insert(0, first);
        }
        agents
    }

    /// Offer the dialog for a line from `actor` to the agents in tab `to`.
    ///
    /// Returns false when there is nothing to relate — no agent on one side,
    /// or the drop landed on the actor's own tab — so the caller does what the
    /// drop did before the dialog existed.
    pub(crate) fn offer_relate(
        &mut self,
        actor: Entity<TerminalView>,
        to: usize,
        join: Option<(EntityId, DropTarget)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let targets: Vec<Entity<TerminalView>> = self
            .tab_agents(to, cx)
            .into_iter()
            .filter(|t| *t != actor)
            .collect();
        self.offer_relate_to(actor, targets, join, window, cx)
    }

    /// Offer the dialog for a line from `actor` to exactly `targets`: the
    /// door every drop comes through, a tab's agents or one agent's INBOX row.
    pub(crate) fn offer_relate_to(
        &mut self,
        actor: Entity<TerminalView>,
        targets: Vec<Entity<TerminalView>>,
        join: Option<(EntityId, DropTarget)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !actor.read(cx).hosts_agent() {
            return false;
        }
        let targets: Vec<Entity<TerminalView>> = targets
            .into_iter()
            .filter(|t| *t != actor && t.read(cx).hosts_agent())
            .collect();
        if targets.is_empty() {
            return false;
        }
        // Open on the line already drawn, so the dialog says what is true now.
        let rung = actor
            .read(cx)
            .seat
            .zip(targets[0].read(cx).seat)
            .and_then(|(a, t)| self.chart.line(a, t))
            .and_then(|l| l.rung.known());
        self.composer_put_away();
        if self.rail_open {
            self.rail_close();
        }
        self.relate = Some(Relate {
            actor: actor.downgrade(),
            targets: targets.iter().map(Entity::downgrade).collect(),
            rung,
            join,
            error: None,
        });
        window.focus(&self.focus_handle, cx);
        cx.notify();
        true
    }

    /// `TD_RELATE_DEMO`: open the dialog once between the first panes of the
    /// first two tabs, agents or not, so a rig can photograph it without a
    /// drag. The Floor's and the confirm dialog's demo hooks, for this one.
    pub(crate) fn relate_demo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static SHOWN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if std::env::var_os("TD_RELATE_DEMO").is_none()
            || self.tabs.len() < 2
            || SHOWN.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        let first = |t: &Tab| {
            let mut v = Vec::new();
            t.root.leaves(&mut v);
            v.first().map(|l| l.downgrade())
        };
        let (Some(actor), Some(target)) = (first(&self.tabs[0]), first(&self.tabs[1])) else {
            return;
        };
        self.relate = Some(Relate {
            actor,
            targets: vec![target],
            rung: Some(threshing_floor::Rung::Ask),
            join: None,
            error: None,
        });
        // The dialog holds the keyboard, as it does when a drop opens it.
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    /// The dialog's keys. Always takes the key while it is up: one that fell
    /// through would reach a terminal behind the dialog.
    pub(crate) fn relate_key(
        &mut self,
        ks: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(r) = self.relate.as_mut() else {
            return false;
        };
        let m = &ks.modifiers;
        if m.control || m.alt || m.platform {
            return true;
        }
        let at = r.rung.and_then(|g| RUNGS.iter().position(|x| *x == g));
        match ks.key.as_str() {
            "escape" => {
                self.relate = None;
                self.focus_active(window, cx);
            }
            "enter" => self.relate_submit(window, cx),
            "s" => self.relate_swap(cx),
            "m" if r.join.is_some() => self.relate_move(window, cx),
            "left" | "up" => {
                r.rung = Some(RUNGS[at.map_or(0, |i| i.saturating_sub(1))]);
                r.error = None;
            }
            "right" | "down" => {
                r.rung = Some(RUNGS[at.map_or(0, |i| (i + 1).min(RUNGS.len() - 1))]);
                r.error = None;
            }
            key => {
                if let Some(n) = key
                    .parse::<usize>()
                    .ok()
                    .filter(|n| (1..=RUNGS.len()).contains(n))
                {
                    r.rung = Some(RUNGS[n - 1]);
                    r.error = None;
                }
            }
        }
        cx.notify();
        true
    }

    /// Turn the line round. Only between two panes: a tab of several agents
    /// pointed at is several lines, and turning that round would give one
    /// line several senders.
    fn relate_swap(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.relate.as_mut() else {
            return;
        };
        if r.targets.len() != 1 {
            return;
        }
        std::mem::swap(&mut r.actor, &mut r.targets[0]);
        r.error = None;
        cx.notify();
    }

    fn relate_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = self.relate.as_ref() else {
            return;
        };
        let Some(rung) = r.rung else {
            let none = self.lang.strings().entry.relate_none.to_string();
            if let Some(r) = self.relate.as_mut() {
                r.error = Some(none);
            }
            cx.notify();
            return;
        };
        let actor = r.actor.upgrade();
        let targets: Option<Vec<Entity<TerminalView>>> =
            r.targets.iter().map(gpui::WeakEntity::upgrade).collect();
        let result = match (actor, targets) {
            (Some(actor), Some(targets)) => self.floor_relate(&actor, &targets, rung, cx),
            _ => Err(self.lang.strings().ui.floor_rel_invalid.to_string()),
        };
        match result {
            Ok(_) => {
                self.relate = None;
                self.focus_active(window, cx);
            }
            Err(error) => {
                if let Some(r) = self.relate.as_mut() {
                    r.error = Some(error);
                }
            }
        }
        cx.notify();
    }

    /// The landing a pane drop had before the dialog: into the tab.
    fn relate_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, target)) = self.relate.take().and_then(|r| r.join) else {
            return;
        };
        self.perform_drop(id, target, window, cx);
        cx.notify();
    }

    /// The dialog, over the main area, or nothing.
    pub(crate) fn render_relate(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let r = self.relate.as_ref()?;
        let s = theme::outer_choice(cx).grade.scale;
        let sk = skin::skin(cx, s);
        let e = self.lang.strings().entry;
        let ui = self.lang.strings().ui;
        let name = |w: &gpui::WeakEntity<TerminalView>| {
            w.upgrade()
                .map(|leaf| self.pane_name(&leaf, cx).full())
                .unwrap_or_else(|| ui.floor_unavailable.to_string())
        };
        let rung_word = |g: threshing_floor::Rung| match g {
            threshing_floor::Rung::See => ui.floor_rung_see,
            threshing_floor::Rung::Read => ui.floor_rung_read,
            threshing_floor::Rung::Ask => ui.floor_rung_ask,
            threshing_floor::Rung::Prompt => ui.floor_rung_prompt,
        };
        let now = r
            .actor
            .upgrade()
            .and_then(|a| a.read(cx).seat)
            .zip(
                r.targets
                    .first()
                    .and_then(gpui::WeakEntity::upgrade)
                    .and_then(|t| t.read(cx).seat),
            )
            .and_then(|(a, t)| self.chart.line(a, t))
            .and_then(|l| l.rung.known());

        let card = |label: String| {
            div()
                .flex_1()
                .min_w_0()
                .px(px(12. * s))
                .py(px(10. * s))
                .rounded(sk.capped(10.))
                .bg(darken(th.bg, 0.4).alpha(0.85))
                .border(sk.box_border(1.))
                .border_color(th.faint.alpha(0.3))
                .text_size(sk.text_px(12. * s))
                .text_color(th.text)
                // One line, cut short: a long name wrapped at its narrowest
                // measure made the card taller than the window.
                .child(div().w_full().truncate().child(label))
        };
        let mut whom = div().flex_1().min_w_0().flex().flex_col().gap(px(4. * s));
        for t in &r.targets {
            whom = whom.child(card(name(t)));
        }
        let pair = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10. * s))
            .child(card(name(&r.actor)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4. * s))
                    .child(
                        div()
                            .text_size(sk.text_px(9. * s))
                            .text_color(th.faint)
                            .child(ui.floor_rel_can),
                    )
                    .child(
                        div()
                            .text_size(sk.text_px(16. * s))
                            .text_color(th.accent)
                            .child("\u{2192}"),
                    ),
            )
            .child(whom);

        let mut verbs = div().flex().flex_col().gap(px(4. * s));
        for (n, g) in RUNGS.iter().copied().enumerate() {
            let on = r.rung == Some(g);
            let what = match g {
                threshing_floor::Rung::See => e.relate_see,
                threshing_floor::Rung::Read => e.relate_read,
                threshing_floor::Rung::Ask => e.relate_ask,
                threshing_floor::Rung::Prompt => e.relate_prompt,
            };
            verbs = verbs.child(
                div()
                    .id(SharedString::from(format!("relate-rung-{n}")))
                    .debug_selector(move || format!("relate-rung-{n}"))
                    .px(px(10. * s))
                    .py(px(6. * s))
                    .rounded(sk.radius())
                    .cursor_pointer()
                    .when(on, |d| d.bg(th.accent.alpha(0.22)))
                    .hover(|st| st.bg(hsla(0., 0., 1., 0.06)))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10. * s))
                    .child(
                        div()
                            .w(px(14. * s))
                            .text_size(sk.text_px(10. * s))
                            .text_color(th.faint)
                            .child(format!("{}", n + 1)),
                    )
                    .child(
                        div()
                            .w(px(84. * s))
                            .text_size(sk.text_px(12.5 * s))
                            .text_color(if on { th.text } else { th.text.alpha(0.8) })
                            .child(rung_word(g)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(sk.text_px(10.5 * s))
                            .text_color(th.faint)
                            .child(what),
                    )
                    .children((now == Some(g)).then(|| {
                        div()
                            .text_size(sk.text_px(9.5 * s))
                            .text_color(th.accent)
                            .child(lang::fill(e.relate_now, &[("rung", rung_word(g))]))
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            if let Some(r) = ws.relate.as_mut() {
                                r.rung = Some(g);
                                r.error = None;
                            }
                            window.focus(&ws.focus_handle, cx);
                            cx.notify();
                        }),
                    ),
            );
        }

        let mut buttons = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8. * s))
            .child(
                Self::bezel_btn_s(&sk, e.relate_save, r.rung.is_some(), s)
                    .id("relate-save")
                    .debug_selector(|| "relate-save".into())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            ws.relate_submit(window, cx);
                        }),
                    ),
            )
            .child(
                Self::bezel_btn_s(&sk, e.relate_cancel, false, s)
                    .id("relate-cancel")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            ws.relate = None;
                            ws.focus_active(window, cx);
                            cx.notify();
                        }),
                    ),
            );
        if r.targets.len() == 1 {
            buttons = buttons.child(
                Self::bezel_btn_s(&sk, e.relate_swap, false, s)
                    .id("relate-swap")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.relate_swap(cx);
                        }),
                    ),
            );
        }
        if r.join.is_some() {
            buttons = buttons.child(div().flex_1()).child(
                div()
                    .id("relate-move")
                    .debug_selector(|| "relate-move".into())
                    .cursor_pointer()
                    .text_size(sk.text_px(10.5 * s))
                    .text_color(th.faint)
                    .hover(|st| st.text_color(th.text))
                    .child(e.relate_move)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            ws.relate_move(window, cx);
                        }),
                    ),
            );
        }

        let column = div()
            .id("relate-column")
            .debug_selector(|| "relate-column".into())
            .w_full()
            .max_w(px(640. * s))
            .flex()
            .flex_col()
            .gap(px(12. * s))
            .p(px(22. * s))
            .map(|d| crate::entry_view::dialog_card(th, &sk, d))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    window.focus(&ws.focus_handle, cx);
                }),
            )
            .child(
                div()
                    .text_size(sk.text_px(20. * s))
                    .text_color(th.text)
                    .child(e.relate_title),
            )
            .child(
                div()
                    .text_size(sk.text_px(11. * s))
                    .text_color(th.faint)
                    .child(e.relate_sub),
            )
            .child(pair)
            .child(verbs)
            .children(r.error.clone().map(|err| {
                div()
                    .text_size(sk.text_px(10.5 * s))
                    .text_color(th.accent)
                    .child(err)
            }))
            .child(buttons)
            .child(
                div()
                    .text_size(sk.text_px(9.5 * s))
                    .text_color(th.faint)
                    .child(e.relate_hint),
            );

        Some(
            div()
                // The card stands over the panes, so the warp's grade is kept
                // off it, the way every floating card in the house is.
                .on_children_prepainted(keep_panels_from_doc_grade)
                .id("relate-page")
                .debug_selector(|| "relate-page".into())
                .absolute()
                .inset_0()
                .occlude()
                .flex()
                .items_center()
                .justify_center()
                .p(px(24. * s))
                .bg(th.bg.alpha(0.72))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(column)
                .into_any_element(),
        )
    }
}
