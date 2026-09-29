//! The PDF backend: every page in one column, drawn by poppler a tile at a
//! time ([`super::poppler`]), scrolled and zoomed by TD.
//!
//! # One column
//!
//! Pages stand one under another, centred, with a gap between them, the way a
//! reader's "fit width, continuous" mode shows them. Every page is scaled by
//! the same amount — the amount that fits the widest page to the view at 100% —
//! so a small page stays smaller than its neighbours, as it is in the file.
//! The zoom steps the reading ladder a page and a Markdown document step
//! ([`super::READING_ZOOM`]), so the strip, ctrl+wheel and the Document face's
//! `0` `1` `+` `-` do here what they do there. Zoomed past the view's width,
//! the column pans sideways, by the wheel's sideways turn, the arrows, or a
//! drag.
//!
//! # Tiles, drawn where the view is looking
//!
//! A page is drawn at the size it is shown, in device pixels, so text is as
//! sharp as the screen. At 300% on a wide HiDPI pane that is over 7,000
//! pixels across, more than a GPU texture can be, so a page is cut into tiles
//! of at most [`TILE`] device pixels a side and only the tiles that meet the
//! view, or lie within half a view above or below it, are drawn. poppler draws
//! a region in about the time its area costs, so a tile costs what it shows.
//! Tiles are fetched nearest the middle of the view first, [`AT_ONCE`] at a
//! time, each in a child process of its own.
//!
//! A zoom or a resize makes every tile the wrong size. They stay up, drawn
//! stretched to where their page now is, soft for a moment, until the new
//! tiles for that page have all landed; then they are given back.
//!
//! # Every texture is given back
//!
//! A tile is an `Arc<RenderImage>` gpui never frees on its own, as a page's
//! tiles are. One that leaves the view's reach is handed to the page
//! backend's [`give_back`], deferred to the end of the effect cycle; closing
//! the square gives back every one. A tile that lands for a region nobody
//! wants any more was never painted, so it was never in an atlas, and it is
//! simply dropped. A render nobody wants any more is stopped: its
//! [`Wanted`] drops, and poppler is killed mid-page.
//!
//! # The file on disk
//!
//! A PDF rewritten while it is open — an agent regenerating an invoice — is
//! read again, and the reader stays on the page they were reading. Until the
//! new pages land, the old ones stay up.

use std::any::Any;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use gpui::{
    div, img, prelude::*, px, AnyElement, App, Context, ImageSource, ObjectFit, Pixels, Point,
    RenderImage, ScrollDelta, Size, Task, Window,
};

use super::backend::{Backend, Drawn};
use super::image::{ImageZoom, ZoomStep};
use super::page::{give_back, snap_offset, Measured, Wanted};
use super::poppler::{self, PageBox, Picture, Refused, Region, Tools};
use super::progress::{Reading, GUTTER};
use super::{DocumentView, FileStamp};
use crate::docopen::DocScroll;
use crate::theme::Theme;

/// Around the column, in logical pixels.
pub const PAD: f32 = 12.0;
/// Between one page and the next.
pub const GAP: f32 = 10.0;
/// A tile's longest side, in device pixels: the page backend's band height,
/// and well inside the smallest texture limit a GPU TD runs on has.
pub const TILE: u32 = 2048;
/// How far past the view, above and below, tiles are drawn ahead, as a
/// fraction of the view's height.
pub const AHEAD: f32 = 0.5;
/// Tiles being drawn at once.
pub const AT_ONCE: usize = 3;
/// Tiles wanted at once, at most. The view and half a view either side at
/// 50% on a tall screen is under twenty; this bounds a pathological column
/// rather than an ordinary one.
pub const MOST_TILES: usize = 32;

/// A box in logical pixels.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The pages laid out one under another, in the column's own logical pixels.
#[derive(Clone, PartialEq, Debug)]
pub struct Column {
    /// Logical pixels per point.
    pub k: f32,
    /// Each page's box, from the column's top-left.
    pub pages: Vec<Rect>,
    pub width: f32,
    pub height: f32,
}

/// Pure. The column for a view `view_w` wide at `zoom`: every page scaled so
/// the widest fills the view less its padding at 100%, each centred.
pub fn column(pages: &[PageBox], view_w: f32, zoom: f32) -> Column {
    let widest = pages
        .iter()
        .map(|p| p.shown().0)
        .fold(0.0f32, f32::max)
        .max(1.0);
    let inner = (view_w - 2.0 * PAD).max(1.0) * zoom.max(0.01);
    let k = inner / widest;
    let width = inner + 2.0 * PAD;
    let mut y = PAD;
    let mut rects = Vec::with_capacity(pages.len());
    for p in pages {
        let (w, h) = p.shown();
        let (w, h) = (w * k, h * k);
        rects.push(Rect {
            x: (width - w) / 2.0,
            y,
            w,
            h,
        });
        y += h + GAP;
    }
    let height = if rects.is_empty() {
        2.0 * PAD
    } else {
        y - GAP + PAD
    };
    Column {
        k,
        pages: rects,
        width,
        height,
    }
}

/// Pure. A scroll kept inside what there is to see: never past either end,
/// and zero when everything fits.
pub fn clamp_scroll(at: f32, content: f32, view: f32) -> f32 {
    at.min(content - view).max(0.0)
}

