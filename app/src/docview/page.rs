//! The HTML backend's view side: a page drawn as tiles, scrolled by TD, with
//! the brief's own links and dialog buttons routed through the pane.
//!
//! # Tiles
//!
//! The engine renders the page at the view's own width and the window's scale
//! and hands back PNG bands 2,048 device rows tall. Every band's PNG is kept
//! on the CPU side (0.4–5.2 MB for a whole brief); at most five are on the GPU
//! at once — the visible ones and one either side — so scrolling moves
//! textures already uploaded, and a band coming back costs a ~20 ms decode
//! rather than a render. Bands are captured nearest the viewport first, so the
//! first pixels arrive after one capture rather than after the whole page.
//!
//! # Every texture is given back
//!
//! A tile is an `Arc<RenderImage>` that gpui never frees on its own. Each one
//! lives in exactly one slot here, and leaving that slot — scrolled out of
//! residency, replaced by a re-render, the view closed — hands it to
//! [`give_back`], which removes it from every window's atlas. Eviction happens
//! inside a window's own event handling, where gpui has taken that window out
//! of its list, so the drop is deferred to the end of the effect cycle, when
//! every window is back and one call reaches them all. A decode that lands
//! for a slot nobody wants any more was never painted, so it was never in an
//! atlas, and it is simply dropped.
//!
//! # Coordinates
//!
//! Scroll is held in page CSS px. A band is placed at whole device pixels:
//! its top is `(top_dev − scroll_dev) / scale`, and the view's own origin is
//! snapped to the device grid first, so on a flat pane two bands meet without
//! a blurred seam. [`place`] maps a page rect into the view the same way, for
//! the links and dialog buttons a press can land on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use gpui::{
    div, hsla, img, point, prelude::*, px, size, App, Bounds, Context, ImageSource, ObjectFit,
    Pixels, Point, RenderImage, ScrollDelta, Size, Task,
};

use super::cache::{self, CacheKey};
use super::engine::ConcurSupport;
use super::engine::{
    bands, layout_hash, Anchor, Band, DialogRender, EngineError, Geometry, Link, Opener,
    PageEngine, PageId, PageLayout, PageRequest, RectCss, Unavailable, TILE_DEV,
};
use super::notes::{self, NotesRead};
use super::notes_ui::{self, LayerPress, Mark, MarkHit, NotesLayer, Said};
use super::snapshot::EXTRACT_VERSION;
use super::{resolve_link, Backend, DocumentView, FollowLink, LinkTarget};
use crate::docopen::DocScroll;
use crate::theme::Theme;

/// Visible tiles and one either side: 61 MiB at 1,549 wide (spike §3).
pub const MAX_RESIDENT: usize = 5;
/// A resize waits this long to settle before the page is laid out again.
const SETTLE: Duration = Duration::from_millis(250);
/// One wheel line, in logical px.
const LINE: f32 = 54.0;

#[derive(Debug, Default, PartialEq)]
pub struct Residency {
    /// Bands to put on the GPU, nearest the viewport first.
    pub decode: Vec<u32>,
    /// Bands to take off it.
    pub drop: Vec<u32>,
    /// Bands with no PNG yet, nearest the viewport first.
    pub fetch: Vec<u32>,
}

/// How far a band starting at `top` lies from the viewport, and on which
/// side: 0 inside it. Ties go below first, the way a page is read.
fn distance(top: u32, view_top: u32, view_h: u32) -> (u32, bool) {
    let bottom = top.saturating_add(TILE_DEV);
    let view_bottom = view_top.saturating_add(view_h.max(1));
    if bottom <= view_top {
        (view_top - bottom + 1, true)
    } else if top >= view_bottom {
        (top - view_bottom + 1, false)
    } else {
        (0, false)
    }
}

/// Pure. Which bands belong on the GPU for a viewport, and what to fetch.
///
/// `all` is every band's top (in device rows); `resident` the tops on the GPU
/// or on their way there; `have_png` the tops whose PNG is in hand. The
/// visible bands are always wanted; one above and one below join them while
/// the total stays within [`MAX_RESIDENT`].
pub fn plan_residency(
    all: &[u32],
    resident: &[u32],
    have_png: &[u32],
    view_top_dev: u32,
    view_h_dev: u32,
) -> Residency {
    let mut order: Vec<u32> = all.to_vec();
    order.sort_by_key(|&t| {
        let (d, above) = distance(t, view_top_dev, view_h_dev);
        (d, above, t)
    });
    let visible: Vec<u32> = order
        .iter()
        .copied()
        .filter(|&t| distance(t, view_top_dev, view_h_dev).0 == 0)
        .collect();
    let mut wanted = visible.clone();
    let first = visible.iter().min().copied();
    let last = visible.iter().max().copied();
    let neighbours = all.iter().copied().filter(|&t| {
        let above = first.is_some_and(|f| t < f && f - t <= TILE_DEV);
        let below = last.is_some_and(|l| t > l && t - l <= TILE_DEV);
        above || below
    });
    let mut neighbours: Vec<u32> = neighbours.collect();
    neighbours.sort_by_key(|&t| distance(t, view_top_dev, view_h_dev));
    for t in neighbours {
        if wanted.len() >= MAX_RESIDENT.max(visible.len()) {
            break;
        }
        wanted.push(t);
    }
    let decode = order
        .iter()
        .copied()
        .filter(|t| wanted.contains(t) && !resident.contains(t) && have_png.contains(t))
        .collect();
    let drop = resident
        .iter()
        .copied()
        .filter(|t| !wanted.contains(t))
        .collect();
    let fetch = order
        .iter()
        .copied()
        .filter(|t| !have_png.contains(t))
        .collect();
    Residency {
        decode,
        drop,
        fetch,
    }
}

/// Page CSS coordinates to the view's own flat coordinates.
#[derive(Clone, Copy, Debug)]
pub struct PageToView {
    /// Where the page's top-left would be at scroll 0, in view coordinates.
    pub origin: Point<Pixels>,
    /// Logical px per CSS px: the zoom, once the page is laid out at the
    /// view's width over the zoom; while a resize settles, the stretch.
    pub px_per_css: f32,
    /// How far the page is scrolled, in logical px.
    pub scroll: Pixels,
    /// The part of the view the page shows through.
    pub clip: Bounds<Pixels>,
}

/// Pure. Where a page rect lands in the view, or `None` when it is scrolled
/// or clipped out of sight.
pub fn place(rect: RectCss, map: &PageToView) -> Option<Bounds<Pixels>> {
    let b = Bounds {
        origin: point(
            map.origin.x + px(rect.x * map.px_per_css),
            map.origin.y + px(rect.y * map.px_per_css) - map.scroll,
        ),
        size: size(px(rect.w * map.px_per_css), px(rect.h * map.px_per_css)),
    };
    let c = map.clip;
    let inside = b.origin.x < c.origin.x + c.size.width
        && b.origin.x + b.size.width > c.origin.x
        && b.origin.y < c.origin.y + c.size.height
        && b.origin.y + b.size.height > c.origin.y;
    inside.then_some(b)
}

/// What a press on a page, or on one of its dialogs, landed on.
#[derive(Clone, Debug, PartialEq)]
pub enum PageHit {
    /// A dialog's own close button.
    Closer,
    /// A `[data-dlg]` button, and the dialog it names.
    Opener(String),
    Link(Link),
}

/// Pure. The thing under a view-local point, through the same mapping the
/// page was drawn with: a close button first, then a dialog button, then a
/// link. Something with no rect (inside a closed dialog) is never under it.
pub fn hit_at(
    openers: &[Opener],
    links: &[Link],
    closers: &[RectCss],
    map: &PageToView,
    at: Point<Pixels>,
) -> Option<PageHit> {
    let under = |rect: Option<RectCss>| {
        rect.and_then(|rc| place(rc, map))
            .is_some_and(|b| b.contains(&at))
    };
    if closers.iter().any(|c| under(Some(*c))) {
        return Some(PageHit::Closer);
    }
    if let Some(o) = openers.iter().find(|o| under(o.rect)) {
        return Some(PageHit::Opener(o.dialog.clone()));
    }
    links
        .iter()
        .find(|l| under(l.rect))
        .cloned()
        .map(PageHit::Link)
}

/// Pure. How far to move the view's content so its origin sits on a whole
/// device pixel.
pub fn snap_offset(origin: f32, scale: f32) -> f32 {
    let (o, s) = (f64::from(origin), f64::from(scale.max(0.01)));
    ((o * s).round() / s - o) as f32
}

/// Pure. A band's top in the view, in logical px, before the snap: its device
/// row less the scroll, over the layout's scale, times the stretch.
pub fn tile_top(top_dev: u32, scroll_dev: u32, layout_scale: f32, stretch: f32) -> f32 {
    ((f64::from(top_dev) - f64::from(scroll_dev)) / f64::from(layout_scale.max(0.01))
        * f64::from(stretch)) as f32
}

/// Hand textures back to every window's atlas once the current effect cycle
/// ends. Deferred because a window busy handling its own event is out of
/// gpui's list while it does, and a drop made then would miss it.
pub fn give_back(images: Vec<Arc<RenderImage>>, cx: &mut App) {
    if images.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for image in images {
            cx.drop_image(image, None);
        }
    });
}

