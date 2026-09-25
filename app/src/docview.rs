//! A document drawn inside a pane: the view a floating square holds.
//!
//! # What is here
//!
//! Images, Markdown and HTML are drawn. HTML is drawn by a page engine
//! ([`engine`]) — today a snapshot taken by headless Chromium ([`snapshot`])
//! — as tiles the view scrolls itself ([`page`]). The router asks
//! [`html_ready`] before it places an HTML document, so a machine without
//! Chromium hands the file to the desktop with a sentence instead of opening
//! a square that can never fill. A browser that exists but will not start is
//! found only once the view is up; the view then says why in its body and
//! emits [`CannotShow`], and the pane hands the file over.
//!
//! # One view, three backends
//!
//! The view does what is the same for every document — the release hook, the
//! file watcher, the canvases that measure it, the theme and the seat — and
//! hands the rest to one [`backend::Backend`]: an image ([`image`]), a
//! Markdown file ([`markdown_view`]) or a page ([`page`]). What a backend
//! cannot do is the trait's default, written once with the reason beside it,
//! rather than a wildcard arm in every method that has to ask. See
//! [`backend`] for the whole contract.
//!
//! # A brief's notes
//!
//! Over an HTML page that is a decision brief, TD draws the brief's notes
//! itself ([`notes_ui`]), from the islands in the file's own bytes
//! ([`notes`]): note buttons where the brief's hidden ones keep their box,
//! CONCUR stamps at the angle the brief's notes.js gives them, a note box
//! listing what the file holds and taking new ones, and a bar that counts
//! them, copies the map and saves. The page's own notes chrome is hidden in
//! the picture. A save writes only the notes regions of the file, through
//! [`notes::commit`] — backed up, renamed into place, read back — and a file
//! changed on disk while it is open is re-read: its notes alone, or the whole
//! page when anything else changed.
//!
//! # No mouse handlers, on purpose
//!
//! Nothing in this module registers a gpui mouse, scroll or hover handler. The
//! CRT pass bends the pane's pixels after layout, and gpui hit-tests the flat
//! layout, so a handler in here would fire beside what it draws. Input reaches
//! a document the way it reaches the workbench: the pane un-bends the pointer
//! and decides, then calls [`DocumentView::press`], [`DocumentView::drag`],
//! [`DocumentView::release`], [`DocumentView::wheel`] or
//! [`DocumentView::zoom`] with flat, view-local numbers. What the view does
//! record for itself is measured at paint by canvases that listen to nothing:
//! its size, and where it was painted. A Markdown document therefore scrolls
//! by hand (its column is offset by the scroll, not placed in a scrolling
//! element) and finds a pressed link from text laid out at the last paint.
//! Guarded by `nothing_in_the_document_view_listens_for_the_mouse`.
//!
//! # Everything on the GPU is ours, and it is given back
//!
//! gpui would cache a decoded image for the life of the process, keyed by its
//! path, and keep its texture in the window's atlas until somebody asks for it
//! back. Neither is acceptable for something a person opens and closes all
//! day: a 4K screenshot is 33 MB decoded and the same again on the GPU. So the
//! view fetches every image through gpui's own loader — the picture an image
//! square shows, and every local picture a Markdown document draws — takes it
//! straight back out of gpui's cache so the view holds the only reference, and
//! when the view is dropped it drops each texture from every window's atlas.
//! A Markdown file that is re-read gives back the pictures it no longer draws
//! at the moment it stops drawing them.
//!
//! That last step hangs on gpui's release hook rather than on whoever closes
//! the square, because a square can end in more ways than Escape: its pane
//! closes, its tab closes, a replica is repaired and the pane rebuilt, a link
//! is followed and another document takes its place. Every one of those drops
//! the view, and dropping the view is what gives the textures back. gpui runs
//! release hooks when its outermost update finishes, after every window has
//! been put back in its list, so one call reaches every atlas.
//!
//! # A file that changes while it is open
//!
//! TD watches no file with inotify. The skin and theme hot-reloads poll the
//! modification time, and a Markdown view does the same every
//! [`WATCH_EVERY`], only while it is open: the poll is a task the view owns, so
//! it ends when the view does. A change re-reads and re-parses the file off the
//! main thread and keeps the reader's place — the block that was at the top of
//! the view stays at the top, or, when that block is gone, the same fraction of
//! the page.

pub mod backend;
pub mod cache;
pub mod cdp;
pub mod engine;
pub mod image;
pub mod markdown;
pub mod markdown_view;
pub mod md_notes;
pub mod notes;
pub mod notes_ui;
pub mod page;
pub mod pref;
pub mod snapshot;

use std::cell::Cell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui::{
    canvas, div, prelude::*, App, Context, EventEmitter, Global, Keystroke, Modifiers, Pixels,
    Point, ScrollDelta, Size, Task, Window,
};

use crate::docopen::{DocKind, DocScroll, DocSeat, DocTarget};
use crate::theme::Theme;
use backend::{Backend, Drawn};
use engine::{PageEngine, Unavailable};

pub use image::{ImageZoom, ZoomStep};

/// How often an open document asks whether its file changed: the skin and
/// theme hot-reload's pattern, at the pace the program design set.
pub const WATCH_EVERY: Duration = Duration::from_millis(500);

/// The zoom a page and a Markdown document step along, as a browser's does:
/// from half size to three times, closest together near 100%. A picture keeps
/// its own ladder ([`image::ZOOM_STEPS`]), because its unit is an image pixel
/// and this one's is the size the document was written at.
///
/// Three times is the top because a page is laid out afresh at every step, at
/// the window's scale times the zoom, and the device rows Chromium has to
/// capture grow with it.
pub const READING_ZOOM: [f32; 13] = [
    0.5, 0.67, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0,
];

/// Pure. One press of a zoom control on a document that is read as text. Past
/// either end it stays where it is. Fit and actual size are both 100%: text
/// has no size that fits, and its actual size is the one it was written at.
pub fn step_reading_zoom(now: f32, step: ZoomStep) -> f32 {
    const SAME: f32 = 1e-3;
    match step {
        ZoomStep::In => READING_ZOOM
            .iter()
            .copied()
            .find(|z| *z > now + SAME)
            .unwrap_or(now),
        ZoomStep::Out => READING_ZOOM
            .iter()
            .rev()
            .copied()
            .find(|z| *z < now - SAME)
            .unwrap_or(now),
        ZoomStep::FitOrActual | ZoomStep::Fit | ZoomStep::Actual => 1.0,
    }
}

