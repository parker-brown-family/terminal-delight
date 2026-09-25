//! What a document view asks of whatever draws its document.
//!
//! # One door
//!
//! [`DocumentView`] shows three kinds of document: a picture
//! ([`super::image::ImageDoc`]), a Markdown file
//! ([`super::markdown::MarkdownDoc`]) and an HTML page
//! ([`super::page::PageDoc`]). It used to tell them apart with an enum,
//! matched afresh in every method that had to know — a press, a wheel turn, a
//! key, a zoom, a scroll to restore, a fragment to land on, a note to save —
//! fifty-odd arms across twenty-odd methods. Every new capability was another
//! match in every one of them, and a kind of document that did not do a
//! thing said so by not being matched, in an `if let` on one variant or a
//! `_ => false`. That reads exactly like a kind nobody remembered, and once it
//! was one: `show_fragment` matched only Markdown, and a link into the middle
//! of a brief opened it at the top (issue 734, "A link into the middle of a
//! brief lands on the element it names").
//!
//! So the view holds one `Box<dyn Backend>` and hands every call through it,
//! and each backend implements the trait beside the rest of its code: in
//! `image.rs`, `markdown_view.rs` and `page.rs`. What a backend cannot do is
//! written down here, once, as the trait's default, with the reason on the
//! default rather than on a wildcard in some method's last arm. A backend that
//! can do a thing overrides it; a new kind of document starts out doing
//! nothing but drawing itself and giving its textures back, and every
//! capability it gains is a method it chose to write.
//!
//! # What has no default
//!
//! Three methods, on purpose. [`Backend::element`], because a document that
//! draws nothing is not a document. [`Backend::give_back`], because
//! everything on the GPU is ours and is given back when the view drops (see
//! the notes in `docview.rs`), and a backend should have to say how rather
//! than inherit a silence. [`Backend::as_any_mut`], because the tasks a
//! backend spawns hold only a weak handle to the view, and have to find their
//! own backend again when they come back through it.
//!
//! # What stays in the view
//!
//! Whatever is the same for every document: the release hook, the file
//! watcher and its stamps, the canvases that measure the view, the theme and
//! the seat. A gesture that needs the view's size is not passed on until the
//! view has been measured, because an unmeasured view is not a zero-sized
//! one; so no backend is ever handed a size nobody measured.
//!
//! # Still no mouse handlers
//!
//! Every point handed through here was un-bent by the pane first (see "No
//! mouse handlers, on purpose" in `docview.rs`). Nothing behind this trait
//! may register a gpui mouse, scroll, hover or drag handler, and
//! `nothing_in_the_document_view_listens_for_the_mouse` scans this file and
//! every backend's.

use std::any::Any;
use std::path::Path;

use gpui::{AnyElement, App, Context, Keystroke, Pixels, Point, ScrollDelta, Size, Window};

use super::image::{ImageZoom, ZoomStep};
use super::markdown::LinkSink;
use super::notes_ui::Said;
use super::page::Measured;
use super::{DocumentView, FileStamp, Frame, NotesCommand};
use crate::docopen::DocScroll;
use crate::theme::Theme;

/// The view as its last paint left it, lent to a backend for one call: the
/// file it shows, and what its canvases measured. Everything measured is an
/// `Option`, `None` until the first paint: a view nobody has measured has no
/// size, which is not a size of zero.
pub struct Drawn<'a> {
    /// The file the view was opened on.
    pub path: &'a Path,
    /// The view's own size and the window's scale factor.
    pub frame: Option<Frame>,
    /// The view's flat top-left in window pixels, as the last paint placed
    /// it. Survives the next render; tiles are snapped against it.
    pub placed: Option<Point<Pixels>>,
    /// The same point, but only once everything in the current render has
    /// been laid out, every link's text included. `None` from the top of each
    /// render until its last canvas paints.
    pub painted_at: Option<Point<Pixels>>,
    /// Paragraphs holding links, as the last render built them.
    pub links: &'a LinkSink,
}

