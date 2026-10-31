//! The artifact rail: every pane in this window whose agent has handed
//! something over, one card each, newest first. Raised by the 📜 in the
//! bottom-right corner.
//!
//! Parker, 2026-10-22: *"a global compilation of all of the agents with
//! artifacts. So we don't show agents that have no artifacts. These agents will
//! be displayed as cards. See how our Git coordination surface works. The card
//! will have the agent, its title, and the most recent artifact. They'll be
//! organized by the most recently completed"*. Then, at approval: *"we will make
//! a NEW glyph in the bottome right tray instead of repurposing the existing
//! agent wall rail - SIMPLE SCroll (documents) ... consider this wil need to
//! follow the design pattern of CLEAN."*
//!
//! ```text
//!   every live pane's ledger ──→ art_cards ──→ arrange ──→ the cards
//!   (TerminalView::handovers)     (one per      (newest first, held
//!                                  pane)         while the rail is up)
//!
//!   card ──click──→ focus that pane (pending_jump)
//!   its artifact ──click──→ focus it, and open the newest handover there
//! ```
//!
//! It keeps no store of its own. A card is a reading of one pane's
//! [`crate::handover::Ledger`], the store the pane's header chip already
//! reads, so the rail and the chip cannot disagree about what was handed over,
//! and a card goes when its pane goes. Unlike the attention spine, opening an
//! artifact takes nothing off the rail: a card is the way back to its agent for
//! as long as the pane lives.
//!
//! The 📜 is one of the corner's menu glyphs under UI CONFIG
//! ([`crate::uiconfig::Key::Artifacts`]). It also carries news: an agent's open
//! that was held because a square was up ([`TerminalView::open_here`]) bumps
//! it once and leaves a dot until the rail is opened. Hidden, the glyph gives
//! the corner to CONFIG and the news goes with it.

use super::*;

/// How long the 📜's bump takes, once per held open.
const BUMP_MS: u64 = 520;

/// How far the bump lifts the glyph at its height, in points before scale.
const BUMP_LIFT: f32 = 4.;

/// The rail's width, in points before scale.
const RAIL_W: f32 = 360.;

/// The dot's ink: green, the house colour for something ready to read. Not the
/// bug report's orange, which means something failed.
fn ready_ink() -> Hsla {
    hsla(0.34, 0.85, 0.58, 1.)
}

/// One card: a pane and the newest thing its agent handed over.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Card {
    /// The pane's row key, its entity id, the way the spine keys its rows.
    pub key: u64,
    /// The tab the pane is in, by index, to tell two cards in one tab apart.
    pub tab: usize,
    /// The project or initiative the tab hangs from, as the left bar names
    /// it. `None` for a tab filed under nothing.
    pub place: Option<String>,
    /// The tab's name on the left bar. A tab nobody named is called by its
    /// pane, because a bare tab number means nothing to the person reading.
    pub task: String,
    /// Which pane, when its tab holds more than one card.
    pub who: Option<String>,
    /// The pane's own name, the source of `who`.
    pub pane_name: String,
    pub state: hud::AgentState,
    pub label: String,
    pub kind: String,
    pub at_ms: u64,
    /// Whether a person has opened it, by any road.
    pub opened: bool,
    /// How many older handovers the same pane made.
    pub earlier: usize,
}

/// The cards in the order they are drawn.
///
/// Newest handover first. With the rail up (`held` not empty), a card keeps
/// the place it had when the rail opened, so nothing moves under the pointer;
/// a pane that hands something over while the rail is up has the newest
/// handover in the window, so its card goes on top, above the held order.
pub(crate) fn arrange(mut cards: Vec<Card>, held: &[u64]) -> Vec<Card> {
    cards.sort_by(|a, b| b.at_ms.cmp(&a.at_ms).then(a.key.cmp(&b.key)));
    if held.is_empty() {
        return cards;
    }
    let at = |k: u64| held.iter().position(|h| *h == k);
    let (mut fresh, mut kept): (Vec<Card>, Vec<Card>) =
        cards.into_iter().partition(|c| at(c.key).is_none());
    kept.sort_by_key(|c| at(c.key));
    fresh.extend(kept);
    fresh
}

/// Name the pane on every card that shares its tab with another, so two
/// agents in one tab do not read as one card twice.
pub(crate) fn tell_apart(cards: &mut [Card]) {
    for i in 0..cards.len() {
        let shared = cards
            .iter()
            .enumerate()
            .any(|(j, c)| j != i && c.tab == cards[i].tab);
        cards[i].who = shared.then(|| cards[i].pane_name.clone());
    }
}