/// Up while someone still wants what a task is fetching; lowered when the
/// task holding it is dropped, so work finishing on the background pool can
/// tell it arrived for nobody.
struct Wanted(Arc<AtomicBool>);

impl Wanted {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }

    fn flag(&self) -> Arc<AtomicBool> {
        self.0.clone()
    }
}

impl Drop for Wanted {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Open a page, and close it again at once if nobody wants it by the time
/// the browser has it open. The call blocks for a load, and a square can be
/// closed in the middle of one; without this, that page would live on in
/// Chromium until the idle shutdown.
fn open_if_wanted(
    engine: &dyn PageEngine,
    req: &PageRequest,
    still: &AtomicBool,
) -> Result<PageLayout, EngineError> {
    let out = engine.open(req);
    if !still.load(Ordering::SeqCst) {
        if let Ok(PageLayout {
            page: Some(page), ..
        }) = &out
        {
            engine.close(*page);
        }
        return Err(EngineError::Closed);
    }
    out
}

/// Every `Arc<RenderImage>` of one render lives in exactly one slot here.
#[derive(Default)]
struct TileSet {
    generation: u64,
    bands: Vec<Band>,
    png: BTreeMap<u32, Arc<[u8]>>,
    gpu: BTreeMap<u32, Arc<RenderImage>>,
    decoding: BTreeSet<u32>,
    /// Bands being captured now, so the fetch loop never asks twice.
    fetching: BTreeSet<u32>,
}

impl TileSet {
    fn new(layout: &PageLayout) -> Self {
        Self {
            generation: layout.generation,
            bands: bands(layout.geometry.height_dev(layout.height_css)),
            ..Default::default()
        }
    }

    fn take_gpu(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.gpu).into_values().collect()
    }
}

struct Rendered {
    layout: PageLayout,
    tiles: TileSet,
}

enum Status {
    /// No size yet: nothing can be laid out until the view has been measured.
    Waiting,
    Drawing,
    Ready,
    Failed(String),
}

struct OpenDialog {
    id: String,
    render: Option<DialogRender>,
    image: Option<Arc<RenderImage>>,
    /// Logical px, for a dialog taller than the view.
    scroll: f32,
    failed: Option<String>,
    _work: Task<()>,
}

/// What a press asked the view to do next.
pub enum Pressed {
    Nothing,
    Took,
    /// A link out of this page, for the pane to route.
    Follow(FollowLink),
}

/// What the last paint measured, flat and in the view's own terms.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Measured {
    pub origin: Point<Pixels>,
    pub size: Size<Pixels>,
    pub scale: f32,
}

pub struct PageDoc {
    path: PathBuf,
    engine: Result<Arc<dyn PageEngine>, Unavailable>,
    status: Status,
    view: Option<Measured>,
    /// What the current render was asked for, or is being.
    asked: Option<Geometry>,
    current: Option<Rendered>,
    /// The render being replaced: drawn until the new one covers the view.
    old: Option<Rendered>,
    key: Option<CacheKey>,
    /// A live page for dialogs: its id and generation.
    live: Option<(PageId, u64)>,
    /// Page CSS px from the top.
    scroll_css: f32,
    dialog: Option<OpenDialog>,
    work: Option<Task<()>>,
    settle: Option<Task<()>>,
    drew: bool,
    /// The file has gone to the desktop; this view only says why now.
    handed_over: bool,
    /// A saved place to restore once the page is laid out: a fraction of
    /// its height.
    pending_top: Option<f32>,
    /// A fragment a link from another document named, to land on once the
    /// page is laid out.
    pending_fragment: Option<String>,
    /// The bands the last frame drew, for `TD_DOCDEBUG`.
    last_frame: Vec<u32>,
    /// The brief's notes, read from the bytes the current render was made
    /// from. `None` until a render is adopted.
    notes: Option<NotesLayer>,
    /// Where the pointer is over the view, flat and view-local; `None` when
    /// it is elsewhere. Note buttons show under it, as a browser shows them.
    pointer: Option<Point<Pixels>>,
    /// A save into the file, running: the write, then the read-back.
    saving: Option<Task<()>>,
    /// The file read again after it changed on disk.
    rereading: Option<Task<()>>,
}

fn debug() -> bool {
    std::env::var_os("TD_DOCDEBUG").is_some()
}

/// The view's page, from a task that only holds a weak handle.
fn page_of(view: &mut DocumentView) -> Option<&mut PageDoc> {
    match &mut view.backend {
        Backend::Page(p) => Some(p),
        _ => None,
    }
}

impl PageDoc {
    pub fn new(path: &Path, engine: Result<Arc<dyn PageEngine>, Unavailable>) -> Self {
        Self {
            path: path.to_path_buf(),
            engine,
            status: Status::Waiting,
            view: None,
            asked: None,
            current: None,
            old: None,
            key: None,
            live: None,
            scroll_css: 0.0,
            dialog: None,
            work: None,
            settle: None,
            drew: false,
            handed_over: false,
            pending_top: None,
            pending_fragment: None,
            last_frame: Vec::new(),
            notes: None,
            pointer: None,
            saving: None,
            rereading: None,
        }
    }

    fn geometry_for(m: Measured) -> Geometry {
        Geometry {
            css_width: f32::from(m.size.width).round().max(1.0) as u32,
            viewport_css_height: f32::from(m.size.height).round().max(1.0) as u32,
            scale: m.scale,
        }
    }