/// Pure. Where the column's left edge sits in the view: centred when it is
/// narrower than the view, else moved by the pan.
pub fn column_x(col_w: f32, view_w: f32, left: f32) -> f32 {
    if col_w <= view_w {
        (view_w - col_w) / 2.0
    } else {
        -clamp_scroll(left, col_w, view_w)
    }
}

/// Pure. The page at height `y` in the column: the one whose box, with half
/// the gap below it, holds `y`. Above the first is the first and below the
/// last is the last. `None` for a column with no pages.
pub fn page_at(col: &Column, y: f32) -> Option<usize> {
    let n = col.pages.len();
    if n == 0 {
        return None;
    }
    Some(
        col.pages
            .partition_point(|r| r.y + r.h + GAP / 2.0 <= y)
            .min(n - 1),
    )
}

/// Pure. Where `top` is, as a page and how far down that page, so the same
/// place can be found in a column laid out at another width or zoom.
pub fn anchor(col: &Column, top: f32) -> Option<(usize, f32)> {
    let i = page_at(col, top)?;
    let r = col.pages[i];
    Some((i, (top - r.y) / r.h.max(1e-3)))
}

/// Pure. The height in `col` of a place [`anchor`] found, on the same page
/// or, when the column has fewer pages now, on its last.
pub fn top_of(col: &Column, (page, down): (usize, f32)) -> f32 {
    match col.pages.get(page.min(col.pages.len().saturating_sub(1))) {
        Some(r) => r.y + down * r.h,
        None => 0.0,
    }
}

/// Pure. A page's size in device pixels, drawn at scale `sf`.
pub fn page_dev(r: &Rect, sf: f32) -> (u32, u32) {
    (
        (r.w * sf).round().max(1.0) as u32,
        (r.h * sf).round().max(1.0) as u32,
    )
}

/// Pure. The regions to draw for a view `view` logical pixels across and
/// down, scrolled to `top` and panned to `left`, at scale `sf`: every tile
/// that meets the view or lies within [`AHEAD`] of it, those in the view
/// first, each group nearest the middle first. At most [`MOST_TILES`].
pub fn wanted(col: &Column, view: (f32, f32), top: f32, left: f32, sf: f32) -> Vec<Region> {
    let (vw, vh) = view;
    let sf = sf.max(0.1);
    let cx = column_x(col.width, vw, left);
    // The view, and the reach around it, in the column's own pixels.
    let (x0, x1) = (-cx, -cx + vw);
    let (seen0, seen1) = (top, top + vh);
    let (y0, y1) = (top - AHEAD * vh, top + vh + AHEAD * vh);
    let middle = (x0 + vw / 2.0, top + vh / 2.0);
    let first = col.pages.partition_point(|r| r.y + r.h <= y0);
    let mut out: Vec<(bool, f32, Region)> = Vec::new();
    for (page, r) in col.pages.iter().enumerate().skip(first) {
        if r.y >= y1 {
            break;
        }
        let full = page_dev(r, sf);
        let (per_x, per_y) = (r.w / full.0 as f32, r.h / full.1 as f32);
        // The part of the page within reach, in the page's device pixels.
        let lx0 = ((x0 - r.x) / per_x).floor().max(0.0) as u32;
        let lx1 = ((x1 - r.x) / per_x).ceil().min(full.0 as f32).max(0.0) as u32;
        let ly0 = ((y0 - r.y) / per_y).floor().max(0.0) as u32;
        let ly1 = ((y1 - r.y) / per_y).ceil().min(full.1 as f32).max(0.0) as u32;
        if lx1 <= lx0 || ly1 <= ly0 {
            continue;
        }
        for ty in ly0 / TILE..ly1.div_ceil(TILE) {
            for tx in lx0 / TILE..lx1.div_ceil(TILE) {
                let (x, y) = (tx * TILE, ty * TILE);
                let region = Region {
                    page,
                    full,
                    x,
                    y,
                    w: TILE.min(full.0 - x),
                    h: TILE.min(full.1 - y),
                };
                // Where the tile is, in the column.
                let (tx0, ty0) = (r.x + x as f32 * per_x, r.y + y as f32 * per_y);
                let (tx1, ty1) = (tx0 + region.w as f32 * per_x, ty0 + region.h as f32 * per_y);
                let seen = ty1 > seen0 && ty0 < seen1 && tx1 > x0 && tx0 < x1;
                let d = ((tx0 + tx1) / 2.0 - middle.0).hypot((ty0 + ty1) / 2.0 - middle.1);
                out.push((seen, d, region));
            }
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.total_cmp(&b.1)));
    out.truncate(MOST_TILES);
    out.into_iter().map(|(_, _, r)| r).collect()
}

/// Pure. The page a `#page=N` fragment names, counted from 0, as RFC 8118
/// reads it: the first `page=` among the fragment's `&`-separated parts.
/// `None` for anything else, which leaves the column where it is.
pub fn fragment_page(fragment: &str) -> Option<usize> {
    fragment
        .split('&')
        .find_map(|part| part.strip_prefix("page="))
        .and_then(|n| n.trim().parse::<usize>().ok())
        .filter(|n| *n >= 1)
        .map(|n| n - 1)
}