/// Pure. Ctrl+wheel notches counted toward zoom steps: what stays counted,
/// and the whole steps now due, positive zooming in. A turn the other way
/// starts the count again.
pub fn count_zoom_notches(counted: f32, notches: f32) -> (f32, i32) {
    let from = if notches * counted < 0.0 {
        0.0
    } else {
        counted
    };
    let total = from + notches;
    let steps = total.trunc();
    (total - steps, steps as i32)
}

/// A document on screen.
pub struct DocumentView {
    target: DocTarget,
    /// Whatever draws the document: the picture, the Markdown file or the
    /// page. Made with the view and kept for its life; see [`backend`].
    backend: Box<dyn Backend>,
    /// The pane's resolved theme, handed down by [`Self::set_theme`]; the
    /// window's until the pane first paints the square.
    theme: Option<Arc<Theme>>,
    /// The floating square, or a pane's Document face. A view is made for
    /// one and can be moved to the other; see [`Self::set_seat`].
    seat: DocSeat,
    /// The view's own size and the window's scale factor, as the last paint
    /// measured them. `None` until it has painted once: an unmeasured view is
    /// not a zero-sized one, and nothing that needs the size runs without it.
    frame: Rc<Cell<Option<Frame>>>,
    /// The view's flat top-left in window pixels as the last paint placed it.
    /// Unlike `painted_at` it survives the next render, which is when a page
    /// needs it: tiles are snapped to the device grid from where the view is.
    placed: Rc<Cell<Option<Point<Pixels>>>>,
    /// The view's flat top-left in window pixels, recorded by the LAST thing
    /// the view paints and cleared at the top of every render. `Some` means
    /// everything above it, every link's text included, was laid out this
    /// frame, which is what makes asking that text about a point safe.
    painted_at: Rc<Cell<Option<Point<Pixels>>>>,
    /// Paragraphs holding links, as the last render built them.
    links: markdown::LinkSink,
    /// The file as last read. `None` before the first read, and while it is
    /// missing.
    seen: Option<FileStamp>,
    /// The file as this view last wrote it: a brief's notes, saved. The
    /// watcher tells that save from anyone else's, so a save never reloads
    /// the page under the person making it.
    own_write: Option<FileStamp>,
    /// Ctrl+wheel notches not yet worth a zoom step: a touchpad reports a
    /// notch in many small pieces, and each step re-lays a page out.
    zoom_notches: f32,
    _watch: Task<()>,
}

/// A view's measured size, in logical pixels, and the scale factor it was
/// measured under.
type Frame = (Size<Pixels>, f32);

/// A press on a link that leaves this document. The pane decides where it
/// goes: a file TD can draw takes the square's place, anything else goes to
/// the desktop.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FollowLink {
    /// An absolute path, or a URL as written.
    pub target: String,
    /// The heading a path's link named, kept apart from the path so a `#` in
    /// a file's name stays part of the name.
    pub fragment: Option<String>,
}

impl EventEmitter<FollowLink> for DocumentView {}

/// ↪ in a brief's notes bar: the map, for the pane to hand to the agent the
/// brief sits beside. The view cannot see past its own pane, so it says what
/// was pressed and what it carries, and the pane and the workspace decide
/// where it goes. The press has already started a save of the waiting edits
/// where one can be made; `unsaved` counts the edits in `map` that one will
/// not put in the file — it is sent as shown, and the bar says so.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SendNotes {
    pub map: String,
    pub unsaved: usize,
}

impl EventEmitter<SendNotes> for DocumentView {}

/// A brief's notes, driven by the control socket: the note box's and the
/// bar's gestures, for a caller with no pointer.
#[derive(Clone, Debug, PartialEq)]
pub enum NotesCommand {
    Add {
        nid: String,
        text: String,
    },
    Delete {
        nid: String,
        text: String,
    },
    /// Put a stamp down on a decision, or peel it off.
    Concur {
        nid: String,
    },
    Save,
}

/// The view cannot show this document after all: the pane hands the file to
/// the desktop. The view keeps saying why.
pub struct CannotShow {
    pub reason: String,
}
impl EventEmitter<CannotShow> for DocumentView {}

// ── the page engine, one per TD process ───────────────────────────────────

/// An engine, or the reason there is none.
type EngineAnswer = Result<Arc<dyn PageEngine>, Unavailable>;

/// An engine, once made, is kept; an unavailable answer is asked again after
/// [`ASK_AGAIN`], so installing Chromium does not need a TD restart.
struct Engines {
    slot: Option<(EngineAnswer, Instant)>,
}
impl Global for Engines {}

const ASK_AGAIN: Duration = Duration::from_secs(30);

/// `~/.config/terminal-delight/documents.toml`.
pub fn prefs_path() -> PathBuf {
    crate::instance::config_dir().join("documents.toml")
}

fn make_engine() -> EngineAnswer {
    let prefs = pref::load(&prefs_path()).map_err(Unavailable::Prefs)?;
    match prefs.html.engine_choice() {
        pref::EngineChoice::Off => Err(Unavailable::Off),
        pref::EngineChoice::Unknown(name) => Err(Unavailable::UnknownEngine(name)),
        pref::EngineChoice::Snapshot => {
            let path = std::env::var_os("PATH");
            let binary = snapshot::SnapshotEngine::locate(&prefs.html, path.as_deref())?;
            // TD_PAGE_IDLE_SECS shortens the idle shutdown for a soak run that
            // wants to watch the browser go; unset, it is five minutes.
            let idle = std::env::var("TD_PAGE_IDLE_SECS")
                .ok()
                .and_then(|s| s.trim().parse().ok())
                .map(Duration::from_secs);
            Ok(Arc::new(match idle {
                Some(idle) => snapshot::SnapshotEngine::with_idle(
                    prefs.html,
                    binary,
                    idle,
                    snapshot::runtime_dir(),
                ),
                None => snapshot::SnapshotEngine::new(prefs.html, binary),
            }))
        }
    }
}