/// The bump's height at `t` through it, 0 to 1 and back to 0.
pub(crate) fn hop(t: f32) -> f32 {
    (t.clamp(0., 1.) * std::f32::consts::PI).sin()
}

/// The dot a hold leaves, unplaced.
pub(crate) fn art_dot(s: f32) -> gpui::Div {
    div()
        .absolute()
        .size(px(6. * s))
        .rounded_full()
        .bg(ready_ink())
}

/// Put the rail's news on `door`, whichever control leads to the rail on this
/// frame: the dot at its top left, clear of the crash dot CONFIG wears at its
/// top right, and the bump, played once per hold — keyed on how many holds
/// there have been, so a new one starts it again and a redraw does not.
pub(crate) fn art_news<E>(door: E, dot: bool, bumps: u32, s: f32) -> AnyElement
where
    E: Styled + ParentElement + IntoElement + 'static,
{
    let door = door.when(dot, |d| d.child(art_dot(s).top(px(-s)).left(px(-s))));
    if bumps == 0 {
        return door.into_any_element();
    }
    door.with_animation(
        ("art-bump", bumps as usize),
        Animation::new(Duration::from_millis(BUMP_MS)),
        move |d, t| d.top(px(-BUMP_LIFT * hop(t) * s)),
    )
    .into_any_element()
}

/// The glyph a card shows for its agent's state, the counter's own cells.
fn state_glyph(state: hud::AgentState) -> &'static str {
    match state {
        hud::AgentState::Working => "\u{25b6}",
        hud::AgentState::Blocked => "\u{2016}",
        hud::AgentState::Error => "\u{2715}",
        hud::AgentState::Finished => "\u{2713}",
        hud::AgentState::Idle => "\u{25cb}",
        hud::AgentState::Resting => "\u{263e}",
        hud::AgentState::Asleep => "z",
        hud::AgentState::Unknown => "?",
    }
}

impl Workspace {
    /// An agent's open was held: bump the 📜 once and leave its dot.
    pub(crate) fn art_held(&mut self, cx: &mut Context<Self>) {
        self.art_bumps = self.art_bumps.wrapping_add(1).max(1);
        self.art_unseen = true;
        cx.notify();
    }

    /// The 📜, pressed.
    pub(crate) fn art_toggle(&mut self, cx: &mut Context<Self>) {
        if self.art_rail {
            self.art_close();
            cx.notify();
        } else {
            self.art_open(cx);
        }
    }

    /// Raise the rail: its order is taken now and held while it is up, and
    /// the dot is cleared, because the person is looking.
    pub(crate) fn art_open(&mut self, cx: &mut Context<Self>) {
        self.art_order = arrange(self.art_cards(cx), &[])
            .iter()
            .map(|c| c.key)
            .collect();
        self.art_cursor = 0;
        self.art_unseen = false;
        self.art_rail = true;
        cx.notify();
    }

    pub(crate) fn art_close(&mut self) {
        self.art_rail = false;
        self.art_order.clear();
    }