/// Which render of which region a tile is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct TileKey {
    /// The reading of the file it was drawn from.
    generation: u64,
    region: Region,
}

struct Tile {
    picture: Arc<RenderImage>,
    width: u32,
    height: u32,
}

/// A tile being drawn: the task that will bring it back, and the flag that
/// stops poppler when this is dropped.
struct Fetch {
    task: Task<()>,
    _wanted: Wanted,
}

enum Pages {
    /// `pdfinfo` is reading the file.
    Reading,
    Ready(Arc<Vec<PageBox>>),
    Failed(String),
}

pub struct PdfDoc {
    path: PathBuf,
    tools: Result<Tools, poppler::Missing>,
    pages: Pages,
    /// Counts the readings of the file. A tile drawn from an earlier one is
    /// kept only to cover its page until this one's tiles land.
    generation: u64,
    zoom: f32,
    /// The column, and the view width and zoom it was laid out for.
    laid: Option<(Column, f32, f32)>,
    /// Logical pixels from the column's top to the view's.
    top: f32,
    /// Logical pixels the column is panned left, when it is wider than the
    /// view.
    left: f32,
    /// Where to go once the column is laid out: a place the saved layout
    /// kept, a page a link named, or where the reader was before the file
    /// was read again.
    land: Option<Land>,
    /// The view's size and scale as last painted.
    frame: Option<(Size<Pixels>, f32)>,
    tiles: BTreeMap<TileKey, Tile>,
    fetching: BTreeMap<TileKey, Fetch>,
    /// Pages poppler would not draw, and why, for this reading.
    broken: BTreeMap<usize, String>,
    /// Where a held press started, and the scroll then.
    held: Option<(Point<Pixels>, f32, f32)>,
    wake: futures::channel::mpsc::UnboundedSender<()>,
    /// A wake is on its way: the paint that sent it need not send another.
    woken: bool,
    reading: Option<(Task<()>, Wanted)>,
    drawn: bool,
    _pump: Task<()>,
}

#[derive(Clone, Copy, Debug)]
enum Land {
    Fraction(f32),
    Page(usize),
    Anchor((usize, f32)),
}

