//! The workbench, as the terminal view draws and drives it.
//!
//! A child module of `pane` rather than a sibling, and the choice is the whole
//! point: a child sees its parent's private fields, so every `self.wb_*`,
//! `self.bench`, `self.session` and `self.mode` below is exactly as it was when
//! this code lived in pane.rs — no field went `pub(crate)`, no accessor was
//! invented, and the terminal view's struct did not change to let this out.
//!
//! What changed is where a person looks. The bench's twenty-two methods and
//! its key handler were a contiguous 1,300 lines inside an 11,000-line file
//! about drawing terminals, and the retro named that as the first thing it
//! would undo: the next developer had to know the bench was in there to find
//! any of it. A guard in pane's tests now fails if a `fn bench_` grows back
//! in the parent.
//!
//! The rule for what goes here versus `workbench.rs`: this file is the
//! terminal view REACHING the bench — keystrokes, clicks, the PTY, the
//! clipboard, the element tree. Decisions live in `workbench.rs` as pure
//! functions with table tests, because a renderer is not a position an
//! assertion can reach.

use super::*;

impl TerminalView {
    /// Un-bend a pointer and find the bench control under it.
    ///
    /// Returns the hit and the FLAT point, because the composer needs the
    /// point as well as the fact — it turns it into a caret position through
    /// the text layout, which was laid out flat.
    pub(super) fn bench_hit_at(
        &self,
        at: gpui::Point<gpui::Pixels>,
    ) -> Option<(crate::workbench::Hit, gpui::Point<gpui::Pixels>)> {
        let b = (*self.content_bounds.lock().ok()?)?;
        let rect = (
            f32::from(b.origin.x),
            f32::from(b.origin.y),
            f32::from(b.size.width),
            f32::from(b.size.height),
        );
        let (k1, k2) = self.warp_k;
        let (fx, fy) = crate::workbench::unwarp(rect, k1, k2, f32::from(at.x), f32::from(at.y));
        let zones = self.wb_zones.borrow();
        let hit = crate::workbench::hit_at(&zones, fx, fy).cloned();
        // TD_HITDEBUG=1 prints the whole chain for a click — where the pointer
        // was, where it un-bent to, and what that landed on — because a
        // shell with no virtual pointer cannot press the surface itself, and
        // the only honest verification of the warp's inverse is a person's
        // click read back from the log. The grid's `viewport_cell` prints
        // under the same flag for the same reason.
        if std::env::var_os("TD_HITDEBUG").is_some() {
            eprintln!(
                "[bench-hit] pointer=({:.1},{:.1}) k=({:.3},{:.3}) flat=({fx:.1},{fy:.1}) zones={} -> {:?}",
                f32::from(at.x),
                f32::from(at.y),
                k1,
                k2,
                zones.len(),
                hit
            );
        }
        let hit = hit?;
        Some((hit, gpui::point(gpui::px(fx), gpui::px(fy))))
    }

    /// Act on a bench click. One `match`, so a control added to [`Hit`] is a
    /// control the compiler makes this handle.
    pub(super) fn bench_hit(
        &mut self,
        hit: crate::workbench::Hit,
        flat: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::workbench::Hit;
        match hit {
            Hit::Choose(i) => self.bench_choose(i, cx),
            Hit::PressNav(at) => self.bench_press_nav(at, cx),
            Hit::Verb { action, target } => self.bench_act(action, target, cx),
            Hit::Review => {
                self.wb_review = Some(0);
            }
            Hit::CloseCard => {
                self.bench.close_card();
            }
            Hit::Launch => cx.emit(OpenAgentLauncher),
            Hit::Composer => {
                self.bench_click(flat, cx);
                window.focus(&self.focus_handle, cx);
            }
            Hit::Arm => {
                if self.wb_compose.is_none() {
                    self.wb_compose = Some(crate::workbench::Line::new());
                }
                window.focus(&self.focus_handle, cx);
            }
            Hit::ToggleRail => self.bench.toggle_rail(),
            Hit::Shelf(shelf) => self.bench.set_shelf(shelf),
            Hit::OpenRow(id) => self.bench_open(&id, cx),
            Hit::GalleryBack => {
                if let Some(n) = self.wb_review.as_mut() {
                    *n = n.saturating_sub(1);
                }
            }
            Hit::GalleryForward => {
                let total = self.bench.reviewable().len();
                if let Some(n) = self.wb_review.as_mut() {
                    *n = (*n + 1).min(total.saturating_sub(1));
                }
            }
            Hit::GalleryClose => self.wb_review = None,
            Hit::Nothing => {}
        }
        cx.notify();
    }