/// The page engine, made on first use from `documents.toml`.
pub fn engine(cx: &mut App) -> EngineAnswer {
    if let Some((answer, at)) = cx.try_global::<Engines>().and_then(|e| e.slot.as_ref()) {
        if answer.is_ok() || at.elapsed() < ASK_AGAIN {
            return answer.clone();
        }
    }
    let answer = make_engine();
    cx.set_global(Engines {
        slot: Some((answer.clone(), Instant::now())),
    });
    answer
}

/// Put `answer` where [`engine`] keeps the one it made, as though it had
/// made it: for a test, which must neither look for a browser on the machine
/// nor read the person's `documents.toml`. An `Err` stands for 30 seconds of
/// the machine's clock, as a real one does, far longer than any test.
#[cfg(test)]
pub(crate) fn set_engine(cx: &mut App, answer: EngineAnswer) {
    cx.set_global(Engines {
        slot: Some((answer, Instant::now())),
    });
}

/// For the router, before it places an HTML document: is there an engine to
/// draw it? A cached PATH lookup; no browser starts.
pub fn html_ready(cx: &mut App) -> Result<(), Unavailable> {
    engine(cx).map(|_| ())
}

/// TD is quitting: close the browser now and remove its profile, rather
/// than leaving it to the kernel's SIGKILL and the next launch's sweep.
pub fn shutdown(cx: &mut App) {
    if let Some((Ok(engine), _)) = cx.try_global::<Engines>().and_then(|e| e.slot.as_ref()) {
        engine.shutdown();
    }
}

/// Where an href in a document points.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LinkTarget {
    /// A heading in the same document.
    Fragment(String),
    /// A file on this machine, and the heading it names, if any.
    File {
        path: PathBuf,
        fragment: Option<String>,
    },
    /// Anything with a scheme that is not `file:`, as written.
    Url(String),
}

/// An href resolved against the document's folder: percent-decoded,
/// `file://` stripped, the `#fragment` kept apart, and `.` and `..` folded so
/// a link back to the same file is recognised as the same file.
pub fn resolve_link(doc_dir: &Path, href: &str) -> LinkTarget {
    let href = href.trim();
    if let Some(fragment) = href.strip_prefix('#') {
        return LinkTarget::Fragment(percent_decode(fragment));
    }
    let (before, fragment) = match href.split_once('#') {
        Some((before, fragment)) => (before, Some(percent_decode(fragment))),
        None => (href, None),
    };
    // `get`, not an index: a link that opens with a multi-byte character
    // would otherwise split it and panic.
    let file_scheme = before
        .get(..5)
        .is_some_and(|s| s.eq_ignore_ascii_case("file:"));
    if let Some(rest) = before.get(5..).filter(|_| file_scheme) {
        // file://<authority>/path: empty or `localhost` is this machine; any
        // other authority is somebody else's disk, and not a file here.
        let path = match rest.strip_prefix("//") {
            Some(r) => r.strip_prefix("localhost").unwrap_or(r),
            None => rest,
        };
        if !path.starts_with('/') {
            return LinkTarget::Url(href.to_string());
        }
        return LinkTarget::File {
            path: fold(Path::new(&percent_decode(path))),
            fragment,
        };
    }
    if has_scheme(before) {
        return LinkTarget::Url(href.to_string());
    }
    let path = before.split('?').next().unwrap_or(before);
    LinkTarget::File {
        path: fold(&doc_dir.join(percent_decode(path))),
        fragment,
    }
}

