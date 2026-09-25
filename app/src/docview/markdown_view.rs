//! The Markdown backend's view side: the file read off the main thread and
//! read again when it changes, the pictures it embeds decoded and owned, and
//! a press looked up among its links.
//!
//! The document itself — its parse, its scroll, the pictures it holds — is
//! [`MarkdownDoc`], which lives beside the renderer in `markdown.rs` because
//! the bench draws Markdown with the same code. What is here needs the view:
//! tasks that come back to it through a weak handle, and the link text its
//! last render laid out. It sat in `docview.rs` until the view's backends went
//! behind one trait ([`super::backend`]), and moved here as it was.
//!
//! # Notes
//!
//! A Markdown document takes notes as a brief does, through the same layer
//! ([`NotesLayer`]): a 💬 on each block under the pointer, the same note box,
//! the same bar with ⎘ copy map and ↪. The blocks are its anchors, placed
//! where the last paint put them; the 💬 sits in a gutter on the column's
//! right. What differs is where the notes go — TD's own store, as each is
//! written, never the file (`md_notes.rs`) — so the bar has no 💾. The bar
//! is always there, at "0 notes" on a file nobody has commented on, as a
//! brief's is: it is what says the file takes notes at all.

use std::any::Any;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use gpui::{
    div, point, prelude::*, px, AnyElement, App, Bounds, Context, ImgResourceLoader, Keystroke,
    Pixels, Point, RenderImage, Resource, ScrollDelta, Size, Task, Window,
};

use super::backend::{Backend, Drawn};
use super::engine::{Anchor, RectCss};
use super::markdown::{self, MarkdownDoc, NOTE_GUTTER, PAD};
use super::md_notes;
use super::notes_ui::{self, LayerPress, MarkHit, NotesLayer, Said, BUTTON_CSS};
use super::page::PageToView;
use super::{
    image, resolve_link, DocumentView, FileStamp, FollowLink, LinkTarget, NotesCommand, SendNotes,
};
use crate::docopen::DocScroll;
use crate::theme::Theme;

/// How far into the column's left padding a block's rule sits.
const RULE_OUT: f32 = 9.;

impl MarkdownDoc {
    /// The blocks that take notes, and the items of lists, as the notes layer
    /// takes anchors: each where the last paint put it, relative to the column's top-left at
    /// scroll 0, so it moves with the scroll. Its box reaches into the left
    /// padding, where its rule is drawn, and across the gutter, where its 💬
    /// is. A block not painted yet has no place and draws no mark.
    fn note_anchors(&self) -> Vec<Anchor> {
        let painted = self.painted.get() == Some(self.generation);
        let col = self.column.get().filter(|_| painted);
        let (blocks, items) = (self.tops.blocks.borrow(), self.tops.items.borrow());
        self.anchors
            .iter()
            .map(|a| {
                // A list item's own box; any other block's.
                let at = match a.item {
                    Some(j) => items.get(a.block).and_then(|v| v.get(j)).copied().flatten(),
                    None => blocks.get(a.block).copied().flatten(),
                };
                let placed = col.zip(at);
                let (rect, button) = match placed {
                    Some((col, b)) => {
                        let x = f32::from(b.origin.x - col.origin.x) - RULE_OUT;
                        let y = f32::from(b.origin.y - col.origin.y);
                        let right = f32::from(col.size.width) - PAD - NOTE_GUTTER;
                        (
                            Some(RectCss {
                                x,
                                y,
                                w: right + NOTE_GUTTER - x,
                                h: f32::from(b.size.height),
                            }),
                            Some(RectCss {
                                x: right + (NOTE_GUTTER - BUTTON_CSS) / 2.0,
                                y,
                                w: BUTTON_CSS,
                                h: BUTTON_CSS,
                            }),
                        )
                    }
                    None => (None, None),
                };
                Anchor {
                    nid: a.nid.clone(),
                    title: a.title.clone(),
                    tag: "md".into(),
                    dialog: None,
                    rect,
                    button,
                    concur_zone: None,
                    has_note: None,
                    has_concur: None,
                    line: a.line,
                }
            })
            .collect()
    }

    /// The column to the view: one logical pixel each, scrolled by hand.
    fn note_map(&self, view: Size<Pixels>) -> PageToView {
        PageToView {
            origin: point(px(0.), px(0.)),
            px_per_css: 1.0,
            scroll: px(self.top),
            clip: Bounds {
                origin: point(px(0.), px(0.)),
                size: view,
            },
        }
    }

