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

/// How many character positions one run may contribute to the highlight walk.
///
/// A guard on a pathological case rather than a tuning knob: a selection over
/// a pasted ten-thousand-word block would otherwise cost one layout query per
/// character, every frame, while the pointer is moving. Past this the run's
/// highlight is short and the copy is still complete — the clipboard does not
/// go through here.
const HIGHLIGHT_CAP: usize = 4096;

impl TerminalView {
    // -- dragging over the bench's text -------------------------------------

    /// How far the pointer may travel before a press stops being a click.
    ///
    /// Five pixels, which is the convention every native list on this desk
    /// uses. It exists because a hand resting on a trackpad moves: a threshold
    /// of zero turns every click on a rail row into a one-character selection
    /// and never opens the card.
    pub(super) const DRAG_SLOP: f32 = 5.0;

    /// Turn a flat point into a caret — which run, and how far into it.
    ///
    /// `None` when there is nothing selectable under or near the point, which
    /// is a real state rather than a failure: an empty bench, a frame that has
    /// not painted, or a pointer over the strip with the whole card scrolled
    /// away.
    pub(super) fn bench_caret_at(
        &self,
        flat: gpui::Point<gpui::Pixels>,
    ) -> Option<crate::workbench::Caret> {
        let (fx, fy) = (f32::from(flat.x), f32::from(flat.y));
        let atoms = self.wb_atoms.borrow();
        let i = crate::workbench::atom_at(&atoms.0, fx, fy)?;
        // The layout is the only thing that can answer this exactly: it walks
        // the wrapped lines the text system actually produced, so a click on
        // the second visual row of a wrapped paragraph lands where the reader
        // pointed rather than at the end of the first row. `Err` is its answer
        // for a point past the end of a line, and it carries the index it
        // would have used, which is the one we want.
        let byte = match atoms.1[i].index_for_position(flat) {
            Ok(b) | Err(b) => b,
        };
        let caret = crate::workbench::Caret {
            atom: i,
            byte: crate::workbench::on_boundary(&atoms.0[i].text, byte),
        };
        // TD_SELDEBUG=1 prints the run a pointer resolved to, and how many
        // runs were collected this frame. A shell with no virtual pointer
        // cannot drag the surface itself, so this is the only honest way to
        // establish that a given piece of text on the bench IS registered —
        // and a run missing from the list is missing from every copy
        // afterwards, silently. `TD_HITDEBUG` prints the click chain beside
        // it, under the same convention and for the same reason.
        if std::env::var_os("TD_SELDEBUG").is_some() {
            eprintln!(
                "[bench-sel] flat=({:.1},{:.1}) runs={} -> atom {} byte {} region {:?} {:?}",
                f32::from(flat.x),
                f32::from(flat.y),
                atoms.0.len(),
                caret.atom,
                caret.byte,
                atoms.0[i].region,
                atoms.0[i].text.chars().take(48).collect::<String>(),
            );
        }
        Some(caret)
    }

    /// Begin a selection at a flat point. `false` when there is nothing there
    /// to select, so the caller can fall through to whatever it would have
    /// done.
    pub(super) fn bench_select_from(&mut self, flat: gpui::Point<gpui::Pixels>) -> bool {
        let Some(caret) = self.bench_caret_at(flat) else {
            self.wb_sel = None;
            return false;
        };
        let text = self.wb_atoms.borrow().0[caret.atom].text.clone();
        self.wb_sel = Some((crate::workbench::Sel::at(caret), text));
        true
    }

    /// A press on the bench: either a control that acts now, or the start of
    /// something that might be a drag.
    ///
    /// **The card body and the rail rows act on RELEASE**, and they are the
    /// only two that do. Both are already click targets and both are the
    /// places worth dragging over, so a press there cannot be resolved until
    /// the button comes back up: acting immediately would open a card every
    /// time somebody selected a sentence on one. Everything else — buttons,
    /// tabs, dials, the composer's caret — still acts on push, because none of
    /// them is somewhere a drag begins.
    ///
    /// `true` when the press was taken and the caller should stop.
    pub(super) fn bench_press_at(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        landed: Option<(crate::workbench::Hit, gpui::Point<gpui::Pixels>)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((_, flat)) = self.bench_flat(at) else {
            return false;
        };
        let deferred = matches!(
            landed.as_ref().map(|(h, _)| h),
            Some(crate::workbench::Hit::Arm) | Some(crate::workbench::Hit::OpenRow(_)) | None
        );
        if !deferred {
            let (hit, flat) = landed.expect("not deferred means a hit landed");
            self.wb_sel = None;
            self.wb_press = None;
            self.bench_hit(hit, flat, window, cx);
            cx.notify();
            return true;
        }
        // A modal is up: no selection behind it, and the press keeps whatever
        // meaning the modal gave it.
        if self.wb_review.is_some() || self.wb_dial.is_some() {
            return false;
        }
        self.bench_select_from(flat);
        self.wb_press = Some((
            at,
            landed
                .map(|(h, _)| h)
                .unwrap_or(crate::workbench::Hit::Nothing),
        ));
        cx.notify();
        true
    }

    /// Extend a live selection to the pointer. `false` when no drag is in
    /// flight, so the terminal's own drag handling still runs on that face.
    pub(super) fn bench_drag_to(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.wb_press.is_none() {
            return false;
        }
        let Some((_, flat)) = self.bench_flat(at) else {
            return false;
        };
        let Some(head) = self.bench_caret_at(flat) else {
            return true;
        };
        if let Some((sel, _)) = self.wb_sel.as_mut() {
            if sel.head != head {
                sel.head = head;
                cx.notify();
            }
        }
        true
    }

    /// The button came back up. Either the press was a click after all, or a
    /// selection just settled.
    pub(super) fn bench_release_at(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((down, hit)) = self.wb_press.take() else {
            return false;
        };
        let moved = f32::from(at.x - down.x).hypot(f32::from(at.y - down.y));
        if moved <= Self::DRAG_SLOP {
            // A click after all. Drop the one-character selection the press
            // seeded — a bare click selects nothing, the way it does anywhere
            // else — and run what the press would have run.
            self.wb_sel = None;
            if !matches!(hit, crate::workbench::Hit::Nothing) {
                if let Some((_, flat)) = self.bench_flat(at) {
                    self.bench_hit(hit, flat, window, cx);
                }
            }
            cx.notify();
            return true;
        }
        // A real drag. Publish to the X11 PRIMARY selection, which is what
        // every other select-to-copy surface on this desk does and what makes
        // middle-click paste work without a keystroke.
        if let Some(text) = self.bench_selected_text() {
            cx.write_to_primary(ClipboardItem::new_string(text));
        }
        cx.notify();
        true
    }

    /// What the current bench selection would put on the clipboard, if
    /// anything.
    ///
    /// `None` rather than an empty string for an empty selection, so the
    /// callers can tell "nothing is selected" from "a selection of nothing" —
    /// and so `ctrl+shift+c` with no selection falls through to the terminal's
    /// own copy instead of clearing the clipboard.
    pub(super) fn bench_selected_text(&self) -> Option<String> {
        let (sel, anchored) = self.wb_sel.as_ref()?;
        if sel.is_empty() {
            return None;
        }
        let atoms = self.wb_atoms.borrow();
        if !sel.still_valid(&atoms.0, anchored) {
            return None;
        }
        let text = crate::workbench::copy_text(&atoms.0, sel);
        (!text.is_empty()).then_some(text)
    }

    /// Copy the bench selection. `false` when there is none, so the chord
    /// falls through to the terminal's copy underneath.
    pub(super) fn bench_copy(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(text) = self.bench_selected_text() else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        cx.write_to_primary(ClipboardItem::new_string(text));
        true
    }

    /// The highlight, as rectangles in the bench root's own coordinates.
    ///
    /// Built from the PREVIOUS frame's atom list, read at the top of the build
    /// immediately before that list is cleared — the same one-frame trick that
    /// places the dial drop-downs, and exact for the same reason: the newest
    /// measurement any frame can draw with is the one the frame before it
    /// recorded.
    ///
    /// One rectangle per wrapped visual row rather than one per run, because a
    /// selection that stops at the end of a run's box would draw a single wide
    /// band across a three-line paragraph instead of following its text.
    pub(super) fn bench_highlight(&self) -> Vec<crate::workbench::Rect> {
        let Some((sel, anchored)) = self.wb_sel.as_ref() else {
            return Vec::new();
        };
        if sel.is_empty() {
            return Vec::new();
        }
        let atoms = self.wb_atoms.borrow();
        if !sel.still_valid(&atoms.0, anchored) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (i, range) in crate::workbench::spans(&atoms.0, sel) {
            let (atom, layout) = (&atoms.0[i], &atoms.1[i]);
            let line = f32::from(layout.line_height()).max(1.0);
            // Walk the character boundaries and group them into visual rows by
            // the y the layout reports. Cheap for a normal selection and
            // bounded by the cap below for a pathological one.
            let mut row: Option<(f32, f32, f32)> = None; // (y, left, right)
            let mut push = |row: &mut Option<(f32, f32, f32)>| {
                if let Some((y, l, r)) = row.take() {
                    out.push(crate::workbench::Rect {
                        x: l,
                        y,
                        w: (r - l).max(1.0),
                        h: line,
                    });
                }
            };
            for (b, _) in atom.text[range.clone()]
                .char_indices()
                .map(|(o, c)| (range.start + o, c))
                .chain(std::iter::once((range.end, ' ')))
                .take(HIGHLIGHT_CAP)
            {
                let Some(p) = layout.position_for_index(b) else {
                    continue;
                };
                let (x, y) = (f32::from(p.x), f32::from(p.y));
                match row.as_mut() {
                    Some((ry, _, r)) if (y - *ry).abs() < line / 2.0 => *r = x.max(*r),
                    _ => {
                        push(&mut row);
                        row = Some((y, x, x));
                    }
                }
            }
            push(&mut row);
        }
        out
    }

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