/// What a document view asks of whatever draws its document. See the module
/// notes for why all but three of these have a default.
pub trait Backend {
    // ── what every backend has to say ───────────────────────────────────────

    /// The document as drawn this frame, inside the view's own box.
    fn element(&mut self, view: &Drawn, window: &mut Window, th: &Theme) -> AnyElement;

    /// Give back everything this backend holds on the GPU, and stop the work
    /// still running for it. Runs once, from the view's release hook, when no
    /// window is mid-update, so a drop that names no window reaches them all.
    fn give_back(&mut self, path: &Path, cx: &mut App);

    /// This backend as `Any`, for a task it spawned to find it again; see
    /// `downcast_mut` on `dyn Backend`, below.
    fn as_any_mut(&mut self) -> &mut dyn Any;

    // ── gestures, already un-bent by the pane ───────────────────────────────

    /// A press, flat and view-local. Answers whether the backend took it: a
    /// press nothing took is the pane's.
    fn press(
        &mut self,
        _at: Point<Pixels>,
        _view: &Drawn,
        _cx: &mut Context<DocumentView>,
    ) -> bool {
        false
    }

    /// The pointer moved with the button still held since the press. `view`
    /// and `sf` are the view's measured size and the window's scale factor.
    /// Answers whether anything moved, for the view to repaint. Only a
    /// picture follows a drag; a document scrolls by the wheel.
    fn drag(&mut self, _at: Point<Pixels>, _view: Size<Pixels>, _sf: f32) -> bool {
        false
    }

    /// The held press ended, wherever the pointer is now.
    fn end_press(&mut self) {}

    /// A wheel turn. `line` is one line of the pane's text, for a wheel that
    /// counts in lines. Answers whether the view should repaint; a backend
    /// that repaints for itself answers false.
    fn wheel(
        &mut self,
        _delta: ScrollDelta,
        _line: f32,
        _view: Size<Pixels>,
        _sf: f32,
        _cx: &mut Context<DocumentView>,
    ) -> bool {
        false
    }

    /// The pointer moved over the view, or left it (`None`). Only a brief
    /// listens: its note buttons show under the pointer, as a browser shows
    /// them.
    fn hover(&mut self, _at: Option<Point<Pixels>>, _cx: &mut Context<DocumentView>) {}

    /// A key the pane's layer ladder handed over. `floating` says the view is
    /// the floating square, which the pane's Escape closes. Answers whether
    /// the key was taken; one that was not is the pane's.
    fn key(&mut self, _ks: &Keystroke, _floating: bool, _cx: &mut Context<DocumentView>) -> bool {
        false
    }

    /// Whether something is being written, so the pane hands the view every
    /// key that is not a chord. Only a brief's note box takes typing.
    fn has_caret(&self) -> bool {
        false
    }

    /// One press of a zoom control, or one notch of ctrl+wheel. Answers
    /// whether anything changed. A picture steps its own ladder; a page and a
    /// Markdown document step [`super::READING_ZOOM`]. A kind of document
    /// that cannot zoom says so here, and the strip draws no controls for it.
    fn zoom(
        &mut self,
        _step: ZoomStep,
        _view: Size<Pixels>,
        _sf: f32,
        _cx: &mut Context<DocumentView>,
    ) -> bool {
        false
    }

    /// The zoom to show on the strip. `None` draws no zoom controls at all,
    /// which is right for a document that cannot zoom.
    fn zoom_now(&self) -> Option<ImageZoom> {
        None
    }

    // ── where the reader is ─────────────────────────────────────────────────

    /// Where the document is scrolled, for the saved layout to keep. `None`
    /// for a document with no scroll, and for one not yet laid out, where
    /// nothing has been measured: either, written down as a number, would be
    /// a position nobody was ever at.
    fn scroll(&self) -> Option<DocScroll> {
        None
    }