    /// The nids whose 💬 the pointer shows.
    fn lit(&self) -> Vec<String> {
        match self.view {
            Some(view) => NotesLayer::lit(&self.note_anchors(), &self.note_map(view), self.pointer),
            None => Vec::new(),
        }
    }

    /// Keep what was written: the edits waiting go to TD's store off the
    /// main thread, applied to what the store holds then. One keep at a
    /// time; edits made while it runs go in the next, started when it lands.
    /// A keep that failed is not retried until the next edit, so a store that
    /// cannot be written is said once rather than hammered.
    fn keep(&mut self, cx: &mut Context<DocumentView>) {
        let Some(layer) = self.notes.as_mut() else {
            return;
        };
        let Some(doc) = layer.store_doc().map(Path::to_path_buf) else {
            return;
        };
        if !layer.wants_keeping() {
            return;
        }
        let edits = layer.begin_save();
        let made = edits.len();
        let known: Vec<String> = self.anchors.iter().map(|a| a.nid.clone()).collect();
        let write = cx.background_spawn(async move {
            let known: Vec<&str> = known.iter().map(String::as_str).collect();
            md_notes::keep(&md_notes::store_for(&doc), &doc, &edits, &known)
        });
        self.keeping = cx.spawn(async move |this, cx| {
            let done = write.await;
            this.update(cx, |view, cx| {
                let Some(md) = view.backend.downcast_mut::<MarkdownDoc>() else {
                    return;
                };
                let landed = done.is_ok();
                if let Some(l) = md.notes.as_mut() {
                    match done {
                        Ok(notes) => l.kept(made, notes),
                        Err(why) => l.refused(why),
                    }
                }
                if landed {
                    md.keep(cx);
                }
                cx.notify();
            })
            .ok();
        });
    }

    /// A press on the notes: the note box and the bar first, as they are
    /// drawn over everything, then a block's 💬. `None` when the notes did
    /// not take it, so a link under it can.
    fn press_notes(
        &mut self,
        at: Point<Pixels>,
        view: &Drawn,
        cx: &mut Context<DocumentView>,
    ) -> Option<bool> {
        let origin = view.placed?;
        let (size, _) = view.frame?;
        let anchors = self.note_anchors();
        let map = self.note_map(size);
        let (beside, pointer) = (self.beside.is_some(), self.pointer);
        let layer = self.notes.as_mut()?;
        let pressed = layer.press(origin + at, &anchors, SystemTime::now(), beside, cx);
        let opened = match pressed {
            LayerPress::Pass => match notes_ui::hit(&layer.marks(&anchors, &map, pointer), at) {
                Some(MarkHit::Open { nid, title }) => {
                    layer.open(nid, title);
                    true
                }
                _ => false,
            },
            _ => false,
        };
        match pressed {
            LayerPress::Pass if !opened => return None,
            LayerPress::Send(sending) => cx.emit(SendNotes {
                map: sending.map,
                unsaved: sending.unsaved,
            }),
            _ => {}
        }
        self.keep(cx);
        cx.notify();
        Some(true)
    }
}

// The view's calls, handed to the document's own methods in `markdown.rs`.
// Where a method there has the trait method's name, it is named with its type
// (`MarkdownDoc::wheel(self, …)`): Rust finds an inherent method before a
// trait's, so that is the document's own and not this one calling itself.
impl Backend for MarkdownDoc {
    /// The column, and over it the notes: each block's rule and 💬, the bar,
    /// and the note box above everything.
    fn element(&mut self, view: &Drawn, window: &mut Window, th: &Theme) -> AnyElement {
        let size = view.frame.map(|(size, _)| size);
        self.view = size;
        let column =
            MarkdownDoc::element(self, view.path, th, size, view.links, NOTE_GUTTER, window);
        let (Some(layer), Some(size)) = (self.notes.as_ref(), size) else {
            return column;
        };
        // Forget where the bar and the note box were; the canvases below
        // record where they land this frame.
        layer.clear_zones();
        let anchors = self.note_anchors();
        let marks = layer.marks(&anchors, &self.note_map(size), self.pointer);
        let mut layers = vec![column];
        layers.extend(layer.draw_marks(&marks, th));
        layers.push(layer.draw_bar(&anchors, th, self.beside.as_deref()));
        layers.extend(layer.draw_box(size, th));
        div()
            .absolute()
            .inset_0()
            .children(layers)
            .into_any_element()
    }