    /// An open dial menu takes the next click, wherever it lands — including
    /// on nothing at all. Answers whether it took this one.
    ///
    /// Closing the menu used to be a second press on the dial and nothing
    /// else, so a person who opened one to look and then went back to reading
    /// left a list of effort levels floating over the card they were reading,
    /// over the spine, over everything, with no way out they would think to
    /// try. Parker, with one open across a whole screenshot: *"effort is stuck
    /// to the workbench after I clicked it open and did not change it"*.
    ///
    /// It is resolved BEFORE the zone lookup because most of the screen is not
    /// a zone: a click on the bench's empty background reaches no control at
    /// all, and that is the click most likely to mean *go away*. And it is
    /// SWALLOWED rather than passed through, which is what every other menu on
    /// this desk does — the press that dismisses a popup is not also a press
    /// on what the popup was covering.
    pub(super) fn bench_dismiss_dial(
        &mut self,
        hit: Option<&crate::workbench::Hit>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !crate::workbench::dial_dismisses(self.wb_dial.is_some(), hit) {
            return false;
        }
        self.wb_dial = None;
        cx.notify();
        true
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
            Hit::AddNote => self.bench_note_open(cx),
            Hit::EndAgent => self.bench_end_agent(cx),
            Hit::PauseTurn => self.bench_pause(cx),
            Hit::ResumeTurn => self.bench_resume(cx),
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
    ///
    /// Running first is also why the SIZE chord has to be answered here.
    /// ctrl+wheel is never a scroll anywhere in this window, and this handler
    /// consumes every turn on the bench — so until it asked, the workbench was
    /// the one surface whose own size dial could not be turned from it. The
    /// question is asked through [`TerminalView::size_by_wheel`] rather than
    /// answered locally, so the region under the pointer decides the dial here
    /// exactly as it does at the pane root: the header above the bench is
    /// still chrome, and still takes the chrome dial.
    pub(super) fn bench_wheel(
        &mut self,
        ev: &ScrollWheelEvent,
        line_height: Pixels,
        cx: &mut Context<Self>,
    ) {
        if self.bench.face() != crate::workbench::Face::Workbench {
            return;
        }
        if self.size_by_wheel(ev, cx) {
            cx.stop_propagation();
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
        let hit = self.bench_flat(at).and_then(|(hit, _)| hit);
        let pointer = hit
            .as_ref()
            .map_or(crate::workbench::Pointer::Arrow, |h| h.pointer());
        // A dragged file crossing the composer, which arrives here as an
        // ordinary mouse move: gpui turns the drag's motion into one, with
        // the paths parked on the app until the drop. A target that gives no
        // sign while you hover over it teaches people it does not work.
        //
        // `has_active_drag` says only that SOMETHING is being carried, not
        // what — and that is exact today, because the only drag this app
        // takes part in is a file drop (its tab, pane and slider drags are
        // hand-rolled state machines, not gpui drags). Add a real gpui drag
        // and this lights for it too while the drop does nothing; the fix
        // then is `on_drag_move::<ExternalPaths>`, which is scoped to the
        // type and fires on every pane rather than only the hovered one.
        let drop = cx.has_active_drag()
            && matches!(
                hit,
                Some(crate::workbench::Hit::Composer | crate::workbench::Hit::Arm)
            );
        // The HIT is compared too, not only the pointer's shape. Moving from
        // one chip to the next leaves the cursor a hand the whole way, so a
        // repaint gated on the shape alone would light the first chip and then
        // never move the light.
        if pointer != self.wb_pointer || drop != self.wb_drop || hit != self.wb_hover {
            self.wb_pointer = pointer;
            self.wb_drop = drop;
            self.wb_hover = hit;
            cx.notify();
        }
    }

    /// The hit zone for a pressable chip, and the lift that says it is one.
    ///
    /// Everything on the bench that a person can press ends in one of these,
    /// so this is the single place that decides what "the pointer is on it"
    /// looks like. See [`crate::benchdraw::zone_lit`] for why the wash belongs
    /// on the zone and not on the chip.
    pub(super) fn live_zone(
        &self,
        hit: crate::workbench::Hit,
        sk: &crate::skin::Skin,
    ) -> impl gpui::IntoElement {
        let lit = self.wb_hover.as_ref() == Some(&hit);
        crate::benchdraw::zone_lit(self.wb_zones.clone(), hit, sk, lit.then_some(sk.ink.hover))
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
                let wheel_weak = weak.clone();
                window.on_mouse_event(move |ev: &ScrollWheelEvent, phase, window, cx| {
                    if phase != gpui::DispatchPhase::Capture || !hitbox.is_hovered(window) {
                        return;
                    }
                    let line_height = window.line_height();
                    let _ = wheel_weak.update(cx, |view, cx| view.bench_wheel(ev, line_height, cx));
                });
                // Putting the drag DOWN, and taking it out of the window, are
                // the two ways the box stops being a target — and neither of
                // them is a mouse move over this pane, which is the only
                // event `on_mouse_move` delivers. A drop on a sibling pane
                // would otherwise leave this one lit, because a pane stops
                // hearing moves the moment the pointer is over its
                // neighbour. Deliberately no hitbox test on either: the drag
                // is over wherever it ended.
                let up_weak = weak.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, _window, cx| {
                    if phase == gpui::DispatchPhase::Bubble {
                        clear_drop(&up_weak, cx);
                    }
                });
                // Leaving the window is the one event in a drag that arrives
                // as itself: gpui turns enter, motion and drop into mouse
                // events and passes this one through.
                window.on_mouse_event(move |_: &gpui::FileDropEvent, phase, _window, cx| {
                    if phase == gpui::DispatchPhase::Bubble {
                        clear_drop(&weak, cx);
                    }
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

    /// The bench's own keys, and only its own.
    ///
    /// `true` when the bench took the keystroke — the gallery, the shelf chords,
    /// the note buffer, the escape ladder, the composer in talking mode, and the
    /// reading-mode chords — and **`false` when it is the terminal's after all**,
    /// which [`super::TerminalView::on_key`] answers by handing the key to the
    /// pseudoterminal underneath.
    ///
    /// That `false` is the whole of the fix this function was rewritten for.
    /// Every path out of it used to end in `cx.stop_propagation()`, so a key the
    /// bench had no use for died here: `ctrl+c` could not interrupt the agent
    /// whose turn was on the screen, and nothing below this call in `on_key`
    /// — the rename box, the note, the pane's own chords — ran at all.
    ///
    /// Two things it no longer does, because they are decided before it is
    /// called. It does not check the face: [`crate::keylayer::Layer::Bench`] is
    /// claimed only when the workbench is showing. And it does not hand back the
    /// window's chords: `keylayer` routes those to the workspace without asking.
    /// Nor does it stop propagation — `on_key` does that in one place, from what
    /// this returns.
    pub(super) fn bench_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
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
            return true;
        }
        // ALT+<n> LANDS ON A SHELF, above everything that could swallow a digit.
        //
        // Above `reading_key` in particular, which reads a bare digit as
        // answering option `n` of a waiting question — so this has to be the
        // thing that consumes the keystroke, not a branch further down that
        // happens to agree. The rule itself is [`crate::workbench::shelf_chord`],
        // where the modifiers are checked and the reasoning lives.
        if let Some(shelf) =
            crate::workbench::shelf_chord(ks.key.as_str(), ks.modifiers.alt, ks.modifiers.control)
        {
            self.bench.set_shelf(shelf);
            cx.notify();
            return true;
        }
        // A NOTE, on `alt+m`. Never on a bare `m`, which is a character.
        //
        // `m` is not in [`crate::keylayer::window_chord`]'s list, so the chord
        // reaches this handler rather than leaving for the workspace. It also
        // moves the board into view: asking for a note while looking at the
        // decisions shelf and then typing into a box on a different tab would be
        // writing somewhere the person cannot see.
        if ks.modifiers.alt && !ks.modifiers.control && ks.key.as_str() == "m" {
            self.bench_note_open(cx);
            return true;
        }
        let talking = self.wb_compose.is_some();
        // THE NOTE BUFFER TAKES ITS KEYS BEFORE `talking` IS EVEN ASKED.
        //
        // Order, not politeness. The branch below this one sends every
        // keystroke it receives straight down the pseudoterminal before
        // applying it locally, so a note falling through to it would type the
        // person's private words into the agent's prompt — the exact failure
        // this whole shelf is defined against. Both composers can be open at
        // once (a note started while a reply was half-written), and when they
        // are, the note is the one in front.
        if self.wb_note.is_some() {
            return self.bench_note_key(ks, cx);
        }
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
                self.wb_dial.is_some(),
                self.wb_review.is_some(),
                talking,
                self.bench.selected().is_some(),
                card_waits,
            ) {
                Peel::Dial => {
                    self.wb_dial = None;
                    cx.notify();
                }
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
            return true;
        }
        if talking {
            // Paste is the one keystroke that cannot go straight through,
            // because what is on the clipboard may not be text at all.
            // See [`Self::bench_paste`].
            if crate::workbench::is_paste_chord(&ks.key, ks.modifiers.control, ks.modifiers.shift) {
                self.bench_paste(cx);
                return true;
            }
            // THE COMPOSER IS A DOCUMENT, NOT A MIRROR. Nothing typed here
            // reaches the agent until SEND, and then it goes as ONE message
            // through the channel (`docs/spec/td-agent-channel.md` §6). The
            // mirror inherited the terminal's meaning for every key, which is
            // how ctrl+c in a text box ended a person's session — Parker:
            // *"the person will SHUT DOWN THEIR SESSION ACCIDENTALLy — I ahve
            // had this painpoint in the past!"*
            let ctrl = ks.modifiers.control;
            let key = ks.key.as_str();
            // ctrl+c COPIES, ctrl+x CUTS: the two chords every text box on
            // this desk answers to, and the first one is the reason for the
            // whole rework.
            if ctrl && !ks.modifiers.alt && key.eq_ignore_ascii_case("c") {
                self.bench_copy_draft(false, cx);
                return true;
            }
            if ctrl && !ks.modifiers.alt && key.eq_ignore_ascii_case("x") {
                self.bench_copy_draft(true, cx);
                return true;
            }
            // The interrupt is a key you AIM: ctrl+g here, and the strip's
            // PAUSE TURN. Not the copy chord, ever again.
            //
            // Both routes land on the same verb, so a turn stopped by the
            // chord reads as Paused and offers the resume exactly as one
            // stopped by the button does. Two gestures with one meaning that
            // left the pane in two different states would be the same bug as
            // two renderers drawing one question.
            if ctrl && !ks.modifiers.alt && key.eq_ignore_ascii_case("g") {
                self.bench_pause(cx);
                return true;
            }
            // Up on an empty draft recalls what this bench last sent — the
            // history the mirror used to borrow from the agent, kept here.
            let recalling =
                self.wb_recall.is_some() || self.wb_compose.as_ref().is_some_and(|l| l.is_empty());
            if crate::workbench::recalls_history(
                key,
                ctrl,
                ks.modifiers.alt,
                ks.modifiers.shift,
                recalling,
            ) {
                self.bench_recall(key == "up", cx);
                return true;
            }
            // One table, in `workbench`, so the conventions can be asserted:
            // word motion, the kills, the line break, undo. A key that is not
            // an edit is a character, and characters go in at the caret.
            match crate::workbench::line_edit(key, ctrl, ks.modifiers.alt, ks.modifiers.shift) {
                Some(crate::workbench::Edit::Submit) => {
                    self.bench_send(cx);
                    return true;
                }
                Some(edit) => {
                    if let Some(line) = self.wb_compose.as_mut() {
                        line.apply(edit);
                    }
                    // A recalled message that has been edited is a draft of
                    // its own; up and down walk its rows from here, not the
                    // history, or the edit would be thrown away by an arrow.
                    self.wb_recall = None;
                }
                None => {
                    if crate::workbench::types_a_character(
                        ks.key_char.as_deref(),
                        ks.modifiers.alt,
                        ctrl,
                        ks.modifiers.platform,
                    ) {
                        if let (Some(line), Some(c)) =
                            (self.wb_compose.as_mut(), ks.key_char.as_deref())
                        {
                            line.insert(c);
                            self.wb_recall = None;
                        }
                    }
                }
            }
            self.composer_follows();
            cx.notify();
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
                // WHICH SHELF YOU ARE READING DOES NOT CHANGE WHERE TYPING GOES.
                //
                // It did, for one build. Standing on the comments board opened
                // a note under the first character, on the reasoning that the
                // bench already works that way and the board should too —
                // Parker had praised the reply composer for exactly that: *"the
                // functionality of JUST TYPE (withough click sleecting the
                // prompt area) is SUPPPPER nice!"*. The extension was wrong and
                // he found it in a minute: *"if COMMENTS is selected and I type
                // ... the keystroke gets caught in the COMMENT instead of the
                // prompt --- this is WRONG! --- comment MUST require alt+m"*.
                //
                // The reason it is wrong is what "just type" was ever for. It
                // means there is ONE place a character goes and you never have
                // to aim at it. A shelf is a thing you are LOOKING at; making
                // it decide where your typing lands turns reading into a mode,
                // and a mode you entered by reading is one nobody chose. The
                // note box is opened on purpose — `alt+m`, or the `+ write a
                // note` row on the board — and it catches keys only once it is
                // open, which is [`Self::bench_note_key`]'s whole job.
                //
                // Nobody to talk to. The bench keeps the key rather than
                // starting a sentence into a shell.
                //
                // A pane with no agent has nobody to SEND to: `shows()` draws
                // no composer there, and a draft that could only ever land in
                // a shell — where the return key runs it as a command line
                // (#509) — is not offered.
                if !self.mode.is_agent() {
                    return true;
                }
                // Start talking, carrying the character that started it —
                // so there is no "click here first". Locally: the draft is
                // a document, and nothing leaves it until SEND.
                self.wb_compose = Some(crate::workbench::Line::new());
                self.wb_recall = None;
                if let (Some(line), Some(c)) = (self.wb_compose.as_mut(), ks.key_char.as_deref()) {
                    line.insert(c);
                }
                self.composer_follows();
                cx.notify();
            }
            // NOT OURS. The terminal underneath gets it — see the note on
            // `false` at the top of this function.
            crate::workbench::Reading::Pass => return false,
        }
        true
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
            self.needs_input || self.wb_live_q.is_some() || self.wb_channel.has_open_question(),
            self.bell_blocked(),
            self.bell,
            self.exited,
            self.agent_is_thinking(),
            self.reading_answer(crate::surfacefeed::now_ms()),
            self.wb_paused_ms.is_some(),
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
        //
        // Recorded first: this is the channel's one impure verb besides
        // `keys`, and the journal is what makes it visible in the record.
        self.journal_out(&crate::channel::Outbound::End);
        self.bench_deliver(vec![0x03, 0x03], cx);
        self.wb_dial = None;
        // Nothing left to resume. The pane is about to demote to a shell and
        // the strip is about to offer LAUNCH; a latch still down would put
        // RESUME TURN on a bench with no agent in it.
        self.wb_paused_ms = None;
    }