/// `scheme:` at the start, as RFC 3986 spells one. Two letters at least, so a
/// Windows drive is not mistaken for one.
fn has_scheme(s: &str) -> bool {
    let Some((scheme, _)) = s.split_once(':') else {
        return false;
    };
    scheme.len() >= 2
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// `%XX` decoded. A malformed escape is left as written: a literal `%` in a
/// file name is likelier than a truncated escape.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if let Some(v) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
            {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `.` and `..` folded away without touching the disk.
fn fold(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// A file as the watcher sees it: when it last changed, and how long it is.
/// The length catches two writes inside one tick of a coarse clock.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FileStamp {
    pub mtime: SystemTime,
    pub len: u64,
}

impl FileStamp {
    pub fn of(path: &Path) -> Option<FileStamp> {
        let meta = std::fs::metadata(path).ok()?;
        Some(FileStamp {
            mtime: meta.modified().ok()?,
            len: meta.len(),
        })
    }
}

/// What one watcher tick found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WatchSays {
    /// As last read.
    Unchanged,
    /// Changed, and exactly as this view wrote it: nothing to reload.
    OwnWrite,
    /// Changed by somebody else, or back after going missing.
    Changed,
    /// Missing. The last render stays up; an editor saving by rename passes
    /// through this for an instant, and a document that vanished is still
    /// worth reading.
    Gone,
}

/// Pure: a watcher tick, from what was read, what this view wrote, and what
/// the disk says now.
pub fn watch_says(
    seen: Option<FileStamp>,
    own_write: Option<FileStamp>,
    now: Option<FileStamp>,
) -> WatchSays {
    match now {
        None if seen.is_some() => WatchSays::Gone,
        None => WatchSays::Unchanged,
        Some(now) if Some(now) == seen => WatchSays::Unchanged,
        Some(now) if Some(now) == own_write => WatchSays::OwnWrite,
        Some(_) => WatchSays::Changed,
    }
}

/// Whether two themes paint a document the same way.
fn paints_alike(a: &Theme, b: &Theme) -> bool {
    markdown::MdPalette::from_theme(a) == markdown::MdPalette::from_theme(b)
        && a.font_size == b.font_size
        && a.font_family == b.font_family
}

impl DocumentView {
    pub fn new(target: DocTarget, cx: &mut Context<Self>) -> Self {
        let mut backend: Box<dyn Backend> = match target.kind {
            DocKind::Image => Box::new(image::ImageDoc::load(&target.path, cx)),
            DocKind::Markdown => Box::new(markdown::MarkdownDoc::new()),
            DocKind::Html => Box::new(page::PageDoc::new(&target.path, engine(cx))),
        };
        cx.on_release(|view, cx| view.give_back(cx)).detach();
        // Markdown is re-read on a change, and starts its first read here; a
        // brief is re-read too, stamped now, and a save of its notes is told
        // apart from anyone else's by `own_write`.
        let seen = backend.opened(&target.path, cx);
        let watch = backend.follows_its_file();
        let mut view = Self {
            target,
            backend,
            theme: None,
            seat: DocSeat::Float,
            frame: Rc::new(Cell::new(None)),
            placed: Rc::new(Cell::new(None)),
            painted_at: Rc::new(Cell::new(None)),
            links: markdown::LinkSink::default(),
            seen,
            own_write: None,
            zoom_notches: 0.0,
            _watch: Task::ready(()),
        };
        if watch {
            view._watch = view.watch(cx);
        }
        view
    }

    /// Move this view between the floating square and a pane's Document face.
    ///
    /// The document, its zoom and its place all stay: promoting a square to a
    /// split hands this same view to the new pane rather than opening the file
    /// again, so nothing is decoded or laid out twice. Only the box it is drawn
    /// in changes, which is why the size it measured in the old seat is
    /// forgotten until it paints in the new one — a zoom pressed in between
    /// would otherwise be placed against the square's size.
    pub fn set_seat(&mut self, seat: DocSeat, cx: &mut Context<Self>) {
        if self.seat == seat {
            return;
        }
        self.seat = seat;
        self.frame.set(None);
        cx.notify();
    }

    pub fn target(&self) -> &DocTarget {
        &self.target
    }

    /// Where the document is scrolled, for the saved layout to keep. `None`
    /// for an image and for a page not yet laid out.
    pub fn scroll(&self) -> Option<DocScroll> {
        self.backend.scroll()
    }

    /// Go back to a place the saved layout kept, as soon as the page has been
    /// laid out: the file is still being read when a restore asks. A picture
    /// has no scroll to go back to.
    pub fn restore_scroll(&mut self, at: DocScroll, cx: &mut Context<Self>) {
        self.backend.restore_scroll(at, cx);
    }

    /// Paint in this palette from now on: the pane's own, which can differ
    /// from the window's. Only a change that alters the page repaints.
    pub fn set_theme(&mut self, theme: Arc<Theme>, cx: &mut Context<Self>) {
        if self
            .theme
            .as_deref()
            .is_some_and(|now| paints_alike(now, &theme))
        {
            return;
        }
        self.theme = Some(theme);
        cx.notify();
    }

    fn theme(&self, cx: &App) -> Arc<Theme> {
        self.theme
            .clone()
            .unwrap_or_else(|| crate::theme::theme(cx))
    }

    fn view_h(&self) -> Option<f32> {
        self.frame.get().map(|(size, _)| f32::from(size.height))
    }

    /// The backend, and the view as the last paint left it, borrowed apart so
    /// the one can be handed the other.
    fn backend_and_view(&mut self) -> (&mut dyn Backend, Drawn<'_>) {
        let view = Drawn {
            path: &self.target.path,
            frame: self.frame.get(),
            placed: self.placed.get(),
            painted_at: self.painted_at.get(),
            links: &self.links,
        };
        (&mut *self.backend, view)
    }

    // ── input, already un-bent by the pane ──────────────────────────────────
    //
    // Every point below is flat and relative to the view's own top-left. The
    // pane found it through the tube's inverse; nothing here asks gpui where
    // the pointer is, because gpui would answer for the flat layout and the
    // picture is bent.

    /// A press on the document. Answers whether the view took it: an image
    /// takes it as the start of a pan; a Markdown document takes a press on a
    /// link, scrolling to a heading in this document itself and emitting
    /// [`FollowLink`] for the pane to route anything else.
    pub fn press(
        &mut self,
        at: Point<Pixels>,
        _mods: Modifiers,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let (backend, view) = self.backend_and_view();
        backend.press(at, &view, cx)
    }

    /// The pointer moved with the left button still held since [`Self::press`].
    pub fn drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((view, sf)) = self.frame.get() else {
            return;
        };
        if self.backend.drag(at, view, sf) {
            cx.notify();
        }
    }

    /// The held press ended, wherever the pointer is now.
    pub fn release(&mut self, _cx: &mut Context<Self>) {
        self.backend.end_press();
    }

    /// A wheel turn over the document: it pans a picture larger than the
    /// view, and scrolls a Markdown document or a page. Ctrl+wheel never
    /// arrives here; it is [`Self::zoom_by_wheel`].
    pub fn wheel(&mut self, delta: ScrollDelta, cx: &mut Context<Self>) {
        let Some((view, sf)) = self.frame.get() else {
            return;
        };
        let line = self.theme(cx).font_size * 1.6;
        if self.backend.wheel(delta, line, view, sf, cx) {
            cx.notify();
        }
    }

    /// The pointer moved over the document, or left it (`None`): flat and
    /// view-local, un-bent by the pane like a press. Only a brief listens,
    /// because its note buttons show under the pointer as a browser shows
    /// them.
    pub fn hover(&mut self, at: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        self.backend.hover(at, cx);
    }

    /// Who the notes bar's ↪ sends to, as the pane works it out every frame:
    /// "agent", a pane's name, or `None` for no button. Repaints only when
    /// that changes what the bar draws.
    pub fn set_beside(&mut self, beside: Option<String>, cx: &mut Context<Self>) {
        if self.backend.set_beside(beside) {
            cx.notify();
        }
    }

    /// What came of a ↪, said in the notes bar.
    pub fn notes_said(&mut self, said: notes_ui::Said, cx: &mut Context<Self>) {
        self.backend.notes_said(said, cx);
    }

    /// What a brief's notes layer shows, for the control socket: its state,
    /// its counts, why it is read-only when it is, and the map it would copy.
    /// `None` for anything but a brief, and for a brief not yet laid out.
    pub fn notes_report(&self) -> Option<serde_json::Value> {
        self.backend.notes_report()
    }

    /// A key the pane's layer ladder handed to the view. Escape answers true
    /// while a brief's note box or one of its own dialogs is open, and closes
    /// it; otherwise false, so the pane's Escape closes the square.
    /// Asked before anything closes or replaces this document on purpose:
    /// answers whether it kept itself open, once, because closing would lose
    /// notes not yet saved into the file. Only a brief carries notes.
    pub fn guard_close(&mut self, cx: &mut Context<Self>) -> bool {
        self.backend.guard_close(cx)
    }

    pub fn key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        self.backend.key(ks, self.seat == DocSeat::Float, cx)
    }

    /// Which seat the view is in, and the palette it was last handed, for a
    /// test driving a pane: both are the pane's decisions, and neither shows
    /// in anything else a test can read.
    #[cfg(test)]
    pub(crate) fn seat_and_theme(&self) -> (DocSeat, Option<Arc<Theme>>) {
        (self.seat, self.theme.clone())
    }

    /// Whether a note is being written in a brief's note box: while it is,
    /// the pane hands the view every key that is not a chord.
    pub fn has_caret(&self) -> bool {
        self.backend.has_caret()
    }

    /// A brief's notes, driven from the control socket. Answers with what
    /// the layer shows afterwards, or the sentence that refused it.
    pub fn notes_command(
        &mut self,
        cmd: NotesCommand,
        cx: &mut Context<Self>,
    ) -> Result<serde_json::Value, String> {
        self.backend.notes_command(cmd, cx)
    }

    /// The last paint measured a new size or scale.
    fn measured(&mut self, cx: &mut Context<Self>) {
        if let Some((size, scale)) = self.frame.get() {
            self.backend.measured(
                page::Measured {
                    origin: self.placed.get(),
                    size,
                    scale,
                },
                cx,
            );
        }
        cx.notify();
    }

    /// One press of a zoom control. Answers whether anything changed.
    pub fn zoom(&mut self, step: ZoomStep, cx: &mut Context<Self>) -> bool {
        let Some((view, sf)) = self.frame.get() else {
            return false;
        };
        let changed = self.backend.zoom(step, view, sf, cx);
        if changed {
            cx.notify();
        }
        changed
    }

    /// Ctrl+wheel over the document: a zoom step a notch, up zooming in, as
    /// a browser does. Answers whether anything changed.
    ///
    /// Notches are counted rather than turned into steps one event at a time,
    /// because a touchpad sends a notch as many small pieces. A turn the other
    /// way drops whatever was counted, so reversing is felt at once.
    pub fn zoom_by_wheel(&mut self, notches: f32, cx: &mut Context<Self>) -> bool {
        let (counted, steps) = count_zoom_notches(self.zoom_notches, notches);
        self.zoom_notches = counted;
        let step = if steps > 0 {
            ZoomStep::In
        } else {
            ZoomStep::Out
        };
        // No ladder is longer than this, so a larger turn would step past
        // its end and change nothing more.
        let most = READING_ZOOM.len().max(image::ZOOM_STEPS.len());
        let mut changed = false;
        for _ in 0..(steps.unsigned_abs() as usize).min(most) {
            changed |= self.zoom(step, cx);
        }
        changed
    }

    /// The geometry the page on screen was laid out at, for a test: `None`
    /// for anything but a page, and for a page not yet laid out.
    #[cfg(test)]
    pub(crate) fn page_laid_out(&mut self) -> Option<engine::Geometry> {
        self.backend.downcast_mut::<page::PageDoc>()?.laid_out()
    }

    /// The zoom the document is at, for the strip's label. `None` for a
    /// document with no zoom to show, so the strip draws no zoom controls.
    pub fn zoom_now(&self) -> Option<ImageZoom> {
        self.backend.zoom_now()
    }

    /// Show what a fragment names as soon as the document has been laid out:
    /// a link that named a heading in another file, or an element of a
    /// brief, opens that file there.
    pub fn show_fragment(&mut self, fragment: String, cx: &mut Context<Self>) {
        let view_h = self.view_h();
        self.backend.show_fragment(fragment, view_h, cx);
    }

    /// Ask the disk every [`WATCH_EVERY`] whether the file changed. Owned by
    /// the view, so it stops when the view is dropped.
    fn watch(&self, cx: &mut Context<Self>) -> Task<()> {
        let path = self.target.path.clone();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(WATCH_EVERY).await;
            let stat = path.clone();
            let now = cx
                .background_executor()
                .spawn(async move { FileStamp::of(&stat) })
                .await;
            if this.update(cx, |view, cx| view.watched(now, cx)).is_err() {
                return;
            }
        })
    }

    fn watched(&mut self, now: Option<FileStamp>, cx: &mut Context<Self>) {
        let says = watch_says(self.seen, self.own_write, now);
        let was_gone = self.seen.is_none();
        match says {
            WatchSays::Unchanged => {}
            WatchSays::OwnWrite => self.seen = now,
            WatchSays::Gone => {
                self.seen = None;
                self.backend.file_gone(cx);
            }
            WatchSays::Changed => {
                // Seen now, so the next tick does not start a second read
                // while this one is still running.
                self.seen = now;
                self.backend.file_changed(&self.target.path, was_gone, cx);
            }
        }
    }

    /// Give back everything this view holds on the GPU. Runs from the release
    /// hook registered in [`Self::new`], once, as the view is dropped.
    fn give_back(&mut self, cx: &mut App) {
        self.backend.give_back(&self.target.path, cx);
        self._watch = Task::ready(());
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let th = self.theme(cx);
        // Nothing is known to be laid out until this render's last element
        // says so; the links are rebuilt below, as their text is.
        self.painted_at.set(None);
        self.links.borrow_mut().clear();
        let (backend, view) = self.backend_and_view();
        let body = backend.element(&view, window, &th);
        // Measured, not listened to: a canvas records the box this view was
        // given and the scale it paints at, and asks for one more frame when
        // either changed, so a zoom placed against a stale size corrects
        // itself at once. A page hears of the change too: its size is the
        // width the brief is laid out at.
        let store = self.frame.clone();
        let placed = self.placed.clone();
        let weak = cx.entity().downgrade();
        let measure = canvas(
            move |bounds, window, cx| {
                placed.set(Some(bounds.origin));
                let now = Some((bounds.size, window.scale_factor()));
                if store.get() != now {
                    store.set(now);
                    let weak = weak.clone();
                    cx.defer(move |cx| {
                        let _ = weak.update(cx, |view, cx| view.measured(cx));
                    });
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        // Last, so gpui has prepainted everything above it, every link's text
        // included, by the time it records where the view was drawn.
        let painted_at = self.painted_at.clone();
        let drawn = canvas(
            move |bounds, _, _| painted_at.set(Some(bounds.origin)),
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .child(measure)
            .child(body)
            .child(drawn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A page or a Markdown document steps the browser's ladder, stops at
    /// either end, and goes back to 100% on the strip's label or the face's
    /// `0` and `1`. A zoom between two steps goes to the next one, not past it.
    #[test]
    fn a_reading_zoom_steps_its_ladder_and_stops_at_the_ends() {
        use ZoomStep::*;
        assert_eq!(step_reading_zoom(1.0, In), 1.1);
        assert_eq!(step_reading_zoom(1.0, Out), 0.9);
        assert_eq!(step_reading_zoom(3.0, In), 3.0, "the top");
        assert_eq!(step_reading_zoom(0.5, Out), 0.5, "the bottom");
        assert_eq!(step_reading_zoom(1.05, In), 1.1);
        assert_eq!(step_reading_zoom(1.05, Out), 1.0);
        for back in [FitOrActual, Fit, Actual] {
            assert_eq!(step_reading_zoom(2.5, back), 1.0, "{back:?}");
        }
    }

    /// A touchpad sends one notch as several small pieces, and each zoom step
    /// lays a page out again, so the pieces are counted until they make a
    /// step. Turning back starts the count again rather than paying off what
    /// was counted the other way first.
    #[test]
    fn wheel_notches_are_counted_into_whole_zoom_steps() {
        let mut counted = 0.0;
        let mut steps = Vec::new();
        for _ in 0..5 {
            let (c, s) = count_zoom_notches(counted, 0.25);
            counted = c;
            steps.push(s);
        }
        assert_eq!(steps, vec![0, 0, 0, 1, 0], "four quarters make one step in");
        assert!((counted - 0.25).abs() < 1e-6, "and the fifth stays counted");
        assert_eq!(
            count_zoom_notches(counted, -1.0),
            (0.0, -1),
            "a notch back is felt at once"
        );
        assert_eq!(
            count_zoom_notches(0.0, 3.0),
            (0.0, 3),
            "a wheel's lines are notches"
        );
    }

    /// This module and everything under it, with each file's tests cut off and
    /// its comments dropped, so a sentence that mentions a handler does not
    /// count as one. Cut at the test MODULE, not at the first `#[cfg(test)]`:
    /// a test-only helper halfway down a file would otherwise hide the rest
    /// of the file from every scan below.
    fn view_sources() -> Vec<(&'static str, String)> {
        let strip = |src: &str| -> String {
            let live = src.split("#[cfg(test)]\nmod tests").next().unwrap_or(src);
            live.lines()
                .filter(|l| {
                    let t = l.trim_start();
                    !t.starts_with("//")
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        vec![
            ("docview.rs", strip(include_str!("docview.rs"))),
            (
                "docview/backend.rs",
                strip(include_str!("docview/backend.rs")),
            ),
            ("docview/image.rs", strip(include_str!("docview/image.rs"))),
            (
                "docview/markdown.rs",
                strip(include_str!("docview/markdown.rs")),
            ),
            (
                "docview/markdown_view.rs",
                strip(include_str!("docview/markdown_view.rs")),
            ),
            ("docview/page.rs", strip(include_str!("docview/page.rs"))),
            (
                "docview/engine.rs",
                strip(include_str!("docview/engine.rs")),
            ),
            (
                "docview/snapshot.rs",
                strip(include_str!("docview/snapshot.rs")),
            ),
            ("docview/cdp.rs", strip(include_str!("docview/cdp.rs"))),
            ("docview/cache.rs", strip(include_str!("docview/cache.rs"))),
            ("docview/pref.rs", strip(include_str!("docview/pref.rs"))),
            ("docview/notes.rs", strip(include_str!("docview/notes.rs"))),
            (
                "docview/md_notes.rs",
                strip(include_str!("docview/md_notes.rs")),
            ),
            (
                "docview/notes_ui.rs",
                strip(include_str!("docview/notes_ui.rs")),
            ),
        ]
    }

    /// One file of [`view_sources`], by name.
    fn source_of(name: &str) -> String {
        view_sources()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| s)
            .unwrap_or_else(|| panic!("{name} is scanned"))
    }

    /// The body of `fn <name>(` in the `impl Backend for <ty>` block of `src`:
    /// what one backend does for one call, as it wrote it.
    fn backend_fn(src: &str, ty: &str, name: &str) -> String {
        let imp = src
            .split(&format!("impl Backend for {ty} {{"))
            .nth(1)
            .unwrap_or_else(|| panic!("impl Backend for {ty}"));
        let imp = imp.split("\n}\n").next().unwrap_or(imp);
        let f = imp
            .split(&format!("fn {name}("))
            .nth(1)
            .unwrap_or_else(|| panic!("{ty} has its own {name}"));
        f.split("\n    }\n").next().unwrap_or(f).to_string()
    }

    /// The CRT pass bends the pixels after layout and gpui hit-tests the flat
    /// layout, so a handler in the document view would fire beside what it
    /// draws on any curved pane — and pass every test on a flat one. The pane
    /// un-bends the pointer and decides; nothing in here may listen.
    #[test]
    fn nothing_in_the_document_view_listens_for_the_mouse() {
        for (name, src) in view_sources() {
            for listener in [
                ".on_mouse_down(",
                ".on_mouse_up(",
                ".on_mouse_move(",
                ".on_click(",
                ".on_scroll_wheel(",
                ".on_hover(",
                ".on_drag(",
                ".on_mouse_down_out(",
                ".on_mouse_up_out(",
                "overflow_y_scroll",
                "overflow_x_scroll",
                "overflow_scroll",
                "InteractiveText",
            ] {
                assert!(
                    !src.contains(listener),
                    "{name} registers {listener}: input reaches the document view through the pane"
                );
            }
        }
    }

    /// Only one function in the document view writes a brief: the notes
    /// writer's commit, which backs the file up, writes a temporary file
    /// beside it and renames it into place. Nothing else — not the notes
    /// layer, not the page, not the view — opens a file for writing, renames,
    /// copies or removes one. The page cache writes, in its own directory,
    /// from `cache.rs`, and the engine its browser's throwaway profile, from
    /// `snapshot.rs`; the brief is never among what either writes.
    #[test]
    fn only_the_commit_writes_a_brief() {
        let writes = [
            "fs::write",
            "File::create",
            "OpenOptions",
            "fs::rename",
            "fs::copy",
            "remove_file",
            "set_permissions",
            "set_len",
            "DirBuilder",
        ];
        let sources = view_sources();
        let find = |name: &str| {
            sources
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(_, s)| s.clone())
                .unwrap_or_else(|| panic!("{name} is scanned"))
        };
        // snapshot.rs makes and removes the browser's own profile, in the
        // runtime directory; that is its only writing, and no brief.
        for name in [
            "docview.rs",
            "docview/backend.rs",
            "docview/markdown_view.rs",
            "docview/notes_ui.rs",
            "docview/page.rs",
            "docview/engine.rs",
        ] {
            let src = find(name);
            for w in writes {
                assert!(
                    !src.contains(w),
                    "{name} holds {w}: only the notes writer's commit writes a brief"
                );
            }
        }
        let notes = find("docview/notes.rs");
        let (before, disk) = notes
            .split_once("pub fn commit(")
            .expect("the commit is in notes.rs");
        for w in writes {
            assert!(
                !before.contains(w),
                "notes.rs writes before its commit: {w}"
            );
        }
        assert!(
            disk.contains("fs::rename(") && disk.contains("create_new(true)"),
            "the commit writes a new file and renames it into place"
        );
    }

    /// A Markdown document's notes go to TD's store and nowhere else. The
    /// backend that shows the file writes nothing (above), and the only
    /// writing in `md_notes.rs` is its `write`, into the store, a new file
    /// renamed into place. That it never touches the document is held by
    /// `keeping_a_note_writes_the_store_and_never_the_document`.
    #[test]
    fn only_the_store_writes_a_markdown_documents_notes() {
        let src = source_of("docview/md_notes.rs");
        let (before, rest) = src.split_once("pub fn write(").expect("the store's write");
        let (write, after) = rest.split_once("\n}\n").expect("its end");
        for w in [
            "fs::write",
            "File::create",
            "OpenOptions",
            "fs::rename",
            "fs::copy",
            "remove_file",
            "create_dir",
            "set_permissions",
        ] {
            assert!(
                !before.contains(w) && !after.contains(w),
                "md_notes.rs holds {w} outside its write"
            );
        }
        assert!(
            write.contains("create_new(true)") && write.contains("fs::rename("),
            "the store's write makes a new file and renames it into place"
        );
    }

    /// A link from another document hands its fragment to whichever kind of
    /// document can land on one. A brief was once left out, and a link into
    /// its middle opened it at the top (issue 734). The trait's default lands
    /// nowhere, so each backend that can land on one has to say so itself.
    #[test]
    fn a_fragment_reaches_every_document_that_can_land_on_one() {
        let (_, src) = &view_sources()[0];
        let show = src
            .split("pub fn show_fragment(")
            .nth(1)
            .expect("DocumentView::show_fragment");
        let show = show.split("\n    }\n").next().unwrap_or(show);
        assert!(show.contains("self.backend.show_fragment("), "{show}");
        let md = backend_fn(
            &source_of("docview/markdown_view.rs"),
            "MarkdownDoc",
            "show_fragment",
        );
        assert!(md.contains("self.go_to_fragment("), "{md}");
        let page = backend_fn(&source_of("docview/page.rs"), "PageDoc", "show_fragment");
        assert!(page.contains("PageDoc::show_fragment(self,"), "{page}");
    }

    /// However a square ends, dropping its view gives the texture back: the
    /// hook is registered where the view is made, not where one caller closes it.
    #[test]
    fn a_document_view_releases_itself_when_it_is_dropped() {
        let (_, src) = &view_sources()[0];
        let new = src.split("pub fn new(").nth(1).expect("DocumentView::new");
        let new = new.split("\n    }\n").next().unwrap_or(new);
        assert!(
            new.contains("cx.on_release(") && new.contains(".give_back(cx)"),
            "DocumentView::new must register its own release"
        );
    }

    /// A page's tiles are all dropped from the atlas when its view goes, and
    /// an eviction while scrolling drops through the deferred path, never
    /// directly from inside a window's own event.
    #[test]
    fn a_page_gives_every_texture_back() {
        let (_, src) = &view_sources()[0];
        let release = src.split("fn give_back(").nth(1).expect("give_back");
        let release = release.split("\n    }\n").next().unwrap_or(release);
        assert!(
            release.contains("self.backend.give_back("),
            "the view hands its release to the backend"
        );
        let src = source_of("docview/page.rs");
        let release = backend_fn(&src, "PageDoc", "give_back");
        assert!(
            release.contains("self.release(cx)"),
            "a page's tiles are given back when its view goes"
        );
        let release = src
            .split("pub fn release(&mut self, cx: &mut App)")
            .nth(1)
            .expect("PageDoc::release");
        let release = release.split("\n    }\n").next().unwrap_or(release);
        for slot in [
            "self.current.take()",
            "self.old.take()",
            "self.dialog.take()",
            "drop_image(",
        ] {
            assert!(release.contains(slot), "release must give back {slot}");
        }
        let plan = src.split("fn plan(&mut self").nth(1).expect("plan");
        let plan = plan.split("\n    }\n").next().unwrap_or(plan);
        assert!(
            plan.contains("give_back("),
            "eviction hands textures to give_back"
        );
        assert!(
            !plan.contains("drop_image("),
            "never a direct drop mid-event"
        );
        let give = src.split("pub fn give_back(").nth(1).expect("give_back");
        let give = give.split("\n}\n").next().unwrap_or(give);
        assert!(give.contains("cx.defer(") && give.contains("drop_image("));
    }

    #[test]
    fn a_missing_engine_is_asked_about_again_later_and_a_found_one_is_kept() {
        // The rule, as code reads it: kept when found, re-asked after 30 s
        // when not, so installing Chromium needs no restart.
        let src = include_str!("docview.rs");
        let f = src
            .split("pub fn engine(cx: &mut App)")
            .nth(1)
            .expect("engine");
        let f = f.split("\n}\n").next().unwrap_or(f);
        assert!(f.contains("answer.is_ok() || at.elapsed() < ASK_AGAIN"));
        assert_eq!(ASK_AGAIN, Duration::from_secs(30));
    }

    /// A Markdown document that holds pictures gives every one of them back:
    /// at release, and the moment a re-read stops drawing one. And each was
    /// taken out of gpui's cache as it was asked for, or dropping the view's
    /// reference would free nothing.
    #[test]
    fn a_markdown_documents_pictures_are_given_back() {
        let src = source_of("docview/markdown_view.rs");
        let release = backend_fn(&src, "MarkdownDoc", "give_back");
        assert!(release.contains("drop_image("), "{release}");

        let read = src
            .split("fn markdown_read(")
            .nth(1)
            .expect("markdown_read");
        let read = read.split("\n}\n").next().unwrap_or(read);
        assert!(
            read.contains("md.replace(") && read.contains("drop_image("),
            "a re-read gives back what it stops drawing: {read}"
        );

        let decode = src
            .split("fn decode_images(")
            .nth(1)
            .expect("decode_images");
        let decode = decode.split("\n}\n").next().unwrap_or(decode);
        let fetch = decode
            .find("fetch_asset::<ImgResourceLoader>")
            .expect("fetch");
        let remove = decode
            .find("remove_asset::<ImgResourceLoader>")
            .expect("taken out of gpui's cache");
        assert!(fetch < remove);
    }

    #[test]
    fn a_link_is_resolved_against_the_documents_folder() {
        let dir = Path::new("/a/b");
        assert_eq!(
            resolve_link(dir, "c%20d.md#x"),
            LinkTarget::File {
                path: "/a/b/c d.md".into(),
                fragment: Some("x".into())
            }
        );
        assert_eq!(resolve_link(dir, "#y"), LinkTarget::Fragment("y".into()));
        assert_eq!(
            resolve_link(dir, "https://example.com/a#b"),
            LinkTarget::Url("https://example.com/a#b".into())
        );
        assert_eq!(
            resolve_link(dir, "file:///e"),
            LinkTarget::File {
                path: "/e".into(),
                fragment: None
            }
        );
        assert_eq!(
            resolve_link(dir, "file://localhost/e%20f.png"),
            LinkTarget::File {
                path: "/e f.png".into(),
                fragment: None
            }
        );
        // Another machine's disk is not a file here.
        assert!(matches!(
            resolve_link(dir, "file://nas/e"),
            LinkTarget::Url(_)
        ));
        // Up a folder, and back to the same file, folded.
        assert_eq!(
            resolve_link(dir, "../x/./y.md"),
            LinkTarget::File {
                path: "/a/x/y.md".into(),
                fragment: None
            }
        );
        assert!(matches!(
            resolve_link(dir, "mailto:p@example.com"),
            LinkTarget::Url(_)
        ));
        // A name that opens with a multi-byte character is a name.
        assert_eq!(
            resolve_link(dir, "ééé.md"),
            LinkTarget::File {
                path: "/a/b/ééé.md".into(),
                fragment: None
            }
        );
        assert_eq!(percent_decode("100%"), "100%", "a stray % is kept");
    }

    /// Nothing measured is not the top of the page, and an image has no
    /// scroll at all: either answer written down as a number would be a
    /// position nobody was ever at.
    #[test]
    fn an_image_has_no_scroll_to_save() {
        let image: Box<dyn Backend> = Box::new(image::ImageDoc::unloaded());
        assert_eq!(image.scroll(), None);
        let md: Box<dyn Backend> = Box::new(markdown::MarkdownDoc::new());
        assert_eq!(md.scroll(), None, "a page not yet laid out");
    }

    /// Saving notes into a file (a later slice) changes its stamp. A watcher
    /// that took that for somebody else's edit would re-read the file under
    /// the person saving it, every time.
    #[test]
    fn the_documents_own_save_does_not_look_like_someone_elses() {
        let at = |s: u64, len: u64| FileStamp {
            mtime: SystemTime::UNIX_EPOCH + Duration::from_secs(s),
            len,
        };
        let read = at(10, 5);
        let saved = at(20, 7);
        assert_eq!(
            watch_says(Some(read), Some(saved), Some(saved)),
            WatchSays::OwnWrite
        );
        assert_eq!(
            watch_says(Some(read), None, Some(saved)),
            WatchSays::Changed
        );
        assert_eq!(
            watch_says(Some(read), Some(saved), Some(at(20, 9))),
            WatchSays::Changed,
            "the same second, another length: somebody else"
        );
        assert_eq!(
            watch_says(Some(read), None, Some(read)),
            WatchSays::Unchanged
        );
        assert_eq!(watch_says(Some(read), None, None), WatchSays::Gone);
        assert_eq!(watch_says(None, None, None), WatchSays::Unchanged);
        assert_eq!(
            watch_says(None, None, Some(read)),
            WatchSays::Changed,
            "a missing file that came back"
        );
    }

    /// The watcher is the view's own task: an open document is polled, and
    /// one that is closed stops being polled with nothing to remember.
    #[test]
    fn a_document_is_watched_only_while_it_is_open() {
        let (_, src) = &view_sources()[0];
        let new = src.split("pub fn new(").nth(1).expect("DocumentView::new");
        let new = new.split("\n    }\n").next().unwrap_or(new);
        assert!(new.contains("view._watch = view.watch(cx)"), "{new}");
        let release = src.split("fn give_back(").nth(1).expect("give_back");
        assert!(release.contains("self._watch = Task::ready(())"));
        assert!(
            !src.contains("_watch.detach()"),
            "a detached watcher outlives the view"
        );
    }
}