    /// The bench's own keys, ahead of the terminal's.
    ///
    /// `true` when the bench took the keystroke — the gallery, the escape
    /// ladder, the composer in talking mode, and the reading-mode chords —
    /// and `false` when it is the terminal's after all. Moved out of `on_key`
    /// whole, so that the one function which decides where a key goes is the
    /// one function a person opens to find out.
    pub(super) fn bench_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        if self.bench.face() != crate::workbench::Face::Workbench {
            return false;
        }
        // The GALLERY first, and it takes every key.
        //
        // It is drawn over everything and it was opened by a deliberate
        // press, so attention is there — the arrows belong to it until it
        // closes. Swallowing the keys it does not use is the other half:
        // a left arrow falling through would walk the caret in a composer
        // hidden behind the overlay.
        if self.wb_review.is_some() {
            use crate::workbench::Gallery;
            let total = self.bench.reviewable().len();
            match crate::workbench::gallery_key(&ks.key) {
                Gallery::Back => {
                    if let Some(n) = self.wb_review.as_mut() {
                        *n = n.saturating_sub(1);
                    }
                }
                Gallery::Forward => {
                    if let Some(n) = self.wb_review.as_mut() {
                        *n = (*n + 1).min(total.saturating_sub(1));
                    }
                }
                Gallery::Close => self.wb_review = None,
                Gallery::Ignore => {}
            }
            cx.notify();
            cx.stop_propagation();
            return true;
        }
        let talking = self.wb_compose.is_some();
        if ks.key.as_str() == "escape" {
            // One layer at a time, and a question waiting on a person is
            // the floor — see [`crate::workbench::peel`] for why escape is
            // not allowed to take that one.
            use crate::workbench::Peel;
            let card_waits = matches!(
                self.bench.selected().map(|s| &s.kind),
                Some(crate::surface::Kind::Question(q))
                    if q.answer == crate::surface::Answered::Waiting
            );
            match crate::workbench::peel(
                self.wb_review.is_some(),
                talking,
                self.bench.selected().is_some(),
                card_waits,
            ) {
                Peel::Gallery => {
                    self.wb_review = None;
                    cx.notify();
                }
                Peel::Typing => {
                    self.wb_compose = None;
                    cx.notify();
                }
                Peel::Card => {
                    self.bench.close_card();
                    cx.notify();
                }
                Peel::Face => self.set_face(crate::workbench::Face::Terminal, cx),
                // Deliberately nothing. The TERM chip is the way out of a
                // pane that is waiting on you, because leaving should be a
                // move a person makes rather than the same key they have
                // been dismissing overlays with.
                Peel::Nothing => {}
            }
            cx.stop_propagation();
            return true;
        }
        if talking {
            // Paste is the one keystroke that cannot go straight through,
            // because what is on the clipboard may not be text at all.
            // See [`Self::bench_paste`].
            if crate::workbench::is_paste_chord(&ks.key, ks.modifiers.control, ks.modifiers.shift) {
                self.bench_paste(cx);
                cx.stop_propagation();
                return true;
            }
            // Everything else straight through, byte for byte. The echo
            // comes back from the agent itself, which is why this needs
            // no local editing model at all.
            if let Some(bytes) = keystroke_bytes(ks) {
                // The bytes have already gone; this applies the SAME edit
                // to the local mirror so the box can draw where the
                // agent's caret now is. See [`crate::workbench::Line`] for
                // why a mirror and not a model.
                if let Some(line) = self.wb_compose.as_mut() {
                    // One table, in `workbench`, so the conventions can be
                    // asserted: word motion, the kills, and the readline
                    // chords the agent's own editor answers to. A key that
                    // is not an edit is a character, and characters go in
                    // at the caret.
                    match crate::workbench::line_edit(
                        &ks.key,
                        ks.modifiers.control,
                        ks.modifiers.alt,
                    ) {
                        Some(edit) => line.apply(edit),
                        None => {
                            if let Some(c) = ks.key_char.as_deref() {
                                if !c.is_empty() && !c.chars().any(char::is_control) {
                                    line.insert(c);
                                }
                            }
                        }
                    }
                }
                self.send(bytes, cx);
            }
            cx.stop_propagation();
            return true;
        }
        // The rules themselves are a table in `workbench`, so they can be
        // asserted without a render.
        let answerable = match self.bench.selected().map(|s| &s.kind) {
            Some(crate::surface::Kind::Question(q))
                if q.answer == crate::surface::Answered::Waiting =>
            {
                Some(q.options.len())
            }
            _ => None,
        };
        let printable = ks
            .key_char
            .as_deref()
            .filter(|c| !c.is_empty() && !c.chars().any(char::is_control))
            .is_some();
        match crate::workbench::reading_key(ks.key.as_str(), printable, answerable) {
            crate::workbench::Reading::Down => {
                self.bench.step(1);
                cx.notify();
            }
            crate::workbench::Reading::Up => {
                self.bench.step(-1);
                cx.notify();
            }
            crate::workbench::Reading::NextShelf => {
                let shelves = crate::surface::Shelf::ALL;
                let at = shelves
                    .iter()
                    .position(|s| *s == self.bench.shelf())
                    .unwrap_or(0);
                self.bench.set_shelf(shelves[(at + 1) % shelves.len()]);
                cx.notify();
            }
            crate::workbench::Reading::Act => {
                let first = self
                    .bench
                    .selected()
                    .and_then(|s| s.actions.first().cloned());
                if let Some(action) = first {
                    if action.wants_comment() {
                        self.wb_compose = Some(crate::workbench::Line::new());
                        cx.notify();
                    } else {
                        self.bench_act(action, None, cx);
                    }
                }
            }
            crate::workbench::Reading::Choose(i) => self.bench_choose(i, cx),
            crate::workbench::Reading::Talk => {
                // Start talking, carrying the character that started it —
                // so there is no "click here first".
                self.wb_compose = Some(crate::workbench::Line::new());
                if let Some(bytes) = keystroke_bytes(ks) {
                    if let (Some(line), Some(c)) =
                        (self.wb_compose.as_mut(), ks.key_char.as_deref())
                    {
                        line.insert(c);
                    }
                    self.send(bytes, cx);
                }
            }
            crate::workbench::Reading::Ignore => {}
        }
        cx.stop_propagation();
        true
    }

    /// How many surfaces this pane is holding that nobody has looked at.
    pub fn bench_unseen(&self) -> usize {
        self.bench.unseen_total()
    }

    /// What the agent is doing, in the header's own words.
    ///
    /// Read off the same three flags the header status uses rather than
    /// computed again here. Two surfaces disagreeing about whether an agent is
    /// waiting on you is the class of bug the attention rail's lane mapping
    /// was built to end, and it starts with a second copy of the rule.
    /// What this pane's agent is doing.
    ///
    /// The order is a priority: a pane can be several of these at once — an
    /// agent that finished its turn and is now asking is asking — and the
    /// question is always the one to report.
    pub(super) fn bench_status(&self) -> crate::workbench::AgentState {
        use crate::workbench::AgentState;
        // The live question is part of the answer, not just `needs_input`.
        //
        // Both read the same sensor, and the sensor blinks during a redraw —
        // so the title card said "Idle" directly above a question card that
        // was still up and still waiting on somebody. A surface that
        // contradicts itself is worse than either half of it alone, and the
        // card is the half with the evidence: it is holding an actual
        // question. See [`crate::workbench::SETTLE_SWEEPS`].
        if self.needs_input || self.wb_live_q.is_some() {
            AgentState::Asking
        } else if self.bell_blocked() {
            AgentState::Blocked
        } else if self.bell {
            AgentState::Done
        } else if self.exited {
            AgentState::Exited
        } else if self.agent_is_thinking() {
            AgentState::Working
        } else {
            AgentState::Idle
        }
    }

    /// The question this pane is asking right now, as a surface — or the
    /// retirement of the one it has stopped asking.
    ///
    /// Called once a second from the workspace sweep, and it reads the SCREEN
    /// rather than the transcript because the transcript does not have it: an
    /// `AskUserQuestion` is buffered until its result arrives, so a pending
    /// question exists only as pixels until it stops being pending. See
    /// [`crate::screenread::question_on_screen`].
    pub fn live_questions(&mut self, now_ms: u64) -> Vec<crate::surface::Post> {
        use crate::surface::{Kind, Op, Post, Surface, Weight};
        use crate::workbench::LiveMove;
        // A LIST, because answering one question and being asked the next
        // happens between two sweeps and produces two facts at once: the old
        // one is over, and a new one has started. Returning a single Post
        // could only report the second, so the first was never retired.
        //
        // WHAT to do is decided by [`crate::workbench::live_move`] rather than
        // here, because the rule has a case that is easy to get wrong and
        // impossible to see: a parse that fails while the agent is still
        // waiting means the screen scrolled, NOT that the question is over.
        let mut out = Vec::new();
        let asking = self
            .needs_input
            .then(|| crate::screenread::question_on_screen(&self.live_rows()))
            .flatten();
        let now_id = asking.as_ref().map(crate::screenread::screen_question_id);
        let was = self.wb_live_q.clone();
        // Counted here rather than in the rule, because the rule is a pure
        // decision and this is the pane remembering what it has seen.
        self.wb_quiet = if self.needs_input {
            0
        } else {
            self.wb_quiet.saturating_add(1)
        };

        match crate::workbench::live_move(
            self.needs_input,
            now_id.as_ref(),
            was.as_ref(),
            self.wb_quiet,
        ) {
            LiveMove::Keep => return out,
            LiveMove::Retire => {
                if let Some(id) = was {
                    out.push(Post {
                        op: Op::Retire,
                        id,
                        pane: None,
                        surface: None,
                    });
                }
                self.wb_live_q = None;
                return out;
            }
            LiveMove::Replace => {
                if let Some(id) = was {
                    out.push(Post {
                        op: Op::Retire,
                        id,
                        pane: None,
                        surface: None,
                    });
                }
                self.wb_live_q = None;
            }
        }

        if let (Some(q), Some(id)) = (asking, now_id) {
            let title: String = q.question.chars().take(72).collect();
            let kind = Kind::Question(q);
            let actions = kind.default_actions();
            self.wb_live_q = Some(id.clone());
            out.push(Post {
                op: Op::Present,
                pane: None,
                surface: Some(Surface {
                    id: id.clone(),
                    title,
                    kind,
                    weight: Weight::default(),
                    actions,
                    source: None,
                    arrived_ms: now_ms,
                }),
                id,
            });
        }
        out
    }

    /// A single click on a rail row.
    ///
    /// A document opens in whatever this desktop opens documents with — HTML
    /// in a browser, Markdown wherever Markdown goes — because that is what a
    /// click on a file means everywhere else on this machine, and a terminal
    /// that invented its own viewer would override a choice the person
    /// already made in their MIME database.
    ///
    /// Anything that is not a document opens as a card over the conversation.
    pub fn bench_open(&mut self, id: &crate::surface::SurfaceId, cx: &mut Context<Self>) {
        let href = self.bench.get(id).and_then(|s| match &s.kind {
            crate::surface::Kind::Artifact(a) => Some(a.href.clone()),
            _ => None,
        });
        self.bench.select(id);
        if let Some(target) = href {
            open_with_system(&target);
            // Opened elsewhere, so the bench does not ALSO fill itself with a
            // card nobody asked for — the click meant "show me this", and the
            // desktop is now showing it.
            self.bench.close_card();
        }
        cx.notify();
    }

    /// The verb chips for the card that is open, if it has any.
    /// The answer chips for a question: one per option, each pressing the
    /// agent's own menu.
    ///
    /// One builder for both places a question can appear, because they are the
    /// same gesture and were built twice. The inline block had chips and the
    /// opened CARD had a list of numbered sentences and a `comment` button —
    /// so answering worked in the place you were not looking. Parker, with the
    /// two side by side: *"The decision tab work surface should look a LOT
    /// more like [the waiting block]"*.
    pub(super) fn answer_chips(
        &mut self,
        q: &crate::surface::Question,
        sk: &crate::skin::Skin,
        th: &Theme,
    ) -> gpui::Div {
        // The picker calls it `Next` on every question of a round but the
        // last, and the bench says whichever word the picker is showing —
        // promising "Submit" and delivering "Next" is a small lie that costs
        // a person one wasted press and all of their trust in the button.
        let submit_word = match &q.round {
            Some(r) if !r.submitting && r.answered() + 1 < r.total() => "NEXT",
            _ => "SUBMIT",
        };
        let answered = q.answer != crate::surface::Answered::Waiting;
        let chips: Vec<gpui::Div> = q
            .options
            .iter()
            .enumerate()
            .map(|(i, o)| {
                // A ticked box is lit the same way a chosen option is: it IS
                // the answer so far. Reading the tick out of the label was
                // what made a click flicker — the same option parsed two ways
                // one second apart — and this is where that state lands now.
                let lit = matches!(q.answer, crate::surface::Answered::Chose(n) if n == i)
                    || o.checked == Some(true);
                // No leading number.
                //
                // The digit is the TERMINAL's affordance — it is there so a
                // person can press 1 — and on a chip you click it is a
                // catalogue number in front of the word that matters. Parker:
                // *"1, and 2... no that is for the terminal if someone wants
                // to TYPE 1 or two - we are assuming a mouse user at this
                // point"*. The tick stays, because that is state.
                let label = match o.checked {
                    Some(true) => format!("\u{2713} {}", o.label),
                    _ => o.label.clone(),
                };
                // LIT MEANS CHOSEN, and nothing else — but an unlit option
                // is still a BUTTON.
                //
                // Taking the bloom off every chip was right and went one step
                // too far: it took the button with it, and `Submit answers`
                // and `Cancel` came out as two grey words in a row. Parker:
                // *"should look like buttons - and be coloured"*. So the
                // border and the padding are unconditional, and the three
                // things that vary are colour, weight and bloom.
                //
                // PRIMARY is where the agent's own cursor is sitting. That is
                // measured — the picker draws its gutter mark on the row it
                // would take if you pressed return — rather than guessed from
                // the label, which on a confirm would mean sniffing for the
                // word "submit" and getting it wrong in every other language
                // the agent might answer in.
                let primary = !answered && q.cursor == Some(i);
                let chip = crate::benchdraw::option_button(
                    sk.chip(lit || primary).child(label),
                    primary,
                    lit,
                    th,
                );
                if answered {
                    // A question already answered keeps its chips so the
                    // record reads the same as the decision did, but they do
                    // not press: answering twice sends a second keystroke to a
                    // menu that has already closed.
                    return chip;
                }
                chip.cursor_pointer()
                    .relative()
                    .child(crate::benchdraw::zone(
                        self.wb_zones.clone(),
                        crate::workbench::Hit::Choose(i),
                    ))
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(px(6.))
                    .children(chips),
            )
            // Review, beside Submit, and only once there is something to
            // review. A gallery of nothing is a button that punishes a press.
            .when(!self.bench.reviewable().is_empty() && !answered, |d| {
                d.child(sk.rule_h()).child(
                    div().flex().flex_row().gap(px(8.)).justify_end().child(
                        sk.chip(false)
                            .cursor_pointer()
                            .text_size(px(11.5))
                            .child("\u{21ba} REVIEW ANSWERS".to_string())
                            .relative()
                            .child(crate::benchdraw::zone(
                                self.wb_zones.clone(),
                                crate::workbench::Hit::Review,
                            )),
                    ),
                )
            })
            // The picker's own Submit, where it has one — on a row of its
            // own, behind a rule.
            //
            // Ticking boxes commits nothing without it, so a multi-select
            // without this chip is a question the bench can ask and cannot
            // answer. And it is not one of the options: wrapped in beside
            // `6 · Chat about this` it read as a seventh thing to pick.
            // Parker: *"SUBMIT lives in its own space"*. It is also the only
            // element in this card that glows, which is what makes the glow
            // legible again — one primary action, one bloom.
            .when_some(q.submit.filter(|_| !answered), |d, at| {
                d.child(sk.rule_h()).child(
                    div().flex().flex_row().justify_end().child(
                        crate::benchdraw::verb_button(
                            sk.chip(true)
                                .cursor_pointer()
                                .child(format!("\u{2714} {submit_word}")),
                            true,
                            th,
                        )
                        .relative()
                        .child(crate::benchdraw::zone(
                            self.wb_zones.clone(),
                            crate::workbench::Hit::PressNav(at),
                        )),
                    ),
                )
            })
    }

    pub(super) fn bench_verbs(&mut self, sk: &crate::skin::Skin, th: &Theme) -> Option<gpui::Div> {
        let surface = self.bench.selected()?;
        let actions = surface.actions.clone();
        let hunks: Vec<String> = match &surface.kind {
            crate::surface::Kind::Changeset(c) => c.hunks.iter().map(|h| h.id.clone()).collect(),
            _ => Vec::new(),
        };
        if actions.is_empty() {
            return None;
        }
        // WHAT EACH BUTTON WILL DO, in the bytes it will do it with. A chip
        // said `reject x`, and `x` was whatever the agent had put in its own
        // hunk id — so the label came from the one party the click is meant
        // to be a check on. The lines under the row are produced by the same
        // function that produces the typed line, so the two cannot disagree,
        // and they are printed whole: an elided command that looks copyable
        // is a trap, and an elided instruction that looks readable is the
        // same trap.
        let tag = crate::surfacefeed::tag();
        let comment = self
            .wb_compose
            .as_ref()
            .map(|l| l.text().to_string())
            .filter(|c| !c.trim().is_empty());
        let where_to = format!(
            "\u{2192} pane {} \u{b7} {}{}",
            self.pane_id
                .map_or_else(|| "?".to_string(), |p| p.to_string()),
            self.mode.label(),
            self.staged
                .cwd
                .as_deref()
                .map_or(String::new(), |c| format!(" \u{b7} {c}")),
        );
        let previews: Vec<(String, String)> = actions
            .iter()
            .flat_map(|action| {
                let needs_part = matches!(
                    action,
                    crate::surface::Action::AcceptPart | crate::surface::Action::RejectPart
                );
                let targets: Vec<Option<String>> = if needs_part {
                    hunks.iter().map(|id| Some(id.clone())).collect()
                } else {
                    vec![None]
                };
                targets
                    .into_iter()
                    .map(|target| {
                        let chip = match &target {
                            Some(t) => {
                                format!("{} {}", action.label(), t.rsplit('/').next().unwrap_or(t))
                            }
                            None => action.label(),
                        };
                        let what = crate::workbench::verb_preview(
                            surface,
                            action,
                            target.as_deref(),
                            comment.as_deref(),
                            tag,
                        );
                        (chip, what)
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let shown = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .pt(px(2.))
            .font_family(th.font_family.clone())
            .text_size(px(9.5))
            .child(div().text_color(th.faint).child(where_to))
            .children(previews.into_iter().map(|(chip, what)| {
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(div().flex_none().text_color(th.faint).child(chip))
                    .child(div().text_color(th.text.alpha(0.72)).child(what))
            }));
        let row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(6.))
            .items_center()
            .children(actions.into_iter().flat_map(|action| {
                let needs_part = matches!(
                    action,
                    crate::surface::Action::AcceptPart | crate::surface::Action::RejectPart
                );
                let targets: Vec<Option<String>> = if needs_part {
                    hunks.iter().map(|id| Some(id.clone())).collect()
                } else {
                    vec![None]
                };
                let label = action.label();
                targets
                    .into_iter()
                    .map(|target| {
                        let text = match &target {
                            Some(t) => {
                                format!("{label} {}", t.rsplit('/').next().unwrap_or(t))
                            }
                            None => label.clone(),
                        };
                        // The PRIMARY verb is a button you can hit without
                        // aiming. `open` on an artifact is the whole point
                        // of the card — the reason a person opened it was
                        // to get to the thing — and it was drawn as a
                        // ten-point word in a row of ten-point words, all
                        // the same weight, none of them looking pressable.
                        // Parker: *"click to open the artifact needs to be
                        // a chunky button!"*. The rest stay chips: a card
                        // with five buttons has no primary verb either.
                        let primary = matches!(
                            action,
                            crate::surface::Action::Open | crate::surface::Action::Approve
                        );
                        let action = action.clone();
                        crate::benchdraw::verb_button(
                            sk.chip(primary)
                                .cursor_pointer()
                                .font_family(th.font_family.clone())
                                .child(text),
                            primary,
                            th,
                        )
                        .relative()
                        .child(crate::benchdraw::zone(
                            self.wb_zones.clone(),
                            crate::workbench::Hit::Verb {
                                action: action.clone(),
                                target: target.clone(),
                            },
                        ))
                    })
                    .collect::<Vec<_>>()
            }));
        Some(div().flex().flex_col().gap(px(6.)).child(row).child(shown))
    }

    /// Press one of the selected question's answers, by zero-based index.
    ///
    /// Public so the control socket can reach it: the chip and this call end
    /// up in exactly the same place, which is what makes the socket a test of
    /// the button rather than a second implementation of it.
    pub fn bench_choose(&mut self, index: usize, cx: &mut Context<Self>) {
        // OPTION index in, NAVIGATION index out. The picker puts its Submit
        // button between the last real option and the trailing `Chat about
        // this`, so from option five onward the two lists disagree by one and
        // an answer sent by option index lands on the wrong row. See
        // [`crate::workbench::nav_index`].
        let submit = match self.bench.selected().map(|s| &s.kind) {
            Some(crate::surface::Kind::Question(q)) => q.submit,
            _ => None,
        };
        let nav = crate::workbench::nav_index(index, submit);
        self.bench_act(crate::surface::Action::Choose, Some(nav.to_string()), cx);
    }

    /// A click in the composer: arm it, and put the caret where the pointer is.
    ///
    /// The caret does not merely move here — the agent's own line editor is
    /// told to move too, one arrow per column, because ITS caret is the one
    /// that decides where the next character lands. Moving only the drawing
    /// would put the block where the person clicked and the text somewhere
    /// else, which is worse than not offering the gesture at all.
    pub(super) fn bench_click(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        if self.wb_compose.is_none() {
            self.wb_compose = Some(crate::workbench::Line::new());
            return;
        }
        let Some(layout) = self.wb_slots.layout.borrow().clone() else {
            return;
        };
        let Some(line) = self.wb_compose.as_mut() else {
            return;
        };
        // `index_for_position` answers in BYTES and answers `Err` with the
        // nearest index when the point is outside the text — past the last
        // character, or below the last row. Both are ordinary: a person
        // clicking the empty space after a short line means the end of it.
        let byte = match layout.index_for_position(at) {
            Ok(i) => i,
            Err(i) => i,
        };
        let to = line
            .text()
            .char_indices()
            .position(|(i, _)| i >= byte)
            .unwrap_or(line.chars())
            .min(line.chars());
        let bytes = crate::workbench::caret_move(line.caret(), to);
        line.seek(to);
        if !bytes.is_empty() {
            self.send(bytes, cx);
        }
    }

    /// Press a row of the agent's menu by its NAVIGATION index.
    ///
    /// The raw half of [`Self::bench_choose`], for the rows that are not
    /// options at all: the picker's Submit button sits in the same up/down
    /// order and is pressed the same way, but it has no option number to be
    /// translated from.
    pub(super) fn bench_press_nav(&mut self, nav: usize, cx: &mut Context<Self>) {
        self.bench_act(crate::surface::Action::Choose, Some(nav.to_string()), cx);
    }

    /// Paste into the agent — text, files, or an IMAGE.
    ///
    /// A pseudoterminal carries bytes, so an image cannot be typed into one.
    /// What can be typed is a PATH, and every agent worth pasting an image
    /// into already knows how to read one: Claude Code opens the file and
    /// looks at it, and so does anything else that takes a filename. So a
    /// pasted image is written next to the pane's other state and its path is
    /// typed, which turns "I cannot paste a screenshot into a terminal" into
    /// one keystroke without inventing a transport.
    ///
    /// No trailing return. A paste is material for a sentence, not the
    /// sentence — the person finishes typing and presses enter themselves.
    pub(super) fn bench_paste(&mut self, cx: &mut Context<Self>) {
        use gpui::ClipboardEntry;
        // An image on the clipboard WINS over the text beside it, because a
        // person who copied a picture meant the picture. gpui's own read
        // cannot make that call: it tries text first and only falls back to an
        // image when no text type is offered at all, so a browser copy — which
        // offers `text/html` next to `image/png` — pastes markup.
        if self.clipboard_holds_an_image() {
            // On an AGENT pane the agent does this better than we can. Claude
            // Code binds ctrl+v to its own image paste, reads the clipboard
            // with `wl-paste --type image/png`, and puts `[Image #1]` in the
            // prompt — a chip it can then delete with one backspace. Typing a
            // path instead produced a hundred-character filename in the middle
            // of a sentence; Parker, on seeing both: *"the terminal shows
            // `[Image #n]` --- We want the terminal short style"*.
            //
            // So on an agent pane the chord goes straight down the PTY and the
            // agent shows its own short form. We only save a file where there
            // is nobody on the far end to do it — a shell has no idea what an
            // image is, and there a path is the only thing that can be typed.
            if self.mode.is_agent() {
                self.send(vec![0x16], cx);
                // The composer cannot show the agent's `[Image #7]` — it does
                // not know the number and inventing one would desynchronise
                // the mirror. It shows that an image went, which is the part
                // it does know. See [`crate::workbench::Line::note_paste`].
                if let Some(line) = self.wb_compose.as_mut() {
                    line.note_paste();
                }
                cx.notify();
                return;
            }
            if let Some(path) = self.clipboard_image_path() {
                self.bench_typed(path, cx);
                return;
            }
        }
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let mut parts: Vec<String> = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::String(text) => {
                    // A pasted newline would SUBMIT, mid-paste, and send half
                    // of what was pasted. Spaces instead — the agent gets the
                    // words and the person keeps the turn.
                    parts.push(text.text().replace(['\n', '\r'], " "));
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    parts.extend(paths.paths().iter().map(|p| p.display().to_string()));
                }
                ClipboardEntry::Image(image) => match self.save_pasted_image(image) {
                    Some(path) => parts.push(path),
                    None => eprintln!("terminal-delight: could not save the pasted image"),
                },
            }
        }
        let text = parts.join(" ");
        self.bench_typed(text, cx);
    }

    /// Put text into the agent's line as though it had been typed.
    ///
    /// No trailing return: what is pasted is material for a sentence, not the
    /// sentence. The local copy is only so the box has something to draw
    /// before the agent's echo arrives.
    pub(super) fn bench_typed(&mut self, text: String, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        if let Some(line) = self.wb_compose.as_mut() {
            line.insert(&text);
        }
        self.send(text.into_bytes(), cx);
    }

    /// Does the clipboard hold an image at all?
    ///
    /// The type LIST, asked once, rather than attempting a read and reading
    /// the failure — a compositor with nothing on the clipboard and one
    /// holding a picture fail an image read identically, and the difference
    /// decides which of two quite different pastes happens.
    pub(super) fn clipboard_holds_an_image(&self) -> bool {
        self.clipboard_image_mime().is_some()
    }

    /// Which image type the clipboard is offering, if any.
    pub(super) fn clipboard_image_mime(&self) -> Option<&'static str> {
        use std::process::Command;
        std::env::var_os("WAYLAND_DISPLAY")?;
        let listed = Command::new("wl-paste").arg("--list-types").output().ok()?;
        if !listed.status.success() {
            return None;
        }
        let types: Vec<String> = String::from_utf8_lossy(&listed.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        crate::workbench::best_image_mime(&types)
    }

    /// Ask the compositor's clipboard for an image, saved, and give its path.
    ///
    /// Two subprocesses and only on a paste, which is cheap enough to pay for
    /// the one thing it buys: the ability to paste a screenshot to an agent.
    /// `wl-paste` is asked for the type LIST first, so the decision of whether
    /// this clipboard even holds an image is made on evidence rather than by
    /// attempting a read and interpreting the failure.
    ///
    /// [`None`] whenever this is not a Wayland session, `wl-paste` is not
    /// installed, or nothing on offer is an image — all three are ordinary,
    /// and the caller simply pastes text instead.
    pub(super) fn clipboard_image_path(&self) -> Option<String> {
        use std::process::Command;
        let mime = self.clipboard_image_mime()?;
        let ext = crate::workbench::ext_of_image_mime(mime)?;
        let got = Command::new("wl-paste")
            .args(["--no-newline", "--type", mime])
            .output()
            .ok()?;
        if !got.status.success() || got.stdout.is_empty() {
            return None;
        }
        self.write_paste(&got.stdout, ext)
    }

    /// Write a pasted image beside the pane's own state and return its path.
    ///
    /// Beside the surfaces rather than in `/tmp`: it belongs to this pane, an
    /// agent asked to look at it can still find it tomorrow, and it is swept
    /// with everything else when the session goes.
    pub(super) fn save_pasted_image(&self, image: &gpui::Image) -> Option<String> {
        let ext = match image.format {
            gpui::ImageFormat::Png => "png",
            gpui::ImageFormat::Jpeg => "jpg",
            gpui::ImageFormat::Webp => "webp",
            gpui::ImageFormat::Gif => "gif",
            gpui::ImageFormat::Svg => "svg",
            gpui::ImageFormat::Bmp => "bmp",
            gpui::ImageFormat::Tiff => "tiff",
            gpui::ImageFormat::Ico => "ico",
            gpui::ImageFormat::Pnm => "pnm",
        };
        self.write_paste(&image.bytes, ext)
    }

    /// The writing half, shared by both routes in.
    ///
    /// Named `pasted-<n>.<ext>`, counting up, deduplicated by content.
    ///
    /// The first version named the file by its content hash, which was tidy
    /// and produced `15868dd2f4a1c093.png` — and an agent that then declared
    /// it put "Bench Paste 15868dd2" on the bench as a TITLE. A filename is
    /// not private: it travels into deliverable lines, into rails, into
    /// whatever a person reads next, so it has to be a name rather than a
    /// checksum.
    ///
    /// Dedupe survives, because it was worth having: the hash moves to a
    /// sidecar, so the same screenshot pasted twice still resolves to one
    /// file, and the file is still called something a person can say out loud.
    pub(super) fn write_paste(&self, bytes: &[u8], ext: &str) -> Option<String> {
        use std::hash::{Hash, Hasher};
        let dir = self.bench_dir()?.join("pastes");
        std::fs::create_dir_all(&dir).ok()?;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut h);
        let stamp = dir.join(format!(".{:016x}", h.finish()));
        if let Ok(known) = std::fs::read_to_string(&stamp) {
            let path = dir.join(known.trim());
            if path.exists() {
                return Some(path.display().to_string());
            }
        }
        let n = (1..)
            .find(|n| !dir.join(format!("pasted-{n}.{ext}")).exists())
            .unwrap_or(1);
        let name = format!("pasted-{n}.{ext}");
        std::fs::write(dir.join(&name), bytes).ok()?;
        let _ = std::fs::write(&stamp, &name);
        Some(dir.join(&name).display().to_string())
    }

    /// Type a line into the composer and into the agent — without sending it.
    ///
    /// The bytes go down the pseudoterminal exactly as a person's keystrokes
    /// would, so the agent's own line editor holds the same text and its caret
    /// sits where ours does; what is missing is the return. See
    /// [`crate::workbench::Line`] on why the two are mirrored rather than one
    /// owning the other.
    pub fn bench_type(&mut self, line: &str, cx: &mut Context<Self>) {
        let text = line.replace(['\n', '\r'], " ");
        // APPEND to the mirror, because the bytes append on the far end.
        //
        // This REPLACED the shadow with the new text while sending the bytes
        // down a pseudoterminal whose line editor added them to what was
        // already there — so after a second call the box showed one fragment
        // and the agent held two, and the caret was wrong by the length of
        // the first. The composer diagnostic saw the result as three seams in
        // one submission, `sentencehalf` and `grow.Spin` and `Delight.I'm`,
        // and read them as a missing separator. They are not: a keystroke
        // stream has no separators either, and the verb is a keystroke
        // stream. What was missing was the mirror keeping up.
        //
        // No space is inserted. The caller controls spacing exactly as a
        // person typing does, and a verb that quietly added one would make
        // `bench type "half"` then `bench type "way"` unable to spell a word.
        match self.wb_compose.as_mut() {
            Some(existing) => existing.insert(&text),
            None => self.wb_compose = Some(crate::workbench::Line::holding(text.clone())),
        }
        self.send(text.into_bytes(), cx);
        cx.notify();
    }

    /// Say a whole line to the agent through the bench.
    ///
    /// The scripted composer: same destination, same encoding, one call. Used
    /// by `ctl bench say`, which is how a caller with no pointer tests the
    /// thing a pointer would do.
    pub fn bench_say(&mut self, line: &str, cx: &mut Context<Self>) {
        self.wb_compose = Some(crate::workbench::Line::holding(line));
        self.bench_send(cx);
    }

    /// Send whatever is in the composer to the agent, as if typed.
    pub(super) fn bench_send(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self
            .wb_compose
            .take()
            .map(|l| l.text().to_string())
            .filter(|t| !t.trim().is_empty())
        else {
            self.wb_compose = None;
            cx.notify();
            return;
        };
        self.session
            .notifier
            .notify(crate::workbench::typed_line(&text));
        // Stay on the bench. Flipping to the terminal on send was the first
        // thing that felt wrong about this surface: a person who just asked
        // something wants to watch the answer arrive where they asked it, and
        // the live strip above already shows the agent picking it up.
        cx.notify();
    }

    /// Where this pane's agent should drop its surfaces.
    ///
    /// Empty when the pane has no host id, which is the honest answer: a
    /// window-owned terminal is not addressable by a session key, so there is
    /// no directory to name and the empty bench says so instead of printing a
    /// path that would never be watched.
    pub(super) fn bench_dir(&self) -> Option<std::path::PathBuf> {
        let pane = self.pane_id?;
        let key = crate::surfacefeed::session()?;
        Some(crate::surfacefeed::pane_dir(key, pane))
    }

    /// Carry out a press.
    ///
    /// Local verbs (open a document, open its source) are done by this window
    /// because the desktop already knows how. Everything else is the agent's
    /// business and is typed into the agent's own terminal — the channel that
    /// was already there, and the reason this feature needs no new pipe to
    /// send an answer down.
    pub(super) fn bench_act(
        &mut self,
        action: crate::surface::Action,
        target: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let comment = self
            .wb_compose
            .take()
            .map(|l| l.text().to_string())
            .filter(|c| !c.trim().is_empty());
        match self.bench.act(&action, target, comment) {
            crate::workbench::Dispatch::Open(href) => open_with_system(&href),
            crate::workbench::Dispatch::Tell(report) => {
                // The journal first: a line typed into a terminal can be eaten
                // by whatever the program is doing at that instant, and the
                // file is what makes the answer recoverable when it is.
                if let (Some(key), Some(pane)) = (crate::surfacefeed::session(), self.pane_id) {
                    let _ = crate::surfacefeed::journal(
                        &crate::surfacefeed::actions_path(key, pane),
                        &report,
                    );
                }
                // Tagged with this session's secret and typed the way the
                // composer types — a carriage return submits it and nothing
                // inside it can. See [`crate::hostproto::session_tag`].
                let line = report.to_prompt(crate::surfacefeed::tag());
                self.session
                    .notifier
                    .notify(crate::workbench::typed_line(&line));
                // Answering is looking: the person has dealt with this surface,
                // so the pane turns back to the conversation it just fed,
                // where the reply to what they said will appear.
                //
                // Only when there IS one. A shell pane has nothing to turn
                // back to, and facing it at a prompt that has just printed
                // "command not found" reads as the bench falling over rather
                // than as an answer being delivered.
                if self.mode.is_agent() {
                    self.bench.set_face(crate::workbench::Face::Terminal);
                }
            }
            crate::workbench::Dispatch::Keys { bytes, note } => {
                // Straight into the pane's pseudoterminal, because whatever is
                // waiting there is waiting for a person — a menu with its
                // cursor on the first option, or a REPL at a prompt. This is
                // the same path a keystroke takes; the bench is just typing.
                self.session.notifier.notify(bytes);
                if let (Some(key), Some(pane)) = (crate::surfacefeed::session(), self.pane_id) {
                    let _ = crate::surfacefeed::journal(
                        &crate::surfacefeed::actions_path(key, pane),
                        &crate::surface::ActionReport {
                            surface: self
                                .bench
                                .selected()
                                .map(|s| s.id.clone())
                                .unwrap_or(crate::surface::SurfaceId(String::new())),
                            action: action.clone(),
                            target: Some(note),
                            comment: None,
                        },
                    );
                }
            }
            crate::workbench::Dispatch::Refused(_why) => {
                // Refusals are shown by the button being absent rather than by
                // a toast. Nothing to do but repaint.
            }
        }
        cx.notify();
    }

    /// The bench, at whatever size this pane can give it.
    pub(super) fn bench_el(
        &mut self,
        th: &Theme,
        sk: &crate::skin::Skin,
        pane_w: f32,
        pane_h: f32,
        focused: bool,
    ) -> gpui::AnyElement {
        use crate::workbench::RailFit;
        // A fresh zone list per frame: the elements about to paint fill it.
        self.wb_zones.borrow_mut().clear();
        // Every size decision on this surface, resolved in one call and
        // asserted by a table of panes in `workbench`. The render draws what
        // this says; it no longer decides anything itself. Each of these was
        // once a condition written inline here, and each cost a round trip
        // with a photograph to find — a render is not a position you can make
        // an assertion about. See [`crate::workbench::shows`].
        let shows = crate::workbench::shows(
            pane_w,
            pane_h,
            self.mode.is_agent(),
            self.bench.rail_wanted(),
            self.wb_compose.is_some(),
        );
        let fit = shows.rail;
        let rail_px = match fit {
            RailFit::Open(w) => w as f32,
            RailFit::Ticks => crate::workbench::RAIL_TICK_W,
            RailFit::Hidden => 0.0,
        };
        let how = shows.how;
        let full = how == crate::workbench::Embodiment::Full;
        // TD_BENCHDEBUG=1 prints the numbers that decide this whole layout. It
        // found the rail-width bug in one run after two rounds of guessing from
        // screenshots — the bench looked broken because the rail was taking 208
        // of a 540-pixel pane, which no photograph says — and then found that a
        // composer photographed as ARMED was armed because the screenshot
        // harness had focused its own window and caught somebody's typing.
        //
        // The line says whether the composer is armed and HOW LONG the line is,
        // never the line itself. A debug switch that prints what a person is in
        // the middle of typing puts it in a log file, and the count answers the
        // same question.
        if std::env::var_os("TD_BENCHDEBUG").is_some() {
            eprintln!(
                "[bench] w={pane_w} h={pane_h} rail={rail_px} how={how:?} agent={} armed={} chars={} focus={focused}",
                self.mode.is_agent(),
                self.wb_compose.is_some(),
                self.wb_compose.as_ref().map_or(0, |l| l.chars())
            );
        }

        // ── what the agent is doing, in one line ────────────────────────────
        let live = self.mode.is_agent().then(|| {
            // One counter for the whole agent: how long it has been in the
            // state the bar names. Reset the moment the state changes, so
            // "Waiting on you · 2m" means two minutes of THIS wait.
            let state = self.bench_status();
            let now = crate::surfacefeed::now_ms();
            let since = match self.wb_state_since {
                Some((was, t)) if was == state => t,
                _ => {
                    self.wb_state_since = Some((state, now));
                    now
                }
            };
            crate::benchdraw::title_card(
                state,
                now.saturating_sub(since),
                self.tool_face.as_ref().map(|f| f.verb.as_str()),
                sk,
                th,
            )
        });

        // ── the main area ───────────────────────────────────────────────────
        //
        // The conversation by default, because "what is this agent doing" is
        // the question a person arriving at a pane has. A surface takes the
        // area only when it has been OPENED from the rail — a card over the
        // conversation, dismissed with esc or its own ✕, never something the
        // rail silently swapped in underneath the reader.
        // A card reads from the top and a conversation from the bottom, so the
        // one flex box they share cannot have a fixed alignment. Photographing
        // the build caught this: an opened table sat pinned to the floor of a
        // 900px pane under an acre of empty, because `justify_end` — correct
        // for a transcript — had been applied to the slot rather than to the
        // thing in it.
        let card_open = self.bench.selected().is_some();
        let body = match self.bench.selected() {
            Some(surface) => {
                let tint = crate::benchdraw::ink(crate::workbench::tint_of(&surface.kind), th);
                let drawn = crate::benchdraw::body(surface, how, sk, th);
                // A question opened from the rail is still a question, so it
                // gets the chips the inline block gets. Built before the verb
                // row because both borrow `self`.
                let asked = match &surface.kind {
                    crate::surface::Kind::Question(q) => Some(q.clone()),
                    _ => None,
                };
                let answers = asked.map(|q| self.answer_chips(&q, sk, th));
                let verbs = self.bench_verbs(sk, th);
                // One title, not two. The card drew `kind · title` here and
                // then [`benchdraw::body`] drew its own heading directly
                // underneath — the same two strings twice, six pixels apart,
                // which is what an opened artifact looked like in Parker's
                // screenshot. The renderer owns the heading, because the
                // renderer is what knows how a KIND wants to introduce
                // itself; the card keeps only the close, which is chrome.
                crate::benchdraw::raised(
                    sk.panel()
                        .relative()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .p(px(16.))
                        .bg(th.surface)
                        .border_l(px(3.))
                        .border_color(tint),
                    tint,
                    th,
                )
                .child(
                    div()
                        .absolute()
                        .right(px(10.))
                        .top(px(8.))
                        .text_size(px(13.))
                        .text_color(th.faint)
                        .cursor_pointer()
                        .child("\u{2715}")
                        .relative()
                        .child(crate::benchdraw::zone(
                            self.wb_zones.clone(),
                            crate::workbench::Hit::CloseCard,
                        )),
                )
                .child(drawn)
                .children(answers)
                .children(verbs)
            }
            // No card: the conversation, and whatever the agent is waiting on.
            None => {
                let tail = self.recent_lines(if full { 18 } else { 12 });
                let waiting = self.bench.waiting_question().and_then(|s| match &s.kind {
                    crate::surface::Kind::Question(q) => Some(q.clone()),
                    _ => None,
                });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(!self.mode.is_agent() && self.bench.is_empty(), |d| {
                        d.child(
                            crate::benchdraw::empty(false, "", sk, th).child(
                                sk.chip(true)
                                    .cursor_pointer()
                                    .text_size(px(12.))
                                    .child("\u{2301} LAUNCH AGENT")
                                    .relative()
                                    .child(crate::benchdraw::zone(
                                        self.wb_zones.clone(),
                                        crate::workbench::Hit::Launch,
                                    )),
                            ),
                        )
                    })
                    .when(shows.mirror, |d| {
                        d.child(crate::benchdraw::conversation(&tail, th))
                    })
                    .when_some(waiting, |d, q| {
                        let chips = self.answer_chips(&q, sk, th);
                        d.child(crate::benchdraw::waiting_block(&q, sk, th).child(chips))
                    })
            }
        };

        // ── the composer ────────────────────────────────────────────────────
        let composer = shows.composer.then(|| {
            crate::benchdraw::composer(
                self.wb_compose.as_ref(),
                focused,
                &shows,
                &self.wb_slots,
                sk,
                th,
            )
            .relative()
            .child(crate::benchdraw::zone(
                self.wb_zones.clone(),
                crate::workbench::Hit::Composer,
            ))
        });

        // ── the rail, and the handle that closes it ─────────────────────────
        let handle = (fit != RailFit::Hidden).then(|| {
            crate::benchdraw::rail_handle(matches!(fit, RailFit::Open(_)), th)
                .relative()
                .child(crate::benchdraw::zone(
                    self.wb_zones.clone(),
                    crate::workbench::Hit::ToggleRail,
                ))
        });
        let rail = match fit {
            RailFit::Hidden => None,
            RailFit::Ticks => {
                let ticks: Vec<gpui::Div> = self
                    .bench
                    .all_newest_first()
                    .take(24)
                    .map(|s| {
                        crate::benchdraw::rail_tick(
                            crate::workbench::tint_of(&s.kind),
                            false,
                            sk,
                            th,
                        )
                    })
                    .collect();
                Some(
                    div()
                        .w(px(crate::workbench::RAIL_TICK_W))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.))
                        .pt(px(6.))
                        .cursor_pointer()
                        .children(ticks)
                        .relative()
                        .child(crate::benchdraw::zone(
                            self.wb_zones.clone(),
                            crate::workbench::Hit::ToggleRail,
                        )),
                )
            }
            RailFit::Open(w) => {
                let shelf_now = self.bench.shelf();
                let tabs = div().flex().flex_row().gap(px(3.)).children(
                    crate::surface::Shelf::ALL.into_iter().map(|shelf| {
                        let (count, unseen) = self.bench.counts(shelf);
                        crate::benchdraw::shelf_tab(
                            shelf,
                            shelf == shelf_now,
                            count,
                            unseen,
                            sk,
                            th,
                        )
                        .relative()
                        .child(crate::benchdraw::zone(
                            self.wb_zones.clone(),
                            crate::workbench::Hit::Shelf(shelf),
                        ))
                    }),
                );
                let rows = self.bench.rows();
                // The RAIL wears the attention spine's own frame.
                //
                // It is the same object one scale down — a queue of things
                // wanting a person, at the right edge of a surface — and it
                // was drawn as a plain panel while the window's spine beside
                // it had a real border, a darkened fill and a phosphor bloom.
                // Parker: *"rework the right bar to reflect the styling we
                // landed on for the outer attention spine - truly that is
                // GOLD"*. See [`crate::benchdraw::spine_frame`], which uses
                // the spine's shadows rather than a second recipe that agrees
                // with them today.
                //
                // Tinted by what the shelf is HOLDING: a shelf with something
                // waiting frames in the waiting colour and lights up, a quiet
                // one frames dim. The rail then says whether it is worth
                // looking at before a single row is read.
                let waiting_here = self
                    .bench
                    .rows()
                    .iter()
                    .any(|r| r.standing == crate::workbench::Standing::Waiting);
                let frame_tint = if waiting_here {
                    crate::benchdraw::ink(crate::workbench::Tint::Waiting, th)
                } else {
                    th.accent
                };
                Some(
                    crate::benchdraw::spine_frame(
                        div()
                            .w(px(w as f32))
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .p(px(7.))
                            .overflow_hidden(),
                        frame_tint,
                        if waiting_here { 1.0 } else { 0.35 },
                        sk,
                        th,
                    )
                    .child(tabs)
                    .child(sk.rule_h())
                    // An empty shelf says where work would come from.
                    // The path is the one an agent computes for itself
                    // from its own environment, so a person reading it
                    // can drop a file there by hand and watch it land.
                    // "No decisions", not a filesystem path. The path is
                    // how an AGENT delivers a surface and it was written
                    // where a PERSON looks at an empty shelf — Parker:
                    // *"default text is for a robot... should be 'No
                    // artifacts' -- 'No decisions' etc."*. The path lives
                    // in the protocol doc, which is where somebody asking
                    // that question is already standing.
                    .when(rows.is_empty(), |d| {
                        d.child(
                            div()
                                .text_size(px(11.))
                                .text_color(th.faint)
                                .font_family(th.font_family.clone())
                                .child(format!("No {}", shelf_now.empty_word())),
                        )
                    })
                    .children(rows.into_iter().map(|row| {
                        let id = row.id.clone();
                        crate::benchdraw::rail_row(&row, sk, th).relative().child(
                            crate::benchdraw::zone(
                                self.wb_zones.clone(),
                                crate::workbench::Hit::OpenRow(id.clone()),
                            ),
                        )
                    })),
                )
            }
        };

        // The review gallery, drawn OVER everything rather than in place of
        // it. The thing underneath is a question somebody is part-way
        // through answering, and replacing it with a history is exactly the
        // navigation the flyout exists to avoid.
        let gallery = self.wb_review.and_then(|at| {
            let all = self.bench.reviewable();
            if all.is_empty() {
                return None;
            }
            let at = at.min(all.len() - 1);
            let item = all[at].clone();
            let total = all.len();
            let back = at > 0;
            let fwd = at + 1 < total;
            Some(
                // The centring wrapper: it fills the bench and puts the panel
                // in the middle of it, over whatever is underneath.
                div()
                    .absolute()
                    .inset_0()
                    .relative()
                    .child(crate::benchdraw::zone(
                        self.wb_zones.clone(),
                        crate::workbench::Hit::Nothing,
                    ))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        crate::benchdraw::review_flyout(
                            at,
                            total,
                            &item.title,
                            &item.answer,
                            sk,
                            th,
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(8.))
                                .items_center()
                                .child(
                                    sk.chip(back)
                                        .cursor_pointer()
                                        .child("\u{2190}".to_string())
                                        .relative()
                                        .child(crate::benchdraw::zone(
                                            self.wb_zones.clone(),
                                            crate::workbench::Hit::GalleryBack,
                                        )),
                                )
                                .child(
                                    sk.chip(fwd)
                                        .cursor_pointer()
                                        .child("\u{2192}".to_string())
                                        .relative()
                                        .child(crate::benchdraw::zone(
                                            self.wb_zones.clone(),
                                            crate::workbench::Hit::GalleryForward,
                                        )),
                                )
                                .child(div().flex_1())
                                .child(
                                    sk.chip(false)
                                        .cursor_pointer()
                                        .child("CLOSE".to_string())
                                        .relative()
                                        .child(crate::benchdraw::zone(
                                            self.wb_zones.clone(),
                                            crate::workbench::Hit::GalleryClose,
                                        )),
                                ),
                        ),
                    ),
            )
        });

        div()
            .relative()
            .size_full()
            .flex()
            .flex_row()
            .gap(px(4.))
            .p(px(10.))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .children(live)
                    // The conversation sits ON the composer, the way every
                    // conversation does: newest last, just above where you
                    // answer it. Top-aligned it floated in a field of empty
                    // pane with the input stranded at the far edge, and the
                    // two read as unrelated surfaces rather than one place.
                    //
                    // Clicking ANY of it arms the line. The composer already
                    // took its own click, and that was the wrong target size:
                    // Parker, on the surface as it stood — *"not quite clear
                    // enough with a SINGLE click EXACTLY where I am supposed
                    // to look or if I type right away or have to click again
                    // somewhere"*. The honest answer to "where do I click" is
                    // *anywhere*, so the whole body is the target, and the
                    // caret it lights is the thing to look at.
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.))
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .when(!card_open, |d| d.justify_end())
                            .cursor_text()
                            .when(self.mode.is_agent(), |d| {
                                d.relative().child(crate::benchdraw::zone(
                                    self.wb_zones.clone(),
                                    crate::workbench::Hit::Arm,
                                ))
                            })
                            .child(body),
                    )
                    .children(composer),
            )
            .children(handle)
            .children(rail)
            .children(gallery)
            .into_any_element()
    }
}