    /// Stop the turn that is running, and remember having stopped it.
    ///
    /// The strip's PAUSE TURN and `ctrl+g` on the composer — one `0x03`,
    /// recorded first, never the copy chord. This is the gesture Parker
    /// reaches for with his own hands: *"ctrl+c is the terminal command I
    /// usually use for this"*, and what it is for is not stopping the agent
    /// but stopping a turn that went out on the wrong model or the wrong
    /// effort.
    ///
    /// **One byte, not two.** [`Self::bench_end_agent`] sends the pair inside
    /// the harness's own double-press window, and that is the difference
    /// between the two controls: one interrupt cancels the turn, the second
    /// quits the session. Sending one and stopping is the whole feature.
    ///
    /// The stamp is what makes the pane readable afterwards. See
    /// [`crate::workbench::AgentState::Paused`] — a harness back at its prompt
    /// after an interrupt looks exactly like one that finished, and nothing on
    /// the screen can tell them apart.
    fn bench_pause(&mut self, cx: &mut Context<Self>) {
        self.journal_out(&crate::channel::Outbound::Interrupt);
        self.bench_deliver(vec![0x03], cx);
        self.wb_paused_ms = Some(crate::surfacefeed::now_ms());
    }

    /// Tell a paused turn to carry on — the strip's RESUME TURN.
    ///
    /// An ordinary message down the ordinary path, which is the point:
    /// Parker asked for a resume that *"just sends the message to the agent"*,
    /// and a bespoke pipe for one sentence would be a second way of talking to
    /// a harness that already has one. It is journalled as a `say` like any
    /// other, it joins the bench's own sent history, and it is held and
    /// drained under the same three rules if the pane is off screen.
    ///
    /// Whatever the dials were told while the turn was stopped has already
    /// gone down the same wire ahead of it, in order. The latch comes down in
    /// [`Self::bench_send`], which is where every message ends a pause.
    fn bench_resume(&mut self, cx: &mut Context<Self>) {
        self.bench_say(crate::workbench::RESUME_SAY, cx);
    }

    /// Let go of the pause when the turn has started again without us.
    ///
    /// Called from the workspace sweep, once a second, and it exists for the
    /// route the bench cannot see: a person pauses from the bench, flips to
    /// the TERM face, and types there. That turn is running and
    /// [`crate::workbench::agent_state`]'s `!thinking` guard already reports
    /// it correctly — but the latch would still be down when the turn ended,
    /// and the strip would offer to resume a turn that had just finished.
    ///
    /// Reading the sensor is the whole rule: a turn is running, so whatever
    /// was stopped is over.
    pub fn bench_pause_settle(&mut self) {
        if self.wb_paused_ms.is_some() && self.agent_is_thinking() {
            self.wb_paused_ms = None;
        }
    }

    /// Put the SELECTION on the clipboard — or the whole draft when nothing
    /// is selected — and take it out of the box when `cut`.
    ///
    /// The fallback to the whole draft is deliberate and is the older
    /// behaviour: a person who pressed ctrl+c with nothing highlighted meant
    /// the words in front of them, and a copy that silently did nothing would
    /// be the worse answer. What changed is that "nothing highlighted" is now
    /// a real question — until `Line` grew an anchor, select-all was the only
    /// selection there was, so this always took everything.
    fn bench_copy_draft(&mut self, cut: bool, cx: &mut Context<Self>) {
        let Some(line) = self.wb_compose.as_mut() else {
            return;
        };
        if line.is_empty() {
            return;
        }
        let partial = line.selected_text().map(str::to_string);
        let text = partial.clone().unwrap_or_else(|| line.text().to_string());
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        if cut {
            if partial.is_some() {
                // Delete consumes the selection and leaves the rest standing.
                line.apply(crate::workbench::Edit::Delete);
            } else {
                line.apply(crate::workbench::Edit::SelectAll);
                line.apply(crate::workbench::Edit::Delete);
            }
        }
        cx.notify();
    }

    /// Walk back (`up`) or forward (`down`) through what this bench has sent.
    ///
    /// The history the mirror used to get for free from the agent's own
    /// editor, kept here instead: the bench knows exactly what it sent. Walking
    /// past the newest entry lands on a fresh, empty draft.
    fn bench_recall(&mut self, up: bool, cx: &mut Context<Self>) {
        if self.wb_sent.is_empty() {
            return;
        }
        let n = self.wb_sent.len();
        let next = match (self.wb_recall, up) {
            (None, true) => Some(n - 1),
            (None, false) => None,
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < n => Some(i + 1),
            (Some(_), false) => None,
        };
        self.wb_recall = next;
        self.wb_compose = Some(match next {
            Some(i) => crate::workbench::Line::holding(self.wb_sent[i].clone()),
            None => crate::workbench::Line::new(),
        });
        self.composer_follows();
        cx.notify();
    }

    /// Record one thing this bench is about to do through the channel.
    ///
    /// Best effort and before the act: a line typed into a terminal can be
    /// eaten by whatever the program is doing at that instant, and the file is
    /// what makes the record recoverable when it is.
    fn journal_out(&self, record: &crate::channel::Outbound) {
        if let (Some(key), Some(pane)) = (crate::surfacefeed::session(), self.pane_id) {
            let _ = crate::surfacefeed::journal_event(
                &crate::surfacefeed::outbound_path(key, pane),
                &record.to_json(crate::surfacefeed::now_ms(), key, pane),
            );
        }
    }

    /// Records this pane's inbound journal gained since the last sweep, and
    /// what each one changes on the bench. See [`crate::channel::State::take`].
    /// Bind this bench to a conversation, and show that conversation's record.
    ///
    /// Called from the window's sweep, which is the only place that knows the
    /// answer: binding a pane to a conversation is a fact about the whole
    /// window — two panes may not hold one — and it is established once there
    /// and handed down.
    ///
    /// Idempotent by key. The sweep calls this every pass, so the work below
    /// happens on the edge where the key actually changes: a pane arriving at
    /// an agent, a conversation compacting into a new segment, a window opening
    /// on a conversation that already has a record.
    ///
    /// **Restored surfaces come back with [`crate::surface::Origin::Unknown`]**,
    /// and that is the honest word rather than a gap. Origin is stamped by the
    /// transport that accepted a document — which process sent it, and whether
    /// it was this pane's own agent — and none of that is in the document. A
    /// window reading a record days later has not re-established any of it, so
    /// it says so instead of repeating a claim it cannot check.
    pub(crate) fn bench_adopt_conversation(
        &mut self,
        key: crate::benchstore::ConvKey,
        bond: crate::vitals::Bond,
        cx: &mut Context<Self>,
    ) {
        self.wb_conv_bond = bond;
        if self.wb_conv.as_ref() == Some(&key) {
            return;
        }
        let dir = crate::benchstore::store_root();
        let mut writer = crate::benchstore::Writer::open(&dir, &key.root);
        // A boundary is a line like any other, and its name is its sequence
        // number, so a window restart that hands down a key it has seen before
        // does not add a second one. The record's history of compactions stays
        // a history of compactions rather than becoming one of window launches.
        writer.write(
            &dir,
            crate::benchstore::Rec::Segment {
                seq: key.seq,
                at_ms: crate::surfacefeed::now_ms(),
                source: if key.seq == 0 { "startup" } else { "continued" }.into(),
                agent: self.mode.label().to_string(),
                pid: self.shell_pid().unwrap_or(0),
            },
        );
        // Everything said before the window could name this conversation, in
        // the order it was said, shifted onto the end of what the record
        // already had. Written before the load below, so the load sees them.
        let base = writer.next_turn();
        for rec in std::mem::take(&mut self.wb_unfiled) {
            writer.write(&dir, rec.shifted(base));
        }
        let loaded = crate::benchstore::load(&dir, &key.root);
        self.wb_writer = Some(writer);
        self.wb_conv = Some(key);
        let now = crate::surfacefeed::now_ms();
        for (id, doc) in loaded.surfaces {
            // `bench.apply` rather than `present`: the two things `present`
            // adds are about a LIVE arrival — deciding whether an MCP caller
            // was this pane's own agent, and telling the channel a reply has
            // already been seen so the hook's copy of the same turn is
            // dropped. Replaying yesterday's record through either would
            // answer a question about today with an old fact.
            let post = crate::surface::parse_lenient(&doc, now, &id);
            if post.surface.is_some() {
                self.bench.apply(post);
            }
        }
        cx.notify();
    }