impl PdfDoc {
    /// Find poppler and start listening for wakes; the file is read once the
    /// view is made (see [`Backend::opened`]).
    pub fn open(path: &Path, cx: &mut Context<DocumentView>) -> Self {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<()>();
        let pump = cx.spawn(async move |this, cx| {
            while rx.next().await.is_some() {
                // Wakes sent while this one waited are the same wake.
                while rx.try_recv().is_ok() {}
                let alive = this.update(cx, |view, cx| {
                    let moved = view
                        .backend
                        .downcast_mut::<PdfDoc>()
                        .is_some_and(|doc| doc.pump(cx));
                    if moved {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            path: path.to_path_buf(),
            tools: super::pdf_tools(cx),
            pages: Pages::Reading,
            generation: 0,
            zoom: 1.0,
            laid: None,
            top: 0.0,
            left: 0.0,
            land: None,
            frame: None,
            tiles: BTreeMap::new(),
            fetching: BTreeMap::new(),
            broken: BTreeMap::new(),
            held: None,
            wake: tx,
            woken: false,
            reading: None,
            drawn: false,
            _pump: pump,
        }
    }

    fn wake(&mut self) {
        if !self.woken {
            self.woken = true;
            let _ = self.wake.unbounded_send(());
        }
    }

    /// Read the file's pages, as a new generation. What was drawn from the
    /// last reading stays up until this one's tiles cover it.
    fn read(&mut self, cx: &mut Context<DocumentView>) {
        let Ok(tools) = self.tools.clone() else {
            return;
        };
        self.generation += 1;
        // Tiles still being drawn from the last reading are not wanted.
        self.fetching.clear();
        let generation = self.generation;
        let path = self.path.clone();
        let wanted = Wanted::new();
        let still = wanted.flag();
        let task = cx.spawn(async move |this, cx| {
            let got = cx
                .background_executor()
                .spawn(async move { poppler::info(&tools, &path, &still) })
                .await;
            let _ = this.update(cx, |view, cx| {
                if let Some(doc) = view.backend.downcast_mut::<PdfDoc>() {
                    doc.read_landed(generation, got, cx);
                }
                cx.notify();
            });
        });
        self.reading = Some((task, wanted));
    }

    fn read_landed(
        &mut self,
        generation: u64,
        got: Result<Vec<PageBox>, Refused>,
        cx: &mut Context<DocumentView>,
    ) {
        if generation != self.generation {
            return;
        }
        // Detached, not dropped: this runs inside that task, which is ending.
        if let Some((task, _)) = self.reading.take() {
            task.detach();
        }
        match got {
            Ok(pages) => {
                // The reader stays where they were, unless something else
                // already asked for a place.
                if self.land.is_none() {
                    if let Some((col, ..)) = &self.laid {
                        self.land = anchor(col, self.top).map(Land::Anchor);
                    }
                }
                self.laid = None;
                self.broken.clear();
                if std::env::var_os("TD_DOCDEBUG").is_some() {
                    eprintln!(
                        "[doc] pdf read {}: {} pages",
                        self.path.display(),
                        pages.len()
                    );
                }
                self.pages = Pages::Ready(Arc::new(pages));
                self.pump(cx);
            }
            // A re-read that fails leaves the pages already up where they are;
            // only a first reading has nothing better to show than why.
            Err(why) => {
                if !matches!(self.pages, Pages::Ready(_)) {
                    self.pages = Pages::Failed(why.sentence());
                }
                if std::env::var_os("TD_DOCDEBUG").is_some() {
                    eprintln!("[doc] {}: {}", self.path.display(), why.sentence());
                }
            }
        }
    }

    /// Lay the column out for the view as it is now, keeping the reader's
    /// place across a change of width or zoom. Answers whether it moved.
    fn lay_out(&mut self) -> bool {
        let (Pages::Ready(pages), Some((size, _))) = (&self.pages, self.frame) else {
            return false;
        };
        let (vw, vh) = (f32::from(size.width), f32::from(size.height));
        let fresh = match &self.laid {
            Some((_, w, z)) => (*w - vw).abs() > 0.5 || (*z - self.zoom).abs() > 1e-4,
            None => true,
        };
        if fresh {
            // The place at the view's top, and the point of the column under
            // the view's middle, as a fraction of its width.
            let before = self.laid.as_ref().and_then(|(col, ..)| {
                let centre = (vw / 2.0 - column_x(col.width, vw, self.left)) / col.width.max(1.0);
                anchor(col, self.top).map(|a| (a, centre))
            });
            let col = column(pages, vw, self.zoom);
            if let Some((a, centre)) = before {
                self.top = top_of(&col, a);
                self.left = centre * col.width - vw / 2.0;
            }
            self.laid = Some((col, vw, self.zoom));
        }
        let Some((col, ..)) = &self.laid else {
            return false;
        };
        if let Some(land) = self.land.take() {
            self.top = match land {
                Land::Fraction(f) => f.clamp(0.0, 1.0) * col.height,
                Land::Page(p) => top_of(col, (p, 0.0)) - PAD,
                Land::Anchor(a) => top_of(col, a),
            };
        }
        let (top, left) = (
            clamp_scroll(self.top, col.height, vh),
            clamp_scroll(self.left, col.width, vw),
        );
        let moved = fresh || top != self.top || left != self.left;
        self.top = top;
        self.left = left;
        moved
    }

    /// The tiles the view wants now, for this reading.
    fn wanted_now(&self) -> Vec<TileKey> {
        let (Some((col, ..)), Some((size, sf))) = (&self.laid, self.frame) else {
            return Vec::new();
        };
        let view = (f32::from(size.width), f32::from(size.height));
        wanted(col, view, self.top, self.left, sf)
            .into_iter()
            .filter(|r| !self.broken.contains_key(&r.page))
            .map(|region| TileKey {
                generation: self.generation,
                region,
            })
            .collect()
    }

    /// Bring what is drawn up to date with where the view is: give back the
    /// tiles it no longer reaches, stop drawing what it no longer wants, and
    /// start drawing what it wants and has not got. Answers whether anything
    /// on screen changed.
    fn pump(&mut self, cx: &mut Context<DocumentView>) -> bool {
        self.woken = false;
        let mut changed = self.lay_out();
        let (Ok(tools), Pages::Ready(pages)) = (&self.tools, &self.pages) else {
            return changed;
        };
        let (tools, pages) = (tools.clone(), pages.clone());
        let wanted = self.wanted_now();
        let want: BTreeSet<TileKey> = wanted.iter().copied().collect();
        // A page's tiles from another size or reading cover it only until
        // every tile of it that is in the view now is in.
        let seen = self.seen_now();
        let uncovered: BTreeSet<usize> = seen
            .iter()
            .filter(|k| !self.tiles.contains_key(k))
            .map(|k| k.region.page)
            .collect();
        let seen_pages: BTreeSet<usize> = seen.iter().map(|k| k.region.page).collect();
        let n = pages.len();
        let gone: Vec<TileKey> = self
            .tiles
            .keys()
            .filter(|k| {
                if want.contains(k) {
                    return false;
                }
                let current = k.generation == self.generation
                    && self.full_of(k.region.page) == Some(k.region.full);
                let p = k.region.page;
                current || p >= n || !seen_pages.contains(&p) || !uncovered.contains(&p)
            })
            .copied()
            .collect();
        if !gone.is_empty() {
            let pictures = gone
                .iter()
                .filter_map(|k| self.tiles.remove(k))
                .map(|t| t.picture)
                .collect();
            give_back(pictures, cx);
            changed = true;
            if std::env::var_os("TD_DOCDEBUG").is_some() {
                eprintln!(
                    "[doc] pdf gave back {} tiles, {} on the GPU",
                    gone.len(),
                    self.tiles.len()
                );
            }
        }
        // Stop drawing what the view has moved away from.
        self.fetching.retain(|k, _| want.contains(k));
        for key in wanted {
            if self.fetching.len() >= AT_ONCE {
                break;
            }
            if self.tiles.contains_key(&key) || self.fetching.contains_key(&key) {
                continue;
            }
            let rot = pages[key.region.page].rot;
            let fetch = self.fetch(key, rot, tools.clone(), cx);
            self.fetching.insert(key, fetch);
        }
        changed
    }

    /// The tiles of this reading, at this size, that meet the view itself.
    fn seen_now(&self) -> Vec<TileKey> {
        let (Some((col, ..)), Some((size, _))) = (&self.laid, self.frame) else {
            return Vec::new();
        };
        let (vw, vh) = (f32::from(size.width), f32::from(size.height));
        let cx = column_x(col.width, vw, self.left);
        self.wanted_now()
            .into_iter()
            .filter(|k| {
                let r = col.pages[k.region.page];
                let (fw, fh) = k.region.full;
                let (px_x, px_y) = (r.w / fw as f32, r.h / fh as f32);
                let y0 = r.y + k.region.y as f32 * px_y - self.top;
                let y1 = y0 + k.region.h as f32 * px_y;
                let x0 = cx + r.x + k.region.x as f32 * px_x;
                let x1 = x0 + k.region.w as f32 * px_x;
                y1 > 0.0 && y0 < vh && x1 > 0.0 && x0 < vw
            })
            .collect()
    }

    /// The size page `page` is drawn at now, in device pixels.
    fn full_of(&self, page: usize) -> Option<(u32, u32)> {
        let (col, ..) = self.laid.as_ref()?;
        let (_, sf) = self.frame?;
        Some(page_dev(col.pages.get(page)?, sf))
    }

    fn fetch(&self, key: TileKey, rot: u16, tools: Tools, cx: &mut Context<DocumentView>) -> Fetch {
        let wanted = Wanted::new();
        let still = wanted.flag();
        let path = self.path.clone();
        let task = cx.spawn(async move |this, cx| {
            let started = std::time::Instant::now();
            let got = cx
                .background_executor()
                .spawn(async move { poppler::render(&tools, &path, &key.region, rot, &still) })
                .await;
            let took = started.elapsed();
            let _ = this.update(cx, |view, cx| {
                let landed = view
                    .backend
                    .downcast_mut::<PdfDoc>()
                    .is_some_and(|doc| doc.landed(key, got, took, cx));
                if landed {
                    cx.notify();
                }
            });
        });
        Fetch {
            task,
            _wanted: wanted,
        }
    }

    /// A tile came back. Answers whether it is drawn.
    fn landed(
        &mut self,
        key: TileKey,
        got: Result<Picture, Refused>,
        took: std::time::Duration,
        cx: &mut Context<DocumentView>,
    ) -> bool {
        // Detached, not dropped: this runs inside that task, which is ending.
        // A tile nobody wants any more was never painted, so it is only
        // dropped.
        match self.fetching.remove(&key) {
            Some(fetch) => fetch.task.detach(),
            None => return false,
        }
        if key.generation != self.generation {
            return false;
        }
        match got {
            Ok(p) => {
                if std::env::var_os("TD_DOCDEBUG").is_some() {
                    eprintln!(
                        "[doc] pdf page {} tile {},{} {}x{} of {}x{} in {} ms",
                        key.region.page + 1,
                        key.region.x,
                        key.region.y,
                        p.width,
                        p.height,
                        key.region.full.0,
                        key.region.full.1,
                        took.as_millis()
                    );
                }
                let Some(buf) = ::image::RgbaImage::from_raw(p.width, p.height, p.bgra) else {
                    return false;
                };
                let picture = Arc::new(RenderImage::new(vec![::image::Frame::new(buf)]));
                self.tiles.insert(
                    key,
                    Tile {
                        picture,
                        width: p.width,
                        height: p.height,
                    },
                );
            }
            Err(why) => {
                self.broken.insert(key.region.page, why.sentence());
            }
        }
        self.pump(cx);
        true
    }

    /// The page count, `None` until the file has been read, and the tiles on
    /// the GPU: what a pane test waits on.
    #[cfg(test)]
    pub(crate) fn shown(&self) -> (Option<usize>, usize) {
        let pages = match &self.pages {
            Pages::Ready(p) => Some(p.len()),
            _ => None,
        };
        (pages, self.tiles.len())
    }

    fn note(s: String, th: &Theme) -> AnyElement {
        div()
            .p(px(14.))
            .text_color(th.text.alpha(0.75))
            .child(s)
            .into_any_element()
    }
}

impl Backend for PdfDoc {
    /// The pages in view: each page's outline first, then the tiles of an
    /// older size or reading still covering it, then this one's, and the page
    /// counter over all of them.
    fn element(&mut self, view: &Drawn, _window: &mut Window, th: &Theme) -> AnyElement {
        if let Err(why) = &self.tools {
            return Self::note(why.reason(), th);
        }
        match &self.pages {
            Pages::Reading => return Self::note("opening…".into(), th),
            Pages::Failed(why) => return Self::note(why.clone(), th),
            Pages::Ready(_) => {}
        }
        let Some((size, sf)) = view.frame else {
            return Self::note("opening…".into(), th);
        };
        if self.frame != view.frame {
            self.frame = view.frame;
            self.wake();
        }
        self.lay_out();
        let Some((col, ..)) = &self.laid else {
            return Self::note("opening…".into(), th);
        };
        let (vw, vh) = (f32::from(size.width), f32::from(size.height));
        let cx = column_x(col.width, vw, self.left);
        // Whole device pixels against the view's origin, as a page's bands
        // are, so the tiles of a page meet without a blurred seam. Unplaced,
        // for the one frame before the first paint measures the origin.
        let (off_x, off_y) = view.placed.map_or((0.0, 0.0), |o| {
            (snap_offset(o.x.into(), sf), snap_offset(o.y.into(), sf))
        });
        let snap = |v: f32| (v * sf).round() / sf.max(0.1);
        let first = col.pages.partition_point(|r| r.y + r.h <= self.top);
        let mut outlines = Vec::new();
        let mut shown = BTreeMap::new();
        for (page, r) in col.pages.iter().enumerate().skip(first) {
            if r.y - self.top >= vh {
                break;
            }
            let (x, y) = (snap(cx + r.x) + off_x, snap(r.y - self.top) + off_y);
            shown.insert(page, (x, y, *r));
            let mut outline = div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(r.w))
                .h(px(r.h))
                .bg(th.text.alpha(0.06))
                .border_1()
                .border_color(th.text.alpha(0.12));
            if let Some(why) = self.broken.get(&page) {
                outline = outline
                    .p(px(14.))
                    .text_size(px(12.))
                    .text_color(th.text.alpha(0.75))
                    .child(format!("Page {}: {why}", page + 1));
            }
            outlines.push(outline.into_any_element());
        }
        let (mut under, mut over) = (Vec::new(), Vec::new());
        for (key, tile) in &self.tiles {
            let Some(&(x, y, r)) = shown.get(&key.region.page) else {
                continue;
            };
            let (fw, fh) = key.region.full;
            let (per_x, per_y) = (r.w / fw as f32, r.h / fh as f32);
            let el = img(ImageSource::Render(tile.picture.clone()))
                .absolute()
                .left(px(x + key.region.x as f32 * per_x))
                .top(px(y + key.region.y as f32 * per_y))
                .w(px(tile.width as f32 * per_x))
                .h(px(tile.height as f32 * per_y))
                .object_fit(ObjectFit::Fill)
                .into_any_element();
            let current = key.generation == self.generation && page_dev(&r, sf) == key.region.full;
            if current {
                over.push(el);
            } else {
                under.push(el);
            }
        }
        let counter = page_at(col, self.top + vh / 2.0).map(|p| {
            div()
                .absolute()
                .right(px(GUTTER))
                .bottom(px(8.))
                .px(px(7.))
                .py(px(2.))
                .rounded(px(4.))
                .bg(th.bg.alpha(0.85))
                .border_1()
                .border_color(th.accent.alpha(0.4))
                .text_size(px(11.))
                .text_color(th.accent)
                .child(format!("{} / {}", p + 1, col.pages.len()))
                .into_any_element()
        });
        if !self.drawn && !over.is_empty() {
            self.drawn = true;
            if std::env::var_os("TD_DOCDEBUG").is_some() {
                if let Pages::Ready(pages) = &self.pages {
                    eprintln!(
                        "[doc] drew {} {} pages at {:.0}%",
                        view.path.display(),
                        pages.len(),
                        self.zoom * 100.0
                    );
                }
            }
        }
        // A tile the view shows that nobody is drawing yet: a drag or a
        // resize moved the view without a context to start one from.
        let missing = self
            .seen_now()
            .into_iter()
            .any(|k| !self.tiles.contains_key(&k) && !self.fetching.contains_key(&k));
        if missing {
            self.wake();
        }
        div()
            .absolute()
            .inset_0()
            .children(outlines)
            .children(under)
            .children(over)
            .children(counter)
            .into_any_element()
    }

    /// Every tile, and every render and reading still running for it.
    fn give_back(&mut self, path: &Path, cx: &mut App) {
        self.fetching.clear();
        self.reading = None;
        let n = self.tiles.len();
        for (_, tile) in std::mem::take(&mut self.tiles) {
            cx.drop_image(tile.picture, None);
        }
        if std::env::var_os("TD_DOCDEBUG").is_some() {
            eprintln!("[doc] released {} ({n} tiles)", path.display());
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// A press grabs the column, to move it by dragging, as on a picture.
    fn press(&mut self, at: Point<Pixels>, _view: &Drawn, _cx: &mut Context<DocumentView>) -> bool {
        if !matches!(self.pages, Pages::Ready(_)) {
            return false;
        }
        self.held = Some((at, self.top, self.left));
        true
    }

    fn drag(&mut self, at: Point<Pixels>, _view: Size<Pixels>, _sf: f32) -> bool {
        let Some((from, top, left)) = self.held else {
            return false;
        };
        self.top = top - f32::from(at.y - from.y);
        self.left = left - f32::from(at.x - from.x);
        self.lay_out();
        self.wake();
        true
    }

    fn end_press(&mut self) {
        self.held = None;
    }

    /// Scrolls the column, and pans it when it is wider than the view.
    fn wheel(
        &mut self,
        delta: ScrollDelta,
        line: f32,
        view: Size<Pixels>,
        sf: f32,
        cx: &mut Context<DocumentView>,
    ) -> bool {
        let (dx, dy) = match delta {
            ScrollDelta::Lines(l) => (l.x * line * 3.0, l.y * line * 3.0),
            ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
        };
        self.frame = Some((view, sf));
        self.top -= dy;
        self.left -= dx;
        self.pump(cx);
        true
    }

    /// Steps the reading ladder; the place at the top of the view stays
    /// there, and the middle of the column stays in the middle.
    fn zoom(
        &mut self,
        step: ZoomStep,
        view: Size<Pixels>,
        sf: f32,
        cx: &mut Context<DocumentView>,
    ) -> bool {
        let next = super::step_reading_zoom(self.zoom, step);
        if (next - self.zoom).abs() < 1e-3 {
            return false;
        }
        self.zoom = next;
        self.frame = Some((view, sf));
        self.pump(cx);
        true
    }

    fn zoom_now(&self) -> Option<ImageZoom> {
        Some(ImageZoom::Scale(self.zoom))
    }

    /// Laid out at it when the view is next measured, which a view handed
    /// back always is, keeping the place as any new size does: laid out now,
    /// it would be laid out at the reader's size and then again.
    fn zoom_back(&mut self, was: ImageZoom, cx: &mut Context<DocumentView>) -> bool {
        let ImageZoom::Scale(z) = was else {
            return false;
        };
        if (z - self.zoom).abs() < 1e-3 {
            return false;
        }
        self.zoom = z;
        cx.notify();
        true
    }

    fn scroll(&self) -> Option<DocScroll> {
        let (col, ..) = self.laid.as_ref()?;
        Some(DocScroll {
            top: (self.top / col.height.max(1.0)).clamp(0.0, 1.0),
        })
    }

    fn reading(&self) -> Option<Reading> {
        let (col, ..) = self.laid.as_ref()?;
        let (size, _) = self.frame?;
        Reading::of(self.top, f32::from(size.height), col.height)
    }

    fn restore_scroll(&mut self, at: DocScroll, cx: &mut Context<DocumentView>) {
        self.land = Some(Land::Fraction(at.top));
        self.pump(cx);
        cx.notify();
    }

    /// `#page=N` opens on that page, as a browser's PDF viewer does.
    fn show_fragment(
        &mut self,
        fragment: String,
        _view_h: Option<f32>,
        cx: &mut Context<DocumentView>,
    ) {
        if let Some(page) = fragment_page(&fragment) {
            self.land = Some(Land::Page(page));
            self.pump(cx);
            cx.notify();
        }
    }

    fn measured(&mut self, m: Measured, cx: &mut Context<DocumentView>) {
        self.frame = Some((m.size, m.scale));
        self.pump(cx);
    }

    fn follows_its_file(&self) -> bool {
        true
    }

    /// Stamped before the first reading starts, so a write that lands during
    /// it is seen by the watcher's next tick and read again.
    fn opened(&mut self, path: &Path, cx: &mut Context<DocumentView>) -> Option<FileStamp> {
        let stamp = FileStamp::of(path);
        self.read(cx);
        stamp
    }

    fn file_changed(&mut self, _path: &Path, _came_back: bool, cx: &mut Context<DocumentView>) {
        self.read(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letter() -> PageBox {
        PageBox {
            w: 612.0,
            h: 792.0,
            rot: 0,
        }
    }

    /// Every page scaled alike, so the widest fills the view less its
    /// padding at 100%, each centred, with a gap between them.
    #[test]
    fn the_column_fits_the_widest_page_and_scales_the_rest_alike() {
        let small = PageBox {
            w: 306.0,
            h: 396.0,
            rot: 0,
        };
        let turned = PageBox {
            w: 612.0,
            h: 792.0,
            rot: 90,
        };
        let col = column(&[letter(), small, turned], 624.0 + 2.0 * PAD, 1.0);
        // The turned page is the widest: 792 points across 624 pixels.
        assert!((col.k - 624.0 / 792.0).abs() < 1e-5);
        let [a, b, c] = [col.pages[0], col.pages[1], col.pages[2]];
        assert!((c.w - 624.0).abs() < 1e-3 && (c.x - PAD).abs() < 1e-3);
        assert!((b.w - a.w / 2.0).abs() < 1e-3, "half the size, still");
        assert!((a.x - (col.width - a.w) / 2.0).abs() < 1e-3, "centred");
        assert_eq!(a.y, PAD);
        assert!((b.y - (a.y + a.h + GAP)).abs() < 1e-3);
        assert!((col.height - (c.y + c.h + PAD)).abs() < 1e-3);
        let zoomed = column(&[letter()], 624.0 + 2.0 * PAD, 2.0);
        assert!((zoomed.pages[0].w - 1248.0).abs() < 1e-3);
        assert!((zoomed.width - (1248.0 + 2.0 * PAD)).abs() < 1e-3);
    }

    /// A column narrower than the view sits in its middle whatever the pan
    /// says; a wider one pans, and never past either edge.
    #[test]
    fn a_narrow_column_is_centred_and_a_wide_one_pans_within_its_edges() {
        assert_eq!(column_x(400.0, 600.0, 999.0), 100.0);
        assert_eq!(column_x(1000.0, 600.0, 100.0), -100.0);
        assert_eq!(column_x(1000.0, 600.0, 999.0), -400.0);
        assert_eq!(column_x(1000.0, 600.0, -5.0), 0.0);
        assert_eq!(clamp_scroll(-3.0, 100.0, 50.0), 0.0);
        assert_eq!(clamp_scroll(80.0, 100.0, 50.0), 50.0);
        assert_eq!(clamp_scroll(80.0, 40.0, 50.0), 0.0, "all of it fits");
    }

    /// The page counter's page: the one under a height, the gap below a page
    /// counting half to it; and a place found again at another zoom.
    #[test]
    fn a_place_is_a_page_and_how_far_down_it() {
        let col = column(&[letter(), letter(), letter()], 400.0, 1.0);
        let r = col.pages[1];
        assert_eq!(page_at(&col, 0.0), Some(0));
        assert_eq!(page_at(&col, r.y - GAP / 2.0 + 0.1), Some(1));
        assert_eq!(page_at(&col, r.y - GAP / 2.0 - 0.1), Some(0));
        assert_eq!(page_at(&col, 1e9), Some(2));
        assert_eq!(page_at(&column(&[], 400.0, 1.0), 10.0), None);
        let here = r.y + r.h * 0.25;
        let (p, down) = anchor(&col, here).unwrap();
        assert_eq!(p, 1);
        assert!((down - 0.25).abs() < 1e-4);
        let bigger = column(&[letter(), letter(), letter()], 400.0, 2.0);
        let there = top_of(&bigger, (p, down));
        let r2 = bigger.pages[1];
        assert!((there - (r2.y + r2.h * 0.25)).abs() < 1e-3);
        let fewer = column(&[letter()], 400.0, 1.0);
        let last = fewer.pages[0];
        assert!((top_of(&fewer, (5, 0.5)) - (last.y + last.h * 0.5)).abs() < 1e-3);
    }

    /// At reading size each page is one tile; the view's pages come first,
    /// and the pages within half a view are fetched after them, and nothing
    /// further off at all.
    #[test]
    fn the_view_wants_its_own_tiles_first_and_half_a_view_either_side() {
        let pages = vec![letter(); 20];
        let col = column(&pages, 800.0, 1.0);
        // A page is about 1,004 pixels tall here: the view shows most of
        // page index 5, and its reach runs 450 above and below.
        let view = (800.0, 900.0);
        let top = col.pages[5].y;
        let got = wanted(&col, view, top, 0.0, 1.0);
        let seen: Vec<usize> = got.iter().map(|r| r.page).collect();
        assert_eq!(seen[0], 5, "{seen:?}");
        assert!(seen.contains(&4) && seen.contains(&6), "{seen:?}");
        assert!(!seen.contains(&3) && !seen.contains(&8), "{seen:?}");
        for r in &got {
            assert_eq!(r.full, page_dev(&col.pages[r.page], 1.0));
            assert!(r.w <= TILE && r.h <= TILE);
        }
    }

    /// Zoomed in on a HiDPI view, a page is wider than a texture can be, so
    /// it is cut into tiles no side of which passes TILE, and only those the
    /// view reaches are asked for.
    #[test]
    fn a_page_too_big_for_one_texture_is_cut_into_tiles() {
        let col = column(&[letter()], 1500.0, 3.0);
        let sf = 2.0;
        let full = page_dev(&col.pages[0], sf);
        assert!(full.0 > TILE * 3, "{full:?}");
        let got = wanted(&col, (1500.0, 900.0), 0.0, 0.0, sf);
        assert!(!got.is_empty());
        for r in &got {
            assert!(r.w <= TILE && r.h <= TILE, "{r:?}");
            assert!(r.x + r.w <= full.0 && r.y + r.h <= full.1, "{r:?}");
            assert_eq!(r.x % TILE, 0);
        }
        // The view is 3,000 device pixels across at the left edge: two
        // columns of tiles, never the page's far side.
        assert!(got.iter().all(|r| r.x < 2 * TILE), "{got:?}");
        // Panned to the far side, the far side is what is wanted.
        let far = wanted(&col, (1500.0, 900.0), 0.0, 1e9, sf);
        assert!(far.iter().all(|r| r.x + r.w > full.0 - 3000));
        assert!(far.iter().any(|r| r.x + r.w == full.0));
    }

    /// However tall the view and small the zoom, the tiles wanted at once are
    /// bounded.
    #[test]
    fn the_tiles_wanted_at_once_are_bounded() {
        let tiny = PageBox {
            w: 612.0,
            h: 20.0,
            rot: 0,
        };
        let col = column(&vec![tiny; 500], 800.0, 0.5);
        let got = wanted(&col, (800.0, 5000.0), 0.0, 0.0, 2.0);
        assert_eq!(got.len(), MOST_TILES);
    }

    #[test]
    fn a_page_fragment_names_a_page_counted_from_one() {
        assert_eq!(fragment_page("page=3"), Some(2));
        assert_eq!(fragment_page("zoom=200&page=12"), Some(11));
        assert_eq!(fragment_page("page=0"), None);
        assert_eq!(fragment_page("page=x"), None);
        assert_eq!(fragment_page("section-2"), None);
    }
}