    /// The view was measured. The first measurement starts the render; a
    /// later size waits [`SETTLE`] and then lays the page out again, keeping
    /// the old picture up meanwhile.
    pub fn measured(&mut self, m: Measured, cx: &mut Context<DocumentView>) {
        if m.size.width <= px(1.) || m.size.height <= px(1.) {
            return;
        }
        self.view = Some(m);
        let g = Self::geometry_for(m);
        match self.asked {
            None => self.start(g, cx),
            Some(asked) if asked != g => {
                self.settle = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(SETTLE).await;
                    this.update(cx, |view, cx| {
                        let Some(page) = page_of(view) else { return };
                        let now = page.view.map(Self::geometry_for);
                        if now == Some(g) && page.asked != Some(g) {
                            page.start(g, cx);
                        }
                    })
                    .ok();
                }));
            }
            Some(_) => {}
        }
        self.plan(cx);
    }

    fn fail(&mut self, why: String, desktop: bool, cx: &mut Context<DocumentView>) {
        if debug() {
            eprintln!("[doc] page failed {}: {why}", self.path.display());
        }
        self.status = Status::Failed(why.clone());
        self.work = None;
        // Once, however the view is resized afterwards: a file handed to the
        // desktop on every resize would open a browser window each time.
        if desktop && !self.handed_over {
            self.handed_over = true;
            cx.emit(super::CannotShow { reason: why });
        }
        cx.notify();
    }

    /// Whether a failure means the engine itself cannot draw, so the file is
    /// better off with the desktop.
    fn engine_broken(e: &EngineError) -> bool {
        matches!(
            e,
            EngineError::Launch(_) | EngineError::Timeout(_) | EngineError::Protocol(_)
        )
    }

    fn start(&mut self, g: Geometry, cx: &mut Context<DocumentView>) {
        self.asked = Some(g);
        // Handed to the desktop already: this view only says why from now on.
        if self.handed_over {
            return;
        }
        let engine = match &self.engine {
            Ok(e) => e.clone(),
            Err(u) => {
                // The reason alone: whether the file then goes to the desktop
                // is the pane's call, and it says so in its own words.
                let why = u.reason();
                self.fail(why, true, cx);
                return;
            }
        };
        if matches!(self.status, Status::Waiting | Status::Failed(_)) {
            self.status = Status::Drawing;
        }
        let path = self.path.clone();
        // Lowered when this render's task is dropped — the view closed, or a
        // newer render replaced it — so a page the browser finishes opening
        // after that is closed at once instead of living on in Chromium.
        let wanted = Wanted::new();
        self.work = Some(cx.spawn(async move |this, cx| {
            let wanted = wanted;
            let root = cache::root();
            let name = engine.name();
            // Read, hash and look the render up, off the foreground. The
            // notes are read from these same bytes, so what the layer shows
            // belongs to the render it is drawn over.
            let found = cx
                .background_spawn({
                    let (path, root) = (path.clone(), root.clone());
                    async move {
                        let bytes = std::fs::read(&path).map_err(|e| format!("Could not read {}: {e}", path.display()))?;
                        let hash = layout_hash(&bytes);
                        let read = notes::read(&bytes);
                        let key = cache::key(&path, hash, g, name, EXTRACT_VERSION);
                        let cached = cache::lookup(&root, key).and_then(|c| {
                            let mut pngs = Vec::with_capacity(c.tiles.len());
                            for (band, file) in &c.tiles {
                                pngs.push((*band, Arc::<[u8]>::from(std::fs::read(file).ok()?)));
                            }
                            Some((c.layout, pngs))
                        });
                        Ok::<_, String>((hash, read, key, cached))
                    }
                })
                .await;
            let (mut hash, mut read, key, cached) = match found {
                Ok(f) => f,
                Err(why) => {
                    this.update(cx, |view, cx| {
                        if let Some(p) = page_of(view) {
                            p.fail(why, false, cx);
                        }
                    })
                    .ok();
                    return;
                }
            };
            if let Some((layout, pngs)) = cached {
                if debug() {
                    eprintln!("[doc] page cache hit {} ({} tiles)", path.display(), pngs.len());
                }
                this.update(cx, |view, cx| {
                    if let Some(p) = page_of(view) {
                        p.adopt(layout, key, None, read, cx);
                        for (band, png) in pngs {
                            p.landed(band, png, cx);
                        }
                        p.status = Status::Ready;
                        p.work = None;
                    }
                })
                .ok();
                return;
            }
            // Render. A file that changed between the read and the load is
            // read again, once.
            let mut layout = None;
            for attempt in 0..2 {
                let req = PageRequest {
                    path: path.clone(),
                    geometry: g,
                    expect: hash,
                };
                let (engine, still) = (engine.clone(), wanted.flag());
                let opened = cx.background_spawn(async move { open_if_wanted(&*engine, &req, &still) });
                match opened.await {
                    Ok(l) => {
                        layout = Some(Ok(l));
                        break;
                    }
                    Err(EngineError::FileChanged) if attempt == 0 => {
                        let p = path.clone();
                        let reread = cx.background_spawn(async move {
                            std::fs::read(&p).map(|bytes| (layout_hash(&bytes), notes::read(&bytes)))
                        });
                        match reread.await {
                            Ok((h, r)) => (hash, read) = (h, r),
                            Err(e) => {
                                layout = Some(Err(EngineError::Page(e.to_string())));
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        layout = Some(Err(e));
                        break;
                    }
                }
            }
            let layout = match layout {
                Some(Ok(l)) => l,
                other => {
                    let e = match other {
                        Some(Err(e)) => e,
                        _ => EngineError::FileChanged,
                    };
                    this.update(cx, |view, cx| {
                        if let Some(p) = page_of(view) {
                            let desktop = Self::engine_broken(&e);
                            p.fail(e.to_string(), desktop, cx);
                        }
                    })
                    .ok();
                    return;
                }
            };
            // A key for the bytes actually rendered, which may be a re-read.
            let key = if layout.rendered == hash { cache::key(&path, hash, g, name, EXTRACT_VERSION) } else { key };
            let (page, generation) = (layout.page, layout.generation);
            if debug() {
                eprintln!(
                    "[doc] page laid out {} {}x{} css, {} links, {} openers, {} dialogs, {} anchors",
                    path.display(),
                    layout.geometry.css_width,
                    layout.height_css,
                    layout.links.len(),
                    layout.openers.len(),
                    layout.dialogs.len(),
                    layout.anchors.len()
                );
            }
            let adopted = this
                .update(cx, |view, cx| page_of(view).map(|p| p.adopt(layout, key, page.map(|id| (id, generation)), read, cx)))
                .ok()
                .flatten();
            if adopted.is_none() {
                // The view went while the page was loading.
                if let Some(id) = page {
                    cx.background_spawn(async move { engine.close(id) }).detach();
                }
                return;
            }
            let Some(page) = page else { return };
            // Tiles, in whatever order the view wants them now: a scroll
            // while this runs changes what is nearest.
            loop {
                let next = this
                    .update(cx, |view, _| page_of(view).and_then(|p| p.next_fetch(generation)))
                    .ok()
                    .flatten();
                let Some(band) = next else { break };
                let engine = engine.clone();
                let got = cx.background_spawn(async move { engine.tile(page, generation, band) }).await;
                let ok = this
                    .update(cx, |view, cx| {
                        let Some(p) = page_of(view) else { return false };
                        match got {
                            Ok(t) if p.is_current(generation) => {
                                if debug() {
                                    eprintln!("[doc] tile at {} is {}x{}", t.band.top_dev, t.width_dev, t.height_dev);
                                }
                                p.landed(t.band, Arc::from(t.png), cx);
                                true
                            }
                            Ok(_) | Err(EngineError::Stale) => false,
                            Err(e) => {
                                let desktop = Self::engine_broken(&e);
                                p.fail(e.to_string(), desktop, cx);
                                false
                            }
                        }
                    })
                    .unwrap_or(false);
                if !ok {
                    return;
                }
            }
            // Every band landed: keep the render whole on disk.
            let whole = this
                .update(cx, |view, cx| {
                    let p = page_of(view)?;
                    p.status = Status::Ready;
                    cx.notify();
                    let r = p.current.as_ref()?;
                    let tiles: Vec<(Band, Arc<[u8]>)> = r
                        .tiles
                        .bands
                        .iter()
                        .filter_map(|b| r.tiles.png.get(&b.top_dev).map(|png| (*b, png.clone())))
                        .collect();
                    Some((r.layout.clone(), tiles))
                })
                .ok()
                .flatten();
            if let Some((layout, tiles)) = whole {
                cx.background_spawn(async move {
                    let borrowed: Vec<(Band, &[u8])> = tiles.iter().map(|(b, p)| (*b, &p[..])).collect();
                    if let Err(e) = cache::store(&root, key, &layout, &borrowed) {
                        if debug() {
                            eprintln!("[doc] page cache store failed: {e}");
                        }
                    }
                    let _ = cache::sweep(&root, cache::CAP_BYTES);
                })
                .await;
            }
        }));
    }

    fn is_current(&self, generation: u64) -> bool {
        self.current
            .as_ref()
            .is_some_and(|r| r.tiles.generation == generation)
    }

    /// A new layout becomes current. The one it replaces stays drawn, with
    /// only its visible tiles kept, until the new one covers the view.
    fn adopt(
        &mut self,
        layout: PageLayout,
        key: CacheKey,
        live: Option<(PageId, u64)>,
        read: NotesRead,
        cx: &mut Context<DocumentView>,
    ) {
        // The notes the new render's bytes hold, read as its own page reads
        // them. A note box open on an anchor the new render still has stays
        // open: a resize re-lays the page out and moves nothing it says.
        //
        // What was being written comes along too: the edits waiting to be
        // saved are deltas, so they apply to the new bytes as they did to
        // the old, and one on a passage the new render lacks is refused at
        // save with its words still in the note box.
        let mut layer = NotesLayer::new(
            read,
            layout.notes_file.clone(),
            layout.capability.concur,
            layout.capability.tagged,
            &self.path,
        );
        if let Some(old) = self.notes.take() {
            layer.carry_from(old);
        }
        self.notes = Some(layer);
        self.land(&layout);
        let previous_live = self.live.take();
        if let (Some((old_page, _)), Ok(engine)) = (previous_live, &self.engine) {
            let engine = engine.clone();
            cx.background_spawn(async move { engine.close(old_page) })
                .detach();
        }
        self.live = live;
        self.key = Some(key);
        let incoming = Rendered {
            tiles: TileSet::new(&layout),
            layout,
        };
        if let Some(mut outgoing) = self.current.replace(incoming) {
            let visible = self.visible_tops(&outgoing);
            let mut spare = Vec::new();
            outgoing.tiles.gpu.retain(|top, image| {
                let keep = visible.contains(top);
                if !keep {
                    spare.push(image.clone());
                }
                keep
            });
            outgoing.tiles.png.clear();
            give_back(spare, cx);
            if let Some(mut older) = self.old.replace(outgoing) {
                give_back(older.tiles.take_gpu(), cx);
            }
        }
        self.clamp_scroll();
        cx.notify();
    }

    /// Where the page is scrolled when `layout` becomes current. The reader's
    /// place is kept as the same fraction of the page, or the one a saved
    /// layout asked for before there was a page to scroll. A fragment a link
    /// named before then wins over both: a link into the middle of a brief
    /// lands where it points, and one naming nothing leaves the page where it
    /// was. Apart from `adopt` so a test can reach it without a window.
    fn land(&mut self, layout: &PageLayout) {
        if let Some(top) = self.pending_top.take() {
            self.scroll_css = top * layout.height_css;
        } else if let Some(prev) = &self.current {
            if prev.layout.height_css > 0.0 {
                self.scroll_css *= layout.height_css / prev.layout.height_css;
            }
        }
        let fragment = self.pending_fragment.take();
        if let Some(top) = fragment.and_then(|f| layout.fragment_top_css(&f)) {
            self.scroll_css = top;
        }
    }

    /// The next band to capture for `generation`, nearest the view first.
    fn next_fetch(&mut self, generation: u64) -> Option<Band> {
        let (plan, bands) = {
            let r = self
                .current
                .as_ref()
                .filter(|r| r.tiles.generation == generation)?;
            (self.residency(r), r.tiles.bands.clone())
        };
        let r = self.current.as_mut()?;
        let top = plan
            .fetch
            .into_iter()
            .find(|t| !r.tiles.fetching.contains(t))?;
        r.tiles.fetching.insert(top);
        bands.into_iter().find(|b| b.top_dev == top)
    }

    /// A band's PNG is in hand.
    fn landed(&mut self, band: Band, png: Arc<[u8]>, cx: &mut Context<DocumentView>) {
        if let Some(r) = self.current.as_mut() {
            r.tiles.fetching.remove(&band.top_dev);
            r.tiles.png.insert(band.top_dev, png);
        }
        self.plan(cx);
    }

    fn view_dev(&self, r: &Rendered) -> Option<(u32, u32)> {
        let m = self.view?;
        let g = r.layout.geometry;
        let stretch = self.stretch(r);
        let top = (f64::from(self.scroll_css) * f64::from(g.scale))
            .round()
            .max(0.0) as u32;
        let h = (f64::from(f32::from(m.size.height)) / f64::from(stretch) * f64::from(g.scale))
            .ceil() as u32;
        Some((top, h))
    }

    fn residency(&self, r: &Rendered) -> Residency {
        let all: Vec<u32> = r.tiles.bands.iter().map(|b| b.top_dev).collect();
        let resident: Vec<u32> = r
            .tiles
            .gpu
            .keys()
            .chain(r.tiles.decoding.iter())
            .copied()
            .collect();
        let have: Vec<u32> = r.tiles.png.keys().copied().collect();
        let (top, h) = self.view_dev(r).unwrap_or((0, TILE_DEV));
        plan_residency(&all, &resident, &have, top, h)
    }

    fn visible_tops(&self, r: &Rendered) -> Vec<u32> {
        let Some((top, h)) = self.view_dev(r) else {
            return Vec::new();
        };
        r.tiles
            .bands
            .iter()
            .map(|b| b.top_dev)
            .filter(|&t| distance(t, top, h).0 == 0)
            .collect()
    }

    /// Bring the GPU in line with the viewport: evict what is out of reach,
    /// decode what is wanted and in hand.
    fn plan(&mut self, cx: &mut Context<DocumentView>) {
        let Some(r) = self.current.as_ref() else {
            return;
        };
        let plan = self.residency(r);
        let generation = r.tiles.generation;
        let svg = cx.svg_renderer();
        let Some(r) = self.current.as_mut() else {
            return;
        };
        let mut evicted = Vec::new();
        for top in &plan.drop {
            if let Some(image) = r.tiles.gpu.remove(top) {
                evicted.push(image);
            }
        }
        give_back(evicted, cx);
        for top in plan.decode {
            let Some(png) = r.tiles.png.get(&top).cloned() else {
                continue;
            };
            r.tiles.decoding.insert(top);
            let svg = svg.clone();
            cx.spawn(async move |this, cx| {
                let decoded = cx
                    .background_spawn(async move {
                        gpui::Image::from_bytes(gpui::ImageFormat::Png, png.to_vec())
                            .to_image_data(svg)
                    })
                    .await;
                this.update(cx, |view, cx| {
                    if let Some(p) = page_of(view) {
                        p.decoded(generation, top, decoded.ok(), cx);
                    }
                })
                .ok();
            })
            .detach();
        }
        // The outgoing render goes once the new one covers the view.
        if self.old.is_some() && self.covers_view() {
            if let Some(mut old) = self.old.take() {
                give_back(old.tiles.take_gpu(), cx);
            }
        }
    }

    fn decoded(
        &mut self,
        generation: u64,
        top: u32,
        image: Option<Arc<RenderImage>>,
        cx: &mut Context<DocumentView>,
    ) {
        let wanted = {
            let Some(r) = self
                .current
                .as_ref()
                .filter(|r| r.tiles.generation == generation)
            else {
                // A decode for a render that is gone was never painted, so it
                // was never in an atlas: dropping the Arc frees it.
                return;
            };
            let plan = self.residency(r);
            !plan.drop.contains(&top)
        };
        let Some(r) = self.current.as_mut() else {
            return;
        };
        r.tiles.decoding.remove(&top);
        match image {
            Some(image) if wanted && !r.tiles.gpu.contains_key(&top) => {
                r.tiles.gpu.insert(top, image);
            }
            _ => {}
        }
        self.plan(cx);
        cx.notify();
    }

    fn covers_view(&self) -> bool {
        let Some(r) = self.current.as_ref() else {
            return false;
        };
        let visible = self.visible_tops(r);
        !visible.is_empty() && visible.iter().all(|t| r.tiles.gpu.contains_key(t))
    }

    /// Logical px per CSS px for a render: 1 when it was laid out at this
    /// view's width, else the stretch until the re-render lands.
    fn stretch(&self, r: &Rendered) -> f32 {
        let Some(m) = self.view else { return 1.0 };
        let w = f32::from(m.size.width);
        if r.layout.geometry.css_width == w.round().max(1.0) as u32 {
            1.0
        } else {
            w / r.layout.geometry.css_width.max(1) as f32
        }
    }

    fn max_scroll_css(&self) -> f32 {
        let (Some(r), Some(m)) = (self.current.as_ref(), self.view) else {
            return 0.0;
        };
        let view_css = f32::from(m.size.height) / self.stretch(r);
        (r.layout.height_css - view_css).max(0.0)
    }

    fn clamp_scroll(&mut self) {
        self.scroll_css = self.scroll_css.clamp(0.0, self.max_scroll_css());
    }

    pub fn wheel(&mut self, delta: ScrollDelta, cx: &mut Context<DocumentView>) {
        let dy = f32::from(delta.pixel_delta(px(LINE)).y);
        if let Some(d) = self.dialog.as_mut() {
            d.scroll = (d.scroll - dy).max(0.0);
            cx.notify();
            return;
        }
        let stretch = self
            .current
            .as_ref()
            .map(|r| self.stretch(r))
            .unwrap_or(1.0);
        self.scroll_css -= dy / stretch;
        self.clamp_scroll();
        self.plan(cx);
        cx.notify();
    }

    /// Scroll so a page CSS offset is at the top of the view.
    fn scroll_to(&mut self, top_css: f32, cx: &mut Context<DocumentView>) {
        self.scroll_css = top_css;
        self.clamp_scroll();
        self.plan(cx);
        cx.notify();
    }

    fn map_for(&self, r: &Rendered) -> Option<PageToView> {
        let m = self.view?;
        let stretch = self.stretch(r);
        Some(PageToView {
            origin: point(px(0.), px(0.)),
            px_per_css: stretch,
            scroll: px(self.scroll_css * stretch),
            clip: Bounds {
                origin: point(px(0.), px(0.)),
                size: m.size,
            },
        })
    }

    /// The marks the notes layer draws on the page itself: none while one of
    /// the brief's own dialogs covers it.
    fn page_marks(&self) -> Vec<Mark> {
        let (Some(layer), Some(r)) = (self.notes.as_ref(), self.current.as_ref()) else {
            return Vec::new();
        };
        match (self.dialog.is_some(), self.map_for(r)) {
            (false, Some(map)) => layer.marks(&r.layout.anchors, &map, self.pointer),
            _ => Vec::new(),
        }
    }

    /// The open dialog's anchors, relative to its picture, and the mapping
    /// its picture is drawn through.
    fn dialog_anchors(&self) -> Option<(&[Anchor], PageToView)> {
        let (frame, per_css) = self.dialog_frame()?;
        let render = self.dialog.as_ref()?.render.as_ref()?;
        Some((
            &render.anchors,
            PageToView {
                origin: frame.origin,
                px_per_css: per_css,
                scroll: px(0.),
                clip: frame,
            },
        ))
    }

    /// The marks on the open dialog's own anchors, drawn over its picture.
    fn dialog_marks(&self) -> Vec<Mark> {
        let (Some(layer), Some((anchors, map))) = (self.notes.as_ref(), self.dialog_anchors())
        else {
            return Vec::new();
        };
        layer.marks(anchors, &map, self.pointer)
    }

    /// Which anchors the pointer lights, on the dialog when one is open.
    fn lit(&self) -> Vec<String> {
        if self.dialog.is_some() {
            return self
                .dialog_anchors()
                .map(|(a, map)| NotesLayer::lit(a, &map, self.pointer))
                .unwrap_or_default();
        }
        match self
            .current
            .as_ref()
            .and_then(|r| Some((r, self.map_for(r)?)))
        {
            Some((r, map)) => NotesLayer::lit(&r.layout.anchors, &map, self.pointer),
            None => Vec::new(),
        }
    }

    /// The pointer moved over the view, or left it. Repaints only when that
    /// changes which note buttons show.
    pub fn hover(&mut self, at: Option<Point<Pixels>>, cx: &mut Context<DocumentView>) {
        if self.pointer == at {
            return;
        }
        let before = self.lit();
        self.pointer = at;
        if self.lit() != before {
            cx.notify();
        }
    }

    /// The notes, for the control socket: what the bar says and the map.
    /// `None` until the page has been laid out.
    pub fn notes_report(&self) -> Option<serde_json::Value> {
        let layer = self.notes.as_ref()?;
        let r = self.current.as_ref()?;
        Some(layer.report(&r.layout.anchors))
    }

    /// Save the waiting edits into the file.
    ///
    /// Off the main thread, in the order the program design fixed: read the
    /// file fresh; refuse if its layout changed since the page was drawn;
    /// apply the edits to the islands it holds now, never to a map kept in
    /// memory; check in memory that only the notes regions moved; commit —
    /// the backup ring, a temporary file, a rename, the bytes on disk read
    /// back and compared; then open the written file in a fresh page and
    /// check that a browser shows a note and a stamp exactly where they were
    /// written. Every refusal is said in the bar, and the edits wait.
    pub fn save(&mut self, cx: &mut Context<DocumentView>) {
        let (Some(layer), Some(r)) = (self.notes.as_mut(), self.current.as_ref()) else {
            return;
        };
        if let Err(why) = layer.can_save() {
            layer.say(Said::Refused(why));
            cx.notify();
            return;
        }
        let engine = match &self.engine {
            Ok(e) => e.clone(),
            Err(u) => {
                layer.say(Said::Refused(u.reason()));
                cx.notify();
                return;
            }
        };
        let edits = layer.begin_save();
        let label = layer.label().to_string();
        let concurs = r.layout.capability.concur == ConcurSupport::Supported;
        let anchors: Vec<(String, String)> = r
            .layout
            .anchors
            .iter()
            .map(|a| (a.nid.clone(), a.title.clone()))
            .collect();
        let drawn: Vec<(String, Option<RectCss>)> = r
            .layout
            .anchors
            .iter()
            .map(|a| (a.nid.clone(), a.rect))
            .collect();
        let (rendered, geometry) = (r.layout.rendered, r.layout.geometry);
        let path = self.path.clone();
        let file = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let made = edits.len();
        cx.notify();
        self.saving = Some(cx.spawn(async move |this, cx| {
            let write = cx
                .background_spawn({
                    let path = path.clone();
                    async move { write_notes(&path, &edits, &anchors, &label, concurs, rendered) }
                })
                .await;
            let (plan, written) = match write {
                Ok(w) => w,
                Err(why) => {
                    this.update(cx, |view, cx| {
                        if let Some(p) = page_of(view) {
                            if let Some(l) = p.notes.as_mut() {
                                l.refused(why);
                            }
                            if let Some(t) = p.saving.take() {
                                t.detach();
                            }
                        }
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            this.update(cx, |view, cx| {
                // The watcher will see this write; it is the view's own.
                view.own_write = Some(super::FileStamp {
                    mtime: written.mtime,
                    len: written.len,
                });
                if let Some(l) = page_of(view).and_then(|p| p.notes.as_mut()) {
                    l.saved(made, plan.notes.clone(), plan.concurs.clone(), &file);
                }
                cx.notify();
            })
            .ok();
            let back = cx
                .background_spawn({
                    let path = path.clone();
                    async move { engine.read_back(&path, geometry) }
                })
                .await;
            this.update(cx, |view, cx| {
                let Some(p) = page_of(view) else { return };
                let backup = written.backup.display();
                let verdict = match &back {
                    Ok(layout) => notes::confirm(&plan, &layout.anchors).map_err(|why| {
                        format!("saved into {file}, but the file reopened fresh does not show it ({why}); the file from before is at {backup}")
                    }),
                    Err(e) => Err(format!(
                        "saved into {file}, but it could not be reopened to check ({e}); the file from before is at {backup}"
                    )),
                };
                if let Some(l) = p.notes.as_mut() {
                    match verdict {
                        Ok(()) => l.confirmed(&file),
                        Err(why) => l.unconfirmed(why),
                    }
                }
                // Least-confident decision 2, checked on every save: a note
                // is data, not layout, so the written file lays out as the
                // one on screen. If it ever does not, the render on screen
                // and in the cache is stale, and both are made again.
                if let Ok(layout) = &back {
                    let moved = moved_anchors(&drawn, &layout.anchors);
                    if !moved.is_empty() {
                        if debug() {
                            eprintln!("[doc] a saved note moved anchors: {moved:?}");
                        }
                        p.redraw_from_scratch(cx);
                    }
                }
                if let Some(t) = p.saving.take() {
                    t.detach();
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// Forget this render in the cache and draw the page again.
    fn redraw_from_scratch(&mut self, cx: &mut Context<DocumentView>) {
        if let Some(key) = self.key {
            let root = cache::root();
            cx.background_spawn(async move { cache::forget(&root, key) })
                .detach();
        }
        if let Some(g) = self.asked {
            self.start(g, cx);
        }
    }

    /// The file changed on disk, and not by this view. Only its notes
    /// changed — an agent edited the island — and the page shows the file's
    /// notes with the edits waiting here on top, and nothing is drawn again;
    /// anything else changed, and the page is drawn again, the old picture
    /// up until the new one covers it, the waiting edits carried over.
    pub fn changed_on_disk(&mut self, cx: &mut Context<DocumentView>) {
        // A save running sees any other writer itself, and refuses.
        if self.saving.is_some() {
            return;
        }
        let (Some(r), Some(g)) = (self.current.as_ref(), self.asked) else {
            return;
        };
        let (rendered, tagged) = (r.layout.rendered, r.layout.capability.tagged);
        let path = self.path.clone();
        self.rereading = Some(cx.spawn(async move |this, cx| {
            let got = cx
                .background_spawn(async move {
                    std::fs::read(&path).map(|b| (layout_hash(&b), notes::read(&b)))
                })
                .await;
            this.update(cx, |view, cx| {
                let Some(p) = page_of(view) else { return };
                match got {
                    Ok((hash, read)) if hash == rendered => {
                        let path = p.path.clone();
                        if let Some(l) = p.notes.as_mut() {
                            l.rebase(read, tagged, &path);
                        }
                    }
                    Ok(_) => p.start(g, cx),
                    Err(_) => {
                        if let Some(l) = p.notes.as_mut() {
                            l.set_gone(true);
                        }
                    }
                }
                if let Some(t) = p.rereading.take() {
                    t.detach();
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// The file went missing, or came back.
    pub fn gone(&mut self, gone: bool, cx: &mut Context<DocumentView>) {
        if let Some(l) = self.notes.as_mut() {
            l.set_gone(gone);
        }
        cx.notify();
    }

    /// A notes command from the control socket: the note box's and the
    /// bar's gestures, for a caller with no pointer.
    pub fn notes_command(
        &mut self,
        cmd: super::NotesCommand,
        cx: &mut Context<DocumentView>,
    ) -> Result<(), String> {
        use super::NotesCommand;
        let (Some(layer), Some(r)) = (self.notes.as_mut(), self.current.as_ref()) else {
            return Err("the document is not a laid-out HTML page yet".into());
        };
        let anchor = |nid: &str| r.layout.anchors.iter().find(|a| a.nid == nid);
        let now = SystemTime::now();
        match cmd {
            NotesCommand::Save => {
                layer.can_save()?;
                self.save(cx);
                return Ok(());
            }
            NotesCommand::Add { nid, text } => {
                layer.can_edit()?;
                let a = anchor(&nid).ok_or(format!("There is no anchor [{nid}] on this page."))?;
                let text = text.trim().to_string();
                if text.is_empty() {
                    return Err("A note needs some words.".into());
                }
                layer.add_note(nid, a.title.clone(), text, notes::utc_minute(now));
            }
            NotesCommand::Delete { nid, text } => {
                layer.can_edit()?;
                layer.delete_note(nid, text, None);
            }
            NotesCommand::Concur { nid } => {
                let takes = anchor(&nid).is_some_and(|a| a.concur_zone.is_some());
                if !takes {
                    return Err(format!(
                        "[{nid}] is not a decision that takes a CONCUR stamp."
                    ));
                }
                layer.toggle_concur(&nid, now)?;
            }
        }
        cx.notify();
        Ok(())
    }

    /// Whether the note box holds a draft being written: while it does,
    /// every key is the view's.
    pub fn has_caret(&self) -> bool {
        self.notes.as_ref().is_some_and(NotesLayer::has_caret)
    }

    /// A key the view was handed. The note box takes every key while it is
    /// open; otherwise Escape puts away one of the brief's own dialogs, and,
    /// in a floating square that the next Escape would close, keeps it open
    /// once to say that edits are unsaved.
    pub fn key(
        &mut self,
        ks: &gpui::Keystroke,
        floating: bool,
        cx: &mut Context<DocumentView>,
    ) -> bool {
        if let Some(layer) = self.notes.as_mut() {
            if layer.key(ks, SystemTime::now()) {
                cx.notify();
                return true;
            }
        }
        if ks.key != "escape" {
            return false;
        }
        if self.escape(cx) {
            return true;
        }
        // Nothing open over the page: Escape would close the document, and
        // with it every edit not yet saved.
        floating && self.guard_close(cx)
    }

    /// Keep the document open once if closing it would lose edits not yet
    /// saved, and say so. See [`NotesLayer::guard_close`].
    pub fn guard_close(&mut self, cx: &mut Context<DocumentView>) -> bool {
        if self.notes.as_mut().is_some_and(NotesLayer::guard_close) {
            cx.notify();
            return true;
        }
        false
    }

    /// Escape puts away the note box first, then one of the brief's own
    /// dialogs, and nothing else here.
    pub fn escape(&mut self, cx: &mut Context<DocumentView>) -> bool {
        if self.notes.as_mut().is_some_and(NotesLayer::escape) {
            cx.notify();
            return true;
        }
        match self.dialog.take() {
            Some(d) => {
                give_back(d.image.into_iter().collect(), cx);
                cx.notify();
                true
            }
            None => false,
        }
    }

    /// A press, flat and relative to the view's top-left. `origin` is where
    /// the view was last painted, in window pixels: the notes bar and the
    /// note box were laid out by gpui and recorded there.
    ///
    /// In the order they are drawn, top first: the note box, which takes
    /// every press while it is open; the bar; a note button or concur space;
    /// then the brief's own dialog, buttons and links.
    pub fn press(
        &mut self,
        at: Point<Pixels>,
        origin: Option<Point<Pixels>>,
        cx: &mut Context<DocumentView>,
    ) -> Pressed {
        if let (Some(layer), Some(origin), Some(r)) =
            (self.notes.as_mut(), origin, self.current.as_ref())
        {
            match layer.press(origin + at, &r.layout.anchors, SystemTime::now(), cx) {
                LayerPress::Took => {
                    cx.notify();
                    return Pressed::Took;
                }
                LayerPress::Save => {
                    self.save(cx);
                    return Pressed::Took;
                }
                LayerPress::Pass => {}
            }
        }
        let marks = match self.dialog.is_some() {
            true => self.dialog_marks(),
            false => self.page_marks(),
        };
        match notes_ui::hit(&marks, at) {
            Some(MarkHit::Open { nid, title }) => {
                if let Some(layer) = self.notes.as_mut() {
                    layer.open(nid, title);
                }
                cx.notify();
                return Pressed::Took;
            }
            // A stamp put down, or peeled off; saved with the notes.
            Some(MarkHit::Concur(nid)) => {
                if let Some(layer) = self.notes.as_mut() {
                    if let Err(why) = layer.toggle_concur(&nid, SystemTime::now()) {
                        layer.say(Said::Refused(why));
                    }
                }
                cx.notify();
                return Pressed::Took;
            }
            None => {}
        }
        if self.dialog.is_some() {
            return self.press_dialog(at, cx);
        }
        let Some(r) = self.current.as_ref() else {
            return Pressed::Nothing;
        };
        let Some(map) = self.map_for(r) else {
            return Pressed::Nothing;
        };
        match hit_at(&r.layout.openers, &r.layout.links, &[], &map, at) {
            Some(PageHit::Opener(id)) => {
                self.open_dialog(id, cx);
                Pressed::Took
            }
            Some(PageHit::Link(l)) => self.follow(&l, cx),
            Some(PageHit::Closer) | None => Pressed::Nothing,
        }
    }

    fn follow(&mut self, link: &Link, cx: &mut Context<DocumentView>) -> Pressed {
        if let Some(top) = link.fragment_top_css {
            if let Some(d) = self.dialog.take() {
                give_back(d.image.into_iter().collect(), cx);
            }
            self.scroll_to(top, cx);
            return Pressed::Took;
        }
        let dir = self.path.parent().unwrap_or(Path::new("/"));
        match resolve_link(dir, &link.href) {
            // A fragment naming nothing on this page goes nowhere, as in a browser.
            LinkTarget::Fragment(_) => Pressed::Took,
            LinkTarget::File { path, .. } if path == self.path => Pressed::Took,
            LinkTarget::File { path, fragment } => Pressed::Follow(FollowLink {
                target: path.to_string_lossy().into_owned(),
                fragment,
            }),
            LinkTarget::Url(url) => Pressed::Follow(FollowLink {
                target: url,
                fragment: None,
            }),
        }
    }

    /// Scroll to where a fragment lands, one a link from another document
    /// named: now, if the page has been laid out, else as soon as it is. A
    /// fragment naming nothing leaves the page where it is, as a browser does.
    pub fn show_fragment(&mut self, fragment: String, cx: &mut Context<DocumentView>) {
        let Some(r) = self.current.as_ref() else {
            self.pending_fragment = Some(fragment);
            return;
        };
        if let Some(top) = r.layout.fragment_top_css(&fragment) {
            if let Some(d) = self.dialog.take() {
                give_back(d.image.into_iter().collect(), cx);
            }
            self.scroll_to(top, cx);
        }
    }

    /// Scroll to a fraction of the page's height: now, if it has been laid
    /// out, else as soon as it is.
    pub fn restore_scroll(&mut self, top: f32, cx: &mut Context<DocumentView>) {
        let top = top.clamp(0.0, 1.0);
        match self.current.as_ref().map(|r| r.layout.height_css) {
            Some(height) => self.scroll_to(top * height, cx),
            None => self.pending_top = Some(top),
        }
    }

    /// Where the page is scrolled, as a fraction of its height. `None`
    /// before it has been laid out.
    pub fn scroll(&self) -> Option<DocScroll> {
        let r = self.current.as_ref()?;
        (r.layout.height_css > 0.0).then(|| DocScroll {
            top: (self.scroll_css / r.layout.height_css).clamp(0.0, 1.0),
        })
    }

    /// Where the open dialog's picture sits in the view, and logical px per
    /// dialog CSS px.
    fn dialog_frame(&self) -> Option<(Bounds<Pixels>, f32)> {
        let d = self.dialog.as_ref()?;
        let render = d.render.as_ref()?;
        let m = self.view?;
        let r = self.current.as_ref()?;
        let g = r.layout.geometry;
        let stretch = self.stretch(r);
        let (vw, vh) = (f32::from(m.size.width), f32::from(m.size.height));
        let mut per_css = stretch;
        let mut w = render.width_dev as f32 / g.scale * per_css;
        if w > vw && w > 0.0 {
            per_css *= vw / w;
            w = vw;
        }
        let h = render.height_dev as f32 / g.scale * per_css;
        let off_x = snap_offset(m.origin.x.into(), m.scale);
        let off_y = snap_offset(m.origin.y.into(), m.scale);
        let x = ((vw - w) / 2.0).max(0.0);
        let y = if h <= vh {
            (vh - h) / 2.0
        } else {
            -d.scroll.min(h - vh)
        };
        let x = (x * m.scale).round() / m.scale + off_x;
        let y = (y * m.scale).round() / m.scale + off_y;
        Some((
            Bounds {
                origin: point(px(x), px(y)),
                size: size(px(w), px(h)),
            },
            per_css,
        ))
    }

    fn press_dialog(&mut self, at: Point<Pixels>, cx: &mut Context<DocumentView>) -> Pressed {
        let Some((frame, per_css)) = self.dialog_frame() else {
            // Still opening: a press anywhere puts it away.
            self.escape(cx);
            return Pressed::Took;
        };
        if !frame.contains(&at) {
            self.escape(cx);
            return Pressed::Took;
        }
        let Some(render) = self.dialog.as_ref().and_then(|d| d.render.clone()) else {
            return Pressed::Took;
        };
        let map = PageToView {
            origin: frame.origin,
            px_per_css: per_css,
            scroll: px(0.),
            clip: frame,
        };
        match hit_at(&render.openers, &render.links, &render.closers, &map, at) {
            Some(PageHit::Closer) => {
                self.escape(cx);
                Pressed::Took
            }
            Some(PageHit::Opener(id)) => {
                self.open_dialog(id, cx);
                Pressed::Took
            }
            Some(PageHit::Link(l)) => self.follow(&l, cx),
            None => Pressed::Took,
        }
    }

    /// Open one of the brief's own dialogs: from the cache if it was drawn
    /// before, else rendered now, in a live page opened for it if the one
    /// the tiles came from has gone.
    fn open_dialog(&mut self, id: String, cx: &mut Context<DocumentView>) {
        if let Some(old) = self.dialog.take() {
            give_back(old.image.into_iter().collect(), cx);
        }
        let (Ok(engine), Some(key), Some(r)) =
            (self.engine.clone(), self.key, self.current.as_ref())
        else {
            return;
        };
        let geometry = r.layout.geometry;
        let expect = r.layout.rendered;
        let live = self.live;
        let path = self.path.clone();
        let svg = cx.svg_renderer();
        let wanted = id.clone();
        let alive = Wanted::new();
        let work = cx.spawn(async move |this, cx| {
            let alive = alive;
            let root = cache::root();
            let cached = cx
                .background_spawn({
                    let (root, id) = (root.clone(), wanted.clone());
                    async move { cache::load_dialog(&root, key, &id) }
                })
                .await;
            let render = match cached {
                Some(r) => Ok((r, None)),
                None => {
                    let (engine, id, still) = (engine.clone(), wanted.clone(), alive.flag());
                    cx.background_spawn(async move {
                        let try_live =
                            |page: PageId, generation: u64| engine.dialog(page, generation, &id);
                        let first = match live {
                            Some((page, generation)) => try_live(page, generation),
                            None => Err(EngineError::Closed),
                        };
                        // The page the tiles came from is gone (closed, or the
                        // browser idled out): a fresh one serves the dialog,
                        // and is closed again if the dialog is not drawn or
                        // nobody is waiting for it any more.
                        let (out, fresh) = match first {
                            Err(EngineError::Closed) | Err(EngineError::Stale) => {
                                let req = PageRequest {
                                    path,
                                    geometry,
                                    expect,
                                };
                                let l = open_if_wanted(&*engine, &req, &still)?;
                                let page = l.page.ok_or(EngineError::Closed)?;
                                let drawn = try_live(page, l.generation);
                                if drawn.is_err() || !still.load(Ordering::SeqCst) {
                                    engine.close(page);
                                }
                                (drawn?, Some((page, l.generation)))
                            }
                            other => (other?, None),
                        };
                        if let Err(e) = cache::store_dialog(&root, key, &out) {
                            if debug() {
                                eprintln!("[doc] dialog cache store failed: {e}");
                            }
                        }
                        Ok::<_, EngineError>((out, fresh))
                    })
                    .await
                }
            };
            let (render, fresh) = match render {
                Ok(r) => r,
                Err(e) => {
                    this.update(cx, |view, cx| {
                        if let Some(d) = page_of(view)
                            .and_then(|p| p.dialog.as_mut())
                            .filter(|d| d.id == wanted)
                        {
                            d.failed = Some(e.to_string());
                        }
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let png = render.png.clone();
            let image = cx
                .background_spawn(async move {
                    gpui::Image::from_bytes(gpui::ImageFormat::Png, png).to_image_data(svg)
                })
                .await
                .ok();
            this.update(cx, |view, cx| {
                let Some(p) = page_of(view) else { return };
                if let Some(fresh) = fresh {
                    // The page opened for this dialog serves the next one too.
                    if let (Some((gone, _)), Ok(engine)) = (p.live.replace(fresh), &p.engine) {
                        let engine = engine.clone();
                        cx.background_spawn(async move { engine.close(gone) })
                            .detach();
                    }
                }
                // Closed, or another opened, while this one rendered: then it
                // was never painted, so it holds no atlas space and just goes.
                if let Some(d) = p.dialog.as_mut().filter(|d| d.id == wanted) {
                    d.render = Some(render);
                    d.image = image;
                }
                cx.notify();
            })
            .ok();
        });
        if debug() {
            eprintln!("[doc] page dialog {id}");
        }
        self.dialog = Some(OpenDialog {
            id,
            render: None,
            image: None,
            scroll: 0.0,
            failed: None,
            _work: work,
        });
        cx.notify();
    }

    /// Everything this page holds on the GPU, and the live page, given back.
    /// Runs from the view's release hook, when every window is in gpui's list,
    /// so the textures are dropped at once rather than deferred.
    pub fn release(&mut self, cx: &mut App) -> usize {
        let mut images = Vec::new();
        if let Some(mut r) = self.current.take() {
            images.extend(r.tiles.take_gpu());
        }
        if let Some(mut r) = self.old.take() {
            images.extend(r.tiles.take_gpu());
        }
        if let Some(d) = self.dialog.take() {
            images.extend(d.image);
        }
        self.work = None;
        self.settle = None;
        let n = images.len();
        for image in images {
            cx.drop_image(image, None);
        }
        if let (Some((page, _)), Ok(engine)) = (self.live.take(), &self.engine) {
            let engine = engine.clone();
            cx.background_spawn(async move { engine.close(page) })
                .detach();
        }
        n
    }

    fn tiles_of(&self, r: &Rendered, m: Measured) -> Vec<gpui::AnyElement> {
        let g = r.layout.geometry;
        let stretch = self.stretch(r);
        let scroll_dev = (f64::from(self.scroll_css) * f64::from(g.scale))
            .round()
            .max(0.0) as u32;
        let off_x = snap_offset(m.origin.x.into(), m.scale);
        let off_y = snap_offset(m.origin.y.into(), m.scale);
        let width = g.css_width as f32 * stretch;
        r.tiles
            .bands
            .iter()
            .filter_map(|b| {
                let image = r.tiles.gpu.get(&b.top_dev)?.clone();
                let top = off_y + tile_top(b.top_dev, scroll_dev, g.scale, stretch);
                // The PNG's own height, not the band's: where a page ends on
                // a fractional device row, Chromium stops a row short, and a
                // stretched last row would blur the whole band.
                let rows = image.size(0).height.0.max(0) as f32;
                let height = rows / g.scale * stretch;
                Some(
                    img(ImageSource::Render(image))
                        .absolute()
                        .left(px(off_x))
                        .top(px(top))
                        .w(px(width))
                        .h(px(height))
                        .object_fit(ObjectFit::Fill)
                        .into_any_element(),
                )
            })
            .collect()
    }

    /// The page as drawn this frame. `origin` is where the view was last
    /// painted, which moves when the square is dragged without resizing, so
    /// the tiles are snapped against it rather than against the origin they
    /// were first measured at.
    pub fn element(&mut self, th: &Theme, origin: Option<Point<Pixels>>) -> gpui::AnyElement {
        let note = |s: String| {
            div()
                .absolute()
                .left(px(14.))
                .top(px(14.))
                .right(px(14.))
                .text_color(th.text.alpha(0.75))
                .child(s)
                .into_any_element()
        };
        let mut layers: Vec<gpui::AnyElement> = Vec::new();
        let Some(mut m) = self.view else {
            return div().into_any_element();
        };
        if let Some(origin) = origin {
            m.origin = origin;
        }
        if let Some(old) = self.old.as_ref() {
            layers.extend(self.tiles_of(old, m));
        }
        if let Some(r) = self.current.as_ref() {
            layers.extend(self.tiles_of(r, m));
            if debug() {
                // What this frame hands the GPU, each time it changes: the
                // soak's evidence that no more than five are ever resident.
                let now: Vec<u32> = r.tiles.gpu.keys().copied().collect();
                if now != self.last_frame {
                    eprintln!("[doc] frame resident={} {now:?}", now.len());
                    self.last_frame = now;
                }
            }
            let covered = self.covers_view();
            if covered && !self.drew {
                self.drew = true;
                if debug() {
                    let r = self
                        .current
                        .as_ref()
                        .map(|r| r.tiles.gpu.len())
                        .unwrap_or(0);
                    eprintln!("[doc] drew {} tiles={r}", self.path.display());
                }
            }
        }
        // Forget where the bar and the note box were; the canvases below
        // record where they land this frame.
        if let Some(layer) = &self.notes {
            layer.clear_zones();
            layers.extend(layer.draw_marks(&self.page_marks(), th));
        }
        match &self.status {
            Status::Failed(why) => layers.push(note(why.clone())),
            Status::Waiting | Status::Drawing if layers.is_empty() => {
                layers.push(note("drawing the page…".into()))
            }
            _ => {}
        }
        if let Some(d) = &self.dialog {
            layers.push(
                div()
                    .absolute()
                    .inset_0()
                    .bg(hsla(0., 0., 0., 0.62))
                    .into_any_element(),
            );
            match (&d.image, self.dialog_frame(), &d.failed) {
                (_, _, Some(why)) => {
                    layers.push(note(format!("This dialog could not be drawn: {why}")))
                }
                (Some(image), Some((frame, _)), None) => layers.push(
                    img(ImageSource::Render(image.clone()))
                        .absolute()
                        .left(frame.origin.x)
                        .top(frame.origin.y)
                        .w(frame.size.width)
                        .h(frame.size.height)
                        .object_fit(ObjectFit::Fill)
                        .into_any_element(),
                ),
                _ => layers.push(note("opening…".into())),
            }
            if let Some(layer) = &self.notes {
                layers.extend(layer.draw_marks(&self.dialog_marks(), th));
            }
        }
        // TD's own chrome over the page: the bar, then the note box above
        // everything, as a browser's dialog sits above its notebar.
        if let (Some(layer), Some(r)) = (&self.notes, &self.current) {
            layers.push(layer.draw_bar(&r.layout.anchors, th));
            layers.extend(layer.draw_box(m.size, th));
        }
        div()
            .absolute()
            .inset_0()
            .children(layers)
            .into_any_element()
    }
}

/// A save, off the main thread: read fresh, check the layout, plan, verify,
/// commit. The error is the sentence the bar says; nothing was written.
fn write_notes(
    path: &Path,
    edits: &[notes::NoteEdit],
    anchors: &[(String, String)],
    label: &str,
    concurs: bool,
    rendered: super::engine::LayoutHash,
) -> Result<(notes::WritePlan, notes::Written), String> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| {
        format!("Could not read {name} to save into it ({e}); nothing was written.")
    })?;
    if layout_hash(&bytes) != rendered {
        return Err(format!(
            "{name} changed on disk since it was drawn, so nothing was written; it is being drawn again, and your notes wait here to save once it is."
        ));
    }
    let pairs: Vec<(&str, &str)> = anchors
        .iter()
        .map(|(n, t)| (n.as_str(), t.as_str()))
        .collect();
    let rev = notes::iso_millis(SystemTime::now());
    let plan = notes::plan_write(
        &bytes,
        edits,
        &notes::WriteArgs {
            anchors: &pairs,
            label,
            concurs,
            rev: &rev,
            path,
        },
    )
    .map_err(|r| r.sentence())?;
    notes::verify(&bytes, &plan).map_err(|why| {
        format!("TD's check of its own write failed ({why}), so nothing was written.")
    })?;
    let written =
        notes::commit(path, &bytes, &plan, &notes::backups_for(path)).map_err(|e| e.sentence())?;
    Ok((plan, written))
}

/// Pure. The anchors whose place differs between two layouts of one page,
/// by more than half a CSS pixel, or that one has and the other lacks.
pub fn moved_anchors(drawn: &[(String, Option<RectCss>)], now: &[Anchor]) -> Vec<String> {
    let mut out = Vec::new();
    for (nid, rect) in drawn {
        let there = now.iter().find(|a| a.nid == *nid).map(|a| a.rect);
        let same = match (rect, there) {
            (_, None) => false,
            (None, Some(None)) => true,
            (Some(a), Some(Some(b))) => {
                (a.x - b.x).abs() <= 0.5
                    && (a.y - b.y).abs() <= 0.5
                    && (a.w - b.w).abs() <= 0.5
                    && (a.h - b.h).abs() <= 0.5
            }
            _ => false,
        };
        if !same {
            out.push(nid.clone());
        }
    }
    if now.len() != drawn.len() {
        out.extend(
            now.iter()
                .filter(|a| !drawn.iter().any(|(n, _)| *n == a.nid))
                .map(|a| a.nid.clone()),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tops(n: u32) -> Vec<u32> {
        (0..n).map(|k| k * TILE_DEV).collect()
    }

    /// A link from another document names a fragment before the brief has
    /// been laid out, so it is held until the layout arrives: the page lands
    /// on the element, over a saved place, and a fragment naming nothing
    /// leaves the page at its top. Issue 734: it used to open at the top
    /// whatever the link said.
    #[test]
    fn a_fragment_named_before_the_layout_is_landed_on_when_it_arrives() {
        use super::super::engine::Target;
        let layout = PageLayout::bare(
            5000.0,
            Some(vec![Target {
                id: "fig-02".into(),
                top: Some(3120.5),
            }]),
        );
        let fresh = || PageDoc::new(Path::new("/r/brief.html"), Err(Unavailable::Off));

        let mut page = fresh();
        page.pending_fragment = Some("fig-02".into());
        page.pending_top = Some(0.25);
        page.land(&layout);
        assert_eq!(page.scroll_css, 3120.5);
        assert_eq!(
            page.pending_fragment, None,
            "landed once, not on every layout"
        );

        let mut page = fresh();
        page.pending_fragment = Some("nowhere".into());
        page.land(&layout);
        assert_eq!(page.scroll_css, 0.0);

        let mut page = fresh();
        page.pending_top = Some(0.25);
        page.land(&layout);
        assert_eq!(page.scroll_css, 1250.0, "no fragment: the saved place");
    }

    #[test]
    fn scrolling_keeps_at_most_five_tiles_resident() {
        // The tallest brief: 13 bands. A viewport of 1,280 device rows, the
        // float's height at scale 1.6, scrolled top to bottom and back.
        let all = tops(13);
        let view_h = 1280;
        let mut resident: Vec<u32> = Vec::new();
        let mut positions: Vec<u32> = (0..=24_000).step_by(500).collect();
        positions.extend((0..=24_000).rev().step_by(700));
        for top in positions {
            let plan = plan_residency(&all, &resident, &all, top, view_h);
            resident.retain(|t| !plan.drop.contains(t));
            resident.extend(&plan.decode);
            assert!(resident.len() <= MAX_RESIDENT, "at {top}: {resident:?}");
            for t in &resident {
                let (d, _) = distance(*t, top, view_h);
                assert!(
                    d <= TILE_DEV,
                    "a band {d} rows from the view was left behind at {top}"
                );
            }
            // Everything on screen is on the GPU.
            for t in &all {
                if distance(*t, top, view_h).0 == 0 {
                    assert!(resident.contains(t), "visible band {t} missing at {top}");
                }
            }
        }
        // A tall split: 4,000 device rows spans three bands; with one either
        // side that is five, and never more.
        let plan = plan_residency(&all, &[], &all, 3000, 4000);
        assert_eq!(plan.decode.len(), 5);
        // Taller still, 6,200 rows spans four bands: all four stay, and only
        // one neighbour joins them — the nearer, here the one above (953 rows
        // off, against 1,041 below).
        let plan = plan_residency(&all, &[], &all, 3000, 6200);
        assert_eq!(plan.decode.len(), MAX_RESIDENT, "{:?}", plan.decode);
        for visible in [2048, 4096, 6144, 8192] {
            assert!(plan.decode.contains(&visible), "{visible} is on screen");
        }
        assert!(plan.decode.contains(&0) && !plan.decode.contains(&10240));
    }

    #[test]
    fn the_viewports_tiles_are_asked_for_first() {
        let all = tops(13);
        // Straddling the seam between bands 5 and 6, as far from 4 as from 7.
        let top = 6 * TILE_DEV - 640;
        let plan = plan_residency(&all, &[], &[], top, 1280);
        assert_eq!(
            &plan.fetch[..2],
            &[5 * TILE_DEV, 6 * TILE_DEV],
            "on screen first"
        );
        // Then outward, nearest first, below before above at equal distance.
        assert_eq!(plan.fetch[2], 7 * TILE_DEV);
        assert_eq!(plan.fetch[3], 4 * TILE_DEV);
        assert_eq!(
            plan.fetch.len(),
            13,
            "every band is fetched, so the render can be cached whole"
        );
        let distances: Vec<u32> = plan
            .fetch
            .iter()
            .map(|t| distance(*t, top, 1280).0)
            .collect();
        assert!(distances.windows(2).all(|w| w[0] <= w[1]), "{distances:?}");
        // Nearer above than below: above comes first.
        let near_top = plan_residency(&all, &[], &[], 5 * TILE_DEV + 100, 1280);
        assert_eq!(&near_top.fetch[..2], &[5 * TILE_DEV, 4 * TILE_DEV]);
        // Nothing in hand: nothing to decode yet.
        assert!(plan.decode.is_empty());
    }

    #[test]
    fn a_note_button_sits_where_the_browser_puts_it_after_scroll_and_zoom() {
        // Scale 1.6 and zoom 1.25: an 800 px view lays the page out at 640
        // CSS px, 1.25 logical px per CSS px, 2.0 device px per CSS px. A
        // scroll of 3,000 device rows is 1,500 CSS px, 1,875 logical px.
        let (s, zoom, scroll_dev) = (1.6f32, 1.25f32, 3000.0f32);
        let map = PageToView {
            origin: point(px(40.), px(100.)),
            px_per_css: 800.0 / (800.0 / zoom),
            scroll: px(scroll_dev / s),
            clip: Bounds {
                origin: point(px(40.), px(100.)),
                size: size(px(800.), px(700.)),
            },
        };
        // notes.css puts the button 26 × 26 at the element's top right.
        let button = RectCss {
            x: 600.0,
            y: 2000.0,
            w: 26.0,
            h: 26.0,
        };
        let b = place(button, &map).expect("on screen");
        assert_eq!(b.origin.x, px(40. + 600. * 1.25));
        assert_eq!(b.origin.y, px(100. + 2000. * 1.25 - 1875.));
        assert_eq!(b.size, size(px(32.5), px(32.5)));
        // Scrolled past, it is not there to press.
        let above = RectCss { y: 100.0, ..button };
        assert!(place(above, &map).is_none());
    }

    #[test]
    fn tile_edges_land_on_whole_device_pixels() {
        for s in [1.0f32, 1.25, 1.6, 2.0] {
            for origin in [0.0f32, 37.3, 211.55] {
                for scroll_dev in [0u32, 1, 3001, 20_479] {
                    let off = snap_offset(origin, s);
                    for k in 0..13 {
                        let top = off + tile_top(k * TILE_DEV, scroll_dev, s, 1.0);
                        let dev = f64::from(origin + top) * f64::from(s);
                        assert!(
                            (dev - dev.round()).abs() < 0.01,
                            "scale {s}, origin {origin}, scroll {scroll_dev}, band {k}: {dev}"
                        );
                    }
                }
            }
        }
    }

    /// A press is found where the page drew the thing pressed: through the
    /// same scroll and scale as the tiles, a dialog's own buttons before
    /// anything under them, and never on something not laid out.
    #[test]
    fn a_press_on_a_dialog_button_opens_its_dialog_and_one_on_a_link_follows_it() {
        let rect = |x, y, w, h| Some(RectCss { x, y, w, h });
        let openers = vec![Opener {
            dialog: "d-evidence".into(),
            rect: rect(20.0, 3000.0, 90.0, 28.0),
            inside: None,
        }];
        let link = |href: &str, r| Link {
            href: href.into(),
            resolved: format!("file:///r/brief.html{href}"),
            rect: r,
            dialog: None,
            fragment_top_css: None,
        };
        let links = vec![
            link("#sec-2", rect(20.0, 3100.0, 120.0, 18.0)),
            // Inside a closed dialog: known, keyed, and not a place.
            link("#inside", None),
        ];
        let view = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(600.), px(800.)),
        };
        let scrolled = PageToView {
            origin: point(px(0.), px(0.)),
            px_per_css: 1.0,
            scroll: px(2900.),
            clip: view,
        };
        let at = |x: f32, y: f32| point(px(x), px(y));
        assert_eq!(
            hit_at(&openers, &links, &[], &scrolled, at(30., 110.)),
            Some(PageHit::Opener("d-evidence".into()))
        );
        assert_eq!(
            hit_at(&openers, &links, &[], &scrolled, at(30., 205.)),
            Some(PageHit::Link(links[0].clone()))
        );
        assert_eq!(
            hit_at(&openers, &links, &[], &scrolled, at(300., 400.)),
            None
        );
        assert_eq!(hit_at(&openers, &links, &[], &scrolled, at(0., 0.)), None);
        // At the top of the page the button is 3,000 px below the view: a
        // press where it would be at scroll 0 finds nothing.
        let top = PageToView {
            scroll: px(0.),
            ..scrolled
        };
        assert_eq!(hit_at(&openers, &links, &[], &top, at(30., 110.)), None);
        // In an open dialog, drawn at 0.8 px per CSS px from (40, 60): its
        // close button wins over a dialog button under the same point.
        let dialog = PageToView {
            origin: point(px(40.), px(60.)),
            px_per_css: 0.8,
            scroll: px(0.),
            clip: view,
        };
        let closer = RectCss {
            x: 500.0,
            y: 8.0,
            w: 24.0,
            h: 24.0,
        };
        let under = vec![Opener {
            dialog: "d-books".into(),
            rect: Some(closer),
            inside: Some("d-evidence".into()),
        }];
        assert_eq!(
            hit_at(&under, &[], &[closer], &dialog, at(445., 70.)),
            Some(PageHit::Closer)
        );
        assert_eq!(
            hit_at(&under, &[], &[], &dialog, at(445., 70.)),
            Some(PageHit::Opener("d-books".into()))
        );
    }
}