    /// Put this sweep's documents into the conversation's record.
    ///
    /// The mailbox copy is **not** removed: draining it is a separate change
    /// that needs a sentinel to tell a file this build has already filed from
    /// one that was there before it existed. Until then a restart re-delivers
    /// what it re-loads, and the two land on one row because the bench keys
    /// surfaces by id — the cost of not draining is a duplicate write, and the
    /// cost of draining too early is a surface that existed and then did not.
    pub(crate) fn bench_file_docs(&mut self, docs: &[(String, serde_json::Value)]) {
        if docs.is_empty() {
            return;
        }
        let dir = crate::benchstore::store_root();
        // The turn these answer is the one that is open: `wb_turn` is the ask
        // that has not been made yet. Before any ask has been recorded there is
        // no open turn and everything belongs to the first one.
        let now = crate::surfacefeed::now_ms();
        for (id, doc) in docs {
            match self.wb_writer.as_mut() {
                // `None` from the writer is an id that must never become a
                // record key. Skipped rather than repaired: a filing under a
                // name nobody chose is worse than a surface that is only on
                // the bench.
                Some(w) => {
                    w.write_surface(&dir, id, doc, self.wb_conv_bond, now);
                }
                None => {
                    if let Some(rec) = crate::benchstore::said(
                        self.wb_unfiled
                            .iter()
                            .filter_map(crate::benchstore::Rec::n)
                            .max()
                            .unwrap_or(0),
                        id,
                        doc,
                        self.wb_conv_bond,
                        now,
                    ) {
                        self.bench_write(rec);
                    }
                }
            }
        }
    }

    /// Put one line in the conversation's record, or hold it until there is a
    /// conversation to put it in.
    ///
    /// **The hold is the point.** A pane's first prompt arrives within a second
    /// of its agent starting, and the window cannot bind a pane to a
    /// conversation until that agent's process is far enough along to be
    /// identified — so on a fresh pane the binding is a sweep or two behind the
    /// first thing said. The channel journal is read by byte offset and the
    /// mailbox by mtime, so neither is delivered twice: a line dropped for want
    /// of a key is dropped for good, and the record would begin at whatever the
    /// window happened to notice first. Measured on a live demo window before
    /// this existed — the file opened with its segment line and no first ask,
    /// while the prompt sat in the mailbox unfiled.
    ///
    /// Held lines count their turns from zero and are shifted onto the end of
    /// whatever the conversation already had, when the key lands.
    fn bench_write(&mut self, rec: crate::benchstore::Rec) {
        let dir = crate::benchstore::store_root();
        match self.wb_writer.as_mut() {
            Some(w) => {
                w.write(&dir, rec);
            }
            // Bounded, because a pane that never binds must not grow a list
            // forever: a shell pane with a drop box, or an agent whose siblings
            // make it unidentifiable. What the cap costs is the OLDEST held
            // line, so the run that survives is the one nearest the moment a
            // key could arrive.
            None => {
                const HELD_CAP: usize = 256;
                if self.wb_unfiled.len() >= HELD_CAP {
                    self.wb_unfiled.remove(0);
                }
                self.wb_unfiled.push(rec);
            }
        }
    }

    /// Put one journal event in the conversation's record.
    ///
    /// From the raw event, before the channel folds it into bench state: the
    /// record is of what the conversation was asked and told, and the
    /// channel's own view of a round — which step is open, which press is
    /// pending — is working state that belongs to the live pane.
    ///
    /// The writer decides what is worth keeping and refuses a line it has
    /// already written, which is what makes a restart's replay of the whole
    /// journal cost nothing. A pane with no conversation yet holds the line
    /// instead; see `bench_write`.
    fn bench_record_event(&mut self, ev: &crate::channel::Inbound) {
        let dir = crate::benchstore::store_root();
        let now = crate::surfacefeed::now_ms();
        match self.wb_writer.as_mut() {
            Some(w) => {
                w.write_event(&dir, ev, now);
            }
            None => {
                let mut held = crate::benchstore::Writer::held(
                    self.wb_unfiled
                        .iter()
                        .filter_map(crate::benchstore::Rec::n)
                        .max()
                        .map_or(0, |n| n + 1),
                );
                if let Some(rec) = held.record_for(ev, now) {
                    self.bench_write(rec);
                }
            }
        }
    }

    pub fn channel_events(&mut self, events: Vec<crate::channel::Inbound>, cx: &mut Context<Self>) {
        use crate::channel::Effect;
        use crate::surface::{Op, Post};
        if events.is_empty() {
            return;
        }
        let now = crate::surfacefeed::now_ms();
        for ev in events {
            self.bench_record_event(&ev);
            match self.wb_channel.take(ev, now) {
                Effect::Asked { text } => {
                    // The record already has the whole of it, written from the
                    // raw event above. What the caption keeps is the first few
                    // lines, which is a drawing decision and not what the
                    // conversation should remember.
                    //
                    // The harness's own words outrank anything read off the
                    // screen, and once a pane has heard them the screen latch
                    // stops overwriting the caption — see `latch_asked`.
                    self.wb_asked = text
                        .lines()
                        .map(str::to_string)
                        .take(crate::screenread::ASKED_LINES)
                        .collect();
                    self.wb_asked_by_hook = true;
                    // The person has spoken, so whatever woke the agent before
                    // them is no longer what the reply below answers.
                    self.wb_woken = None;
                }
                Effect::Woken(w) => {
                    // Deliberately NOT `wb_asked`, and deliberately not
                    // `wb_asked_by_hook` either: a pane whose first channel
                    // record was a task notification used to latch that flag
                    // and blind the screen reader for the rest of its life, so
                    // one background job could cost the caption permanently.
                    self.wb_woken = Some(w);
                }
                Effect::Present(surfaces) => {
                    for s in surfaces {
                        let id = s.id.clone();
                        self.present(
                            Post {
                                op: Op::Present,
                                id,
                                pane: None,
                                surface: Some(s),
                            },
                            cx,
                        );
                    }
                }
                Effect::Reply { text, n, ended } => {
                    // The rounds this reply proved over, first: the reply is
                    // the agent having moved on, and a card still offering
                    // chips underneath that is the thing being fixed.
                    for s in ended {
                        let id = s.id.clone();
                        self.present(
                            Post {
                                op: Op::Present,
                                id,
                                pane: None,
                                surface: Some(s),
                            },
                            cx,
                        );
                    }
                    if let Some(s) = crate::channel::reply_surface(&text, now, n) {
                        let id = s.id.clone();
                        self.present(
                            Post {
                                op: Op::Present,
                                id,
                                pane: None,
                                surface: Some(s),
                            },
                            cx,
                        );
                    }
                }
                Effect::Nothing => {}
            }
        }
        cx.notify();
    }

    /// Refresh this pane's liveness marker, which a hook reads before it holds
    /// a picker for the bench (`docs/spec/td-agent-channel.md` §7).
    ///
    /// About once a second, and at once when the answer changes: a bench that
    /// just closed must release the picker promptly. "Open" means the bench is
    /// the face on screen — a bench on a tab nobody is looking at does not hold
    /// an agent's menu for them.
    pub fn bench_beacon(&mut self) {
        if !self.mode.is_agent() {
            return;
        }
        let Some(dir) = self.bench_dir() else {
            return;
        };
        let open = self.bench.face() == crate::workbench::Face::Workbench && self.wb_on_screen;
        let now = crate::surfacefeed::now_ms();
        if !crate::channel::beacon_due(self.wb_beacon, open, now) {
            return;
        }
        if let Err(err) = crate::surfacefeed::write_marker(&dir, open, now) {
            // Said once rather than at one hertz per pane.
            if self.wb_beacon.is_none() {
                eprintln!(
                    "terminal-delight: could not write the bench marker for pane {:?}: {err}",
                    self.pane_id
                );
            }
        }
        self.wb_beacon = Some((open, now));
    }

