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

pub mod cache;
pub mod cdp;
pub mod engine;
pub mod image;
pub mod markdown;
pub mod page;
pub mod pref;
pub mod snapshot;

use std::cell::Cell;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use gpui::{
    canvas, div, prelude::*, px, App, Context, EventEmitter, Global, ImgResourceLoader, Keystroke,
    Modifiers, Pixels, Point, RenderImage, Resource, ScrollDelta, Size, Task, Window,
};

use crate::docopen::{DocKind, DocScroll, DocSeat, DocTarget};
use crate::theme::Theme;
use engine::{PageEngine, Unavailable};

pub use image::{ImageZoom, ZoomStep};

/// How far one notch of a wheel that counts in lines moves a picture, in
/// logical pixels. Three lines of text at a common size, which is what a
/// notch scrolls in a browser.
const WHEEL_LINE_PX: f32 = 48.0;

/// How often an open document asks whether its file changed: the skin and
/// theme hot-reload's pattern, at the pace the program design set.
pub const WATCH_EVERY: Duration = Duration::from_millis(500);

/// A document on screen.
pub struct DocumentView {
    target: DocTarget,
    backend: Backend,
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
    /// The file as this view last wrote it. Nothing writes yet (notes come in
    /// a later slice); the watcher already tells its own save from anyone
    /// else's so that a save never reloads under the person making it.
    own_write: Option<FileStamp>,
    reading: Task<()>,
    _watch: Task<()>,
}

/// A view's measured size, in logical pixels, and the scale factor it was
/// measured under.
type Frame = (Size<Pixels>, f32);

enum Backend {
    Image(image::ImageDoc),
    Markdown(markdown::MarkdownDoc),
    /// Boxed: a page carries its tiles, dialog and tasks, ten times an image.
    Page(Box<page::PageDoc>),
}

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

