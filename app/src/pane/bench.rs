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
use crate::workbench::Step;

impl TerminalView {
    /// Un-bend a pointer and look it up, quietly.
    ///
    /// The half of [`Self::bench_hit_at`] that the wheel and the hover share:
    /// both fire far more often than a click and neither wants a log line.
    /// `None` only while the pane has no content bounds yet; a point over no
    /// zone is `Some((None, flat))`, because "on the bench but on nothing" is
    /// an answer the wheel needs.
    pub(super) fn bench_flat(
        &self,
        at: gpui::Point<gpui::Pixels>,
    ) -> Option<(Option<crate::workbench::Hit>, gpui::Point<gpui::Pixels>)> {
        let b = (*self.content_bounds.lock().ok()?)?;
        let rect = (
            f32::from(b.origin.x),
            f32::from(b.origin.y),
            f32::from(b.size.width),
            f32::from(b.size.height),
        );
        let (k1, k2) = self.warp_k;
        let (fx, fy) = crate::workbench::unwarp(rect, k1, k2, f32::from(at.x), f32::from(at.y));
        let hit = crate::workbench::hit_at(&self.wb_zones.borrow(), fx, fy).cloned();
        Some((hit, gpui::point(gpui::px(fx), gpui::px(fy))))
    }

    /// Un-bend a pointer and find the bench control under it.
    ///
    /// Returns the hit and the FLAT point, because the composer needs the
    /// point as well as the fact — it turns it into a caret position through
    /// the text layout, which was laid out flat.
    pub(super) fn bench_hit_at(
        &self,
        at: gpui::Point<gpui::Pixels>,
    ) -> Option<(crate::workbench::Hit, gpui::Point<gpui::Pixels>)> {
        let (hit, flat) = self.bench_flat(at)?;
        // TD_HITDEBUG=1 prints the whole chain for a click — where the pointer
        // was, where it un-bent to, and what that landed on — because a
        // shell with no virtual pointer cannot press the surface itself, and
        // the only honest verification of the warp's inverse is a person's
        // click read back from the log. The grid's `viewport_cell` prints
        // under the same flag for the same reason.
        if std::env::var_os("TD_HITDEBUG").is_some() {
            let (k1, k2) = self.warp_k;
            eprintln!(
                "[bench-hit] pointer=({:.1},{:.1}) k=({:.3},{:.3}) flat=({:.1},{:.1}) zones={} -> {:?}",
                f32::from(at.x),
                f32::from(at.y),
                k1,
                k2,
                f32::from(flat.x),
                f32::from(flat.y),
                self.wb_zones.borrow().len(),
                hit
            );
        }
        Some((hit?, flat))
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
        // A click is on a pane that is on screen, whatever the sweep last
        // said: the sweep runs once a second and a tab can have changed since.
        self.wb_on_screen = true;
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
            Hit::EndAgent => self.bench_end_agent(cx),
            // A second press on the open dial closes it. A menu with no way
            // back out except picking something is a menu that has taken a
            // decision hostage.
            Hit::Dial(which) => {
                self.wb_dial = (self.wb_dial != Some(which)).then_some(which);
            }
            Hit::DialPick(which, at) => self.bench_dial_pick(which, at, cx),
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
            Hit::PickTab { id, group } => self.bench.pick_tab(&id, group),
            Hit::PickRegister { id, key } => self.bench.pick_register(&id, &key),
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

    /// A wheel turn over the bench, un-bent.
    ///
    /// Registered in the CAPTURE phase by [`Self::pointer_hook`], ahead of
    /// every bubble handler — gpui's own scroll container on the composer,
    /// and the pane root's terminal scroll — and it stops propagation
    /// whenever the pointer is on the bench, so neither of those ever runs
    /// flat. What it moves is [`crate::workbench::wheel_target`]'s call; how
    /// far, [`crate::workbench::wheel_offset`]'s.
    pub(super) fn bench_wheel(
        &mut self,
        ev: &ScrollWheelEvent,
        line_height: Pixels,
        cx: &mut Context<Self>,
    ) {
        if self.bench.face() != crate::workbench::Face::Workbench {
            return;
        }
        let Some((hit, _)) = self.bench_flat(ev.position) else {
            return;
        };
        match crate::workbench::wheel_target(
            hit.as_ref(),
            self.wb_card_at.is_some(),
            self.wb_mirror,
        ) {
            crate::workbench::Wheel::Composer => {
                let delta = ev.delta.pixel_delta(line_height);
                let at = self.wb_slots.scroll.offset();
                let max = self.wb_slots.scroll.max_offset();
                let y = crate::workbench::wheel_offset(
                    f32::from(at.y),
                    f32::from(delta.y),
                    f32::from(max.y),
                );
                self.wb_slots
                    .scroll
                    .set_offset(gpui::point(at.x, gpui::px(y)));
                cx.notify();
            }
            // The same three lines as the composer, against the card's own
            // handle. gpui does the clipping and the clamping; what it cannot
            // do is decide WHICH box the turn is over, because under the tube
            // its hit test is flat and the picture is bent.
            crate::workbench::Wheel::Card => {
                let delta = ev.delta.pixel_delta(line_height);
                let at = self.wb_card_scroll.offset();
                let max = self.wb_card_scroll.max_offset();
                let y = crate::workbench::wheel_offset(
                    f32::from(at.y),
                    f32::from(delta.y),
                    f32::from(max.y),
                );
                self.wb_card_scroll
                    .set_offset(gpui::point(at.x, gpui::px(y)));
                cx.notify();
            }
            crate::workbench::Wheel::Mirror => self.scroll_by_wheel(ev, cx),
            crate::workbench::Wheel::Nothing => {}
        }
        cx.stop_propagation();
    }

    /// The pointer's shape over the bench, from the UN-BENT position.
    ///
    /// Called from the pane's mouse-move handler. It only notifies on a
    /// change, so ordinary mousing costs nothing; [`Self::pointer_hook`]
    /// paints whatever this last decided.
    pub(super) fn bench_hover(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let pointer = self
            .bench_flat(at)
            .and_then(|(hit, _)| hit)
            .map_or(crate::workbench::Pointer::Arrow, |h| h.pointer());
        if pointer != self.wb_pointer {
            self.wb_pointer = pointer;
            cx.notify();
        }
    }

    /// The bench's pointer hook: one element, painted last and covering the
    /// bench, that owns the two things gpui would otherwise decide FLAT — the
    /// wheel, and the shape of the pointer.
    ///
    /// Its hitbox is what makes both honest. The wheel handler runs only
    /// while that hitbox is hovered, so a modal scrim occluding the pane —
    /// the FOCUS reader — keeps its own wheel. And the cursor request is made
    /// against that hitbox, painted after every child's, so it wins whenever
    /// the pointer is on the bench: a child's `cursor_pointer()` would have
    /// put the hand over where gpui laid the button, not where the tube shows
    /// it, so no child on the bench asks for a cursor any more.
    fn pointer_hook(&self, weak: gpui::WeakEntity<Self>) -> impl IntoElement {
        let pointer = match self.wb_pointer {
            crate::workbench::Pointer::Arrow => gpui::CursorStyle::Arrow,
            crate::workbench::Pointer::Text => gpui::CursorStyle::IBeam,
            crate::workbench::Pointer::Hand => gpui::CursorStyle::PointingHand,
        };
        gpui::canvas(
            |bounds, window, _cx| window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal),
            move |_bounds, hitbox, window, _cx| {
                window.set_cursor_style(pointer, &hitbox);
                window.on_mouse_event(move |ev: &ScrollWheelEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture || !hitbox.is_hovered(window) {
                        return;
                    }
                    let line_height = window.line_height();
                    let _ = weak.update(cx, |view, cx| view.bench_wheel(ev, line_height, cx));
                });
            },
        )
        .absolute()
        .inset_0()
    }

    /// Keep the end of the draft on screen after an edit at the end of it.
    ///
    /// The composer scrolls like a chat box (see `benchdraw::composer`), so an
    /// edit with the caret at its usual place asks the box for its bottom
    /// before the next frame. Whether it follows at all is
    /// [`crate::workbench::follows`]'s call: a caret parked earlier in a long
    /// draft leaves the view where the person scrolled it.
    fn composer_follows(&self) {
        if let Some(line) = self.wb_compose.as_ref() {
            if crate::workbench::follows(line.caret(), line.chars()) {
                self.wb_slots.scroll.scroll_to_bottom();
            }
        }
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
        // THE WINDOW'S CHORDS LEAVE FIRST, ahead of everything below — the
        // gallery included.
        //
        // Every path out of this function ends in `cx.stop_propagation()`, so
        // anything not declined here can never reach the workspace. That is
        // what stranded `alt+w`, `alt+r`, the split chords and the directional
        // focus keys on the workbench face: not a collision in any table, just
        // this handler running first and keeping what it could not use (#524).
        //
        // Above the gallery rather than below it, because "the gallery takes
        // every key" was a rule about NAVIGATION — an arrow falling through to
        // a composer hidden behind the overlay — and the window's chords were
        // never the gallery's to take. A plain arrow still reaches it:
        // [`crate::workbench::window_chord`] answers only for the modified
        // forms.
        if crate::workbench::window_chord(ks.key.as_str(), ks.modifiers.alt, ks.modifiers.control) {
            return false;
        }
        // The GALLERY next, and it takes every key.
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
                // Deliberately nothing, for both floors: a question waiting on
                // a person, and a bench with nothing left on it. alt+k and the
                // TERM chip are the ways out, because leaving should be a move
                // a person makes rather than the same key they have been
                // dismissing overlays with.
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
            if let Some(mut bytes) = keystroke_bytes(ks) {
                // The bytes have already gone; this applies the SAME edit
                // to the local mirror so the box can draw where the
                // agent's caret now is. See [`crate::workbench::Line`] for
                // why a mirror and not a model.
                if let Some(line) = self.wb_compose.as_mut() {
                    let selected = line.marked();
                    // One table, in `workbench`, so the conventions can be
                    // asserted: word motion, the kills, and the readline
                    // chords the agent's own editor answers to. A key that
                    // is not an edit is a character, and characters go in
                    // at the caret.
                    let replaced = match crate::workbench::line_edit(
                        &ks.key,
                        ks.modifiers.control,
                        ks.modifiers.alt,
                    ) {
                        Some(edit) => {
                            line.apply(edit);
                            selected
                                && matches!(
                                    edit,
                                    crate::workbench::Edit::Backspace
                                        | crate::workbench::Edit::Delete
                                )
                        }
                        None => {
                            let mut typed = false;
                            if let Some(c) = ks.key_char.as_deref() {
                                if !c.is_empty() && !c.chars().any(char::is_control) {
                                    line.insert(c);
                                    typed = true;
                                }
                            }
                            selected && typed
                        }
                    };
                    // A selected draft is REPLACED, and the far end has to be
                    // told so in bytes it already understands: its caret is at
                    // column zero (the ctrl+a that made the selection put it
                    // there), so one kill-to-end empties the line ahead of
                    // whatever this keystroke is. Without it the mirror would
                    // show the replacement and the agent would receive the
                    // replacement APPENDED to what was there.
                    if replaced {
                        let mut pre = crate::workbench::replace_bytes();
                        pre.append(&mut bytes);
                        bytes = pre;
                    }
                }
                self.composer_follows();
                self.bench_keystroke(bytes, cx);
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
        // A modified keystroke is not a character, however gpui fills its
        // `key_char` — see [`crate::workbench::types_a_character`].
        let printable = crate::workbench::types_a_character(
            ks.key_char.as_deref(),
            ks.modifiers.alt,
            ks.modifiers.control,
            ks.modifiers.platform,
        );
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
                } else if crate::workbench::return_launches(
                    crate::workbench::strip_verb(self.bench_status(), self.mode.is_agent()),
                    self.bench.selected().is_some(),
                ) {
                    // The one thing this bench offers. Until now return did
                    // nothing at all here — `Act` takes the selected surface's
                    // first verb, and a bench nobody has run an agent on has no
                    // surfaces to select — so the key that means *do the obvious
                    // thing* was the one key with no effect on the emptiest
                    // screen in the window.
                    cx.emit(OpenAgentLauncher);
                }
            }
            crate::workbench::Reading::Choose(i) => self.bench_choose(i, cx),
            crate::workbench::Reading::Talk => {
                // Nobody to talk to. The bench keeps the key rather than
                // starting a sentence into a shell.
                //
                // The composer is a MIRROR of the agent's own line editor and
                // not a buffer of our own, so a pane with no agent has nothing
                // for it to mirror: `shows()` draws no composer there, and
                // typing anyway opened an invisible one and put every character
                // down the pseudoterminal, where the shell gathered them into a
                // command line and the return key ran it (#509).
                if !self.mode.is_agent() {
                    cx.stop_propagation();
                    return true;
                }
                // Start talking, carrying the character that started it —
                // so there is no "click here first".
                self.wb_compose = Some(crate::workbench::Line::new());
                if let Some(bytes) = keystroke_bytes(ks) {
                    if let (Some(line), Some(c)) =
                        (self.wb_compose.as_mut(), ks.key_char.as_deref())
                    {
                        line.insert(c);
                    }
                    self.composer_follows();
                    self.bench_keystroke(bytes, cx);
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
        //
        // The ladder itself is [`crate::workbench::agent_state`], a pure
        // function with a table test; this is only the pane reading its
        // sensors. `reading` is the one sensor the bench owns: it typed an
        // answer within the window and nothing has moved since.
        let _ = AgentState::Idle;
        crate::workbench::agent_state(
            self.needs_input || self.wb_live_q.is_some(),
            self.bell_blocked(),
            self.bell,
            self.exited,
            self.agent_is_thinking(),
            self.reading_answer(crate::surfacefeed::now_ms()),
        )
    }

    /// End the agent in this pane, by asking it to quit the way a person does.
    ///
    /// Two `0x03`s down the pseudoterminal, through the same path the composer
    /// writes keystrokes on. Parker asked for a kill that ends the session
    /// *"abruptly"*, and this is as abrupt as his own hands are — but it is
    /// deliberately not a signal, for a reason that is the whole point of the
    /// button:
    ///
    /// **A signalled process never lowers the alternate screen.** The host
    /// holds a pane at `Claude` for as long as the alternate screen is up
    /// (`host::next_mode`), which is right — an agent shelling out to `rg` must
    /// not rename itself twice a second — so a `SIGKILL` would end the agent
    /// and then leave the pane reading as an agent forever. The LAUNCH verb
    /// this button exists to bring back would never appear. Asking the agent to
    /// quit gets the alternate screen lowered, the demotion made honestly, and
    /// the transcript flushed on the way out.
    ///
    /// A wedged agent is the case this cannot serve, and the escalation to a
    /// signal is [`docs/plans/bench-kill-and-relaunch`]'s second rung — it
    /// needs a host verb the running host does not have, so it is not here.
    fn bench_end_agent(&mut self, cx: &mut Context<Self>) {
        // Two, not one. A single interrupt cancels the turn; the second is
        // what quits, and sending them as one write keeps them inside the
        // harness's own double-press window rather than racing a paint.
        self.bench_deliver(vec![0x03, 0x03], cx);
        self.wb_dial = None;
    }

    /// The strip's right-hand run: the two dials, then the one verb.
    ///
    /// Built here rather than in [`crate::benchdraw`] because every element in
    /// it carries a click zone, and a zone is a statement about what a press
    /// MEANS — which is the one thing that file is asserted not to contain.
    ///
    /// The dials are drawn only while an agent is actually in the pane. A dial
    /// on an ended pane would be a control for changing the mind of something
    /// that is not there.
    fn strip_trailing(
        &mut self,
        state: crate::workbench::AgentState,
        agent_now: bool,
        sk: &crate::skin::Skin,
        th: &Theme,
    ) -> Vec<gpui::Div> {
        use crate::workbench::{Dial, Hit, StripVerb};
        let mut out: Vec<gpui::Div> = Vec::new();
        let pressable = agent_now && crate::workbench::dials_live(state);
        if agent_now {
            for which in [Dial::Model, Dial::Effort] {
                let (value, known) = self.dial_now(which);
                let mut chip = crate::benchdraw::dial(
                    &value,
                    known,
                    self.wb_dial == Some(which),
                    pressable,
                    sk,
                    th,
                );
                // No zone when it cannot be pressed. A control that looks
                // disabled and still fires is worse than one that does not
                // exist, and the un-bent hit test would happily record one.
                if pressable {
                    chip = chip.relative().child(crate::benchdraw::zone(
                        self.wb_zones.clone(),
                        Hit::Dial(which),
                    ));
                }
                out.push(chip);
            }
        }
        let (label, glyph, hit, primary) = match crate::workbench::strip_verb(state, agent_now) {
            // No glyph: the stop square rendered as a colour emoji, which put
            // the loudest thing on the strip beside the one control nobody
            // should press by accident. The words say what it does.
            StripVerb::End => ("END SESSION", "", Hit::EndAgent, false),
            StripVerb::Launch => ("LAUNCH AGENT", "\u{2301}", Hit::Launch, true),
        };
        out.push(
            crate::benchdraw::strip_button(label, glyph, primary, sk, th)
                .relative()
                .child(crate::benchdraw::zone(self.wb_zones.clone(), hit)),
        );
        out
    }

    /// The harness in this pane, from its mode.
    fn dial_harness(&self) -> crate::launcher::Harness {
        match self.mode {
            crate::pane::PaneMode::Codex => crate::launcher::Harness::Codex,
            _ => crate::launcher::Harness::Claude,
        }
    }

    /// What a dial is showing, and whether anybody actually SAID it.
    ///
    /// Three sources in order: what a press on this dial set, then what the
    /// pane's agent was STARTED with — a reading and not a guess, since the
    /// resume command is built from `/proc` and a `--model` on it is a fact
    /// about the process that is running — and finally the harness itself,
    /// which is the one thing still true when nobody has said anything. The
    /// `bool` is the difference between the middle two and the last: a value
    /// somebody chose reads as text, an inferred one reads faint, and neither
    /// reads as the other.
    ///
    /// **One resolver, two readers.** The button had this chain and the open
    /// list had a different, shorter one — `wb_model` alone — so a pane running
    /// a model it was LAUNCHED with showed `OPUS` on the button and lit nothing
    /// in the list underneath it. Parker: *"the current model is not
    /// highlighted in the options list"*. Two copies of "what is this dial on"
    /// can only ever agree by accident.
    fn dial_now(&self, which: crate::workbench::Dial) -> (String, bool) {
        use crate::workbench::Dial;
        let harness = self.dial_harness();
        let launched = self.runtime().resume;
        match which {
            Dial::Model => self
                .wb_model
                .clone()
                .map(|m| (m, true))
                .or_else(|| {
                    launched
                        .as_deref()
                        .and_then(|c| crate::workbench::flag_value(c, "--model"))
                        .map(|m| (m, true))
                })
                // Nobody said, so the button says the one thing that is true
                // anyway — which harness is in there. A faint CLAUDE is a
                // better button than a crisp `model ?`, and it still never
                // claims a model was chosen. It matches no row in the list,
                // which is correct: nothing is lit because nothing is known.
                .unwrap_or_else(|| (harness.label().to_string(), false)),
            Dial::Effort => self
                .wb_effort
                .map(|e| (e.id().to_string(), true))
                .or_else(|| {
                    launched
                        .as_deref()
                        .and_then(|c| {
                            crate::workbench::flag_value(c, "--effort").or_else(|| {
                                // Codex spells it as a config key.
                                crate::workbench::flag_value(c, "model_reasoning_effort")
                            })
                        })
                        .map(|e| (e, true))
                })
                // The level the harness runs at when nobody passes the flag —
                // TD's own claim, made in `default_effort`, and drawn faint
                // because nobody chose it here.
                .unwrap_or_else(|| (harness.default_effort().id().to_string(), false)),
        }
    }

    /// The list an open dial drops, which row of it is lit, and whether that
    /// row was CHOSEN or merely read off the launch command.
    ///
    /// The values are the harness's own — [`crate::launcher::Harness::models`]
    /// and [`crate::launcher::Harness::efforts`], already checked against
    /// `claude --help` and already clamped per harness. A second copy of that
    /// list here is how a menu goes stale and silently starts the wrong model.
    ///
    /// The lit row is whatever [`Self::dial_now`] says the BUTTON is showing,
    /// matched case-insensitively because the button uppercases what it draws
    /// and a `--model Opus` on somebody's launch command is the same model as
    /// `opus`. A value the list does not contain lights nothing — an agent
    /// started on a model this window does not offer is a fact, and inventing
    /// a nearest row for it would be a claim.
    fn dial_values(&self, which: crate::workbench::Dial) -> (Vec<String>, Option<usize>, bool) {
        use crate::workbench::Dial;
        let harness = self.dial_harness();
        let vals: Vec<String> = match which {
            Dial::Model => harness
                .models()
                .iter()
                .map(|m| m.label.to_string())
                .collect(),
            Dial::Effort => harness
                .efforts()
                .iter()
                .map(|e| e.id().to_string())
                .collect(),
        };
        let (now, chosen) = self.dial_now(which);
        let at = vals.iter().position(|v| v.eq_ignore_ascii_case(&now));
        (vals, at, chosen)
    }

    /// Take a value from an open dial: remember it, and tell the agent.
    ///
    /// `/model` and `/effort` are the harness's own commands — both present in
    /// the installed Claude Code and both taking an inline argument — so this
    /// needs no host verb, no wire change, and nothing about the running
    /// session upgraded.
    ///
    /// It goes in BESIDE the draft, through [`crate::workbench::aside_bytes`],
    /// and never through the composer. This used to be one `bench_say`, which
    /// puts its argument IN the composer and sends it — and the composer is the
    /// person's unsent prompt, sitting on the agent's line editor because that
    /// is what a mirror is. So changing the dial mid-sentence sent the sentence,
    /// at the strength being changed away from.
    fn bench_dial_pick(
        &mut self,
        which: crate::workbench::Dial,
        at: usize,
        cx: &mut Context<Self>,
    ) {
        use crate::workbench::Dial;
        let (vals, _, _) = self.dial_values(which);
        let Some(value) = vals.get(at).cloned() else {
            return;
        };
        // Recorded BEFORE the write, and recorded as what we asked for rather
        // than as what happened: this is the only claim the dial ever makes —
        // *this pane was told this* — and it stays true whether or not the
        // harness liked the value. If it did not, it says so on its own screen
        // in its own words, which is a better answer than a dial guessing.
        match which {
            Dial::Model => self.wb_model = Some(value.clone()),
            Dial::Effort => {
                let harness = match self.mode {
                    crate::pane::PaneMode::Codex => crate::launcher::Harness::Codex,
                    _ => crate::launcher::Harness::Claude,
                };
                self.wb_effort = harness.efforts().get(at).copied();
            }
        }
        self.wb_dial = None;
        let draft = self.wb_compose.clone().unwrap_or_default();
        let bytes = crate::workbench::aside_bytes(&format!("{} {value}", which.command()), &draft);
        // The erase takes any pasted image with it, and nothing this side can
        // type one back. Say so in the only place that can: the mirror stops
        // counting attachments the agent is no longer holding.
        if draft.pasted() > 0 {
            if let Some(line) = self.wb_compose.as_mut() {
                line.forget_pastes();
            }
        }
        self.bench_deliver(bytes, cx);
    }

    /// Did the bench type into this pane recently enough that the agent is
    /// still taking it in? See [`crate::workbench::READING_WINDOW_MS`].
    pub(super) fn reading_answer(&self, now_ms: u64) -> bool {
        self.wb_delivered_ms
            .is_some_and(|t| now_ms.saturating_sub(t) < crate::workbench::READING_WINDOW_MS)
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
        // While the agent is reading an answer the bench just typed, the
        // picker is still on screen and would be read back as a fresh,
        // unanswered question — which erased the choice the person made and
        // re-armed the chips, one second after they pressed. Nothing is
        // presented or retired in that window; the record the press left
        // stands until the agent moves.
        if self.reading_answer(now_ms) {
            return out;
        }
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
                    origin: crate::surface::Origin::Derived,
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
                    sk,
                    th,
                );
                if answered {
                    // A question already answered keeps its chips so the
                    // record reads the same as the decision did, but they do
                    // not press: answering twice sends a second keystroke to a
                    // menu that has already closed.
                    return chip;
                }
                chip.relative().child(crate::benchdraw::zone(
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
                            .text_size(px(sk.pt(Step::Small)))
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
                            sk.chip(true).child(format!("\u{2714} {submit_word}")),
                            true,
                            sk,
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
            .text_size(px(sk.pt(Step::Fine)))
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
                            sk,
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
            self.bench_keystroke(bytes, cx);
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
                self.bench_keystroke(vec![0x16], cx);
                // The composer cannot show the agent's `[Image #7]` — it does
                // not know the number and inventing one would desynchronise
                // the mirror. It shows that an image went, which is the part
                // it does know. See [`crate::workbench::Line::note_paste`].
                if let Some(line) = self.wb_compose.as_mut() {
                    line.note_paste();
                }
                self.composer_follows();
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
        self.composer_follows();
        self.bench_keystroke(text.into_bytes(), cx);
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
        self.composer_follows();
        // The same rules as a submitted line: keystrokes into a pane nobody is
        // looking at, or into one with no agent to read them, wait.
        self.bench_keystroke(text.into_bytes(), cx);
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

    /// Put bytes into the pseudoterminal — or, if nobody is looking at this
    /// pane, hold them until somebody is.
    ///
    /// The one place the bench writes to the terminal, so the rule lives
    /// once: a write happens only while the pane is on screen. Invisible
    /// authority — an instruction landing in a terminal nobody was watching,
    /// from a sender nobody could name — is the whole shape of the failure
    /// this window exists to avoid, and a queue is how it is refused without
    /// dropping anything. Every delivery stamps the reading window and the
    /// flash, so a write is something a person sees happen.
    pub(super) fn bench_deliver(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if self.bench_may_write() {
            let now = crate::surfacefeed::now_ms();
            self.session.notifier.notify(bytes);
            self.wb_delivered_ms = Some(now);
            self.wb_flash_until_ms = Some(now + 450);
        } else {
            self.wb_queued.push(bytes);
        }
        cx.notify();
    }

    /// Both conditions on a bench write, in one place.
    ///
    /// The pane has to be on screen — the rule this surface was built around —
    /// and there has to be an agent in there to receive it. The second was
    /// missing, and what fills the gap is not nothing: a terminal with no agent
    /// is a SHELL, and a shell reads a line and runs it. A person's message
    /// typed at a bash prompt is a command line, and `claude <the whole
    /// message>` is a valid one — which is how a bug report sent from the bench
    /// started a brand new session with itself as the argument and no history,
    /// while the bench went on drawing the conversation it thought it was
    /// talking to (#509).
    ///
    /// `Unknown` is not an agent either. A host-owned pane is born unread, and
    /// "we have not looked" must not be the state that lets a write through.
    fn bench_may_write(&self) -> bool {
        self.wb_on_screen && self.mode.is_agent()
    }

    /// One keystroke from the composer, under the same two rules as a line.
    ///
    /// Held rather than dropped, and in the same queue the lines use, so a
    /// stream that is interrupted mid-sentence arrives in the order it was
    /// typed. The enter key is a keystroke like any other on this path —
    /// `keystroke_bytes` turns it into `\r` — which is exactly why the gate
    /// has to be here and not only on the submit: `\r` is what makes a shell
    /// RUN what is sitting on its line.
    fn bench_keystroke(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if self.bench_may_write() {
            self.send(bytes, cx);
        } else {
            self.wb_queued.push(bytes);
        }
    }

    /// The workspace telling this pane whether it is in the active tab. Going
    /// on screen drains the queue, in order, as if each line had just been
    /// pressed — because for the person now looking, it just was.
    pub fn set_on_screen(&mut self, on: bool, cx: &mut Context<Self>) {
        let was = self.wb_on_screen;
        self.wb_on_screen = on;
        if on && !was {
            self.bench_drain(cx);
        }
    }

    /// Let go of whatever the bench is holding, if both rules now allow it.
    ///
    /// Called from the two places a held write can become deliverable — the
    /// pane coming on screen, and an agent appearing in it — and it re-checks
    /// both rather than assuming the caller's half is the only one outstanding.
    /// A queue drained on one condition while the other still fails is how a
    /// held line ends up in a shell.
    pub(super) fn bench_drain(&mut self, cx: &mut Context<Self>) {
        if self.wb_queued.is_empty() || !self.bench_may_write() {
            return;
        }
        let queued = std::mem::take(&mut self.wb_queued);
        for bytes in queued {
            self.bench_deliver(bytes, cx);
        }
    }

    /// Writes the bench is holding — because nobody is looking at this pane,
    /// or because there is no agent in it to read them.
    pub fn bench_queued(&self) -> usize {
        self.wb_queued.len()
    }

    /// Whether the bench is drawing that it just typed.
    pub(super) fn bench_flashing(&self, now_ms: u64) -> bool {
        self.wb_flash_until_ms.is_some_and(|until| now_ms < until)
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
        self.bench_deliver(crate::workbench::typed_line(&text), cx);
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
                self.bench_deliver(crate::workbench::typed_line(&line), cx);
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
                self.bench_deliver(bytes, cx);
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
        weak: gpui::WeakEntity<Self>,
    ) -> gpui::AnyElement {
        use crate::workbench::RailFit;
        // WHERE THE DIALS WERE, taken before the list is emptied.
        //
        // An overlay has to be placed while the tree is being BUILT, and a
        // control's position is only known once it has been LAID OUT — so the
        // newest measurement any frame can place with is the one the frame
        // before it recorded. That is exact here rather than approximate: the
        // dial is drawn on every frame, and the only frame whose position could
        // be stale is one where the strip reflowed in the same frame the list
        // opened, which no press can cause.
        let dial_rect = |which| {
            self.wb_zones
                .borrow()
                .iter()
                .rev()
                .find(|z| z.hit == crate::workbench::Hit::Dial(which))
                .map(|z| crate::workbench::Rect {
                    x: z.x,
                    y: z.y,
                    w: z.w,
                    h: z.h,
                })
        };
        let dial_was = [
            dial_rect(crate::workbench::Dial::Model),
            dial_rect(crate::workbench::Dial::Effort),
        ];
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
        // The wheel handler asks this between frames, so it is kept rather
        // than recomputed there — one decision, made once, in `shows`.
        self.wb_mirror = shows.mirror;
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

        // ── what the agent is doing, and the dials that change it ───────────
        //
        // The strip draws for a pane that HAS an agent or HAD one. The second
        // half is the state that never existed: an agent quitting demoted the
        // pane to a shell and took the whole bar away with it, along with the
        // only place a person could have started another one.
        let agent_now = self.mode.is_agent();
        let live = (agent_now || self.wb_had_agent).then(|| {
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
            // The turn's clock, tokens and in-flight call, off the same
            // status line the header's badge reads — one parse, two readers.
            let vitals = crate::workbench::turn_vitals(&self.agent_status());
            let tool = self.tool_face.as_ref().map(|f| f.verb.clone());
            let trailing = self.strip_trailing(state, agent_now, sk, th);
            crate::benchdraw::title_card(
                state,
                now.saturating_sub(since),
                vitals.as_ref(),
                tool.as_deref(),
                trailing,
                sk,
                th,
            )
        });

        // ── what YOU said, over the reply to it ─────────────────────────────
        //
        // Read off the pane's own scrollback rather than kept as a second
        // record of the conversation: the terminal already holds every turn,
        // including the ones typed at the terminal face instead of through
        // this composer, and a copy this side would be a second truth that
        // could disagree with the first. [`crate::workbench::ask_lines`] owns
        // whether it is drawn at all.
        let asked_above = crate::workbench::ask_lines(
            self.bench.shelf(),
            self.bench.standing_in(),
            agent_now,
            how,
        )
        .map(|n| {
            // Read one line longer than the block draws, so a message that ran
            // on can say so rather than stopping mid-word.
            let lines = crate::workbench::ask_clipped(self.last_human_message(n + 1), n);
            crate::benchdraw::asked(&lines, sk, th)
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
        // `showing` is the opened card, or on the overview the newest reply
        // standing in for one — see `Bench::showing`. Only an opened card
        // gets the close: the stand-in was not opened and cannot be closed,
        // and a ✕ that did nothing would be a control that lies.
        let card_open = self.bench.selected().is_some();
        // Is there a card IN the body — not "did somebody open one". The
        // overview stands the newest reply in the room without anybody opening
        // it, and that stand-in is a card in every way this code cares about:
        // it reads from the top, it can be taller than the pane, and the wheel
        // has to be able to reach it.
        let showing_id = self.bench.showing().map(|s| s.id.clone());
        // A card is a different document from the one before it, so the scroll
        // offset does not carry across — opening a short reply after scrolling
        // a long one would land in the middle of it, or past its end.
        //
        // Compared here rather than reset at each site that can change the
        // card (open a row, close one, change shelf, a new reply arriving,
        // a surface retired under the reader): one comparison cannot miss a
        // site, and five resets can.
        if self.wb_card_at != showing_id {
            self.wb_card_scroll
                .set_offset(gpui::point(gpui::px(0.), gpui::px(0.)));
            self.wb_card_at = showing_id.clone();
        }
        // The offer to start an agent, keyed on whether this pane HAS one —
        // never on whether its bench happens to be clean.
        //
        // It read `… && self.bench.is_empty()`, and that made the one pane
        // which had actually run an agent the one pane that could never start
        // another: the surfaces an agent presents outlive it, nothing clears
        // them on a mode change, so the condition was false from its first
        // reply onward. A pane whose agent presented nothing could still
        // offer. Parker: *"when I start a new agent in the workbench and then
        // end that agent session, I do not have the ability to start another
        // agent from the same workbench."*
        //
        // Emptiness is a fact about the RECORD. The offer is a question about
        // the PROCESS. A pane that has had an agent is excluded because its
        // strip carries the verb instead — one slot, two states, not two
        // buttons offering the same thing in different places.
        let offering = showing_id.is_none() && !agent_now && !self.wb_had_agent;
        let body = match self.bench.showing() {
            Some(surface) => {
                let tint = crate::benchdraw::ink(crate::workbench::tint_of(&surface.kind), th);
                let bench = &self.bench;
                // The picks the renderer cannot hold: which tab, and which
                // register inside whichever tab it resolves to. The closure is
                // what lets the renderer ask AFTER it has worked out the open
                // group, which is a thing only it can do — it is the half that
                // knows which groups this reply actually carries.
                let reg = |g: crate::surface::Group| {
                    bench.picked_register(&surface.id, g).map(str::to_string)
                };
                let picks = crate::benchdraw::Picks {
                    id: &surface.id,
                    tab: bench.picked_tab(&surface.id),
                    reg: &reg,
                    zones: self.wb_zones.clone(),
                };
                let drawn = crate::benchdraw::body(surface, how, Some(&picks), sk, th);
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
                        // The card's edge goes to the accent for the moment
                        // after the bench types into the terminal, so a write
                        // is something a person sees happen where they
                        // pressed — the agent bar turning to "Reading your
                        // answer" is the longer signal, this is the flash.
                        .border_color(if self.bench_flashing(crate::surfacefeed::now_ms()) {
                            th.accent
                        } else {
                            tint
                        }),
                    tint,
                    th,
                )
                .when(card_open, |card| {
                    card.child(
                        div()
                            .absolute()
                            .right(px(10.))
                            .top(px(8.))
                            .text_size(px(sk.pt(Step::Lead)))
                            .text_color(th.faint)
                            .child("\u{2715}")
                            .relative()
                            .child(crate::benchdraw::zone(
                                self.wb_zones.clone(),
                                crate::workbench::Hit::CloseCard,
                            )),
                    )
                })
                .child(drawn)
                .children(answers)
                .children(verbs)
            }
            // No card: the conversation.
            None => {
                let tail = self.recent_lines(if full { 18 } else { 12 });
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    // The offer is ONE element: a dialogue card with the action
                    // inside it.
                    //
                    // It was a panel with the button as a sibling above it, and
                    // before that a panel with the button as a chip tacked on
                    // its end. Both failed the same way — the only pressable
                    // thing on the surface and the sentence naming it were not
                    // in the same box, so they aligned independently and read as
                    // two unrelated blocks. A dialogue holds its own action.
                    .when(offering, |d| {
                        d.child(crate::benchdraw::empty(
                            false,
                            "",
                            Some(crate::benchdraw::launch_button(sk, th).child(
                                crate::benchdraw::zone(
                                    self.wb_zones.clone(),
                                    crate::workbench::Hit::Launch,
                                ),
                            )),
                            sk,
                            th,
                        ))
                    })
                    .when(shows.mirror, |d| {
                        d.child(crate::benchdraw::conversation(&tail, sk, th))
                    })
            }
        };

        // ── what the agent is blocked on, whatever else is on the bench ─────
        //
        // OUT of the match, and that is the fix rather than a tidy-up. It was
        // the last child of the `None` arm, so a waiting question could only
        // be drawn when there was no card — and the overview stands the newest
        // reply in the room the moment an agent presents one, which every
        // agent does at the end of every turn. So from an agent's first reply
        // onward the picker it was blocked on had nowhere to be drawn, while
        // `Bench::act` went on resolving `selected().or_else(waiting_question)`
        // and answering it perfectly. A live control with no drawing. Parker:
        // *"when we click chat about this in a multiple choice option, we do
        // not see that coming up on the workbench work surface as an
        // interactable frame."*
        //
        // It sits BELOW the body and outside its scroll, because being asked
        // something is not part of the document you happen to be reading and
        // must not be scrolled away from. See `terminal-delight#533`.
        let waiting = self
            .bench
            .waiting_question()
            .and_then(|s| match &s.kind {
                crate::surface::Kind::Question(q) => Some(q.clone()),
                _ => None,
            })
            .map(|q| {
                let chips = self.answer_chips(&q, sk, th);
                crate::benchdraw::waiting_block(&q, sk, th).child(chips)
            });

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
            crate::benchdraw::rail_handle(matches!(fit, RailFit::Open(_)), sk, th)
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
                                .text_size(px(sk.pt(Step::Small)))
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
                                        .child("\u{2190}".to_string())
                                        .relative()
                                        .child(crate::benchdraw::zone(
                                            self.wb_zones.clone(),
                                            crate::workbench::Hit::GalleryBack,
                                        )),
                                )
                                .child(sk.chip(fwd).child("\u{2192}".to_string()).relative().child(
                                    crate::benchdraw::zone(
                                        self.wb_zones.clone(),
                                        crate::workbench::Hit::GalleryForward,
                                    ),
                                ))
                                .child(div().flex_1())
                                .child(sk.chip(false).child("CLOSE".to_string()).relative().child(
                                    crate::benchdraw::zone(
                                        self.wb_zones.clone(),
                                        crate::workbench::Hit::GalleryClose,
                                    ),
                                )),
                        ),
                    ),
            )
        });

        // ── the open dial's list ────────────────────────────────────────────
        //
        // Drawn last and placed absolutely, UNDER THE DIAL THAT OPENED IT, so
        // it lands over the card rather than pushing it. A menu that reflows
        // the page it opens on is one that moves the thing you were reading;
        // one that always drops at the far end of the strip is one that does
        // not say which button it belongs to. Where it goes is
        // [`crate::workbench::dial_drop`]'s call, from the two rectangles the
        // previous frame measured.
        let dial_list = self.wb_dial.filter(|_| agent_now).map(|which| {
            let (vals, at, chosen) = self.dial_values(which);
            let rows: Vec<gpui::Div> = vals
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    crate::benchdraw::dial_row(v, at == Some(i), chosen, sk, th).child(
                        crate::benchdraw::zone(
                            self.wb_zones.clone(),
                            crate::workbench::Hit::DialPick(which, i),
                        ),
                    )
                })
                .collect();
            let was = match which {
                crate::workbench::Dial::Model => dial_was[0],
                crate::workbench::Dial::Effort => dial_was[1],
            };
            let holder = div().absolute();
            // The old fixed corner is the fallback and nothing else: on the
            // first frame a window ever paints, nothing has been measured, and
            // a list in the top-left would be worse than a list in the wrong
            // corner of the right area.
            let holder = match crate::workbench::dial_drop(was, *self.wb_bench_rect.borrow()) {
                Some((right, top)) => holder.right(px(right)).top(px(top)),
                None => holder.top(px(46.)).right(px(rail_px + 18.)),
            };
            holder.child(crate::benchdraw::dial_menu(rows, sk, th))
        });

        div()
            .relative()
            .size_full()
            .flex()
            .flex_row()
            .gap(px(4.))
            .p(px(10.))
            // WHERE THIS BOX IS. Everything absolutely positioned inside it is
            // placed in coordinates relative to here, and the rectangles those
            // placements are computed FROM are recorded in window space — so
            // without this measurement there is no way to turn one into the
            // other. First child, so it paints under everything and covers the
            // whole box; it carries no hit and takes no click.
            .child(crate::benchdraw::probe(self.wb_bench_rect.clone()))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(px(9.))
                    .children(live)
                    // Above the reply and OUTSIDE its scroll, for the same
                    // reason the waiting block sits below it: the question
                    // this card is answering is not part of the document, and
                    // scrolling a long reply must not take it off the surface.
                    .children(asked_above)
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
                    .child({
                        use crate::workbench::Anchor;
                        // `showing_id.is_some()` rather than `card_open`: the
                        // overview stands the newest reply in the room without
                        // anybody having opened it, and that stand-in IS a card
                        // — it reads from the top and was being bottom-anchored
                        // because nothing had selected it. Bottom belongs to the
                        // conversation and to nothing else, and it matters twice
                        // over now the body scrolls.
                        let anchor = crate::workbench::body_anchor(showing_id.is_some(), offering);
                        div()
                            // Stateful, because a scroll container IS state:
                            // gpui keeps the offset against this id between
                            // frames. Constant, which is safe because ids are
                            // unique within one view's tree and this is one
                            // box in one pane.
                            .id("bench-body")
                            .flex_1()
                            .min_h(px(0.))
                            .flex()
                            .flex_col()
                            // A CARD SCROLLS; a conversation does not.
                            //
                            // The two cannot share one rule, and the reason is
                            // the trap the composer already carries a
                            // paragraph about: a `justify_end` box overflows
                            // its TOP, and a gpui scroll container holds its
                            // offset between zero and the content's overhang,
                            // so an overhang at the top is on the wrong side of
                            // zero and the wheel can never reach it. The
                            // conversation wants `justify_end` and has its own
                            // scrollback elsewhere; the card wants neither.
                            //
                            // Before this the box was plain `overflow_hidden`
                            // for both, so a card taller than the pane was
                            // simply cut — with the folds already built and
                            // already unable to save it, because one unfolded
                            // section can exceed the pane on its own.
                            .when(showing_id.is_some(), |d| {
                                d.overflow_y_scroll().track_scroll(&self.wb_card_scroll)
                            })
                            .when(showing_id.is_none(), |d| d.overflow_hidden())
                            .when(anchor == Anchor::Bottom, |d| d.justify_end())
                            .when(self.mode.is_agent(), |d| {
                                d.relative().child(crate::benchdraw::zone(
                                    self.wb_zones.clone(),
                                    crate::workbench::Hit::Arm,
                                ))
                            })
                            // `Eye` is a box of four fifths the height with the
                            // content centred in it, rather than a justify on
                            // this container: the fraction has to resolve
                            // against the HEIGHT, and a padding fraction in
                            // taffy resolves against the width.
                            .child(match anchor {
                                Anchor::Eye => div()
                                    .flex()
                                    .flex_col()
                                    .justify_center()
                                    .h(gpui::relative(0.8))
                                    .child(body),
                                _ => div().flex().flex_col().child(body),
                            })
                    })
                    // Below the body and OUTSIDE its scroll. A question the
                    // agent is blocked on is not part of whatever document is
                    // open above it, and a person must not have to scroll back
                    // to a thing that is holding the session up.
                    .children(waiting)
                    .children(composer),
            )
            .children(handle)
            .children(rail)
            // After the body, so the list's zones are recorded after the
            // card's and win the lookup — last painted wins. Before the
            // gallery, which is a modal and must win over both.
            .children(dial_list)
            .children(gallery)
            // Last, so its hitbox and its cursor request are painted after
            // every control's — see the hook for why that order is the rule.
            .child(self.pointer_hook(weak))
            .into_any_element()
    }
}