    /// A press on a card the channel carried: record it, then take whichever
    /// road the hook left open — the answer file, the picker's keys, or a
    /// sentence. See [`crate::channel::State::press`].
    fn bench_hook_press(
        &mut self,
        id: &crate::surface::SurfaceId,
        nav: usize,
        cx: &mut Context<Self>,
    ) {
        use crate::channel::{Outbound, Press, Route};
        use crate::surface::{Op, Post};
        let now = crate::surfacefeed::now_ms();
        match self.wb_channel.press(id, nav, now) {
            Press::WriteAnswers {
                tool_use_id,
                answers,
            } => {
                self.journal_out(&Outbound::Answer {
                    tool_use_id: tool_use_id.clone(),
                    answers: answers.clone(),
                    route: Route::File,
                });
                match self.bench_dir() {
                    Some(dir) => {
                        if let Err(err) =
                            crate::surfacefeed::write_answers(&dir, &tool_use_id, &answers)
                        {
                            eprintln!(
                                "terminal-delight: the answer was recorded but not delivered: {err}"
                            );
                        }
                    }
                    None => eprintln!(
                        "terminal-delight: this pane has no surfaces directory, so the answer has nowhere to go"
                    ),
                }
            }
            Press::Keys { bytes, note } => {
                self.journal_out(&Outbound::Keys {
                    bytes: bytes.clone(),
                    why: format!("the picker had painted: {note}"),
                });
                self.bench_deliver(bytes, cx);
            }
            Press::Sentence { label } => {
                let report = crate::surface::ActionReport {
                    surface: id.clone(),
                    action: crate::surface::Action::Choose,
                    target: Some(label),
                    comment: None,
                };
                if let (Some(key), Some(pane)) = (crate::surfacefeed::session(), self.pane_id) {
                    let _ = crate::surfacefeed::journal(
                        &crate::surfacefeed::actions_path(key, pane),
                        &report,
                    );
                }
                let line = report.to_prompt(crate::surfacefeed::tag());
                self.bench_deliver(crate::workbench::typed_line(&line), cx);
            }
            Press::Recorded | Press::Refused(_) => {}
        }
        // Whatever the road, the cards say what was pressed.
        let surfaces = self.wb_channel.round_surfaces(id, now);
        // WHERE TO STAND NEXT, decided from the cards we are about to present
        // rather than from the ones already on the bench: the press just landed
        // and the bench's copy is one moment stale.
        //
        // Answering a question of a round and being left looking at it is the
        // jam Parker photographed from the other side — the terminal had moved
        // on to `Orphans` and the bench was still showing `Ended state`, with no
        // way to tell and nowhere to press. Parker: *"the workbench will AUTO
        // navigate if a person clicks an answer"*. It could not, while the only
        // question the bench could see was the one the picker was painting.
        //
        // A round that has just been completed moves nowhere: the last press
        // sent the answers, and throwing the person onto another card at that
        // moment would hide the thing they just did.
        let advance = surfaces
            .iter()
            .find(|s| s.id == *id)
            .and_then(|s| match &s.kind {
                crate::surface::Kind::Question(q) => q.round.as_ref(),
                _ => None,
            })
            .and_then(|r| {
                let here = r.current?;
                r.steps.get(here)?.done.then_some(())?;
                let next = r.next_open(here)?;
                r.steps.get(next)?.id.clone()
            });
        for s in surfaces {
            let sid = s.id.clone();
            self.present(
                Post {
                    op: Op::Present,
                    id: sid,
                    pane: None,
                    surface: Some(s),
                },
                cx,
            );
        }
        if let Some(next) = advance {
            self.bench.select(&next);
        }
        cx.notify();
    }