    /// Every picture the document holds, and every decode and read still
    /// running for it.
    fn give_back(&mut self, path: &Path, cx: &mut App) {
        self.decoding.clear();
        for (_, image) in self.images.drain() {
            if let Ok(image) = image {
                cx.drop_image(image, None);
            }
        }
        if std::env::var_os("TD_DOCDEBUG").is_some() {
            eprintln!("[doc] released {}", path.display());
        }
        self.reading = Task::ready(());
        // A note being kept is let finish: it is the person's words, and it
        // holds nothing on the GPU.
        std::mem::replace(&mut self.keeping, Task::ready(())).detach();
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// A press on the document: the notes first (see
    /// [`MarkdownDoc::press_notes`]), then its links: a heading in this
    /// document is scrolled to here, and anything else is emitted as
    /// [`FollowLink`] for the pane to route.
    fn press(&mut self, at: Point<Pixels>, view: &Drawn, cx: &mut Context<DocumentView>) -> bool {
        if let Some(took) = self.press_notes(at, view, cx) {
            return took;
        }
        let (Some(origin), Some((size, _))) = (view.painted_at, view.frame) else {
            return false;
        };
        // Above the view is the square's strip; a link scrolled up under it
        // is not what was pressed.
        if at.x < px(0.) || at.y < px(0.) || at.x >= size.width || at.y >= size.height {
            return false;
        }
        let Some(href) = markdown::link_at(&view.links.borrow(), origin + at) else {
            return false;
        };
        let dir = view.path.parent().unwrap_or(Path::new("/"));
        let view_h = Some(f32::from(size.height));
        match resolve_link(dir, &href) {
            LinkTarget::Fragment(fragment) => self.go_to_fragment(fragment, view_h),
            LinkTarget::File { path, fragment } if path == view.path => {
                if let Some(fragment) = fragment {
                    self.go_to_fragment(fragment, view_h);
                }
            }
            LinkTarget::File { path, fragment } => cx.emit(FollowLink {
                target: path.to_string_lossy().into_owned(),
                fragment,
            }),
            LinkTarget::Url(url) => cx.emit(FollowLink {
                target: url,
                fragment: None,
            }),
        }
        cx.notify();
        true
    }

    /// Scrolls the column by hand: nothing under `docview` scrolls itself.
    fn wheel(
        &mut self,
        delta: ScrollDelta,
        line: f32,
        view: Size<Pixels>,
        _sf: f32,
        _cx: &mut Context<DocumentView>,
    ) -> bool {
        MarkdownDoc::wheel(self, delta, line, Some(f32::from(view.height)))
    }

    /// The text grows and the column re-flows at the new size; the block the
    /// reader was on stays at the top.
    fn zoom(
        &mut self,
        step: image::ZoomStep,
        _view: Size<Pixels>,
        _sf: f32,
        cx: &mut Context<DocumentView>,
    ) -> bool {
        let next = super::step_reading_zoom(self.zoom, step);
        if (next - self.zoom).abs() < 1e-3 {
            return false;
        }
        self.set_zoom(next);
        cx.notify();
        true
    }

    fn zoom_now(&self) -> Option<image::ImageZoom> {
        Some(image::ImageZoom::Scale(self.zoom))
    }

    fn scroll(&self) -> Option<DocScroll> {
        MarkdownDoc::scroll(self)
    }

    fn restore_scroll(&mut self, at: DocScroll, cx: &mut Context<DocumentView>) {
        self.restore_fraction(at.top);
        cx.notify();
    }

    fn show_fragment(
        &mut self,
        fragment: String,
        view_h: Option<f32>,
        cx: &mut Context<DocumentView>,
    ) {
        self.go_to_fragment(fragment, view_h);
        cx.notify();
    }

    /// Re-read on every change, keeping the reader's place.
    fn follows_its_file(&self) -> bool {
        true
    }

    /// The first read starts now. Its stamp comes back with it, so there is
    /// none to give the watcher yet.
    fn opened(&mut self, path: &Path, cx: &mut Context<DocumentView>) -> Option<FileStamp> {
        read_markdown(self, path, cx);
        None
    }

    fn file_changed(&mut self, path: &Path, _came_back: bool, cx: &mut Context<DocumentView>) {
        read_markdown(self, path, cx);
    }

    // ── notes ───────────────────────────────────────────────────────────────

    /// The 💬 on the block under the pointer shows; repaints only when that
    /// changes which one.
    fn hover(&mut self, at: Option<Point<Pixels>>, cx: &mut Context<DocumentView>) {
        if self.pointer == at {
            return;
        }
        let before = self.lit();
        self.pointer = at;
        if self.lit() != before {
            cx.notify();
        }
    }

    /// The note box takes every key while it is open, and Escape puts it
    /// away before it closes the square. A note added by Ctrl+Enter is kept
    /// at once. With nothing open, Escape in a floating square that would
    /// lose a note not yet kept says so once.
    fn key(&mut self, ks: &Keystroke, floating: bool, cx: &mut Context<DocumentView>) -> bool {
        let Some(layer) = self.notes.as_mut() else {
            return false;
        };
        if layer.key(ks, SystemTime::now()) {
            self.keep(cx);
            cx.notify();
            return true;
        }
        if ks.key != "escape" {
            return false;
        }
        floating && self.guard_close(cx)
    }

    fn has_caret(&self) -> bool {
        self.notes.as_ref().is_some_and(NotesLayer::has_caret)
    }

    fn set_beside(&mut self, beside: Option<String>) -> bool {
        if self.beside == beside {
            return false;
        }
        self.beside = beside;
        self.notes.is_some()
    }

    /// What came of a ↪, on its own line of the bar.
    fn notes_said(&mut self, said: Said, cx: &mut Context<DocumentView>) {
        if let Some(layer) = self.notes.as_mut() {
            layer.say_sent(said);
        }
        cx.notify();
    }

    /// What the bar shows and the map, as a brief's report has them, with
    /// who ↪ would send to. `None` until the file and its notes have been
    /// read.
    fn notes_report(&self) -> Option<serde_json::Value> {
        let layer = self.notes.as_ref()?;
        let anchors = self.note_anchors();
        let mut report = layer.report(&anchors);
        report["send_to"] = serde_json::json!(layer
            .send_button(&anchors, self.beside.as_deref())
            .map(|(label, _)| label));
        Some(report)
    }

    /// The note box's gestures from the control socket. There is no save to
    /// ask for, because every note is kept as it is written; asking for one
    /// keeps whatever is still waiting. A Markdown block takes no stamp.
    fn notes_command(
        &mut self,
        cmd: NotesCommand,
        cx: &mut Context<DocumentView>,
    ) -> Result<serde_json::Value, String> {
        let not_read = "the Markdown document has not been read yet";
        let layer = self.notes.as_mut().ok_or(not_read)?;
        match cmd {
            NotesCommand::Save => {}
            NotesCommand::Add { nid, text } => {
                layer.can_edit()?;
                let a = self
                    .anchors
                    .iter()
                    .find(|a| a.nid == nid)
                    .ok_or(format!("There is no block [{nid}] in this document."))?;
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err("A note needs some words.".into());
                }
                layer.add_note(
                    nid,
                    a.title.clone(),
                    text,
                    super::notes::utc_minute(SystemTime::now()),
                );
            }
            NotesCommand::Delete { nid, text } => {
                layer.can_edit()?;
                layer.delete_note(nid, text, None);
            }
            NotesCommand::Concur { nid } => {
                return Err(format!(
                    "[{nid}] is a Markdown block, which takes no CONCUR stamp."
                ));
            }
        }
        self.keep(cx);
        cx.notify();
        Backend::notes_report(self).ok_or_else(|| not_read.into())
    }