/// The scroll a backend has to save. `None` for an image, which has none, and
/// for a page never laid out, where nothing has been measured.
fn scroll_of(backend: &Backend) -> Option<DocScroll> {
    match backend {
        Backend::Markdown(md) => md.scroll(),
        Backend::Page(page) => page.scroll(),
        Backend::Image(_) => None,
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

impl DocumentView {
    pub fn new(target: DocTarget, cx: &mut Context<Self>) -> Self {
        let backend = match target.kind {
            DocKind::Image => Backend::Image(image::ImageDoc::load(&target.path, cx)),
            DocKind::Markdown => Backend::Markdown(markdown::MarkdownDoc::new()),
            DocKind::Html => Backend::Page(Box::new(page::PageDoc::new(&target.path, engine(cx)))),
        };
        cx.on_release(|view, cx| view.give_back(cx)).detach();
        let watch = matches!(backend, Backend::Markdown(_));
        let mut view = Self {
            target,
            backend,
            theme: None,
            seat: DocSeat::Float,
            frame: Rc::new(Cell::new(None)),
            placed: Rc::new(Cell::new(None)),
            painted_at: Rc::new(Cell::new(None)),
            links: markdown::LinkSink::default(),
            seen: None,
            own_write: None,
            reading: Task::ready(()),
            _watch: Task::ready(()),
        };
        if watch {
            view.read_markdown(cx);
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
        scroll_of(&self.backend)
    }

    /// Go back to a place the saved layout kept, as soon as the page has been
    /// laid out: the file is still being read when a restore asks. A picture
    /// has no scroll to go back to.
    pub fn restore_scroll(&mut self, at: DocScroll, cx: &mut Context<Self>) {
        match &mut self.backend {
            Backend::Markdown(md) => {
                md.restore_fraction(at.top);
                cx.notify();
            }
            Backend::Page(page) => page.restore_scroll(at.top, cx),
            Backend::Image(_) => {}
        }
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
        match &mut self.backend {
            Backend::Image(img) => img.press(at),
            Backend::Markdown(_) => self.press_link(at, cx),
            Backend::Page(page) => match page.press(at, cx) {
                page::Pressed::Follow(link) => {
                    cx.emit(link);
                    true
                }
                page::Pressed::Took => true,
                page::Pressed::Nothing => false,
            },
        }
    }

    /// A press on a Markdown document, looked up among its links.
    fn press_link(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let (Some(origin), Some((size, _))) = (self.painted_at.get(), self.frame.get()) else {
            return false;
        };
        // Above the view is the square's strip; a link scrolled up under it
        // is not what was pressed.
        if at.x < px(0.) || at.y < px(0.) || at.x >= size.width || at.y >= size.height {
            return false;
        }
        let Backend::Markdown(md) = &mut self.backend else {
            return false;
        };
        let Some(href) = markdown::link_at(&self.links.borrow(), origin + at) else {
            return false;
        };
        let dir = self.target.path.parent().unwrap_or(Path::new("/"));
        let view_h = Some(f32::from(size.height));
        match resolve_link(dir, &href) {
            LinkTarget::Fragment(fragment) => md.go_to_fragment(fragment, view_h),
            LinkTarget::File { path, fragment } if path == self.target.path => {
                if let Some(fragment) = fragment {
                    md.go_to_fragment(fragment, view_h);
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

    /// The pointer moved with the left button still held since [`Self::press`].
    pub fn drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some((view, sf)) = self.frame.get() else {
            return;
        };
        if let Backend::Image(img) = &mut self.backend {
            if img.drag(at, view, sf) {
                cx.notify();
            }
        }
    }

    /// The held press ended, wherever the pointer is now.
    pub fn release(&mut self, _cx: &mut Context<Self>) {
        if let Backend::Image(img) = &mut self.backend {
            img.end_pan();
        }
    }

    /// A wheel turn over the document: it pans a picture larger than the
    /// view, and scrolls a Markdown document. Ctrl+wheel never arrives here;
    /// that is the pane's text dial.
    pub fn wheel(&mut self, delta: ScrollDelta, cx: &mut Context<Self>) {
        let Some((view, sf)) = self.frame.get() else {
            return;
        };
        let line = self.theme(cx).font_size * 1.6;
        let moved = match &mut self.backend {
            Backend::Image(img) => {
                let (dx, dy) = match delta {
                    ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
                    ScrollDelta::Lines(l) => (l.x * WHEEL_LINE_PX, l.y * WHEEL_LINE_PX),
                };
                img.pan_by(dx, dy, view, sf)
            }
            Backend::Markdown(md) => md.wheel(delta, line, Some(f32::from(view.height))),
            Backend::Page(page) => {
                // The page notifies for itself: a turn also moves tiles on
                // and off the GPU.
                page.wheel(delta, cx);
                false
            }
        };
        if moved {
            cx.notify();
        }
    }

    /// A key the pane's layer ladder handed to the view. Escape answers true
    /// while one of a brief's own dialogs is open, and closes it; otherwise
    /// false, so the pane's Escape closes the square.
    pub fn key(&mut self, ks: &Keystroke, cx: &mut Context<Self>) -> bool {
        match &mut self.backend {
            Backend::Page(page) if ks.key == "escape" => page.escape(cx),
            _ => false,
        }
    }

    /// The last paint measured a new size or scale.
    fn measured(&mut self, cx: &mut Context<Self>) {
        if let (Backend::Page(page), Some((size, scale))) = (&mut self.backend, self.frame.get()) {
            let origin = self.placed.get().unwrap_or_default();
            page.measured(
                page::Measured {
                    origin,
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
        let changed = match &mut self.backend {
            Backend::Image(img) => img.zoom(step, view, sf),
            Backend::Markdown(_) | Backend::Page(_) => false,
        };
        if changed {
            cx.notify();
        }
        changed
    }

    /// The zoom a picture is at, for the strip's label. `None` for a document
    /// with no zoom to show, so the strip draws no zoom controls for it.
    pub fn zoom_now(&self) -> Option<ImageZoom> {
        match &self.backend {
            Backend::Image(img) => Some(img.zoom_now()),
            Backend::Markdown(_) | Backend::Page(_) => None,
        }
    }

    /// Show a heading as soon as the document has been laid out: a link that
    /// named one in another file opens that file there.
    pub fn show_fragment(&mut self, fragment: String, cx: &mut Context<Self>) {
        let view_h = self.view_h();
        if let Backend::Markdown(md) = &mut self.backend {
            md.go_to_fragment(fragment, view_h);
            cx.notify();
        }
    }

    /// Read and parse the Markdown file off the main thread.
    fn read_markdown(&mut self, cx: &mut Context<Self>) {
        let path = self.target.path.clone();
        let read = cx.background_executor().spawn(async move {
            // Stamped BEFORE reading: a write landing in between leaves a
            // stamp older than the text, and the next tick reads again, which
            // is harmless. The other order could miss that write for good.
            let stamp = FileStamp::of(&path);
            let parsed = std::fs::read(&path)
                .map(|bytes| markdown::parse(&String::from_utf8_lossy(&bytes), path.parent()))
                .map_err(|e| format!("Could not read {}: {e}", path.display()));
            (stamp, parsed)
        });
        self.reading = cx.spawn(async move |this, cx| {
            let (stamp, parsed) = read.await;
            this.update(cx, |view, cx| view.markdown_read(stamp, parsed, cx))
                .ok();
        });
    }

    fn markdown_read(
        &mut self,
        stamp: Option<FileStamp>,
        parsed: Result<markdown::MdDoc, String>,
        cx: &mut Context<Self>,
    ) {
        self.seen = stamp;
        let Backend::Markdown(md) = &mut self.backend else {
            return;
        };
        match parsed {
            Ok(doc) => {
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
                    eprintln!("[doc] cannot read {}", self.target.path.display());
                }
                md.doc = Some(Err(why));
            }
        }
        self.decode_images(cx);
        cx.notify();
    }

    /// Start decoding every local picture the document draws and the view
    /// does not hold yet, owned the way the image backend owns its one.
    fn decode_images(&mut self, cx: &mut Context<Self>) {
        let Backend::Markdown(md) = &mut self.backend else {
            return;
        };
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
                this.update(cx, |view, cx| view.image_decoded(key, result, cx))
                    .ok();
            });
            md.decoding.insert(path, task);
        }
    }

    fn image_decoded(
        &mut self,
        path: PathBuf,
        result: Result<Arc<RenderImage>, String>,
        cx: &mut Context<Self>,
    ) {
        let Backend::Markdown(md) = &mut self.backend else {
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
        match watch_says(self.seen, self.own_write, now) {
            WatchSays::Unchanged => {}
            WatchSays::OwnWrite => self.seen = now,
            WatchSays::Gone => self.seen = None,
            WatchSays::Changed => {
                // Seen now, so the next tick does not start a second read
                // while this one is still running.
                self.seen = now;
                self.read_markdown(cx);
            }
        }
    }

    /// Give back everything this view holds on the GPU. Runs from the release
    /// hook registered in [`Self::new`], once, as the view is dropped.
    fn give_back(&mut self, cx: &mut App) {
        match &mut self.backend {
            Backend::Image(img) => img.release(&self.target.path, cx),
            Backend::Markdown(md) => {
                md.decoding.clear();
                for (_, image) in md.images.drain() {
                    if let Ok(image) = image {
                        cx.drop_image(image, None);
                    }
                }
                if std::env::var_os("TD_DOCDEBUG").is_some() {
                    eprintln!("[doc] released {}", self.target.path.display());
                }
            }
            Backend::Page(page) => {
                let n = page.release(cx);
                if std::env::var_os("TD_DOCDEBUG").is_some() {
                    eprintln!("[doc] released {} textures={n}", self.target.path.display());
                }
            }
        }
        self.reading = Task::ready(());
        self._watch = Task::ready(());
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let th = self.theme(cx);
        let frame = self.frame.get();
        // Nothing is known to be laid out until this render's last element
        // says so; the links are rebuilt below, as their text is.
        self.painted_at.set(None);
        self.links.borrow_mut().clear();
        let body = match &mut self.backend {
            Backend::Image(img) => img.element(&self.target.path, frame, window, &th),
            Backend::Markdown(md) => md.element(
                &self.target.path,
                &th,
                frame.map(|(size, _)| size),
                &self.links,
                window,
            ),
            Backend::Page(page) => page.element(&th, self.placed.get()),
        };
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

    /// This module and everything under it, with each file's tests cut off and
    /// its comments dropped, so a sentence that mentions a handler does not
    /// count as one.
    fn view_sources() -> Vec<(&'static str, String)> {
        let strip = |src: &str| -> String {
            let live = src.split("#[cfg(test)]").next().unwrap_or(src);
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
            ("docview/image.rs", strip(include_str!("docview/image.rs"))),
            (
                "docview/markdown.rs",
                strip(include_str!("docview/markdown.rs")),
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
        ]
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
            release.contains("page.release(cx)"),
            "a page's tiles are given back when its view goes"
        );
        let (_, src) = view_sources()
            .into_iter()
            .find(|(name, _)| *name == "docview/page.rs")
            .expect("page.rs is scanned");
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
        let (_, src) = &view_sources()[0];
        let release = src.split("fn give_back(").nth(1).expect("give_back");
        let release = release.split("\n    }\n").next().unwrap_or(release);
        let md_arm = release
            .split("Backend::Markdown(md) =>")
            .nth(1)
            .expect("release has a Markdown arm");
        let md_arm = md_arm.split("Backend::Page").next().unwrap_or(md_arm);
        assert!(md_arm.contains("drop_image("), "{md_arm}");

        let read = src
            .split("fn markdown_read(")
            .nth(1)
            .expect("markdown_read");
        let read = read.split("\n    }\n").next().unwrap_or(read);
        assert!(
            read.contains("md.replace(") && read.contains("drop_image("),
            "a re-read gives back what it stops drawing: {read}"
        );

        let decode = src
            .split("fn decode_images(")
            .nth(1)
            .expect("decode_images");
        let decode = decode.split("\n    }\n").next().unwrap_or(decode);
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
        let image = Backend::Image(image::ImageDoc::unloaded());
        assert_eq!(scroll_of(&image), None);
        let md = Backend::Markdown(markdown::MarkdownDoc::new());
        assert_eq!(scroll_of(&md), None, "a page not yet laid out");
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