    /// The strip's right-hand run: the two dials, then the one verb.
    ///
    /// Built here rather than in [`crate::benchdraw`] because every element in
    /// it carries a click zone, and a zone is a statement about what a press
    /// MEANS — which belongs with the pane that dispatches it.
    ///
    /// **This used to say the renderer is "asserted not to contain" press
    /// meanings, and no such assertion exists.** The guard next door,
    /// `a_renderer_contains_no_decisions`, refuses clocks, environment reads and
    /// numeric thresholds; it has never looked at `Hit`, and `benchdraw::zone`
    /// takes one as a parameter. A sentence claiming a problem is already
    /// policed is worse than no sentence: it stops the next reader checking, and
    /// it cost one this afternoon, who went looking for a guard they were about
    /// to break and found it did not exist. The convention is real and worth
    /// keeping — it is simply a convention, held by people, not a test.
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
        use crate::workbench::{Dial, Hit, StripVerb, TurnControl};
        let mut out: Vec<gpui::Div> = Vec::new();
        // THE TURN'S OWN CONTROL, ahead of the dials, and absent far more
        // often than it is there — see `workbench::turn_control`, which owns
        // the whole rule. It leads the run because it acts on the turn the
        // state beside it is describing, and because END SESSION keeps the far
        // edge: the control nobody should press by accident does not move
        // around under a hand that is reaching for the one next to it.
        //
        // RESUME takes the primary face, the one the launch wears. It is the
        // only control on this strip that offers rather than takes, and a
        // paused pane should look like it is waiting to be let go.
        if let Some(control) = crate::workbench::turn_control(state, agent_now) {
            let (label, hit) = match control {
                TurnControl::Pause => ("PAUSE TURN", Hit::PauseTurn),
                TurnControl::Resume => ("RESUME TURN", Hit::ResumeTurn),
            };
            out.push(
                crate::benchdraw::strip_button(label, "", control == TurnControl::Resume, sk, th)
                    .relative()
                    .child(crate::benchdraw::zone(self.wb_zones.clone(), hit)),
            );
        }
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
        // The far end's line is EMPTY now: the composer is a document and
        // nothing in it has been typed at the agent, so there is nothing to
        // move aside and nothing to type back. `dial_bytes` beside an empty
        // line is just the command. The hold below still matters — it keeps
        // SEND out of the harness's own confirmation picker.
        let bytes = crate::workbench::dial_bytes(
            &format!("{} {value}", which.command()),
            &crate::workbench::Line::new(),
        );
        self.bench_deliver(bytes, cx);
        self.wb_dial_sent = Some(crate::workbench::DialSent {
            which,
            text: String::new(),
            caret: 0,
            sent_ms: crate::surfacefeed::now_ms(),
            answered: false,
        });
    }

    /// Carry a dial press through the harness's own confirmation, then give
    /// the person their sentence back.
    ///
    /// Called from the pane's 120ms clock with the bottom of the screen, and
    /// only while a press is in flight. The judgement is
    /// [`crate::workbench::dial_step`] and the reading is
    /// [`crate::screenread::harness_confirm`]; what is left here is the
    /// writing, which is the part that cannot be tested without a terminal.
    pub(super) fn dial_watch(&mut self, rows: &[String], cx: &mut Context<Self>) {
        use crate::workbench::DialStep;
        let Some(sent) = self.wb_dial_sent.clone() else {
            return;
        };
        let picker = crate::screenread::harness_confirm(rows, sent.which.confirm_word());
        match crate::workbench::dial_step(&sent, picker, crate::surfacefeed::now_ms()) {
            DialStep::Wait => {}
            DialStep::Answer { to, from } => {
                if let Some(live) = self.wb_dial_sent.as_mut() {
                    live.answered = true;
                }
                self.dial_answer(crate::workbench::menu_keys(to, from), cx);
            }
            DialStep::Settle => {
                // Cleared BEFORE the write, so the write is not held by the
                // hold it is ending.
                self.wb_dial_sent = None;
                let bytes = crate::workbench::restore_bytes(&sent.text, sent.caret);
                if !bytes.is_empty() {
                    self.bench_deliver(bytes, cx);
                }
                // Then everything they typed while it was in flight, in the
                // order they typed it.
                self.bench_drain(cx);
                cx.notify();
            }
        }
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
        // A question the HOOK already carried whole must not arrive a second
        // time as a screen reading. The reading still has the one thing the
        // hook does not — where the picker's highlight is — so it is merged
        // into the hook's card as a cursor, and that card answers by keys.
        let mut reading = crate::workbench::LiveRead::Unreadable;
        let asking = asking.and_then(|q| match self.wb_channel.matching(&q.question) {
            Some(id) => {
                if let Some(cursor) = q.cursor {
                    // The cursor onto the card; the picker's own Submit
                    // position stays beside it in the channel, because the
                    // card's Submit slot is the round's and not the screen's.
                    self.wb_channel.saw_cursor(&id, cursor, q.submit);
                    self.bench.with_question_mut(&id, |hq| {
                        hq.cursor = Some(cursor);
                    });
                }
                // Read, and it belongs to somebody else — which is a FINDING,
                // not a failure to read. Saying so is what lets the rule below
                // retire a live card the channel has taken over; reported as
                // `None` it was indistinguishable from a screen that could not
                // be parsed at all, and a stranded card outlived every round.
                reading = crate::workbench::LiveRead::Folded;
                None
            }
            None => Some(q),
        });
        let now_id = asking.as_ref().map(crate::screenread::screen_question_id);
        if let Some(id) = now_id.as_ref() {
            reading = crate::workbench::LiveRead::Fresh(id.clone());
        }
        let was = self.wb_live_q.clone();
        // Counted here rather than in the rule, because the rule is a pure
        // decision and this is the pane remembering what it has seen.
        self.wb_quiet = if self.needs_input {
            0
        } else {
            self.wb_quiet.saturating_add(1)
        };

        match crate::workbench::live_move(self.needs_input, &reading, was.as_ref(), self.wb_quiet) {
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
    /// The review, as the whole of the workbench body.
    ///
    /// `None` when nothing is being reviewed, or when there is nothing to
    /// review — a gallery of nothing is a takeover that strands the person on
    /// an empty page, where the old flyout merely declined to open.
    ///
    /// The navigator is built here rather than in [`crate::benchdraw`] because
    /// its three chips need this pane's hit zones, which is also what makes
    /// them light under the pointer.
    pub(super) fn review_body(&self, sk: &crate::skin::Skin, th: &Theme) -> Option<gpui::Div> {
        let at = self.wb_review?;
        let all = self.bench.reviewable();
        if all.is_empty() {
            return None;
        }
        // Clamped rather than trusted: answering a question while the gallery
        // is open can shorten the list under the index.
        let at = at.min(all.len() - 1);
        let item = all[at].clone();
        let total = all.len();
        let back = at > 0;
        let fwd = at + 1 < total;
        Some(
            crate::benchdraw::review_page(at, total, &item.title, &item.answer, sk, th).child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        sk.chip(back)
                            .child("\u{2190}".to_string())
                            .relative()
                            .child(self.live_zone(crate::workbench::Hit::GalleryBack, sk)),
                    )
                    .child(
                        sk.chip(fwd)
                            .child("\u{2192}".to_string())
                            .relative()
                            .child(self.live_zone(crate::workbench::Hit::GalleryForward, sk)),
                    )
                    .child(div().flex_1())
                    .child(
                        sk.chip(false)
                            .child("CLOSE".to_string())
                            .relative()
                            .child(self.live_zone(crate::workbench::Hit::GalleryClose, sk)),
                    ),
            ),
        )
    }

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
                chip.relative()
                    .child(self.live_zone(crate::workbench::Hit::Choose(i), sk))
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
                            .child(self.live_zone(crate::workbench::Hit::Review, sk)),
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
                        )
                        .relative()
                        .child(self.live_zone(crate::workbench::Hit::PressNav(at), sk)),
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
        //
        // **OFF BY DEFAULT since 2026-09-21.** All of that is about what
        // happens when somebody looks; what shipped was four lines of routing
        // and tagged prompt text under every card on every frame, whether or
        // not anybody was auditing anything. Parker: *"that machine stuff at
        // the bottom … human does not need to see that"*. So it is a
        // diagnostic — `TD_VERB_PREVIEW=1` — and the guarantee it was built
        // for survives where it actually lives: `verb_preview` is the same
        // function the typed line comes out of, and a test says so.
        let auditing = std::env::var_os("TD_VERB_PREVIEW").is_some();
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
        let previews: Vec<(String, String)> = if !auditing {
            // Not built at all rather than built and hidden: this calls
            // `verb_preview` once per verb per frame.
            Vec::new()
        } else {
            actions
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
                                    format!(
                                        "{} {}",
                                        action.label(),
                                        t.rsplit('/').next().unwrap_or(t)
                                    )
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
                .collect()
        };
        let shown = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .pt(px(2.))
            .font_family(th.font_family.clone())
            .text_size(px(sk.pt(Step::Fine)))
            .child(div().text_color(sk.ink.ink_faint).child(where_to))
            .children(previews.into_iter().map(|(chip, what)| {
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .child(div().flex_none().text_color(sk.ink.ink_faint).child(chip))
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
                        )
                        .relative()
                        .child(self.live_zone(
                            crate::workbench::Hit::Verb {
                                action: action.clone(),
                                target: target.clone(),
                            },
                            sk,
                        ))
                    })
                    .collect::<Vec<_>>()
            }));
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(row)
                .when(auditing, |d| d.child(shown)),
        )
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
        //
        // NOT for a card the channel carried. Its Submit sits after the
        // options by construction, so option index and navigation index are
        // the same list, and `nav_index` would push the Submit slot one past
        // the end — `ctl bench choose 4` on a three-option multi-select was
        // refused as "no option 5".
        let card = self
            .bench
            .selected()
            .or_else(|| self.bench.waiting_question());
        let hook = card
            .map(|s| s.id.clone())
            .is_some_and(|id| self.wb_channel.owns(&id).is_some());
        let submit = match card.map(|s| &s.kind) {
            Some(crate::surface::Kind::Question(q)) if !hook => q.submit,
            _ => None,
        };
        let nav = crate::workbench::nav_index(index, submit);
        self.bench_act(crate::surface::Action::Choose, Some(nav.to_string()), cx);
    }

    /// A click in the composer: arm it, and put the caret where the pointer is.
    ///
    /// The caret is ours alone: the draft is a document, so there is no far-end
    /// editor to walk with arrow keys, and a click into a selection drops the
    /// selection the way every text box on this desk does (#615).
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
        // One call, both halves: the caret moves and the selection goes.
        // See [`crate::workbench::Line::place`] — this was a `seek` with a
        // `clear_mark` remembered beside it, and `seek` was public.
        line.place(to);
        cx.notify();
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
                    // Line breaks STAY line breaks. They used to become
                    // spaces because a newline typed at the agent submitted
                    // half a paste; the draft is a document now and the send
                    // is a bracketed paste, so a break inside it is just a
                    // break (Gate 1, "pasted line breaks stay line breaks").
                    parts.push(text.text().replace("\r\n", "\n").replace('\r', "\n"));
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    // The same rule a DROP follows, from the same function: a
                    // path is one word. This arm used to join the paths raw,
                    // so copying `Screenshot 2026-09-18.png` pasted two words
                    // and nothing downstream could put them back together.
                    let words = crate::workbench::paths_as_words(paths.paths());
                    // An entry carrying no path at all adds nothing, rather
                    // than a stray space in the middle of the sentence.
                    if !words.is_empty() {
                        parts.push(words);
                    }
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

    /// A file dropped on the pane: its path typed where it landed.
    ///
    /// The compositor's half of this is gpui's and costs us nothing — its
    /// Wayland client asks the drag for `text/uri-list`, turns the URIs into
    /// paths, and hands them over as an ordinary mouse-up with the value
    /// attached. It also DESTROYS any drag whose URIs are not local files, so
    /// an image dragged off a web page never reaches this function and there
    /// is nothing here that could serve it. Files, and only files.
    ///
    /// The listener sits on the pane's root rather than on the composer,
    /// because the bench is bent by the barrel pass and gpui hit-tests the
    /// flat tree — a drop target hung on the composer element would catch
    /// drops beside where the composer appears. So this un-bends the pointer
    /// through [`Self::bench_hit_at`], the same inverse every bench click goes
    /// through, and the composer's own zone answers.
    pub(super) fn bench_drop(
        &mut self,
        paths: &gpui::ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.wb_drop = false;
        let text = crate::workbench::paths_as_words(paths.paths());
        if text.is_empty() {
            return;
        }
        // On the TERMINAL face there is no composer to aim at and no mirror to
        // keep: the path goes to the process as a paste, which is what every
        // other terminal on this machine does with a dropped file.
        if self.bench.face() != crate::workbench::Face::Workbench {
            window.focus(&self.focus_handle, cx);
            self.paste_text(&text);
            cx.notify();
            return;
        }
        let Some((hit, flat)) = self.bench_hit_at(window.mouse_position()) else {
            return;
        };
        // The composer and the field around it, which is the gesture people
        // will actually make — the box is the biggest thing on the bench. A
        // drop on a card is not a drop on the line and does nothing, on the
        // same terms as a click there.
        if !matches!(
            hit,
            crate::workbench::Hit::Composer | crate::workbench::Hit::Arm
        ) {
            return;
        }
        // Arm FIRST. `bench_click` arms a cold line and returns without
        // moving anything, so calling it on an unarmed composer would place
        // no caret and the path would land at the end of a line nobody could
        // see yet.
        if self.wb_compose.is_none() {
            self.wb_compose = Some(crate::workbench::Line::new());
        }
        self.bench_click(flat, cx);
        window.focus(&self.focus_handle, cx);
        self.bench_typed(text, cx);
    }

    /// Put text into the draft as though it had been typed.
    ///
    /// Into the DRAFT, and nowhere else: what is pasted is material for a
    /// sentence, and the sentence goes when the person sends it. A paste into
    /// a composer that was not open opens one — the words have to land
    /// somewhere the person can see.
    pub(super) fn bench_typed(&mut self, text: String, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        if self.wb_compose.is_none() {
            self.wb_compose = Some(crate::workbench::Line::new());
        }
        if let Some(line) = self.wb_compose.as_mut() {
            line.insert(&text);
        }
        self.wb_recall = None;
        self.composer_follows();
        cx.notify();
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

    /// Type a line into the composer — without sending it.
    ///
    /// The scripted half of typing, for `ctl bench type`: a caret in a
    /// half-typed line can be photographed without borrowing somebody's
    /// keyboard. It APPENDS, and no space is inserted — the caller controls
    /// spacing exactly as a person typing does, and a verb that quietly added
    /// one would make `bench type "half"` then `bench type "way"` unable to
    /// spell a word. Nothing reaches the agent: the draft is a document.
    pub fn bench_type(&mut self, line: &str, cx: &mut Context<Self>) {
        let text = line.replace("\r\n", "\n").replace('\r', "\n");
        match self.wb_compose.as_mut() {
            Some(existing) => existing.insert(&text),
            None => self.wb_compose = Some(crate::workbench::Line::holding(text)),
        }
        self.wb_recall = None;
        self.composer_follows();
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
            self.write_through(bytes);
        } else {
            self.wb_queued.push(bytes);
        }
        cx.notify();
    }

    /// The bytes, into the pseudoterminal, stamped. No rules — the callers
    /// above own those, and each of them owns a different set.
    fn write_through(&mut self, bytes: Vec<u8>) {
        let now = crate::surfacefeed::now_ms();
        self.session.notifier.notify(bytes);
        self.wb_delivered_ms = Some(now);
        self.wb_flash_until_ms = Some(now + 450);
    }

    /// The window pressing Yes on the picker its own dial press raised.
    ///
    /// The one write that goes past the dial hold, because the hold exists to
    /// keep the PERSON's keystrokes out of that picker and this keystroke is
    /// what the picker is for. It is not queued either: a queue would press
    /// Yes on a question that had gone, and the two visibility rules still
    /// apply — a picker on a pane nobody is looking at is a picker nobody
    /// asked us to answer.
    fn dial_answer(&mut self, bytes: Vec<u8>, cx: &mut Context<Self>) {
        if self.bench_channel_open() {
            self.write_through(bytes);
            cx.notify();
        }
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
    fn bench_channel_open(&self) -> bool {
        self.wb_on_screen && self.mode.is_agent()
    }

    /// …and the third rule, which is about WHAT IS LISTENING rather than
    /// whether anything is.
    ///
    /// A dial press leaves the harness showing a modal picker, and for as long
    /// as it is up the pane's line editor is not reading: every byte sent
    /// there is a menu keystroke, and a digit in somebody's half-typed
    /// sentence chooses an option. So the bench holds — in the same queue and
    /// with the same promise as the other two rules — until
    /// [`Self::dial_watch`] has seen the picker answered and typed the draft
    /// back. Held, never dropped: the person goes on typing into a composer
    /// that keeps drawing their words, and the keystrokes land in order the
    /// moment the far end is a line editor again.
    fn bench_may_write(&self) -> bool {
        self.bench_channel_open() && self.wb_dial_sent.is_none()
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

    /// Open the note buffer, and bring the board it writes to into view.
    ///
    /// Moving the shelf is part of the gesture rather than a courtesy. Asking
    /// for a note while reading the decisions tab and then typing into a box
    /// whose output lands on a tab you are not looking at is a surface writing
    /// somewhere the person cannot see it land.
    pub(super) fn bench_note_open(&mut self, cx: &mut Context<Self>) {
        if self.wb_note.is_none() {
            self.wb_note = Some(crate::workbench::Line::new());
        }
        self.bench.set_shelf(crate::surface::Shelf::Comments);
        cx.notify();
    }

    /// Every keystroke while a note is being written — and not one of them
    /// leaves this function.
    ///
    /// This is the whole difference between the comments board and the rest of
    /// the bench, so it is written as one function with no call to
    /// [`Self::bench_keystroke`] or [`Self::bench_deliver`] anywhere inside it.
    /// The composer twenty lines below does the opposite by design: it puts
    /// every byte down the pseudoterminal FIRST and applies the edit locally
    /// afterwards, because it is mirroring an editor that lives in the agent's
    /// process. Reusing it here would have typed a person's private note into
    /// their agent's prompt, which is the one outcome this shelf exists to
    /// prevent.
    ///
    /// `escape` discards the draft. That is the same bargain the reply composer
    /// makes — a composer is a mode you can see you are in — and unlike a posted
    /// note there is nothing here anybody else has seen yet.
    fn bench_note_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        match ks.key.as_str() {
            "escape" => {
                self.wb_note = None;
                cx.notify();
            }
            // Return posts it; shift+return puts a line break in. A note long
            // enough to want paragraphs is exactly the note worth keeping, and
            // the card already draws the first line as its title and the rest
            // as its body — see [`crate::benchdraw::comment`].
            "enter" if !ks.modifiers.shift => self.bench_note_post(cx),
            "enter" => {
                if let Some(line) = self.wb_note.as_mut() {
                    line.insert("\n");
                }
                cx.notify();
            }
            _ => {
                if crate::workbench::is_paste_chord(
                    &ks.key,
                    ks.modifiers.control,
                    ks.modifiers.shift,
                ) {
                    self.bench_note_paste(cx);
                } else if let Some(line) = self.wb_note.as_mut() {
                    // The same editing table the reply composer uses, so word
                    // motion and the kills behave identically in both boxes.
                    // Only the destination differs, and that is the point.
                    match crate::workbench::line_edit(
                        &ks.key,
                        ks.modifiers.control,
                        ks.modifiers.alt,
                        ks.modifiers.shift,
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
                    cx.notify();
                }
            }
        }
        true
    }

    /// Clipboard text into the note, and text only.
    ///
    /// No image branch. The agent composer has one because an agent can be
    /// handed a picture and do something with it; a note is words a person will
    /// read later, and saving a PNG into the pane's directory to paste its path
    /// into a sentence is a feature nobody asked this shelf for. Newlines
    /// survive here, unlike in the reply composer where a pasted one would
    /// submit mid-paste — posting is `enter` and a paste is not a keystroke.
    fn bench_note_paste(&mut self, cx: &mut Context<Self>) {
        use gpui::ClipboardEntry;
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let mut parts: Vec<String> = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::String(text) => parts.push(text.text().to_string()),
                ClipboardEntry::ExternalPaths(paths) => {
                    parts.extend(paths.paths().iter().map(|p| p.display().to_string()));
                }
                // Named rather than ignored: a person who copied a picture and
                // pasted it into a note should be told nothing happened, not
                // left wondering whether the paste worked.
                ClipboardEntry::Image(_) => {
                    eprintln!(
                        "terminal-delight: a note holds words; the clipboard image was not pasted"
                    );
                }
            }
        }
        let text = parts.join(" ");
        if text.is_empty() {
            return;
        }
        if let Some(line) = self.wb_note.as_mut() {
            line.insert(&text);
        }
        cx.notify();
    }

    /// Post the note: onto this bench, and into this pane's directory.
    ///
    /// Both, in that order, and the order matters. Putting it on the bench
    /// first means the row appears under the caret immediately rather than
    /// whenever the next sweep happens to run; writing the file is what makes
    /// it still be there on Thursday. The sweep then reads back the file this
    /// window just wrote and delivers it as an ordinary drop — which is true,
    /// and which [`crate::surface::Surface::merge`] declines to let overwrite
    /// the [`crate::surface::Origin::Person`] stamp for the life of the session.
    ///
    /// Nothing here touches the pseudoterminal, the action journal, or the
    /// agent. A note is not an answer to anything.
    fn bench_note_post(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self
            .wb_note
            .as_ref()
            .map(|l| l.text().trim().to_string())
            .filter(|t| !t.is_empty())
        else {
            // An empty draft closes rather than posting a blank row. The
            // buffer is only dropped here, so an accidental return on a note
            // that had content never loses it.
            self.wb_note = None;
            cx.notify();
            return;
        };
        let Some(dir) = self.bench_dir() else {
            // No pane directory means no durable home, and posting a note that
            // would vanish at the next restart while looking exactly like one
            // that would not is worse than refusing. The draft is KEPT so the
            // words are not lost with the keystroke that tried to save them.
            eprintln!(
                "terminal-delight: this pane has no surfaces directory, so the note was not saved"
            );
            return;
        };
        self.wb_note = None;
        // Unique by construction, and sortable. `drop_surface` writes
        // `<id>.json`, so two notes posted in the same millisecond would
        // otherwise be one note — which is rarer than it sounds and still
        // possible with a paste and a fast return.
        let id = crate::surface::SurfaceId(format!(
            "comment-{}-{}",
            crate::surfacefeed::now_ms(),
            self.bench.counts(crate::surface::Shelf::Comments).0
        ));
        let value = serde_json::json!({
            "td": crate::surface::TDSP_VERSION,
            "kind": "comment",
            "id": id.0,
            "model": { "body": text },
        });
        let mut post = crate::surface::parse_lenient(&value, crate::surfacefeed::now_ms(), &id.0);
        if let Some(s) = post.surface.as_mut() {
            // The one place this origin is ever set. See `surface::Origin::Person`.
            s.origin = crate::surface::Origin::Person;
        }
        self.present(post, cx);
        if let Err(err) = crate::surfacefeed::drop_surface(&dir, &id.0, &value) {
            // The note is on the bench either way, so this is a durability
            // failure and not a loss — and it is said out loud rather than
            // swallowed, because the row will look identical to one that saved.
            eprintln!("terminal-delight: the note is on the bench but was not saved: {err}");
        }
        cx.notify();
    }

    /// Send whatever is in the composer to the agent — as ONE message.
    ///
    /// A bracketed paste and a return, when the terminal has bracketed paste on
    /// (every agent TUI this house runs does), so the draft's own line breaks
    /// survive and nothing inside it can submit early; flattened to one line
    /// otherwise, which is what the terminal face's own paste does there. The
    /// record goes first — `outbound.jsonl` — and the delivery rules are the
    /// bench's usual three (on screen, an agent in the pane, no dial press in
    /// flight), so a held message is recorded as held and goes when it can.
    pub(super) fn bench_send(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self
            .wb_compose
            .take()
            .map(|l| l.text().to_string())
            .filter(|t| !t.trim().is_empty())
        else {
            self.wb_compose = None;
            self.wb_recall = None;
            cx.notify();
            return;
        };
        // ANY message ends a pause, whatever it says and wherever it came
        // from — the composer, `ctl bench say`, or the strip's own resume,
        // which is `bench_say` with one sentence in it. The person is talking
        // to the agent again, so the turn they stopped is behind them, and a
        // strip still offering RESUME after a fresh instruction would be
        // offering to send a second one on top of it.
        //
        // Here rather than in `bench_resume`, because this is the funnel every
        // route already goes through. A clear beside the one caller that
        // prompted it would have repaired that caller and armed the next.
        self.wb_paused_ms = None;
        let bracketed = self
            .session
            .term
            .lock()
            .mode()
            .contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE);
        let (bytes, delivery) = crate::channel::say_bytes(&text, bracketed);
        let delivery = if self.bench_may_write() {
            delivery
        } else {
            crate::channel::Delivery::Held
        };
        let id = format!(
            "say-{}-{}",
            crate::surfacefeed::now_ms(),
            self.wb_sent.len()
        );
        self.journal_out(&crate::channel::Outbound::Say {
            id,
            text: text.clone(),
            delivery,
        });
        self.wb_sent.push(text);
        if self.wb_sent.len() > crate::channel::HISTORY_KEPT {
            self.wb_sent.remove(0);
        }
        self.wb_recall = None;
        self.bench_deliver(bytes, cx);
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
        // A question the channel carried answers THROUGH the channel. Decided
        // before `Bench::act`, which knows nothing about hooks and must not:
        // it would drive a menu that never painted.
        if action == crate::surface::Action::Choose {
            let id = self
                .bench
                .selected()
                .or_else(|| self.bench.waiting_question())
                .map(|s| s.id.clone());
            if let Some(id) = id.filter(|id| self.wb_channel.owns(id).is_some()) {
                if let Some(nav) = target.as_deref().and_then(|t| t.parse::<usize>().ok()) {
                    self.bench_hook_press(&id, nav, cx);
                }
                return;
            }
        }
        let comment = self
            .wb_compose
            .take()
            .map(|l| l.text().to_string())
            .filter(|c| !c.trim().is_empty());
        match self.bench.act(&action, target, comment) {
            crate::workbench::Dispatch::Open(href) => open_with_system(&href),
            // No journal entry and no line typed anywhere. A copy is a person
            // moving their own words with their own hands, and the agent has
            // no business hearing about it — which is the entire reason this
            // verb exists on a comment instead of an `ask agent` one.
            crate::workbench::Dispatch::Clipboard(text) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
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
                // AND THE FACE DOES NOT MOVE. Answering used to be read as
                // "the person has dealt with this surface", and an agent pane
                // was turned back to face its conversation so the reply would
                // land in front of them. Parker, having picked an option:
                // *"when I am in workbench and I make a choice the focus SNAPS
                // back to TERM ... if I am in workbench I should stay locked
                // in unless I specifically step out"*.
                //
                // It was also the surface disagreeing with itself. The same
                // click on a question we OBSERVED in the terminal takes the
                // `Keys` arm below, which has never moved the face — so two
                // cards that draw identically answered identically and only
                // one of them threw you out of the room.
                //
                // Fourth in a line. Escape's last rung flipped the face and
                // was deleted ([`crate::workbench::Peel`]); escape over a
                // waiting question retired it and was floored; sending from
                // the composer flipped the face and was stopped
                // ([`Self::bench_send`]). Each was found by Parker, one at a
                // time, because each call site decided the exit for itself.
                // Now none of them do: leaving the bench is alt+k, the TERM
                // chip, or a scripted `bench off`, and
                // `nothing_in_the_bench_half_flips_the_pane_off_the_bench`
                // fails the build for the fifth.
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
        // THE HIGHLIGHT, taken before the run list is emptied — the same
        // one-frame move the dial drop-downs above are placed by, and exact
        // for the same reason: a run's position is only known once it has been
        // laid out, so the newest measurement this frame can draw with is the
        // one the frame before it recorded. The selection is being dragged, so
        // a frame of lag is a frame of lag on a thing already following a
        // hand.
        let highlight = self.bench_highlight();
        // Where that root box was, for the same one-frame reason: the
        // rectangles above are in window space and the overlay is a child of
        // the root, so one has to be turned into the other.
        let bench_rect = *self.wb_bench_rect.borrow();
        // A fresh zone list per frame: the elements about to paint fill it.
        self.wb_zones.borrow_mut().clear();
        // And a fresh run list, armed for the whole build below. The guard
        // disarms on drop, so nothing built after this function returns can
        // land in it — and `sel` outside the guard is an ordinary
        // `StyledText`. The regions are cleared here and refilled by the three
        // `region_probe`s as their containers lay out.
        let _collecting = crate::benchdraw::collecting(self.wb_drawn.clone());
        self.wb_regions.borrow_mut().clear();
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
        //
        // What changed is WHEN it is read, not where from. The read happened
        // here, at paint, and answered nothing once the message had scrolled
        // past the history — so the block went blank on exactly the long
        // turns a person most wants it on. It is now latched off the same
        // scrollback by the pane's own clocks
        // ([`crate::pane::TerminalView::latch_asked`]) and this draws what was
        // last seen, which is still the terminal's record and no longer a
        // question about whether the terminal still has it.
        let asked_above = crate::workbench::ask_lines(
            self.bench.shelf(),
            self.bench.standing_in(),
            agent_now,
            how,
        )
        .map(|n| {
            // Whoever opened the NEWEST turn owns this block, because what it
            // captions is the reply standing underneath it. A turn the harness
            // opened is drawn as itself — see [`crate::benchdraw::woken`] —
            // and their own words are drawn only when the turn was theirs.
            match self.woken_latched() {
                Some(w) => crate::benchdraw::woken(&w, sk, th),
                // Read one line longer than the block draws, so a message that
                // ran on can say so rather than stopping mid-word — the latch
                // keeps [`crate::screenread::ASKED_LINES`], which is more than
                // any caller here asks for.
                None => {
                    let lines = crate::workbench::ask_clipped(self.asked_latched(), n);
                    crate::benchdraw::asked(&lines, sk, th)
                }
            }
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
                            .text_color(sk.ink.ink_faint)
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

        // Asked once, because the anchor and the scroll below both need it
        // and `reviewable()` walks the bench to answer.
        let reviewing = self.wb_review.is_some() && !self.bench.reviewable().is_empty();

        // THE REVIEW TAKES THE WORKBENCH, rather than floating over it.
        //
        // It was an absolutely-positioned panel centred on the bench — which
        // is what a flyout IS — and at a narrow pane it drew outside the
        // bench's own box. Parker: *"the review question BROKE OUT OF THE MAIN
        // WORKBENCH SPACE!!! it should have just taken OVER the main workbench
        // space ... in a very obvious way like the other tabs, comments
        // overview etc. do"*.
        //
        // A shelf is the thing it is most like: you go to it, it fills the
        // space, and you come back. So it is drawn as one, in the body, inside
        // the same box every other shelf is clipped and scrolled by — which is
        // the part that makes breaking out of the bench impossible rather than
        // merely unlikely. The old reasoning for the overlay was that the card
        // underneath is a question somebody is part-way through answering; that
        // is still true, and it survives because the review REPLACES the body
        // without touching the selection, so CLOSE puts them back exactly
        // where they were.
        let body = match self.review_body(sk, th) {
            Some(page) => page,
            None => body,
        };

        // Taken as a bool before the block is moved into the tree, so the
        // anchor far below can ask without borrowing it.
        //
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
        //
        // ...and NOT when the document you happen to be reading IS the
        // question. `waiting_question` hands back the selection when the
        // selection is itself unanswered — deliberately, so the round
        // navigator moves the block — so opening a decision from the rail drew
        // it twice, once as the card with its verbs and once pinned
        // underneath. The rule is `workbench::draws_waiting_block`, held there
        // rather than here so it has a test.
        let waiting = self
            .bench
            .waiting_question()
            .filter(|s| crate::workbench::draws_waiting_block(showing_id.as_ref(), &s.id))
            .and_then(|s| match &s.kind {
                crate::surface::Kind::Question(q) => Some(q.clone()),
                _ => None,
            })
            .map(|q| {
                let chips = self.answer_chips(&q, sk, th);
                let zones = self.wb_zones.clone();
                crate::benchdraw::waiting_block(&q, Some(&zones), sk, th).child(chips)
            });

        // Taken as a bool here, where the block still exists, because the
        // anchor that needs it is built far below and the block itself is
        // moved into the tree before then.
        let has_waiting = waiting.is_some();

        // ── the note box ────────────────────────────────────────────────────
        //
        // Not gated on `shows.composer`, which asks whether there is an agent
        // worth drawing an input for. A note has nothing to do with an agent,
        // so this box is drawn on a plain shell pane too, and its absence is
        // decided by one thing only: whether a note is being written.
        let durable = self.bench_dir().is_some();
        let note = self
            .wb_note
            .as_ref()
            .map(|line| crate::benchdraw::note_box(line, focused, durable, sk, th));

        // ── the composer ────────────────────────────────────────────────────
        let composer = shows.composer.then(|| {
            crate::benchdraw::composer(
                self.wb_compose.as_ref(),
                focused,
                self.wb_drop,
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
            .child(crate::benchdraw::region_probe(
                self.wb_regions.clone(),
                crate::workbench::Region::Composer,
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
                // THE ROW WRAPS, because a fourth tab does not fit.
                //
                // The rail is a SHARE of the pane (`RAIL_SHARE`, clamped into
                // `RAIL_MIN_W..=RAIL_W`), so the strip gets between 118 and 194
                // points of room. Four tabs measure about 161 of those —
                // measured off the running build at the 208-point cap, not
                // computed from a glyph width — so they fit at a wide rail and
                // run over it well before the rail reaches its floor. The frame
                // is `overflow_hidden`, so the overflow would not have shown up
                // as a squeeze or a scrollbar: the last tab simply stops being
                // drawn, and a tab nobody can see is a shelf nobody can reach.
                //
                // THE THREE-TAB ROW WAS FINE. A plan for this feature claimed
                // `decisions` was already being clipped on a 500-point pane, on
                // an estimate of 6.0 points per character that turns out to be
                // about a third too fat. Three tabs are roughly 121 points and
                // fitted at every width an open rail can have. This is a
                // prerequisite for the fourth tab, then, and not a bug fix — and
                // it is written down here because the more flattering version of
                // that sentence was already in a document.
                //
                // Wrapping rather than shrinking the type, abbreviating the
                // labels or scrolling the row. Parker: *"Concur on going
                // 2dimensional"*. Two rows of two costs about sixteen points of
                // rail height and only when the width demands it; the other
                // three answers all cost a word or a gesture, permanently.
                let tabs = div().flex().flex_row().flex_wrap().gap(px(3.)).children(
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
                    .relative()
                    .child(crate::benchdraw::region_probe(
                        self.wb_regions.clone(),
                        crate::workbench::Region::Rail,
                    ))
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
                    // The board's own affordance, ABOVE the empty line and
                    // above the rows, because on a newest-first list the top is
                    // where the next thing goes. It is drawn whether or not the
                    // shelf has anything on it: an empty comments board with no
                    // way to start one would be the only shelf on this rail
                    // that tells you it is empty and not what to do about it.
                    .when(shelf_now == crate::surface::Shelf::Comments, |d| {
                        d.child(crate::benchdraw::add_note_row(sk, th).relative().child(
                            crate::benchdraw::zone(
                                self.wb_zones.clone(),
                                crate::workbench::Hit::AddNote,
                            ),
                        ))
                    })
                    // "No comments yet" is still worth saying underneath it,
                    // but only on the shelves whose emptiness is the whole
                    // message. The board now has a thing to press, so the
                    // sentence would be explaining a slot that explains itself.
                    .when(
                        rows.is_empty() && shelf_now != crate::surface::Shelf::Comments,
                        |d| {
                            d.child(
                                div()
                                    .text_size(px(sk.pt(Step::Small)))
                                    .text_color(sk.ink.ink_faint)
                                    .font_family(th.font_family.clone())
                                    .child(format!("No {}", shelf_now.empty_word())),
                            )
                        },
                    )
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
                        // A review fills the body like a card, so it is
                        // anchored and scrolled like one: bottom-anchored it
                        // would sit at the foot of the pane, and unscrolled a
                        // long answer would simply be cut.
                        let anchor = crate::workbench::body_anchor(
                            showing_id.is_some() || reviewing,
                            offering,
                            has_waiting,
                        );
                        div()
                            // Stateful, because a scroll container IS state:
                            // gpui keeps the offset against this id between
                            // frames. Constant, which is safe because ids are
                            // unique within one view's tree and this is one
                            // box in one pane.
                            .id("bench-body")
                            .relative()
                            // THE CARD'S REGION. Without this the body's runs
                            // resolve to `Chrome` and the main work surface —
                            // the thing this feature is for — is the one part
                            // of the bench nobody can drag over. Clippy's
                            // dead-code check is what caught its absence: with
                            // no caller, `Region::Body` was a variant nothing
                            // constructed.
                            .child(crate::benchdraw::region_probe(
                                self.wb_regions.clone(),
                                crate::workbench::Region::Body,
                            ))
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
                            .when(showing_id.is_some() || reviewing, |d| {
                                d.overflow_y_scroll().track_scroll(&self.wb_card_scroll)
                            })
                            .when(showing_id.is_none() && !reviewing, |d| d.overflow_hidden())
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
                    // ABOVE the agent's composer, and both may be open at once.
                    // A note started while a reply was half-written must not
                    // discard the reply, and the one being typed into is the one
                    // nearest the eye — `bench_key` hands keystrokes to the note
                    // while it exists, so the drawing and the key routing agree.
                    .children(note)
                    .children(composer),
            )
            .children(handle)
            .children(rail)
            // THE SELECTION, over the text rather than behind it.
            //
            // Behind would be nicer and is not available: the runs are
            // scattered across a flex tree with their own backgrounds, and
            // there is no single layer underneath all of them to paint on. A
            // translucent wash over the glyphs is what a terminal does and it
            // reads correctly — the text stays legible through it because the
            // alpha is low and the hue is the one already reserved for the
            // person's own marks.
            //
            // Positioned against the bench root's own box, which is why that
            // box is measured by `probe`: these rectangles are in window
            // space and an absolutely-positioned child is placed relative to
            // its padding box.
            .children(bench_rect.map(|root| {
                div().absolute().inset_0().children(
                    highlight
                        .into_iter()
                        .map(|r| {
                            div()
                                .absolute()
                                .left(px(r.x - root.x))
                                .top(px(r.y - root.y))
                                .w(px(r.w))
                                .h(px(r.h))
                                .rounded(px(1.))
                                .bg(th.human.alpha(0.30))
                        })
                        .collect::<Vec<_>>(),
                )
            }))
            // After the body, so the list's zones are recorded after the
            // card's and win the lookup — last painted wins. Before the
            // gallery, which is a modal and must win over both.
            .children(dial_list)
            // Last, so its hitbox and its cursor request are painted after
            // every control's — see the hook for why that order is the rule.
            .child(self.pointer_hook(weak))
            // LAST OF ALL, and for a harder reason than the hook's: a
            // `TextLayout` panics when asked for bounds it has not measured,
            // and gpui runs every child's prepaint before any child's paint.
            // A paint-phase closure at the bottom of the tree is the one
            // position in the frame where every run above is guaranteed to
            // have been laid out. See `benchdraw::resolve`.
            .child(crate::benchdraw::atom_probe(
                std::env::var_os("TD_SELDEBUG").is_some(),
                self.wb_drawn.clone(),
                self.wb_atoms.clone(),
                self.wb_regions.clone(),
            ))
            .into_any_element()
    }
}

/// Put the composer's drop target out.
///
/// A free function because the two listeners that call it are window-level
/// and hold a weak handle rather than a `self`: a pane can be closed with a
/// drag still in the air, and both listeners outlive the frame that made
/// them.
fn clear_drop(weak: &gpui::WeakEntity<TerminalView>, cx: &mut gpui::App) {
    let _ = weak.update(cx, |view, cx| {
        if std::mem::take(&mut view.wb_drop) {
            cx.notify();
        }
    });
}