    /// Once, when closing would lose a note not yet kept: one whose keep
    /// failed, or is still running.
    fn guard_close(&mut self, cx: &mut Context<DocumentView>) -> bool {
        if self.notes.as_mut().is_some_and(NotesLayer::guard_close) {
            cx.notify();
            return true;
        }
        false
    }
}

/// A picture a Markdown document embeds, as decoded, or the sentence its box
/// says instead.
fn embedded<E: std::fmt::Display>(
    decoded: Result<Arc<RenderImage>, E>,
) -> Result<Arc<RenderImage>, String> {
    let image = decoded.map_err(|e| format!("could not be read: {e}"))?;
    let size = image.size(0);
    let (w, h) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
    if image::too_large(w, h) {
        return Err(format!(
            "{w} × {h} pixels, past the {} the GPU can hold here",
            image::MAX_SIDE
        ));
    }
    Ok(image)
}

/// Read and parse the Markdown file off the main thread, and the notes TD
/// keeps for it with it.
fn read_markdown(md: &mut MarkdownDoc, path: &Path, cx: &mut Context<DocumentView>) {
    let path = path.to_path_buf();
    let read = cx.background_executor().spawn(async move {
        // Stamped BEFORE reading: a write landing in between leaves a
        // stamp older than the text, and the next tick reads again, which
        // is harmless. The other order could miss that write for good.
        let stamp = FileStamp::of(&path);
        let parsed = std::fs::read(&path)
            .map(|bytes| markdown::parse(&String::from_utf8_lossy(&bytes), path.parent()))
            .map_err(|e| format!("Could not read {}: {e}", path.display()));
        let kept = md_notes::read(&md_notes::store_for(&path));
        (stamp, parsed, kept)
    });
    md.reading = cx.spawn(async move |this, cx| {
        let (stamp, parsed, kept) = read.await;
        this.update(cx, |view, cx| markdown_read(view, stamp, parsed, kept, cx))
            .ok();
    });
}