    /// Go back to a place the saved layout kept, as soon as the document has
    /// been laid out. A picture has no scroll to go back to.
    fn restore_scroll(&mut self, _at: DocScroll, _cx: &mut Context<DocumentView>) {}

    /// Land on what a fragment names, now or as soon as the document has been
    /// laid out. `view_h` is the view's measured height. A picture has
    /// nothing a fragment could name.
    fn show_fragment(
        &mut self,
        _fragment: String,
        _view_h: Option<f32>,
        _cx: &mut Context<DocumentView>,
    ) {
    }

    /// The last paint measured a new size or scale. Only a page lays itself
    /// out at the view's width; a picture and a Markdown column read the size
    /// afresh as they draw.
    fn measured(&mut self, _m: Measured, _cx: &mut Context<DocumentView>) {}

    // ── the file on disk ────────────────────────────────────────────────────

    /// Whether the view should ask the disk every `WATCH_EVERY` whether the
    /// file changed, and tell this backend when it did. A picture is decoded
    /// once and never read again, so nothing asks about it.
    fn follows_its_file(&self) -> bool {
        false
    }

    /// The view has just been made. A backend that reads its file itself
    /// starts reading here; one that knows the file's stamp already answers
    /// it, for the watcher to compare every later tick against. `None` while
    /// a read has still to land, and for a backend the watcher never asks
    /// about.
    fn opened(&mut self, _path: &Path, _cx: &mut Context<DocumentView>) -> Option<FileStamp> {
        None
    }

    /// The file changed on disk, and not by this view; `came_back` when it
    /// had been missing until now.
    fn file_changed(&mut self, _path: &Path, _came_back: bool, _cx: &mut Context<DocumentView>) {}

    /// The file went missing. The last render stays up either way.
    fn file_gone(&mut self, _cx: &mut Context<DocumentView>) {}

    // ── notes ───────────────────────────────────────────────────────────────
    //
    // A brief and a Markdown file carry notes, so the page and the Markdown
    // backend override these; a picture takes none.

    /// Who the notes bar's ↪ sends to. Answers whether that changed what is
    /// drawn, so the view repaints only then.
    fn set_beside(&mut self, _beside: Option<String>) -> bool {
        false
    }

    /// What came of a ↪, said in the notes bar.
    fn notes_said(&mut self, _said: Said, _cx: &mut Context<DocumentView>) {}

    /// Alt is held, or let go: a document that takes notes outlines
    /// everything that takes one while the pointer is over it. Answers
    /// whether that changed what is drawn.
    fn reveal(&mut self, _on: bool) -> bool {
        false
    }

    /// What the notes layer shows, for the control socket. `None` for
    /// anything but a brief, and for a brief not yet laid out.
    fn notes_report(&self) -> Option<serde_json::Value> {
        None
    }

    /// A notes command from the control socket, answered with what the layer
    /// shows afterwards or the sentence that refused it.
    fn notes_command(
        &mut self,
        _cmd: NotesCommand,
        _cx: &mut Context<DocumentView>,
    ) -> Result<serde_json::Value, String> {
        Err("the document is not an HTML page".into())
    }

    /// Asked before the document is closed or replaced on purpose: answers
    /// whether it kept itself open, once, because closing would lose notes not
    /// yet saved into the file. Nothing but a brief has anything to lose.
    fn guard_close(&mut self, _cx: &mut Context<DocumentView>) -> bool {
        false
    }
}

impl dyn Backend {
    /// The backend as the type it is, for a task it spawned: such a task
    /// holds only a weak handle to the view and comes back through it. `None`
    /// when the view shows another kind of document. A view keeps the backend
    /// it was made with, so today that never happens; the task has only the
    /// view's word for it all the same.
    pub fn downcast_mut<B: Backend + 'static>(&mut self) -> Option<&mut B> {
        self.as_any_mut().downcast_mut::<B>()
    }
}