    /// One card per live pane that has handed something over.
    pub(crate) fn art_cards(&self, cx: &App) -> Vec<Card> {
        let mut cards = Vec::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            let mut leaves = Vec::new();
            tab.root.leaves(&mut leaves);
            let place = self
                .place_of(i)
                .project
                .map(|p| self.branch_label(BarBranch::Project(p)));
            for leaf in leaves {
                let v = leaf.read(cx);
                let ledger = v.handovers();
                let Some(newest) = ledger.newest() else {
                    continue;
                };
                let pane_name = v.handover_pane_name();
                cards.push(Card {
                    key: leaf.entity_id().as_u64(),
                    tab: i,
                    place: place.clone(),
                    task: tab.name.clone().unwrap_or_else(|| pane_name.clone()),
                    who: None,
                    pane_name,
                    state: v.agent_status().state,
                    label: newest.label.clone(),
                    kind: newest.kind(),
                    at_ms: newest.at_ms,
                    opened: ledger.is_opened(&newest.key()),
                    earlier: ledger.newest_first().len().saturating_sub(1),
                });
            }
        }
        tell_apart(&mut cards);
        cards
    }

    /// The cards as drawn, in the order the keys and the clicks agree on.
    fn art_shown(&self, cx: &App) -> Vec<Card> {
        arrange(self.art_cards(cx), &self.art_order)
    }

    /// Go to a card's pane, and with `open`, open its newest handover there,
    /// the way the pane's own header chip does. The rail closes so the pane is
    /// in view; the card stays on it for next time.
    pub(crate) fn art_go(&mut self, key: u64, open: bool, cx: &mut Context<Self>) {
        self.art_close();
        let Some(leaf) = self.leaf_by_key(key) else {
            cx.notify();
            return;
        };
        if open {
            leaf.update(cx, |v, cx| v.open_newest_handover(cx));
        }
        self.pending_jump = Some(leaf.entity_id());
        cx.notify();
    }

    /// Rebuild the archive away from the UI thread, then open it in the pane
    /// the person was using when they asked.
    fn art_archive(&mut self, cx: &mut Context<Self>) {
        let (active, _, _) = self.bench_targets();
        let focused = self.tabs.get(self.active).and_then(|t| t.focused);
        let leaf = active
            .iter()
            .find(|l| Some(l.entity_id()) == focused)
            .or_else(|| active.first())
            .cloned();
        let Some(leaf) = leaf else { return };
        self.art_close();
        leaf.update(cx, |v, cx| v.open_artifact_archive(cx));
        cx.notify();
    }

    /// The rail owns the arrows and Enter while it is up. Esc goes through
    /// `close_popups` with every other overlay. Returns whether the key was
    /// the rail's, so ordinary typing still reaches the pane underneath.
    pub(crate) fn art_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if !self.art_rail {
            return false;
        }
        let cards = self.art_shown(cx);
        self.art_cursor = self.art_cursor.min(cards.len().saturating_sub(1));
        match key {
            "down" => self.art_cursor = (self.art_cursor + 1).min(cards.len().saturating_sub(1)),
            "up" => self.art_cursor = self.art_cursor.saturating_sub(1),
            "enter" => match cards.get(self.art_cursor) {
                Some(c) => self.art_go(c.key, true, cx),
                None => return false,
            },
            _ => return false,
        }
        true
    }

    /// The rail, over the corner it is raised from.
    pub(crate) fn render_artifact_rail(
        &self,
        th: &theme::Theme,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        if !self.art_rail {
            return None;
        }
        let sk = skin::for_theme(cx, th, th.grade.scale);
        let t = self.lang.strings().ui;
        let now = surfacefeed::now_ms();
        let cards = self.art_shown(cx);
        let cursor = self.art_cursor.min(cards.len().saturating_sub(1));

        let mut list = div().flex().flex_col().gap(px(6.));
        if cards.is_empty() {
            list = list.child(
                div()
                    .py(px(10.))
                    .text_size(sk.text_px(11.))
                    .text_color(sk.ink.ink_dim)
                    .text_center()
                    .child(t.art_empty),
            );
        }
        for (i, c) in cards.iter().enumerate() {
            let ink = agent_state_glow(th, sk.ink.ink_dim, c.state);
            let selected = i == cursor;
            let key = c.key;
            let when = handover::card_time(c.at_ms, now, self.lang);
            let head = div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .text_size(sk.text_px(11.5))
                .when_some(c.place.clone(), |d, place| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(sk.text_px(9.5))
                            .text_color(sk.ink.ink_dim)
                            .child(place.to_uppercase()),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(sk.ink.ink)
                        .child(c.task.clone()),
                )
                .when_some(c.who.clone(), |d, who| {
                    d.child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_color(sk.ink.ink_dim)
                            .child(who),
                    )
                })
                .child(div().flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_color(ink)
                        .child(if sk.text_controls() {
                            c.state.label()
                        } else {
                            state_glyph(c.state)
                        }),
                )
                .when_some(when, |d, when| {
                    d.child(
                        div()
                            .flex_none()
                            .text_size(sk.text_px(9.5))
                            .text_color(sk.ink.ink_dim)
                            .child(when),
                    )
                });
            let artifact = div()
                .id(("art-open", i))
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .text_size(sk.text_px(11.5))
                .cursor_pointer()
                .when(!c.opened, |d| {
                    d.child(
                        div()
                            .flex_none()
                            .size(px(6.))
                            .rounded_full()
                            .bg(ready_ink()),
                    )
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_color(th.accent)
                        .underline()
                        .child(c.label.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(4.))
                        .rounded(sk.rad_raw(4.))
                        .border(sk.box_border(1.))
                        .border_color(sk.ink.ink_dim.alpha(0.5))
                        .text_size(sk.text_px(9.5))
                        .text_color(sk.ink.ink_dim)
                        .child(c.kind.clone()),
                )
                .when(c.earlier > 0, |d| {
                    d.child(div().flex_1()).child(
                        div()
                            .flex_none()
                            .text_size(sk.text_px(9.5))
                            .text_color(sk.ink.ink_dim)
                            .child(format!("+{} {}", c.earlier, t.art_earlier)),
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.art_go(key, true, cx);
                    }),
                );
            list = list.child(
                coordination_view::card_chrome(div(), &sk, 1.0, ink, selected)
                    // The edge says the agent's state on every card, not only
                    // the selected one: it is what the card is about.
                    .when(!selected, |d| d.border_color(ink.alpha(0.45)))
                    .id(("art-card", i))
                    .cursor_pointer()
                    .child(head)
                    .child(artifact)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.art_go(key, false, cx);
                        }),
                    ),
            );
        }

        let header = div()
            .flex()
            .flex_row()
            .items_center()
            .px(px(2.))
            .pb(px(4.))
            .text_size(sk.text_px(9.5))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(sk.ink.ink_dim)
            .child(format!("\u{1f4dc} {}", t.cfg_artifacts.to_uppercase()))
            .child(div().flex_1())
            .when(!cards.is_empty(), |d| d.child(cards.len().to_string()))
            .child(
                div()
                    .id("art-archive")
                    .ml(px(8.))
                    .cursor_pointer()
                    .text_color(th.accent)
                    .child(t.art_archive)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.art_archive(cx);
                        }),
                    ),
            );

        let panel = over_the_footer(div().id("art-rail").absolute(), scale)
            .w(px(RAIL_W))
            .max_h(px(self.tray_max_h(self.tray_band().0)))
            .overflow_x_hidden()
            .overflow_y_scroll()
            .p_2()
            .rounded(sk.rad_raw(8.))
            .border_2()
            .border_color(th.accent.alpha(0.85))
            .bg(sk.region_face(darken(th.surface, 0.45)))
            .text_color(th.text)
            .font_family(skin::for_theme(cx, th, 1.0).font_family)
            .shadow(float_shadows(th.accent, th.grade.phosphor))
            .map(|d| sk.finish(d))
            .flex()
            .flex_col()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
            )
            .child(header)
            .child(list);
        Some(
            div()
                .absolute()
                .inset_0()
                // occlude, or the pane's warped CRT layer paints over the panel
                .occlude()
                .bg(hsla(0., 0., 0., 0.18))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.art_close();
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(key: u64, tab: usize, at_ms: u64) -> Card {
        Card {
            key,
            tab,
            place: None,
            task: format!("tab {tab}"),
            who: None,
            pane_name: format!("pane {key}"),
            state: hud::AgentState::Finished,
            label: format!("page {key}"),
            kind: "html".into(),
            at_ms,
            opened: false,
            earlier: 0,
        }
    }

    fn keys(cards: &[Card]) -> Vec<u64> {
        cards.iter().map(|c| c.key).collect()
    }

    /// "organized by the most recently completed, so reverse chronological,
    /// so the most recent ones are at the very top."
    #[test]
    fn the_newest_handover_is_the_top_card() {
        let cards = vec![card(1, 0, 100), card(2, 1, 300), card(3, 2, 200)];
        assert_eq!(keys(&arrange(cards, &[])), vec![2, 3, 1]);
    }

    /// With the rail up, a card keeps its place, and a pane that hands over
    /// something new goes on top of the held order rather than reshuffling it.
    #[test]
    fn an_open_rail_holds_its_order_and_a_new_handover_goes_on_top() {
        let held = [2, 3, 1];
        // Pane 1 hands over again while the rail is up: still where it was.
        let cards = vec![card(1, 0, 900), card(2, 1, 300), card(3, 2, 200)];
        assert_eq!(keys(&arrange(cards.clone(), &held)), vec![2, 3, 1]);
        // A pane that had no card gets one: on top.
        let mut more = cards;
        more.push(card(4, 3, 950));
        assert_eq!(keys(&arrange(more.clone(), &held)), vec![4, 2, 3, 1]);
        // A pane that closed takes its card with it, and the rest keep order.
        more.retain(|c| c.key != 3);
        assert_eq!(keys(&arrange(more, &held)), vec![4, 2, 1]);
    }

    /// Two agents in one tab are told apart by their panes' names; an agent
    /// alone in its tab is called by the tab only.
    #[test]
    fn two_cards_in_one_tab_are_named_by_their_panes() {
        let mut cards = vec![card(1, 0, 1), card(2, 0, 2), card(3, 1, 3)];
        tell_apart(&mut cards);
        assert_eq!(cards[0].who.as_deref(), Some("pane 1"));
        assert_eq!(cards[1].who.as_deref(), Some("pane 2"));
        assert_eq!(cards[2].who, None);
    }

    /// The bump rises and comes back down to where the glyph sits.
    #[test]
    fn the_bump_rises_and_lands_where_it_started() {
        assert!(hop(0.).abs() < 1e-6);
        assert!((hop(0.5) - 1.).abs() < 1e-6);
        assert!(hop(1.).abs() < 1e-6);
        assert!(hop(2.).abs() < 1e-6, "past its end it stays landed");
    }
}