fn markdown_read(
    view: &mut DocumentView,
    stamp: Option<FileStamp>,
    parsed: Result<markdown::MdDoc, String>,
    kept: Result<super::notes::NoteMap, String>,
    cx: &mut Context<DocumentView>,
) {
    view.seen = stamp;
    let path = view.target.path.clone();
    let Some(md) = view.backend.downcast_mut::<MarkdownDoc>() else {
        return;
    };
    match md.notes.as_mut() {
        Some(layer) => layer.rekept(kept),
        None => md.notes = Some(NotesLayer::for_markdown(&path, kept)),
    }
    match parsed {
        Ok(doc) => {
            md.anchors = md_notes::anchors(&doc);
            for image in md.replace(Rc::new(doc)) {
                cx.drop_image(image, None);
            }
        }
        // A document already on screen stays there when a re-read fails:
        // an editor writing in two steps would otherwise flash an error.
        Err(why) if matches!(md.doc, Some(Ok(_))) => {
            if std::env::var_os("TD_DOCDEBUG").is_some() {
                eprintln!("[doc] kept the last render: {why}");
            }
        }
        Err(why) => {
            if std::env::var_os("TD_DOCDEBUG").is_some() {
                eprintln!("[doc] cannot read {}", view.target.path.display());
            }
            md.doc = Some(Err(why));
        }
    }
    decode_images(md, cx);
    cx.notify();
}

/// Start decoding every local picture the document draws and the view
/// does not hold yet, owned the way the image backend owns its one.
fn decode_images(md: &mut MarkdownDoc, cx: &mut Context<DocumentView>) {
    let Some(Ok(doc)) = md.doc.clone() else {
        return;
    };
    for path in markdown::image_paths(&doc) {
        if md.images.contains_key(&path) || md.decoding.contains_key(&path) {
            continue;
        }
        let resource = Resource::Path(Arc::from(path.as_path()));
        let (decode, _) = cx.fetch_asset::<ImgResourceLoader>(&resource);
        // Out of gpui's cache at once, as the image backend does: the
        // pixels are this view's alone, so dropping them frees them.
        cx.remove_asset::<ImgResourceLoader>(&resource);
        let key = path.clone();
        let task = cx.spawn(async move |this, cx| {
            let result = embedded(decode.await);
            this.update(cx, |view, cx| image_decoded(view, key, result, cx))
                .ok();
        });
        md.decoding.insert(path, task);
    }
}

fn image_decoded(
    view: &mut DocumentView,
    path: PathBuf,
    result: Result<Arc<RenderImage>, String>,
    cx: &mut Context<DocumentView>,
) {
    let Some(md) = view.backend.downcast_mut::<MarkdownDoc>() else {
        return;
    };
    // No longer wanted — a re-read stopped drawing it. It was never
    // painted, so it is in no atlas; letting it drop is the whole release.
    let Some(running) = md.decoding.remove(&path) else {
        return;
    };
    // This is that task, finishing: let it finish rather than cancel it
    // from inside itself.
    running.detach();
    md.images.insert(path, result);
    cx.notify();
}
