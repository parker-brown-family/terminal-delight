//! The floating document square and the Document face, as the terminal view
//! opens, draws and drives them.
//!
//! A child module of `pane`, as `pane/bench.rs` is, and for the same reason: a
//! child sees its parent's private fields, so every `self.float`, `self.doc`,
//! `self.float_zones` and `self.doc_rect` below is exactly as it was when this
//! code lived in pane.rs, and the terminal view's struct did not change to let
//! it out.
//!
//! What lives here is the code whose job is a document on the pane: opening,
//! drawing, hit-testing, dragging and closing the square, and putting a file on
//! the Document face and driving it there. What stays in pane.rs is what this
//! code shares with the rest of the pane: the `FloatingDoc` and `DocFace` types
//! beside the struct that holds them, the event types the workspace listens
//! for, the handlers that choose between a document, the bench and the grid
//! (`on_mouse_down`, `on_mouse_move`, `on_key`, `render`), and `send_notes`,
//! which writes to a terminal and is counted where every such write is, in
//! `docs/security/terminal-input.md`.

use super::*;

/// The arrow a resize shows: the diagonal that runs through the corner being
/// held, or the axis of the edge.
fn resize_cursor(e: crate::docopen::Edges) -> gpui::CursorStyle {
    let vertical = e.top || e.bottom;
    let horizontal = e.left || e.right;
    match (horizontal, vertical) {
        (true, true) if (e.left && e.top) || (e.right && e.bottom) => {
            gpui::CursorStyle::ResizeUpLeftDownRight
        }
        (true, true) => gpui::CursorStyle::ResizeUpRightDownLeft,
        (false, true) => gpui::CursorStyle::ResizeUpDown,
        _ => gpui::CursorStyle::ResizeLeftRight,
    }
}

/// Whether a pointer lands on a floating square, as the bent glass shows it.
///
/// `screen` is the tube in window pixels, `k` its curvature, `rect` the square
/// relative to the screen, `pos` the pointer in window pixels. The pointer is
/// un-bent with the tube's own inverse first, because the square is drawn flat
/// and the barrel pass moves it: near a corner the difference is several
/// pixels, which is the width of the strip a person grabs.
fn point_on_float(
    screen: (f32, f32, f32, f32),
    k: (f32, f32),
    rect: crate::docopen::FloatRect,
    pos: (f32, f32),
) -> bool {
    let (x, y, w, h) = screen;
    let (fx, fy) = crate::workbench::unwarp(screen, k.0, k.1, pos.0, pos.1);
    crate::docopen::clamp_float(rect, w, h).contains(fx - x, fy - y)
}

/// Which part of a floating square a pointer is on, as the bent glass shows
/// it, and the flat point it un-bent to.
///
/// The zones the square recorded as it painted decide: a strip button, the
/// strip, or the body. A pointer inside the square that finds no zone — the
/// one frame between opening it and its first paint — is still the square's,
/// and lands on its body, so a press there never starts a selection behind it.
fn float_hit_through_glass(
    screen: (f32, f32, f32, f32),
    k: (f32, f32),
    rect: crate::docopen::FloatRect,
    zones: &[crate::docopen::FloatZone],
    pos: (f32, f32),
) -> Option<(crate::docopen::FloatZone, (f32, f32))> {
    use crate::docopen::FloatHit;
    let (fx, fy) = crate::workbench::unwarp(screen, k.0, k.1, pos.0, pos.1);
    let zone = crate::docopen::float_hit_at(zones, fx, fy);
    // A button keeps its whole face, even the pixels that lie in an edge's
    // grip; past the buttons, an edge outranks the strip and the document.
    if let Some(z) = zone.filter(|z| !matches!(z.hit, FloatHit::Strip | FloatHit::Body)) {
        return Some((z, (fx, fy)));
    }
    let (x, y, w, h) = screen;
    let r = crate::docopen::clamp_float(rect, w, h);
    if let Some(edges) = crate::docopen::float_edge_at(r, fx - x, fy - y) {
        let grip = crate::docopen::FloatZone {
            x: x + r.x,
            y: y + r.y,
            w: r.w,
            h: r.h,
            hit: FloatHit::Resize(edges),
        };
        return Some((grip, (fx, fy)));
    }
    if let Some(z) = zone {
        return Some((z, (fx, fy)));
    }
    if !point_on_float(screen, k, rect, pos) {
        return None;
    }
    let body = crate::docopen::FloatZone {
        x: x + r.x,
        y: y + r.y,
        w: r.w,
        h: r.h,
        hit: crate::docopen::FloatHit::Body,
    };
    Some((body, (fx, fy)))
}

/// Record, as it paints, the flat rectangle of the element this is a child of
/// as one zone of the floating square — the bench's zone pattern
/// (`benchdraw::zone`), for the square. A canvas that listens to nothing: the
/// pane's own press finds the zone after un-bending the pointer. The parent
/// must be `relative()` so `inset_0` measures it.
fn float_zone(
    into: std::rc::Rc<std::cell::RefCell<Vec<crate::docopen::FloatZone>>>,
    hit: crate::docopen::FloatHit,
) -> impl IntoElement {
    canvas(
        move |bounds, _window, _cx| {
            into.borrow_mut().push(crate::docopen::FloatZone {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                w: f32::from(bounds.size.width),
                h: f32::from(bounds.size.height),
                hit,
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

impl TerminalView {
    /// The document under the pointer, if it is one TD can draw: the path or
    /// `file://` link there, decoded, checked on disk. A web link is never a
    /// document, and neither is a path whose bytes say it is not what its name
    /// claims (see [`crate::docopen::doc_kind`]).
    pub(super) fn document_under(
        &self,
        pos: gpui::Point<Pixels>,
    ) -> Option<crate::docopen::DocTarget> {
        self.document_of(&self.link_under(pos)?)
    }

    /// Whether a link `link_under` resolved is a document TD can draw.
    /// Remembers the last answer, because the Alt chip asks on every move.
    pub(super) fn document_of(&self, link: &str) -> Option<crate::docopen::DocTarget> {
        if let Some((seen, doc)) = self.doc_memo.borrow().as_ref() {
            if seen == link {
                return doc.clone();
            }
        }
        let doc = reveal_target(link)
            .and_then(|path| crate::docopen::drawable_document(std::path::Path::new(&path)));
        *self.doc_memo.borrow_mut() = Some((link.to_string(), doc.clone()));
        doc
    }

    /// The screen's size in logical pixels, once it has been laid out.
    fn screen_size(&self) -> Option<(f32, f32)> {
        let b = (*self.content_bounds.lock().ok()?)?;
        Some((f32::from(b.size.width), f32::from(b.size.height)))
    }

    /// Open a document in a floating square beside the painted row it was
    /// clicked on, or at the top of the screen when there is no row (the
    /// control socket has none). A square already open is replaced, and the
    /// one it replaces gives its texture back as it is dropped.
    ///
    /// A document with nothing on this machine to draw it — an HTML page with
    /// no engine, a video with no libmpv — is handed to the desktop instead,
    /// and the answer is the sentence saying why ([`Self::engine_refused`]).
    pub(crate) fn open_float(
        &mut self,
        target: crate::docopen::DocTarget,
        row: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if let Some(why) = self.engine_refused(&target, row, cx) {
            return Err(why);
        }
        let (w, h) = self.screen_size().unwrap_or((640.0, 480.0));
        let (k1, k2) = self.warp_k;
        let (_, pad_y) = grid_pad_drawn(w, h, k1, k2, self.scale);
        let (top, bottom) = match row {
            Some(r) => (
                pad_y + r as f32 * self.cell_h,
                pad_y + (r + 1) as f32 * self.cell_h,
            ),
            None => (0.0, 0.0),
        };
        // A square already open with notes not yet saved stays, and says so;
        // the click that asked for another document is answered by that line.
        if self.float.is_some() && !self.request_close_float(cx) {
            return Ok(());
        }
        let rect = crate::docopen::float_home(w, h, top, bottom);
        self.float = Some(Self::float_doc(target, None, rect, cx));
        cx.notify();
        Ok(())
    }

    /// The floating square, drawn with the hyperglow every surface that floats
    /// wears (`float_shadows` plus a two-pixel rim in the accent). On the
    /// terminal face and over the bench, where an artifact or a link opens
    /// into it and the card stays underneath; never on the Document face,
    /// which is already a document filling the pane (`float_shows_on`).
    ///
    /// A strip across the top carries the file's name and its controls — zoom
    /// for anything that zooms, "↗ desktop" and "✕ esc" for everything — and
    /// moves the square when dragged. No element here carries a gpui handler:
    /// each control records its flat rectangle as it paints ([`float_zone`]),
    /// and the pane's own press un-bends the pointer and looks it up.
    pub(super) fn float_el(
        &self,
        th: &Theme,
        face: crate::workbench::Face,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        use crate::docopen::FloatHit;
        let float = self.float.as_ref()?;
        if !float_shows_on(face) {
            return None;
        }
        let s = crate::lang::current().strings();
        let (w, h) = self.screen_size().unwrap_or((0.0, 0.0));
        let r = crate::docopen::clamp_float(float.rect, w, h);
        let view = float.view.read(cx);
        let name = view
            .target()
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let zoom = view.zoom_now();
        let zones = self.float_zones.clone();
        let acc = th.accent;
        let button = |label: gpui::SharedString, hit: FloatHit| {
            div()
                .relative()
                .flex_none()
                .px(px(5.))
                .rounded(px(3.))
                .when(float.hover == Some(hit), |d| d.bg(acc.alpha(0.22)))
                .child(label)
                .child(float_zone(zones.clone(), hit))
        };
        let mut strip = div()
            .relative()
            .flex_none()
            .h(px(22.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(4.))
            .border_b_1()
            .border_color(acc.alpha(0.4))
            .text_size(px(11.))
            .text_color(acc)
            // First, so every control recorded after it is found over it.
            .child(float_zone(zones.clone(), FloatHit::Strip))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(gpui::SharedString::from(match float.note {
                        // Why this is a square when a split was asked for.
                        // In the strip, because TD has no toast and this is
                        // where the person is already looking.
                        Some(crate::docopen::FloatNote::FourPanes) => {
                            format!("☰ {name} · {}", s.float_four_panes)
                        }
                        None => format!("☰ {name}"),
                    })),
            );
        if let Some(zoom) = zoom {
            let now: gpui::SharedString = match zoom {
                crate::docview::ImageZoom::Fit => s.float_fit.into(),
                crate::docview::ImageZoom::Scale(z) => format!("{:.0}%", z * 100.0).into(),
            };
            strip = strip
                .child(button("−".into(), FloatHit::ZoomOut))
                .child(button(now, FloatHit::ZoomFit))
                .child(button("+".into(), FloatHit::ZoomIn))
                .child(div().w(px(4.)));
        }
        strip = strip
            .child(button(s.float_split.into(), FloatHit::Split))
            .child(button(s.float_desktop.into(), FloatHit::Desktop))
            .child(button("✕ esc".into(), FloatHit::Close));
        Some(
            div()
                .absolute()
                .left(px(r.x))
                .top(px(r.y))
                .w(px(r.w))
                .h(px(r.h))
                .flex()
                .flex_col()
                .bg(th.bg)
                .border_2()
                .border_color(th.accent)
                .rounded(px(6.))
                .shadow(crate::float_shadows(th.accent))
                .overflow_hidden()
                .child(strip)
                .child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h(px(0.))
                        .child(float_zone(zones, FloatHit::Body))
                        .child(float.view.clone()),
                )
                .into_any_element(),
        )
    }

    /// A square's contents: the document's view, and the subscription that
    /// brings its links back here. `fragment` names a heading to show once the
    /// document has been laid out.
    fn float_doc(
        target: crate::docopen::DocTarget,
        fragment: Option<String>,
        rect: crate::docopen::FloatRect,
        cx: &mut Context<Self>,
    ) -> FloatingDoc {
        let view = cx.new(|cx| crate::docview::DocumentView::new(target, cx));
        if let Some(fragment) = fragment {
            view.update(cx, |v, cx| v.show_fragment(fragment, cx));
        }
        Self::float_of(view, rect, cx)
    }

    /// A square around a view that already exists — a new one, or one carried
    /// across a replica repair — with this pane subscribed to its links.
    pub(super) fn float_of(
        view: gpui::Entity<crate::docview::DocumentView>,
        rect: crate::docopen::FloatRect,
        cx: &mut Context<Self>,
    ) -> FloatingDoc {
        let links = cx.subscribe(&view, |pane, _, link: &crate::docview::FollowLink, cx| {
            pane.follow_doc_link(link, crate::docopen::DocSeat::Float, cx)
        });
        let gave_up = Self::hand_over_on_give_up(&view, cx);
        let send = Self::ask_to_send(&view, crate::docopen::DocSeat::Float, cx);
        FloatingDoc::new(view, rect, links, gave_up, send)
    }

    /// ↪ pressed in a document's notes bar goes to the workspace, which can
    /// see which agent pane the document sits beside.
    fn ask_to_send(
        view: &gpui::Entity<crate::docview::DocumentView>,
        seat: crate::docopen::DocSeat,
        cx: &mut Context<Self>,
    ) -> gpui::Subscription {
        cx.subscribe(view, move |_, _, ev: &crate::docview::SendNotes, cx| {
            cx.emit(SendNotesBeside {
                map: ev.map.clone(),
                unsaved: ev.unsaved,
                seat,
            })
        })
    }

    /// Who ↪ on each of this pane's documents sends to — the floating
    /// square's and the Document face's — as the workspace works it out from
    /// the tab. Repaints only when either changes.
    pub(crate) fn set_notes_beside(
        &mut self,
        float: Option<String>,
        face: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.notes_beside != (float.clone(), face.clone()) {
            self.notes_beside = (float, face);
            cx.notify();
        }
    }

    /// What came of a ↪ on this pane's document, said in its notes bar.
    pub(crate) fn doc_said(
        &mut self,
        seat: crate::docopen::DocSeat,
        said: crate::docview::notes_ui::Said,
        cx: &mut Context<Self>,
    ) {
        let view = match seat {
            crate::docopen::DocSeat::Float => self.float.as_ref().map(|f| f.view.clone()),
            crate::docopen::DocSeat::Face => self.doc.as_ref().map(|d| d.view.clone()),
        };
        if let Some(view) = view {
            view.update(cx, |v, cx| v.notes_said(said, cx));
        }
    }

    /// The view is up but its engine cannot draw after all (a browser that
    /// will not start): the view keeps saying why, and the file goes to the
    /// desktop at once rather than waiting for somebody to ask for it there.
    fn hand_over_on_give_up(
        view: &gpui::Entity<crate::docview::DocumentView>,
        cx: &mut Context<Self>,
    ) -> gpui::Subscription {
        cx.subscribe(view, |_, view, why: &crate::docview::CannotShow, cx| {
            let path = view.read(cx).target().path.clone();
            open_with_system(&path.to_string_lossy());
            eprintln!(
                "terminal-delight: {}: {} Opened with the desktop.",
                path.display(),
                why.reason
            );
        })
    }

    /// A document with nothing on this machine to draw it — an HTML page with
    /// no engine, a video with no libmpv — goes to the desktop instead of into
    /// a square, and this says why. The sentence goes to TD's log and back to
    /// the caller; a short form of it goes on screen, in a chip on `row`, the
    /// painted row the click landed on. `None` for anything that can be
    /// drawn. Asked before a square is placed, so a machine without Chromium
    /// or mpv never opens one that could not fill, and no square exists to
    /// carry the words.
    fn engine_refused(
        &mut self,
        target: &crate::docopen::DocTarget,
        row: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let (sentence, short) = match target.kind {
            crate::docopen::DocKind::Html => {
                let why = crate::docview::html_ready(cx).err()?;
                (why.sentence(), why.short_reason())
            }
            crate::docopen::DocKind::Video => {
                let why = crate::docview::video_ready(cx).err()?;
                (why.sentence(), why.short_reason())
            }
            crate::docopen::DocKind::Markdown | crate::docopen::DocKind::Image => return None,
        };
        open_with_system(&target.path.to_string_lossy());
        eprintln!("terminal-delight: {}: {sentence}", target.path.display());
        let s = crate::lang::current().strings();
        let chip = format!("{short} · {}", s.chip_opened_on_desktop);
        self.say(chip, row, cx);
        Some(sentence)
    }

    /// A link pressed inside a document, routed. `seat` is where the document
    /// that emitted it is sitting.
    ///
    /// A file TD can draw takes the document's place — the same square in the
    /// same spot, or the same pane's Document face — so reading a set of
    /// linked notes stays in one place beside the prompt. A web or mail
    /// address, or a local file TD does not draw, goes to the desktop, as
    /// ctrl+click on a path does. Any other scheme is refused: the decision is
    /// `docopen::link_route`. A heading in the same document never comes here;
    /// the view scrolls to it itself.
    fn follow_doc_link(
        &mut self,
        link: &crate::docview::FollowLink,
        seat: crate::docopen::DocSeat,
        cx: &mut Context<Self>,
    ) {
        let doc = link
            .target
            .starts_with('/')
            .then(|| crate::docopen::drawable_document(std::path::Path::new(&link.target)))
            .flatten();
        match crate::docopen::link_route(&link.target, doc.is_some()) {
            crate::docopen::LinkRoute::Replace => {
                let Some(target) = doc else {
                    return;
                };
                // A brief linking to another brief, on a machine that cannot
                // draw one: the linked file goes to the desktop, and the
                // document that linked to it stays.
                if self.engine_refused(&target, None, cx).is_some() {
                    return;
                }
                // The old view is dropped here, and gives its textures back
                // as it goes, like any other close. Not while it holds notes
                // not yet saved: the first link says so and stays put.
                let current = match seat {
                    crate::docopen::DocSeat::Float => self.float.as_ref().map(|f| f.view.clone()),
                    crate::docopen::DocSeat::Face => self.doc.as_ref().map(|d| d.view.clone()),
                };
                if current.is_some_and(|view| view.update(cx, |doc, cx| doc.guard_close(cx))) {
                    cx.notify();
                    return;
                }
                match seat {
                    crate::docopen::DocSeat::Float => {
                        let Some(rect) = self.float.as_ref().map(|f| f.rect) else {
                            return;
                        };
                        self.float =
                            Some(Self::float_doc(target, link.fragment.clone(), rect, cx));
                    }
                    crate::docopen::DocSeat::Face => {
                        if self.doc.is_none() {
                            return;
                        }
                        self.show_document(target, None, cx);
                        if let (Some(fragment), Some(doc)) = (link.fragment.clone(), &self.doc) {
                            doc.view.update(cx, |v, cx| v.show_fragment(fragment, cx));
                        }
                    }
                }
                cx.notify();
            }
            crate::docopen::LinkRoute::Desktop => open_with_system(&link.target),
            crate::docopen::LinkRoute::Refuse => eprintln!(
                "terminal-delight: not following {} from a document: only files, web and mail links open",
                link.target
            ),
        }
    }

    /// `open_document` with placement "here": the document floats over this
    /// pane, as Alt+click on its path opens it, and the answer is the line
    /// `ctl doc here` would give. A square already showing the file is left as
    /// it is. One holding notes not yet saved is kept, as it is for a click,
    /// and the answer says so rather than claiming a square that never opened.
    pub(crate) fn open_here(
        &mut self,
        target: crate::docopen::DocTarget,
        cx: &mut Context<Self>,
    ) -> String {
        let pane = self
            .pane_id
            .map_or_else(|| "?".to_string(), |p| p.to_string());
        let wanted = std::fs::canonicalize(&target.path).unwrap_or_else(|_| target.path.clone());
        let showing = |v: &Self, cx: &App| {
            v.float_path(cx)
                .map(|p| std::fs::canonicalize(&p).unwrap_or(p))
                == Some(wanted.clone())
        };
        if showing(self, cx) {
            return format!("ok float pane {pane} — it already shows this file");
        }
        match self.open_float(target, None, cx) {
            Err(why) => format!("desktop {why}"),
            Ok(()) if showing(self, cx) => format!("ok float pane {pane}"),
            Ok(()) => "err the square over your pane holds notes not yet saved, so it stays — they have to be saved or dropped first".into(),
        }
    }

    /// Whether a floating square is open on this pane.
    pub(crate) fn has_float(&self) -> bool {
        self.float.is_some()
    }

    /// Scroll the floating document by `dy` logical pixels, down when
    /// positive, as a wheel over it would.
    pub(crate) fn scroll_float(&mut self, dy: f32, cx: &mut Context<Self>) {
        if let Some(view) = self.float.as_ref().map(|f| f.view.clone()) {
            let delta = gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-dy)));
            view.update(cx, |doc, cx| doc.wheel(delta, cx));
        }
    }

    /// Close the floating square the way a person means it. A square holding
    /// notes not yet saved is kept once, saying so in its bar, and goes on the
    /// next ask. Every deliberate close comes through here — the ✕, `ctl doc
    /// close`, an Alt+click or a link that would put another document in its
    /// place, a split of a file already split — as Escape already did. Only a
    /// pane or tab closing drops it without asking. Answers whether it closed.
    pub(crate) fn request_close_float(&mut self, cx: &mut Context<Self>) -> bool {
        let kept = self
            .float
            .as_ref()
            .map(|f| f.view.clone())
            .is_some_and(|view| view.update(cx, |doc, cx| doc.guard_close(cx)));
        if kept {
            cx.notify();
            return false;
        }
        self.close_float(cx)
    }

    /// Close the floating square, unsaved notes and all. Answers whether one
    /// was open. Callers that a person drives go through
    /// [`Self::request_close_float`] instead.
    pub(crate) fn close_float(&mut self, cx: &mut Context<Self>) -> bool {
        let was_open = self.float.take().is_some();
        if was_open {
            cx.notify();
        }
        was_open
    }

    /// Say in the square's strip why it is a square: a split was asked for
    /// and the tab has no room for one.
    pub(crate) fn note_float(&mut self, note: crate::docopen::FloatNote, cx: &mut Context<Self>) {
        if let Some(float) = self.float.as_mut() {
            float.note = Some(note);
            cx.notify();
        }
    }

    /// Take the square down and hand back its view, alive: the view is on
    /// its way to a pane of its own. Nothing is released — the texture goes
    /// with the view, and the view is still wanted.
    pub(crate) fn release_float(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<crate::docview::DocumentView>> {
        let float = self.float.take()?;
        cx.notify();
        Some(float.view)
    }

    /// The path the floating square is showing, if one is up.
    pub(super) fn float_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        let float = self.float.as_ref()?;
        Some(float.view.read(cx).target().path.clone())
    }

    /// Ask for the floating square to become a pane beside this one — its
    /// "⇲ split", or a second Alt+click on its path. The square stays up
    /// until the workspace answers: the view moves into the new pane, or, at
    /// four panes, the square stays and says why.
    pub(super) fn promote_float(&mut self, cx: &mut Context<Self>) {
        let Some(float) = self.float.as_ref() else {
            return;
        };
        let view = float.view.clone();
        let target = view.read(cx).target().clone();
        cx.emit(OpenDoc {
            target,
            carry: Some(view),
            by: crate::docopen::Asker::Float,
            row: None,
            reply: None,
        });
    }

    /// Ask the workspace to open `target` in a pane beside this one. See
    /// [`OpenDoc`] for what it may answer instead.
    pub(crate) fn request_beside(
        &mut self,
        target: crate::docopen::DocTarget,
        row: Option<usize>,
        by: crate::docopen::Asker,
        reply: Option<std::sync::mpsc::Sender<String>>,
        cx: &mut Context<Self>,
    ) {
        // The split asks the same question the square does, before any pane
        // is made: an HTML file with no engine goes to the desktop instead.
        if let Some(why) = self.engine_refused(&target, row, cx) {
            if let Some(reply) = reply {
                let _ = reply.send(format!("desktop {why}"));
            }
            return;
        }
        cx.emit(OpenDoc {
            target,
            carry: None,
            by,
            row,
            reply,
        });
    }

    // ── the Document face ───────────────────────────────────────────────────
    //
    // A pane opened to show a document: Ctrl+Alt+click on a path, or a square
    // promoted to a split. The view fills the pane's screen, bent with the
    // glass like everything else on it, and every press, move, wheel and key
    // reaches it through the pane — un-bent through the tube's inverse, the
    // way the bench and the square are reached. The shell the pane was made
    // with keeps running underneath, alt+k away.

    /// Put a document on this pane's Document face, and the face with it.
    ///
    /// **The only way onto the face.** The document and the face are set
    /// together here, so a pane on the Document face always has something to
    /// show; [`Self::set_face`] refuses the face to a pane without one.
    ///
    /// `carry` is a floating square's view being promoted: it is moved in and
    /// re-seated, never opened again, so its zoom, its place and its decoded
    /// pixels come with it. Without one, the file is opened here.
    pub(crate) fn show_document(
        &mut self,
        target: crate::docopen::DocTarget,
        carry: Option<gpui::Entity<crate::docview::DocumentView>>,
        cx: &mut Context<Self>,
    ) {
        let view = match carry {
            Some(view) => view,
            None => {
                let target = target.clone();
                cx.new(|cx| crate::docview::DocumentView::new(target, cx))
            }
        };
        view.update(cx, |v, cx| v.set_seat(crate::docopen::DocSeat::Face, cx));
        let links = cx.subscribe(&view, |pane, _, link: &crate::docview::FollowLink, cx| {
            pane.follow_doc_link(link, crate::docopen::DocSeat::Face, cx)
        });
        let gave_up = Self::hand_over_on_give_up(&view, cx);
        let send = Self::ask_to_send(&view, crate::docopen::DocSeat::Face, cx);
        self.doc = Some(DocFace {
            view,
            target,
            _links: links,
            _gave_up: Some(gave_up),
            _send: send,
        });
        self.doc_holding = false;
        self.bench.set_face(crate::workbench::Face::Document);
        cx.notify();
    }

    /// The file this pane's Document face shows, if it has one.
    pub(crate) fn document_path(&self) -> Option<&std::path::Path> {
        self.doc.as_ref().map(|d| d.target.path.as_path())
    }

    /// The document this pane shows, for the layout file: its path, and where
    /// the page is scrolled as a fraction of it. The scroll is `None` when it
    /// was never measured — an image, or a page not laid out yet — and stays
    /// `None` rather than being written as the top.
    pub(crate) fn saved_document(&self, cx: &App) -> Option<(String, Option<f32>)> {
        let doc = self.doc.as_ref()?;
        let scroll = doc.view.read(cx).scroll().map(|s| s.top);
        Some((doc.target.path.to_string_lossy().into_owned(), scroll))
    }

    /// Put a saved document back on this pane after a restart.
    ///
    /// Classified by its name alone, because the file may not be there: a
    /// missing file keeps its pane and its face, and the view says it cannot
    /// read it. "Not there right now" — an unmounted drive, a branch switched
    /// away — is a different fact from "never was a document", and turning the
    /// pane back into a shell would throw the difference away. A path whose
    /// name is not a document TD draws was not written by this build, and the
    /// leaf stays a terminal.
    pub(crate) fn restore_document(
        &mut self,
        path: &str,
        scroll: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let path = std::path::PathBuf::from(path);
        let Some(kind) = crate::docopen::doc_kind_by_name(&path) else {
            eprintln!(
                "terminal-delight: a saved pane named {} as its document, which is not a kind TD draws; it comes back as its terminal",
                path.display()
            );
            return;
        };
        self.show_document(crate::docopen::DocTarget { path, kind }, None, cx);
        // Nobody clicked: a brief that cannot be drawn after a restart says
        // why in its pane, and no browser window opens by itself; a video
        // waits paused, and no sound starts by itself either.
        if let Some(doc) = self.doc.as_mut() {
            doc._gave_up = None;
            doc.view.update(cx, |v, _| v.hold());
        }
        if let (Some(top), Some(doc)) = (scroll, self.doc.as_ref()) {
            let at = crate::docopen::DocScroll { top };
            doc.view.update(cx, |v, cx| v.restore_scroll(at, cx));
        }
    }

    /// The Document face's view, when that face is the one showing.
    pub(super) fn doc_on_face(&self) -> Option<&DocFace> {
        (self.bench.face() == crate::workbench::Face::Document)
            .then_some(self.doc.as_ref())
            .flatten()
    }

    /// A pointer, un-bent through the tube and made relative to the Document
    /// face's view. `None` before the view has painted.
    fn doc_face_local(&self, pos: gpui::Point<Pixels>) -> Option<gpui::Point<Pixels>> {
        let (vx, vy, _, _) = self.doc_rect.get()?;
        let screen = self.tube_rect()?;
        let (k1, k2) = self.warp_k;
        let (fx, fy) = crate::workbench::unwarp(screen, k1, k2, f32::from(pos.x), f32::from(pos.y));
        Some(gpui::point(px(fx - vx), px(fy - vy)))
    }

    /// A press on the Document face. Every press there is the document's,
    /// whatever it lands on: the grid behind it is hidden, so a selection
    /// started there is one nobody could see, and the right-click tray would
    /// offer the hidden terminal's links and a paste into its shell.
    pub(super) fn doc_face_press(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self.doc_on_face().map(|d| d.view.clone()) else {
            return false;
        };
        if ev.button == MouseButton::Left {
            if let Some(at) = self.doc_face_local(ev.position) {
                let mods = ev.modifiers;
                self.doc_holding = view.update(cx, |v, cx| v.press(at, mods, window, cx));
            }
        }
        true
    }

    /// The pointer moved while the Document face's view is held. Answers
    /// whether the move was the document's.
    pub(super) fn doc_face_drag_move(
        &mut self,
        ev: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.doc_holding {
            return false;
        }
        if ev.pressed_button != Some(MouseButton::Left) {
            self.doc_face_release(cx);
            return false;
        }
        let (Some(view), Some(at)) = (
            self.doc_on_face().map(|d| d.view.clone()),
            self.doc_face_local(ev.position),
        ) else {
            return true;
        };
        view.update(cx, |v, cx| v.drag(at, cx));
        true
    }

    /// The left button came up over, or away from, a held Document face.
    pub(super) fn doc_face_release(&mut self, cx: &mut Context<Self>) -> bool {
        if !std::mem::take(&mut self.doc_holding) {
            return false;
        }
        if let Some(doc) = self.doc.as_ref() {
            doc.view.update(cx, |v, cx| v.release(cx));
        }
        true
    }

    /// A key on the Document face. Always consumed: the face's own keys move
    /// the document, and every other key stops here rather than typing into
    /// a shell nobody can see. `keylayer` has already let the window's and
    /// the pane's chords past, and alt+k with them.
    pub(super) fn doc_key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> Handled {
        use crate::docopen::DocKey;
        use crate::docview::ZoomStep;
        let Some(view) = self.doc_on_face().map(|d| d.view.clone()) else {
            return Handled::Consumed;
        };
        // The document first: Escape puts away a brief's own dialog.
        if view.update(cx, |v, cx| v.key(ks, cx)) {
            return Handled::Consumed;
        }
        let m = &ks.modifiers;
        let Some(key) = crate::docopen::doc_face_key(&ks.key, m.alt, m.control, m.platform) else {
            return Handled::Consumed;
        };
        let page_h = self.doc_rect.get().map_or(0.0, |(_, _, _, h)| h) * 0.9;
        view.update(cx, |v, cx| match key {
            DocKey::Fit => {
                v.zoom(ZoomStep::Fit, cx);
            }
            DocKey::Actual => {
                v.zoom(ZoomStep::Actual, cx);
            }
            DocKey::ZoomIn => {
                v.zoom(ZoomStep::In, cx);
            }
            DocKey::ZoomOut => {
                v.zoom(ZoomStep::Out, cx);
            }
            DocKey::Pan(dx, dy) => {
                v.wheel(gpui::ScrollDelta::Pixels(point(px(dx), px(dy))), cx);
            }
            DocKey::Page(dir) => {
                let dy = -f32::from(dir) * page_h;
                v.wheel(gpui::ScrollDelta::Pixels(point(px(0.), px(dy))), cx);
            }
        });
        Handled::Consumed
    }

    /// The Document face's view, filling the screen inside the same padding
    /// the grid keeps off the bent edges, with a canvas that records where it
    /// landed so a press can be made relative to it. Nothing here listens.
    pub(super) fn doc_face_el(&self, pad: (f32, f32)) -> Option<gpui::AnyElement> {
        let doc = self.doc_on_face()?;
        let store = self.doc_rect.clone();
        Some(
            div()
                .absolute()
                .inset_0()
                .px(px(pad.0))
                .py(px(pad.1))
                .child(
                    div()
                        .relative()
                        .size_full()
                        .child(
                            canvas(
                                move |bounds, _window, _cx| {
                                    store.set(Some((
                                        f32::from(bounds.origin.x),
                                        f32::from(bounds.origin.y),
                                        f32::from(bounds.size.width),
                                        f32::from(bounds.size.height),
                                    )));
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .child(doc.view.clone()),
                )
                .into_any_element(),
        )
    }

    /// Which part of the floating square a pointer is on as the glass shows
    /// it, and the flat point it un-bent to. `None` when no square is up, or
    /// the face is not the terminal's, or the pointer is off the square.
    ///
    /// Through the warp's inverse, like the bench: the square is drawn inside
    /// the bent tube, and gpui would hit-test its flat layout box and miss.
    pub(super) fn float_hit(
        &self,
        pos: gpui::Point<Pixels>,
    ) -> Option<(crate::docopen::FloatZone, (f32, f32))> {
        let float = self.float.as_ref()?;
        if !float_shows_on(self.bench.face()) {
            return None;
        }
        let hit = float_hit_through_glass(
            self.tube_rect()?,
            self.warp_k,
            float.rect,
            &self.float_zones.borrow(),
            (f32::from(pos.x), f32::from(pos.y)),
        );
        if std::env::var_os("TD_HITDEBUG").is_some() {
            eprintln!(
                "[float-hit] pointer=({:.1},{:.1}) k=({:.3},{:.3}) zones={} -> {:?}",
                f32::from(pos.x),
                f32::from(pos.y),
                self.warp_k.0,
                self.warp_k.1,
                self.float_zones.borrow().len(),
                hit.map(|(z, flat)| (z.hit, flat))
            );
        }
        hit
    }

    /// A press on the floating square: one of its controls, its strip, or the
    /// document in it. Every press on the square is the square's, whatever it
    /// lands on, so none of them starts a selection in the grid behind it.
    pub(super) fn float_press(
        &mut self,
        zone: crate::docopen::FloatZone,
        flat: (f32, f32),
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::docopen::FloatHit;
        use crate::docview::ZoomStep;
        let Some(float) = self.float.as_mut() else {
            return;
        };
        let view = float.view.clone();
        match zone.hit {
            FloatHit::Close => {
                self.request_close_float(cx);
            }
            FloatHit::Split => {
                self.promote_float(cx);
            }
            FloatHit::Desktop => {
                let path = view.read(cx).target().path.to_string_lossy().into_owned();
                open_with_system(&path);
            }
            FloatHit::ZoomOut | FloatHit::ZoomFit | FloatHit::ZoomIn => {
                let step = match zone.hit {
                    FloatHit::ZoomOut => ZoomStep::Out,
                    FloatHit::ZoomIn => ZoomStep::In,
                    _ => ZoomStep::FitOrActual,
                };
                view.update(cx, |v, cx| v.zoom(step, cx));
                // The strip's label reads the zoom, and the strip is ours.
                cx.notify();
            }
            FloatHit::Strip => {
                float.drag = Some(crate::docopen::FloatDrag::new(flat, float.rect));
            }
            FloatHit::Resize(edges) => {
                float.drag = Some(crate::docopen::FloatDrag::resize(flat, float.rect, edges));
            }
            FloatHit::Body => {
                let at = gpui::point(px(flat.0 - zone.x), px(flat.1 - zone.y));
                let mods = ev.modifiers;
                float.holding = view.update(cx, |v, cx| v.press(at, mods, window, cx));
            }
        }
    }

    /// The pointer moved while the strip or the document is held. Answers
    /// whether the move was the square's, so the grid's own drag code does
    /// not also run and build a selection nobody can see.
    pub(super) fn float_drag_move(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        let Some(float) = self.float.as_ref() else {
            return false;
        };
        if float.drag.is_none() && !float.holding {
            return false;
        }
        // A release outside the window can go missing entirely; the next move
        // with no button held is proof it happened.
        if ev.pressed_button != Some(MouseButton::Left) {
            self.float_drag_end(cx);
            return false;
        }
        let Some(screen) = self.tube_rect() else {
            return true;
        };
        let (k1, k2) = self.warp_k;
        let flat = crate::workbench::unwarp(
            screen,
            k1,
            k2,
            f32::from(ev.position.x),
            f32::from(ev.position.y),
        );
        let body = self
            .float_zones
            .borrow()
            .iter()
            .rev()
            .find(|z| z.hit == crate::docopen::FloatHit::Body)
            .copied();
        let Some(float) = self.float.as_mut() else {
            return false;
        };
        if let Some(drag) = float.drag.as_mut() {
            let rect = crate::docopen::drag_to(drag, flat, screen.2, screen.3);
            if rect != float.rect {
                float.rect = rect;
                cx.notify();
            }
        } else if let Some(body) = body {
            let at = gpui::point(px(flat.0 - body.x), px(flat.1 - body.y));
            float.view.update(cx, |v, cx| v.drag(at, cx));
        }
        true
    }

    /// The left button came up: a strip drag or a held document lets go.
    /// Answers whether either was in progress.
    pub(super) fn float_drag_end(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(float) = self.float.as_mut() else {
            return false;
        };
        let was_dragging = float.drag.take().is_some();
        let was_holding = std::mem::take(&mut float.holding);
        if was_holding {
            float.view.update(cx, |v, cx| v.release(cx));
        }
        was_dragging || was_holding
    }

    /// The left button came up somewhere other than this pane. gpui hands a
    /// pane its mouse-up only while the pointer is over it, and a square
    /// dragged hard against an edge leaves the pointer outside; without this
    /// the square would stay stuck to the pointer when it came back.
    pub(super) fn on_float_release_out(
        &mut self,
        _ev: &MouseUpEvent,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.float_drag_end(cx);
        self.doc_face_release(cx);
    }

    /// The pointer's shape over the square's edges: a resize arrow while the
    /// un-bent pointer is in an edge's grip, or while a resize is held. Asked
    /// against a hitbox covering the whole screen, the bench's pointer-hook
    /// pattern, because a cursor asked for by the square's own layout box
    /// would sit where gpui laid the square out, not where the glass shows it.
    pub(super) fn float_resize_cursor(&self) -> Option<gpui::AnyElement> {
        let float = self.float.as_ref()?;
        let edges = match (float.drag.and_then(|d| d.edges), float.hover) {
            (Some(e), _) => e,
            (None, Some(crate::docopen::FloatHit::Resize(e))) => e,
            _ => return None,
        };
        let style = resize_cursor(edges);
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    canvas(
                        |bounds, window, _cx| {
                            window.insert_hitbox(bounds, gpui::HitboxBehavior::Normal)
                        },
                        move |_bounds, hitbox, window, _cx| {
                            window.set_cursor_style(style, &hitbox);
                        },
                    )
                    .size_full(),
                )
                .into_any_element(),
        )
    }

    /// The pointer moved over the square: remember which control it is on, so
    /// that control can say so. Notifies only on a change.
    pub(super) fn float_hover(&mut self, pos: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        if self.float.is_none() {
            return;
        }
        let over = self.float_hit(pos).map(|(z, _)| z.hit).filter(|h| {
            !matches!(
                h,
                crate::docopen::FloatHit::Strip | crate::docopen::FloatHit::Body
            )
        });
        if let Some(float) = self.float.as_mut() {
            if float.hover != over {
                float.hover = over;
                cx.notify();
            }
        }
    }

    /// Tell a document where the pointer is over it, un-bent through the
    /// tube and made relative to the view, or that it is not over it. The
    /// view repaints only when that changes what it draws.
    pub(super) fn doc_hover(&mut self, pos: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(view) = self.float.as_ref().map(|f| f.view.clone()) {
            let at = self
                .float_hit(pos)
                .filter(|(z, _)| z.hit == crate::docopen::FloatHit::Body)
                .map(|(z, flat)| gpui::point(px(flat.0 - z.x), px(flat.1 - z.y)));
            view.update(cx, |v, cx| v.hover(at, cx));
        }
        if let Some(view) = self.doc_on_face().map(|d| d.view.clone()) {
            let at = self.doc_face_local(pos);
            view.update(cx, |v, cx| v.hover(at, cx));
        }
    }

    /// Alt held or let go: the documents on this pane — the floating
    /// square's and the Document face's — outline everything that takes a
    /// note while the pointer is over them, so a press on any of it adds one.
    /// A bare Alt only; Alt with Ctrl or Super is some other chord.
    pub(super) fn doc_alt(&mut self, mods: &gpui::Modifiers, cx: &mut Context<Self>) {
        let held = mods.alt && !mods.control && !mods.platform;
        let float = self.float.as_ref().map(|f| f.view.clone());
        let face = self.doc_on_face().map(|d| d.view.clone());
        for view in [float, face].into_iter().flatten() {
            view.update(cx, |v, cx| v.reveal(held, cx));
        }
    }

    /// What the document on this pane — the floating square's, else the
    /// Document face's — shows of a brief's notes, as one line of JSON for
    /// the control socket. An error sentence when there is none to ask.
    pub(crate) fn doc_notes(&self, cx: &App) -> Result<String, String> {
        let view = self
            .float
            .as_ref()
            .map(|f| f.view.clone())
            .or_else(|| self.doc_on_face().map(|d| d.view.clone()))
            .ok_or("no document is open on this pane")?;
        let report = view
            .read(cx)
            .notes_report()
            .ok_or("the document is not a laid-out HTML page yet")?;
        Ok(report.to_string())
    }

    /// Every document open on this pane, for the MCP snapshot: the floating
    /// square's, and the Document face's while that face is the one showing —
    /// the same two [`Self::doc_notes`] reads, and the same report, so the map
    /// `document_notes` hands an agent is the one `ctl doc notes` prints. An
    /// `Err` says why a document has no report.
    pub(crate) fn documents_open(
        &self,
        cx: &App,
    ) -> Vec<(
        crate::mcp::DocPlace,
        String,
        Result<serde_json::Value, String>,
    )> {
        use crate::mcp::DocPlace;
        let float = self.float.as_ref().map(|f| (DocPlace::Float, &f.view));
        let face = self.doc_on_face().map(|d| (DocPlace::Split, &d.view));
        [float, face]
            .into_iter()
            .flatten()
            .map(|(place, view)| {
                let v = view.read(cx);
                let target = v.target();
                let report = v.notes_report().ok_or_else(|| {
                    match target.kind {
                        crate::docopen::DocKind::Html => "the page is not laid out yet",
                        crate::docopen::DocKind::Markdown => {
                            "the Markdown document is not read yet"
                        }
                        crate::docopen::DocKind::Image => "an image takes no notes",
                        crate::docopen::DocKind::Video => "a video takes no notes",
                    }
                    .to_string()
                });
                (place, target.path.to_string_lossy().into_owned(), report)
            })
            .collect()
    }

    /// Who this document pane was opened beside, by host id. See
    /// [`Self::doc_opened_by`]'s field.
    pub(crate) fn doc_opened_by(&self) -> Option<u64> {
        self.doc_opened_by
    }

    /// Whether the Document face is the one showing.
    pub(crate) fn has_doc_face(&self) -> bool {
        self.doc_on_face().is_some()
    }

    /// Who ↪ on the square's document and on the face's sends to, as the
    /// workspace last said.
    pub(crate) fn notes_beside(&self) -> &(Option<String>, Option<String>) {
        &self.notes_beside
    }

    /// Record who this document pane was opened beside. The workspace calls
    /// it once, as it makes the split.
    pub(crate) fn set_doc_opened_by(&mut self, opener: Option<u64>) {
        self.doc_opened_by = opener;
    }

    /// A notes command for the document on this pane — the floating
    /// square's, else the Document face's — answered with one line of JSON.
    pub(crate) fn doc_note(
        &mut self,
        cmd: crate::docview::NotesCommand,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let view = self
            .float
            .as_ref()
            .map(|f| f.view.clone())
            .or_else(|| self.doc_on_face().map(|d| d.view.clone()))
            .ok_or("no document is open on this pane")?;
        let report = view.update(cx, |v, cx| v.notes_command(cmd, cx))?;
        cx.notify();
        Ok(report.to_string())
    }

    /// Whether a document is open on this pane, floating or on its face.
    pub(crate) fn has_document(&self) -> bool {
        self.float.is_some() || self.doc_on_face().is_some()
    }

    /// The document a pointer is over, un-bent through the tube: the floating
    /// square anywhere on it, its strip and edges included, or the Document
    /// face's content. Not the face's header, which is chrome on every face
    /// and keeps the chrome dial.
    pub(super) fn doc_under(
        &self,
        pos: gpui::Point<Pixels>,
    ) -> Option<gpui::Entity<crate::docview::DocumentView>> {
        if let Some(face) = self.doc_on_face() {
            return (self.size_dial_under(pos) == Some(crate::theme::GradeKey::TextSize))
                .then(|| face.view.clone());
        }
        self.float_hit(pos)?;
        self.float.as_ref().map(|f| f.view.clone())
    }

    /// A wheel turn on the Document face moves the document, and one over the
    /// floating square pans the document in it. Ctrl held zooms the document
    /// instead, which the chord decides, so it is asked first. Answers whether
    /// the turn was taken.
    pub(super) fn doc_wheel(&mut self, ev: &ScrollWheelEvent, cx: &mut Context<Self>) -> bool {
        if self.size_by_wheel(ev, cx) {
            return true;
        }
        // Anywhere on the Document face: the whole screen is the document, so
        // there is no scrollback under the pointer for the turn to reach.
        if let Some(view) = self.doc_on_face().map(|d| d.view.clone()) {
            let delta = ev.delta;
            view.update(cx, |v, cx| v.wheel(delta, cx));
            return true;
        }
        if self.float_hit(ev.position).is_none() {
            return false;
        }
        if let Some(float) = self.float.as_ref() {
            let delta = ev.delta;
            float.view.update(cx, |v, cx| v.wheel(delta, cx));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parker: "gotta be able to RESIZE the floating box". An edge's grip
    /// outranks the strip and the document under it, and never a button: the
    /// ✕ at the strip's right end keeps every pixel of its face.
    #[test]
    fn an_edge_resizes_the_square_but_never_steals_a_button() {
        use crate::docopen::{Edges, FloatHit, FloatRect, FloatZone};
        let screen = (0.0, 0.0, 1000.0, 800.0);
        let rect = FloatRect {
            x: 100.0,
            y: 100.0,
            w: 400.0,
            h: 300.0,
        };
        let zones = [
            FloatZone {
                x: 100.0,
                y: 100.0,
                w: 400.0,
                h: 22.0,
                hit: FloatHit::Strip,
            },
            FloatZone {
                x: 440.0,
                y: 100.0,
                w: 60.0,
                h: 22.0,
                hit: FloatHit::Close,
            },
            FloatZone {
                x: 100.0,
                y: 122.0,
                w: 400.0,
                h: 278.0,
                hit: FloatHit::Body,
            },
        ];
        let hit = |x: f32, y: f32| {
            float_hit_through_glass(screen, (0.0, 0.0), rect, &zones, (x, y)).map(|(z, _)| z.hit)
        };
        assert_eq!(
            hit(498.0, 110.0),
            Some(FloatHit::Close),
            "the button keeps its edge pixels"
        );
        let grip = |left, right, top, bottom| {
            Some(FloatHit::Resize(Edges {
                left,
                right,
                top,
                bottom,
            }))
        };
        assert_eq!(hit(498.0, 250.0), grip(false, true, false, false));
        assert_eq!(hit(102.0, 396.0), grip(true, false, false, true));
        assert_eq!(
            hit(102.0, 102.0),
            grip(true, false, true, false),
            "top left, over the strip"
        );
        let arrow = |left, right, top, bottom| {
            super::resize_cursor(Edges {
                left,
                right,
                top,
                bottom,
            })
        };
        assert_eq!(
            arrow(true, false, true, false),
            gpui::CursorStyle::ResizeUpLeftDownRight
        );
        assert_eq!(
            arrow(false, true, false, true),
            gpui::CursorStyle::ResizeUpLeftDownRight
        );
        assert_eq!(
            arrow(false, true, true, false),
            gpui::CursorStyle::ResizeUpRightDownLeft
        );
        assert_eq!(
            arrow(true, false, false, true),
            gpui::CursorStyle::ResizeUpRightDownLeft
        );
        assert_eq!(
            arrow(false, false, false, true),
            gpui::CursorStyle::ResizeUpDown
        );
        assert_eq!(
            arrow(true, false, false, false),
            gpui::CursorStyle::ResizeLeftRight
        );
        assert_eq!(hit(300.0, 250.0), Some(FloatHit::Body));
        assert_eq!(hit(300.0, 110.0), Some(FloatHit::Strip));
        assert_eq!(hit(50.0, 50.0), None);
    }

    /// The square is drawn flat and the barrel pass moves it, so a pointer on
    /// what the glass shows as its edge is, in flat terms, somewhere else. With
    /// a real curvature there are points the flat test misses and the un-bent
    /// test finds; with none, the two agree everywhere.
    #[test]
    fn a_press_on_a_bent_float_is_found_where_the_glass_shows_it() {
        let screen = (100.0, 40.0, 1000.0, 800.0);
        let rect = crate::docopen::float_home(1000.0, 800.0, 20.0, 40.0);
        let flat_hit = |pos: (f32, f32)| rect.contains(pos.0 - screen.0, pos.1 - screen.1);
        let mut differs = 0;
        for i in 0..200 {
            for j in 0..160 {
                let pos = (screen.0 + i as f32 * 5.0, screen.1 + j as f32 * 5.0);
                assert_eq!(
                    point_on_float(screen, (0.0, 0.0), rect, pos),
                    flat_hit(pos),
                    "a flat pane must hit exactly where it draws: {pos:?}"
                );
                if point_on_float(screen, (0.2, 0.05), rect, pos) != flat_hit(pos) {
                    differs += 1;
                }
            }
        }
        assert!(
            differs > 0,
            "on a bent pane the un-bent hit must differ from the flat one somewhere"
        );
        let centre = (
            screen.0 + rect.x + rect.w / 2.0,
            screen.1 + rect.y + rect.h / 2.0,
        );
        assert!(point_on_float(screen, (0.2, 0.05), rect, centre));

        // The strip — the part a person grabs to move the square — is found
        // through the same inverse. Zones are recorded flat, in window pixels,
        // as the square paints; a pointer is un-bent before it is looked up.
        use crate::docopen::{FloatHit, FloatZone};
        let (x, y) = (screen.0 + rect.x, screen.1 + rect.y);
        let zones = [
            FloatZone {
                x,
                y,
                w: rect.w,
                h: 22.0,
                hit: FloatHit::Strip,
            },
            FloatZone {
                x: x + rect.w - 40.0,
                y,
                w: 40.0,
                h: 22.0,
                hit: FloatHit::Close,
            },
            FloatZone {
                x,
                y: y + 22.0,
                w: rect.w,
                h: rect.h - 22.0,
                hit: FloatHit::Body,
            },
        ];
        // Flat, a press finds the zone it drew, except that an edge's grip
        // outranks the strip and the document (the resize), never a button.
        let flat_zone = |pos: (f32, f32)| {
            let drawn = crate::docopen::float_hit_at(&zones, pos.0, pos.1).map(|z| z.hit);
            if matches!(drawn, Some(FloatHit::Strip) | Some(FloatHit::Body)) {
                // Un-bent the same way a press is (the identity, up to the
                // float rounding a boundary sample can land on).
                let (fx, fy) = crate::workbench::unwarp(screen, 0.0, 0.0, pos.0, pos.1);
                if let Some(e) = crate::docopen::float_edge_at(rect, fx - screen.0, fy - screen.1) {
                    return Some(FloatHit::Resize(e));
                }
            }
            drawn
        };
        let bent = |k: (f32, f32), pos: (f32, f32)| {
            float_hit_through_glass(screen, k, rect, &zones, pos).map(|(z, _)| z.hit)
        };
        let mut strip_only_through_the_glass = 0;
        for i in 0..400 {
            for j in 0..320 {
                let pos = (screen.0 + i as f32 * 2.5, screen.1 + j as f32 * 2.5);
                assert_eq!(
                    bent((0.0, 0.0), pos),
                    flat_zone(pos),
                    "a flat pane finds exactly the zone it drew: {pos:?}"
                );
                if bent((0.2, 0.05), pos) == Some(FloatHit::Strip)
                    && flat_zone(pos) != Some(FloatHit::Strip)
                {
                    strip_only_through_the_glass += 1;
                }
            }
        }
        assert!(
            strip_only_through_the_glass > 0,
            "on a bent pane, somewhere a press misses the strip flat and finds it through the glass"
        );
        // Between opening the square and its first paint there are no zones,
        // and a press on it is still its own.
        assert_eq!(
            float_hit_through_glass(screen, (0.2, 0.05), rect, &[], centre).map(|(z, _)| z.hit),
            Some(FloatHit::Body)
        );
    }
}
