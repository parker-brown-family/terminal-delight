//! Which printed paths open inside the pane, and where the floating square goes.
//!
//! # What this module decides
//!
//! A path in the scrollback can be Alt+clicked open in a floating square over
//! the pane that printed it. That needs two answers before any view exists:
//! *is this a document TD can draw*, and *where does the square go*. Both are
//! here, and neither needs a window to answer.
//!
//! **Drawable** means Markdown, HTML, or an image. The name decides for the
//! first two. An image has to prove it: a `.png` that is really a log file
//! would open as an empty square, so a raster name must also carry a raster
//! signature in its first bytes, and a file with no extension at all counts as
//! an image when it carries one (screenshot tools often drop the suffix).
//! SVG is text, so it has no signature to check and its name decides.
//!
//! **Where it goes** is a square beside the line that was clicked: right-aligned,
//! just below that line when it fits and above it when it does not, and never
//! outside the screen. It is measured in the pane's own logical pixels, relative
//! to the screen's top-left, so a resized pane carries it along.
//!
//! **What a modified click does** on a path or link is one table,
//! [`click_intent`], and the right-click menu's link rows are another,
//! [`link_menu`]. Moving the square by its strip and finding which of its
//! controls a pointer is on are here too ([`drag_to`], [`float_hit_at`]); the
//! pane un-bends the pointer before asking, so everything in this file is flat.
//!
//! # Why this module imports nothing
//!
//! Zero `use` statements, like `keylayer.rs`: it compiles standalone, so
//! `rustc --test app/src/docopen.rs` runs these tests in about a second rather
//! than a full `cargo test`. The one impure function, [`drawable_document`],
//! reads sixteen bytes and nothing else.

/// What kind of document a path is, as far as drawing it goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocKind {
    Markdown,
    Html,
    Image,
}

/// A document a click asked for: the file, and what it was recognised as.
///
/// Not `DocumentSource`: that name is already the FOCUS reader's trait in
/// `doc.rs`, which `pane.rs` imports by name, and a reader of `pane.rs` should
/// never have to guess which of the two a line means.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DocTarget {
    pub path: std::path::PathBuf,
    pub kind: DocKind,
}

/// By the file name alone, case-insensitively. Used where the file may not be
/// there to read, and as the first half of [`doc_kind`].
pub fn doc_kind_by_name(path: &std::path::Path) -> Option<DocKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => Some(DocKind::Markdown),
        "html" | "htm" => Some(DocKind::Html),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "svg" => Some(DocKind::Image),
        _ => None,
    }
}

/// The formats gpui decodes that announce themselves in their first bytes.
fn raster_signature(head: &[u8]) -> bool {
    head.starts_with(b"\x89PNG\r\n\x1a\n")
        || head.starts_with(b"\xff\xd8\xff")
        || head.starts_with(b"GIF87a")
        || head.starts_with(b"GIF89a")
        || (head.len() >= 12 && head.starts_with(b"RIFF") && &head[8..12] == b"WEBP")
        || head.starts_with(b"BM")
}

/// By the name, then checked against the first bytes of the file.
///
/// A raster name on bytes that are not a raster is not drawable. A name TD does
/// not know, on bytes that are a raster, is an image. SVG, Markdown and HTML are
/// by name only: they are text, and text has no signature worth trusting.
pub fn doc_kind(path: &std::path::Path, head: &[u8]) -> Option<DocKind> {
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    match doc_kind_by_name(path) {
        Some(DocKind::Image) if is_svg => Some(DocKind::Image),
        Some(DocKind::Image) => raster_signature(head).then_some(DocKind::Image),
        Some(kind) => Some(kind),
        None if path.extension().is_none() && raster_signature(head) => Some(DocKind::Image),
        None => None,
    }
}

/// The document at `path`, if it is a regular file TD can draw.
///
/// Reads the first sixteen bytes, which is every signature [`doc_kind`] checks.
/// A directory named `notes.md` is not a document, and neither is a file that
/// cannot be opened: the click then falls through to whatever it did before.
pub fn drawable_document(path: &std::path::Path) -> Option<DocTarget> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut head = [0u8; 16];
    let n = {
        let mut f = std::fs::File::open(path).ok()?;
        let mut filled = 0;
        while filled < head.len() {
            match std::io::Read::read(&mut f, &mut head[filled..]) {
                Ok(0) => break,
                Ok(k) => filled += k,
                Err(_) => return None,
            }
        }
        filled
    };
    let kind = doc_kind(path, &head[..n])?;
    Some(DocTarget {
        path: path.to_path_buf(),
        kind,
    })
}

/// Where a document is scrolled: the top edge of the view as a fraction of the
/// document's height, `0.0..=1.0`. A fraction rather than pixels, because it
/// survives the page reflowing at another width.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct DocScroll {
    pub top: f32,
}

/// What following a link out of a document does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkRoute {
    /// A file TD can draw: it takes the square's place, where the square is.
    Replace,
    /// A web or mail address, or a local file TD does not draw: the desktop.
    Desktop,
    /// Any other scheme. A document is somebody's text, and a click on it
    /// should not reach every URL handler the desktop has registered.
    Refuse,
}

/// Where a link a document emitted goes. `target` is an absolute path or a
/// URL; `drawable` is whether the path is a document TD can draw, which the
/// caller asks [`drawable_document`], the one step here that reads the disk.
pub fn link_route(target: &str, drawable: bool) -> LinkRoute {
    if target.starts_with('/') {
        return if drawable {
            LinkRoute::Replace
        } else {
            LinkRoute::Desktop
        };
    }
    let lower = target.to_ascii_lowercase();
    if ["http://", "https://", "mailto:"]
        .iter()
        .any(|s| lower.starts_with(s))
    {
        LinkRoute::Desktop
    } else {
        LinkRoute::Refuse
    }
}

/// Where a document view is sitting: in a floating square over the terminal
/// face, or filling a pane as its Document face. One view can move from the
/// first to the second — promotion hands the square's view to the new pane
/// rather than opening the file again — so the view is told which it is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DocSeat {
    Float,
    Face,
}

/// Who asked for a document to open beside the pane. It decides one thing:
/// what happens when the tab already holds four panes. A floating square
/// asking to become a split stays the square it is and says why; anything
/// else opens a square instead, so the document is on screen either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Asker {
    /// Ctrl+Alt+click on a path in the grid.
    Click,
    /// "Open beside" in the right-click menu.
    Menu,
    /// The floating square's "⇲ split", or a second Alt+click on its path.
    Float,
    /// `ctl doc beside`, the gesture without a pointer.
    Ctl,
}

/// Why a floating square is open when a split was asked for. Said in the
/// square's own strip: TD has no general toast, and the square is where the
/// person is already looking.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatNote {
    /// The tab already holds four panes, which is as many as a split may make.
    FourPanes,
}

/// Whether an Alt+click on `clicked` promotes the square already floating,
/// rather than opening another: it does when the square is showing that same
/// file. The second click is the "yes, I want this beside me" the first one
/// did not say.
pub fn promotes(floating: Option<&std::path::Path>, clicked: &std::path::Path) -> bool {
    floating == Some(clicked)
}

/// What a key does on a pane's Document face.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DocKey {
    /// `0`: fit the picture to the pane.
    Fit,
    /// `1`: one image pixel per device pixel.
    Actual,
    /// `+` (or `=`, the same key unshifted): one zoom step in.
    ZoomIn,
    /// `-`: one zoom step out.
    ZoomOut,
    /// An arrow: move the view by this many logical pixels, as a wheel turn
    /// would — positive moves the document right and down.
    Pan(f32, f32),
    /// PageUp (`-1`) or PageDown (`1`): most of a screen at a time.
    Page(i8),
}

/// How far one arrow press moves the document, in logical pixels: a wheel
/// notch's worth, so the arrows and the wheel travel alike.
pub const DOC_ARROW_STEP: f32 = 48.0;

/// The Document face's own keys. `None` for every other key, which the face
/// swallows all the same: the shell behind it is hidden, and typing into
/// something nobody can see is the bug this face exists not to have.
///
/// Only unmodified keys (Shift aside, which `+` needs on most layouts) are the
/// face's: a chord is somebody else's, and the window and the pane have
/// already been asked for theirs before a key gets this far.
pub fn doc_face_key(key: &str, alt: bool, control: bool, platform: bool) -> Option<DocKey> {
    if alt || control || platform {
        return None;
    }
    Some(match key {
        "0" => DocKey::Fit,
        "1" => DocKey::Actual,
        "+" | "=" => DocKey::ZoomIn,
        "-" => DocKey::ZoomOut,
        "left" => DocKey::Pan(DOC_ARROW_STEP, 0.0),
        "right" => DocKey::Pan(-DOC_ARROW_STEP, 0.0),
        "up" => DocKey::Pan(0.0, DOC_ARROW_STEP),
        "down" => DocKey::Pan(0.0, -DOC_ARROW_STEP),
        "pageup" => DocKey::Page(-1),
        "pagedown" => DocKey::Page(1),
        _ => return None,
    })
}

/// What an Alt+click on the pointer's line does.
#[derive(Clone, PartialEq, Debug)]
pub enum AltClick {
    /// Copy the rejoined command line, as Alt+click always has.
    Copy,
    /// Open the document under the pointer in a floating square.
    OpenHere(DocTarget),
}

/// Alt+click, decided.
///
/// A document under the pointer wins, on any line and on the alt screen too:
/// the pointer is on the path, so the path is what was meant. Otherwise a
/// command line copies, exactly as before, and never on the alt screen, where
/// rows are a canvas rather than flowed text. `None` leaves the click alone.
pub fn alt_click(
    on_alt_screen: bool,
    line_is_command: bool,
    doc: Option<DocTarget>,
) -> Option<AltClick> {
    if let Some(doc) = doc {
        return Some(AltClick::OpenHere(doc));
    }
    (line_is_command && !on_alt_screen).then_some(AltClick::Copy)
}

/// The modifier keys a click was made with, as the click table reads them.
///
/// Built from `gpui::Modifiers` where the click arrives, so this file keeps
/// its zero imports. `platform` is Super on Linux.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mods {
    pub alt: bool,
    pub control: bool,
    pub shift: bool,
    pub platform: bool,
}

/// What a left click on the grid does, before any selection starts.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickIntent {
    /// Open the document under the pointer in a floating square.
    OpenHere,
    /// Ctrl+Alt on a document: open it in a new pane to the right of this
    /// one, keeping focus where it was.
    OpenBeside,
    /// Copy the command line the Alt chip is framing.
    CopyChip,
    /// Show the file in the file manager with the item selected.
    Reveal,
    /// Hand the link or path to the desktop's opener.
    OpenWithDesktop,
    /// None of the above: the click starts a selection, as a plain click does.
    Pass,
}

/// Every modified left click on a path or link, as one table.
///
/// Read top to bottom; the first row that matches decides:
///
/// | held          | under the pointer          | does            |
/// |---------------|----------------------------|-----------------|
/// | ctrl+alt      | a document                 | OpenBeside      |
/// | alt           | a document                 | OpenHere        |
/// | alt           | an armed copy chip         | CopyChip        |
/// | super+ctrl    | a file on this disk        | Reveal          |
/// | shift         | a file on this disk        | Reveal          |
/// | shift or ctrl | any link                   | OpenWithDesktop |
/// | anything else |                            | Pass            |
///
/// Shift+click on a file used to open it, the same as Ctrl+click; it reveals
/// now, so the two modifiers stop meaning one thing. A web link has nothing on
/// disk to reveal, so Shift keeps opening it. Super keeps Alt out of the
/// document rows: Super+Ctrl was already reveal, and nothing should change
/// underneath a gesture somebody already knows.
pub fn click_intent(
    m: Mods,
    on_document: bool,
    on_link: bool,
    revealable: bool,
    chip_armed: bool,
) -> ClickIntent {
    if m.alt && on_document && !m.platform {
        return if m.control {
            ClickIntent::OpenBeside
        } else {
            ClickIntent::OpenHere
        };
    }
    if m.alt && chip_armed {
        return ClickIntent::CopyChip;
    }
    if m.platform && m.control && revealable {
        return ClickIntent::Reveal;
    }
    if m.shift && revealable {
        return ClickIntent::Reveal;
    }
    if (m.shift || m.control) && on_link {
        return ClickIntent::OpenWithDesktop;
    }
    ClickIntent::Pass
}

/// One row of the link half of the right-click menu.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkItem {
    OpenHere,
    OpenBeside,
    OpenWithDesktop,
    Reveal,
    CopyLink,
}

/// The link rows of the right-click menu, in the order they are drawn.
///
/// Only rows that do something: a document opens here or beside, anything
/// opens with the desktop, a file on this disk reveals, and every link can be
/// copied. Copy link is what Alt+click used to give on a command line that
/// was mostly a path; now that Alt+click opens the path, the copy lives here.
pub fn link_menu(is_document: bool, revealable: bool) -> Vec<LinkItem> {
    let mut items = Vec::with_capacity(5);
    if is_document {
        items.push(LinkItem::OpenHere);
        items.push(LinkItem::OpenBeside);
    }
    items.push(LinkItem::OpenWithDesktop);
    if revealable {
        items.push(LinkItem::Reveal);
    }
    items.push(LinkItem::CopyLink);
    items
}

/// The floating square: flat, in logical pixels, relative to the top-left of
/// the pane's screen (the area below the header).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl FloatRect {
    /// Whether a screen-relative point falls inside the square.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Gap between the square and the screen's edges.
const FLOAT_INSET: f32 = 12.0;
/// Gap between the square and the line it was opened from.
const FLOAT_GAP: f32 = 6.0;

/// Where a square first opens.
///
/// Its side is 56% of the screen's width, or 84% of its height when that is
/// smaller, so it never covers the whole pane. Right-aligned with an inset. Its
/// top sits just below the clicked line when the square fits there, otherwise
/// it ends just above the line, and either way it is clamped inside the screen;
/// on a pane too short for either, the clamp can put it over the line itself.
/// The 56% is the mockup's number, not a measurement.
pub fn float_home(screen_w: f32, screen_h: f32, line_top: f32, line_bottom: f32) -> FloatRect {
    let side = (0.56 * screen_w).min(0.84 * screen_h).max(0.0);
    let x = screen_w - FLOAT_INSET - side;
    let below = line_bottom + FLOAT_GAP;
    let y = if below + side <= screen_h - FLOAT_INSET {
        below
    } else {
        line_top - FLOAT_GAP - side
    };
    clamp_float(
        FloatRect {
            x,
            y,
            w: side,
            h: side,
        },
        screen_w,
        screen_h,
    )
}

/// Keep a square inside the screen, shrinking it first if the screen became
/// smaller than the square.
pub fn clamp_float(r: FloatRect, screen_w: f32, screen_h: f32) -> FloatRect {
    let w = r.w.min(screen_w).max(0.0);
    let h = r.h.min(screen_h).max(0.0);
    FloatRect {
        x: r.x.clamp(0.0, (screen_w - w).max(0.0)),
        y: r.y.clamp(0.0, (screen_h - h).max(0.0)),
        w,
        h,
    }
}

/// How far the pointer travels on the strip before a press becomes a drag.
/// The literal the pane drag uses, so the two feel the same under the hand.
pub const FLOAT_DRAG_ENGAGE: f32 = 6.0;

/// A press on the square's strip, on its way to becoming a move.
///
/// Every point is flat (un-bent through the tube's inverse) and in window
/// pixels. `origin` is where the square was when the press landed, so the
/// square follows the pointer's travel rather than accumulating per-move
/// rounding.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatDrag {
    pub start: (f32, f32),
    pub at: (f32, f32),
    pub origin: FloatRect,
    pub engaged: bool,
    /// `None` moves the square; `Some` resizes it by the edges named.
    pub edges: Option<Edges>,
}

impl FloatDrag {
    pub fn new(start: (f32, f32), origin: FloatRect) -> Self {
        Self {
            start,
            at: start,
            origin,
            engaged: false,
            edges: None,
        }
    }

    /// A press on an edge, on its way to becoming a resize.
    pub fn resize(start: (f32, f32), origin: FloatRect, edges: Edges) -> Self {
        Self {
            edges: Some(edges),
            ..Self::new(start, origin)
        }
    }
}

/// Which edges of the square a resize moves. The top edge is the strip, and
/// pressing the strip moves the square, so the top never resizes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub bottom: bool,
}

/// How far inside the square's border an edge can be grabbed, in logical
/// pixels: wide enough to find with a trackpad, narrow enough to leave the
/// document its clicks.
pub const FLOAT_GRIP: f32 = 7.0;
/// The smallest a resize leaves the square: the strip's controls still fit.
pub const FLOAT_MIN_W: f32 = 200.0;
pub const FLOAT_MIN_H: f32 = 120.0;

/// The edges a screen-relative point grabs: within [`FLOAT_GRIP`] of the
/// left, right or bottom border, inside the square. A bottom corner grabs two.
pub fn float_edge_at(r: FloatRect, x: f32, y: f32) -> Option<Edges> {
    if !r.contains(x, y) {
        return None;
    }
    let edges = Edges {
        left: x < r.x + FLOAT_GRIP,
        right: x >= r.x + r.w - FLOAT_GRIP,
        bottom: y >= r.y + r.h - FLOAT_GRIP,
    };
    (edges.left || edges.right || edges.bottom).then_some(edges)
}

/// Move a drag to `flat` and answer where the square is drawn now.
///
/// Nothing moves until the pointer has travelled more than
/// [`FLOAT_DRAG_ENGAGE`] from the press, so a click on the strip that wobbles
/// a pixel does not nudge the square. Once engaged it stays engaged, even if
/// the pointer comes back, and the square is clamped inside the screen: it
/// can be pushed against an edge but never through it.
pub fn drag_to(d: &mut FloatDrag, flat: (f32, f32), screen_w: f32, screen_h: f32) -> FloatRect {
    d.at = flat;
    let (dx, dy) = (flat.0 - d.start.0, flat.1 - d.start.1);
    if !d.engaged && (dx * dx + dy * dy).sqrt() > FLOAT_DRAG_ENGAGE {
        d.engaged = true;
    }
    if !d.engaged {
        return d.origin;
    }
    let Some(edges) = d.edges else {
        return clamp_float(
            FloatRect {
                x: d.origin.x + dx,
                y: d.origin.y + dy,
                ..d.origin
            },
            screen_w,
            screen_h,
        );
    };
    // A resize keeps the far edge where it was: the right edge moves alone,
    // and the left edge moves with the square's right side pinned. Never
    // smaller than the minimum, never past the screen.
    let o = d.origin;
    let mut r = o;
    if edges.right {
        r.w = (o.w + dx).min(screen_w - o.x).max(FLOAT_MIN_W);
    }
    if edges.left {
        let right_side = o.x + o.w;
        let x = (o.x + dx).min(right_side - FLOAT_MIN_W).max(0.0);
        r.x = x;
        r.w = right_side - x;
    }
    if edges.bottom {
        r.h = (o.h + dy).min(screen_h - o.y).max(FLOAT_MIN_H);
    }
    clamp_float(r, screen_w, screen_h)
}

/// What part of the floating square a flat point is on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FloatHit {
    /// The title strip, anywhere a button is not: pressing here moves it.
    Strip,
    /// "⇲ split": move the document into a pane of its own, beside this one.
    Split,
    /// "↗ desktop": open the file with the desktop's own application.
    Desktop,
    /// "✕ esc": close the square, as Escape does.
    Close,
    /// Zoom out one step.
    ZoomOut,
    /// Between fitting the square and one image pixel per device pixel.
    ZoomFit,
    /// Zoom in one step.
    ZoomIn,
    /// The document itself.
    Body,
    /// Within a grip of the left, right or bottom edge: pressing here
    /// resizes the square.
    Resize(Edges),
}

/// One region of the square as it was laid out, flat, in window pixels.
/// Recorded while the square paints, so a button's width follows its font
/// and nothing here guesses it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FloatZone {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub hit: FloatHit,
}

/// The topmost zone containing a flat point: the last recorded wins, because
/// zones are recorded in paint order and a button paints over its strip.
/// Left and top edges are inside, right and bottom outside, so two zones that
/// share an edge never both claim it.
pub fn float_hit_at(zones: &[FloatZone], x: f32, y: f32) -> Option<FloatZone> {
    zones
        .iter()
        .rev()
        .find(|z| x >= z.x && x < z.x + z.w && y >= z.y && y < z.y + z.h)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const JPEG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF\0";

    fn sq() -> FloatRect {
        FloatRect {
            x: 100.0,
            y: 50.0,
            w: 400.0,
            h: 300.0,
        }
    }

    /// Parker: "gotta be able to RESIZE the floating box". The left, right
    /// and bottom edges grab, a bottom corner grabs two, and the top is the
    /// strip, which moves the square rather than resizing it.
    #[test]
    fn a_float_edge_is_grabbed_within_its_grip() {
        let r = sq();
        let e = |x, y| float_edge_at(r, x, y);
        assert_eq!(
            e(102.0, 200.0),
            Some(Edges {
                left: true,
                right: false,
                bottom: false
            })
        );
        assert_eq!(
            e(498.0, 200.0),
            Some(Edges {
                left: false,
                right: true,
                bottom: false
            })
        );
        assert_eq!(
            e(300.0, 346.0),
            Some(Edges {
                left: false,
                right: false,
                bottom: true
            })
        );
        assert_eq!(
            e(497.0, 347.0),
            Some(Edges {
                left: false,
                right: true,
                bottom: true
            })
        );
        assert_eq!(e(300.0, 52.0), None, "the top is the strip");
        assert_eq!(e(300.0, 200.0), None, "the middle is the document");
        assert_eq!(e(90.0, 200.0), None, "outside the square grabs nothing");
    }

    #[test]
    fn a_float_resizes_from_the_edge_it_was_grabbed_by() {
        let (sw, sh) = (1000.0, 800.0);
        // The bottom-right corner, dragged out 150 by 80.
        let mut d = FloatDrag::resize(
            (498.0, 348.0),
            sq(),
            Edges {
                left: false,
                right: true,
                bottom: true,
            },
        );
        let r = drag_to(&mut d, (648.0, 428.0), sw, sh);
        assert_eq!((r.x, r.y, r.w, r.h), (100.0, 50.0, 550.0, 380.0));
        // The left edge, dragged right: the right side stays put.
        let mut d = FloatDrag::resize(
            (101.0, 200.0),
            sq(),
            Edges {
                left: true,
                right: false,
                bottom: false,
            },
        );
        let r = drag_to(&mut d, (151.0, 200.0), sw, sh);
        assert_eq!((r.x, r.w), (150.0, 350.0));
        assert_eq!(r.x + r.w, 500.0);
        // Never smaller than the minimum, from either side.
        let mut d = FloatDrag::resize(
            (101.0, 200.0),
            sq(),
            Edges {
                left: true,
                right: false,
                bottom: false,
            },
        );
        let r = drag_to(&mut d, (900.0, 200.0), sw, sh);
        assert_eq!((r.w, r.x + r.w), (FLOAT_MIN_W, 500.0));
        let mut d = FloatDrag::resize(
            (300.0, 348.0),
            sq(),
            Edges {
                left: false,
                right: false,
                bottom: true,
            },
        );
        assert_eq!(drag_to(&mut d, (300.0, 0.0), sw, sh).h, FLOAT_MIN_H);
        // Never past the screen.
        let mut d = FloatDrag::resize(
            (498.0, 348.0),
            sq(),
            Edges {
                left: false,
                right: true,
                bottom: true,
            },
        );
        let r = drag_to(&mut d, (5000.0, 5000.0), sw, sh);
        assert!(r.x + r.w <= sw && r.y + r.h <= sh, "{r:?}");
        // A wobble under the engage distance changes nothing.
        let mut d = FloatDrag::resize(
            (498.0, 348.0),
            sq(),
            Edges {
                left: false,
                right: true,
                bottom: true,
            },
        );
        assert_eq!(drag_to(&mut d, (501.0, 350.0), sw, sh), sq());
    }

    #[test]
    fn a_markdown_html_or_image_name_is_drawable_and_anything_else_is_not() {
        for (name, want) in [
            ("a.md", DocKind::Markdown),
            ("A.MARKDOWN", DocKind::Markdown),
            ("brief.html", DocKind::Html),
            ("BRIEF.HTM", DocKind::Html),
            ("x.png", DocKind::Image),
            ("x.JPG", DocKind::Image),
            ("x.jpeg", DocKind::Image),
            ("x.webp", DocKind::Image),
            ("x.gif", DocKind::Image),
            ("x.bmp", DocKind::Image),
            ("x.Svg", DocKind::Image),
        ] {
            assert_eq!(doc_kind_by_name(Path::new(name)), Some(want), "{name}");
        }
        for name in [
            "notes.txt",
            "paper.pdf",
            "Makefile",
            "archive.tar.gz",
            ".md",
        ] {
            assert_eq!(doc_kind_by_name(Path::new(name)), None, "{name}");
        }
    }

    #[test]
    fn a_png_name_on_bytes_that_are_not_an_image_is_not_drawable() {
        assert_eq!(doc_kind(Path::new("x.png"), b"hello, world"), None);
        assert_eq!(doc_kind(Path::new("x.png"), PNG), Some(DocKind::Image));
        // Any raster signature will do: gpui decodes by the bytes, not the name.
        assert_eq!(doc_kind(Path::new("x.png"), JPEG), Some(DocKind::Image));
        assert_eq!(doc_kind(Path::new("x.png"), b""), None);
    }

    #[test]
    fn an_image_with_no_extension_is_recognised_by_its_first_bytes() {
        assert_eq!(doc_kind(Path::new("shot"), JPEG), Some(DocKind::Image));
        assert_eq!(
            doc_kind(Path::new("shot"), b"RIFF\x24\0\0\0WEBPVP8 "),
            Some(DocKind::Image)
        );
        assert_eq!(doc_kind(Path::new("shot"), b"#!/bin/sh\n"), None);
        // An extension TD does not know is not a guess at an image, even on
        // image bytes: `.dat` is somebody's format, and it is not ours to draw.
        assert_eq!(doc_kind(Path::new("frame.dat"), PNG), None);
    }

    #[test]
    fn svg_markdown_and_html_are_by_name_whatever_the_bytes() {
        assert_eq!(
            doc_kind(Path::new("a.svg"), b"<svg xmlns"),
            Some(DocKind::Image)
        );
        assert_eq!(doc_kind(Path::new("a.md"), PNG), Some(DocKind::Markdown));
        assert_eq!(doc_kind(Path::new("a.html"), b""), Some(DocKind::Html));
    }

    #[test]
    fn a_directory_named_like_a_document_is_not_drawable() {
        let dir = std::env::temp_dir().join(format!("td-docopen-{}", std::process::id()));
        let named = dir.join("notes.md");
        std::fs::create_dir_all(&named).unwrap();
        assert_eq!(drawable_document(&named), None);
        let png = dir.join("shot.png");
        std::fs::write(&png, PNG).unwrap();
        assert_eq!(
            drawable_document(&png),
            Some(DocTarget {
                path: png.clone(),
                kind: DocKind::Image
            })
        );
        assert_eq!(drawable_document(&dir.join("missing.png")), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn alt_click_opens_a_document_and_copies_a_command_line() {
        let doc = DocTarget {
            path: "/tmp/a.png".into(),
            kind: DocKind::Image,
        };
        assert_eq!(
            alt_click(false, false, Some(doc.clone())),
            Some(AltClick::OpenHere(doc.clone()))
        );
        // The pointer on a path inside a command line opens the path.
        assert_eq!(
            alt_click(false, true, Some(doc.clone())),
            Some(AltClick::OpenHere(doc.clone()))
        );
        assert_eq!(
            alt_click(true, false, Some(doc.clone())),
            Some(AltClick::OpenHere(doc))
        );
        assert_eq!(alt_click(false, true, None), Some(AltClick::Copy));
        assert_eq!(alt_click(true, true, None), None);
        assert_eq!(alt_click(false, false, None), None);
    }

    /// A link inside a document either takes the square's place, when it names
    /// a file TD can draw, or goes to the desktop. Nothing else is opened: a
    /// document is somebody's text, and its links are not all worth following.
    #[test]
    fn a_followed_link_replaces_the_square_or_goes_to_the_desktop() {
        assert_eq!(link_route("/docs/next.md", true), LinkRoute::Replace);
        assert_eq!(link_route("/docs/shot.png", true), LinkRoute::Replace);
        assert_eq!(link_route("/docs/data.csv", false), LinkRoute::Desktop);
        assert_eq!(
            link_route("https://example.com/a.md", false),
            LinkRoute::Desktop
        );
        assert_eq!(link_route("HTTP://example.com", false), LinkRoute::Desktop);
        assert_eq!(
            link_route("mailto:p@example.com", false),
            LinkRoute::Desktop
        );
        for refused in ["javascript:alert(1)", "ssh://host", "steam://run/1", "x"] {
            assert_eq!(link_route(refused, false), LinkRoute::Refuse, "{refused}");
        }
    }

    fn inside(r: FloatRect, w: f32, h: f32) -> bool {
        r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= w + 1e-3 && r.y + r.h <= h + 1e-3
    }

    #[test]
    fn a_float_opens_below_its_line_and_above_it_near_the_bottom() {
        let (w, h) = (1000.0, 800.0);
        // Line near the top: the square starts just under it.
        let top = float_home(w, h, 40.0, 60.0);
        assert!(top.y > 60.0 && top.y < 80.0, "{top:?}");
        assert!((top.w - 560.0).abs() < 1e-3 && top.w == top.h);
        assert!(
            (top.x + top.w - (w - 12.0)).abs() < 1e-3,
            "right-aligned: {top:?}"
        );
        // Line on the last row: the square ends just above it.
        let bottom = float_home(w, h, 760.0, 780.0);
        assert!(bottom.y + bottom.h <= 760.0, "{bottom:?}");
        for line in [0.0, 100.0, 300.0, 500.0, 780.0] {
            assert!(
                inside(float_home(w, h, line, line + 20.0), w, h),
                "line at {line}"
            );
        }
        // A short, wide pane takes 84% of its height instead.
        let wide = float_home(2000.0, 300.0, 0.0, 20.0);
        assert!(
            (wide.h - 252.0).abs() < 1e-3 && inside(wide, 2000.0, 300.0),
            "{wide:?}"
        );
    }

    fn m(alt: bool, control: bool, shift: bool, platform: bool) -> Mods {
        Mods {
            alt,
            control,
            shift,
            platform,
        }
    }

    /// One row per line of the gesture table (mockup 04, "Every click on a
    /// path, before and after"), plus the rows that must NOT change.
    ///
    /// `(mods, on_document, on_link, revealable, chip_armed) -> intent`. A
    /// document is always a link and always revealable; a web link is a link
    /// and nothing else.
    #[test]
    fn every_click_on_a_path_does_what_the_gesture_table_says() {
        use ClickIntent::*;
        let none = Mods::default();
        let alt = m(true, false, false, false);
        let ctrl = m(false, true, false, false);
        let shift = m(false, false, true, false);
        let ctrl_alt = m(true, true, false, false);
        let super_ctrl = m(false, true, false, true);
        let ctrl_shift = m(false, true, true, false);
        let rows: [(&str, Mods, bool, bool, bool, bool, ClickIntent); 17] = [
            // Ctrl+click opens with the desktop: unchanged, path or web link.
            (
                "ctrl, document",
                ctrl,
                true,
                true,
                true,
                false,
                OpenWithDesktop,
            ),
            (
                "ctrl, plain path",
                ctrl,
                false,
                true,
                true,
                false,
                OpenWithDesktop,
            ),
            (
                "ctrl, web link",
                ctrl,
                false,
                true,
                false,
                false,
                OpenWithDesktop,
            ),
            // Shift+click REVEALS a file now. It used to open it.
            ("shift, plain path", shift, false, true, true, false, Reveal),
            ("shift, document", shift, true, true, true, false, Reveal),
            // ...but a web link has nothing on disk, so Shift keeps opening it.
            (
                "shift, web link",
                shift,
                false,
                true,
                false,
                false,
                OpenWithDesktop,
            ),
            // The ladder's order: shift is asked before ctrl.
            (
                "ctrl+shift, path",
                ctrl_shift,
                false,
                true,
                true,
                false,
                Reveal,
            ),
            // Alt on a document opens the square, even on a command line.
            ("alt, document", alt, true, true, true, false, OpenHere),
            ("alt, document, chip", alt, true, true, true, true, OpenHere),
            // Alt anywhere else copies the command line, as it always did.
            (
                "alt, command line",
                alt,
                false,
                false,
                false,
                true,
                CopyChip,
            ),
            ("alt, nothing", alt, false, false, false, false, Pass),
            // Ctrl+Alt on a document is the split's; off one it copies.
            (
                "ctrl+alt, document",
                ctrl_alt,
                true,
                true,
                true,
                false,
                OpenBeside,
            ),
            (
                "ctrl+alt, command",
                ctrl_alt,
                false,
                false,
                false,
                true,
                CopyChip,
            ),
            // Super+Ctrl reveals, unchanged, and a web link still opens.
            (
                "super+ctrl, path",
                super_ctrl,
                false,
                true,
                true,
                false,
                Reveal,
            ),
            (
                "super+ctrl, web",
                super_ctrl,
                false,
                true,
                false,
                false,
                OpenWithDesktop,
            ),
            // No modifier, or a modifier on nothing: a selection starts.
            ("plain, document", none, true, true, true, true, Pass),
            ("shift, nothing", shift, false, false, false, false, Pass),
        ];
        for (name, mods, doc, link, reveal, chip, want) in rows {
            assert_eq!(click_intent(mods, doc, link, reveal, chip), want, "{name}");
        }
    }

    /// A path TD can draw gets "Open here" and "Open beside" first, and every
    /// link can be copied, which is where the copy Alt+click gave up on a path
    /// now lives.
    #[test]
    fn the_menu_on_a_document_path_offers_open_here_open_beside_and_copy_link() {
        assert_eq!(
            link_menu(true, true),
            vec![
                LinkItem::OpenHere,
                LinkItem::OpenBeside,
                LinkItem::OpenWithDesktop,
                LinkItem::Reveal,
                LinkItem::CopyLink
            ]
        );
        // A file TD cannot draw keeps everything but Open here.
        assert_eq!(
            link_menu(false, true),
            vec![
                LinkItem::OpenWithDesktop,
                LinkItem::Reveal,
                LinkItem::CopyLink
            ]
        );
    }

    #[test]
    fn the_menu_on_a_web_link_offers_neither_open_here_nor_reveal() {
        assert_eq!(
            link_menu(false, false),
            vec![LinkItem::OpenWithDesktop, LinkItem::CopyLink]
        );
    }

    /// Alt+click on the path the square is already showing is the second
    /// click that promotes it to a split; on any other path it opens that
    /// path instead, and with no square up it simply opens.
    #[test]
    fn a_second_alt_click_on_the_floating_path_promotes_it() {
        let a = Path::new("/tmp/a.md");
        let b = Path::new("/tmp/b.md");
        assert!(promotes(Some(a), a));
        assert!(!promotes(Some(a), b));
        assert!(!promotes(None, a));
    }

    /// The Document face's keys, as the ruling lists them: 0 fits, 1 is actual
    /// size, + and − step the zoom, the arrows pan, PageUp and PageDown page.
    /// A chord is never the face's, and neither is a letter: those are
    /// swallowed by the face, not acted on.
    #[test]
    fn the_document_face_keys_zoom_pan_and_page() {
        let k = |key| doc_face_key(key, false, false, false);
        assert_eq!(k("0"), Some(DocKey::Fit));
        assert_eq!(k("1"), Some(DocKey::Actual));
        assert_eq!(k("+"), Some(DocKey::ZoomIn));
        assert_eq!(k("="), Some(DocKey::ZoomIn), "+ without its shift");
        assert_eq!(k("-"), Some(DocKey::ZoomOut));
        assert_eq!(k("pageup"), Some(DocKey::Page(-1)));
        assert_eq!(k("pagedown"), Some(DocKey::Page(1)));
        // An arrow moves the view, so the document moves the other way.
        assert_eq!(k("down"), Some(DocKey::Pan(0.0, -DOC_ARROW_STEP)));
        assert_eq!(k("up"), Some(DocKey::Pan(0.0, DOC_ARROW_STEP)));
        assert_eq!(k("right"), Some(DocKey::Pan(-DOC_ARROW_STEP, 0.0)));
        assert_eq!(k("left"), Some(DocKey::Pan(DOC_ARROW_STEP, 0.0)));
        for other in ["a", "2", "enter", "escape", "space", "tab"] {
            assert_eq!(k(other), None, "{other}");
        }
        assert_eq!(doc_face_key("0", false, true, false), None, "ctrl+0");
        assert_eq!(doc_face_key("down", true, false, false), None, "alt+down");
        assert_eq!(doc_face_key("1", false, false, true), None, "super+1");
    }

    fn origin() -> FloatRect {
        FloatRect {
            x: 400.0,
            y: 100.0,
            w: 300.0,
            h: 300.0,
        }
    }

    /// A press that wobbles is a click, not a move: the square stays put
    /// until the pointer has gone more than six pixels, then follows it
    /// exactly, and keeps following when it comes back inside the six.
    #[test]
    fn a_float_drag_engages_only_past_six_pixels() {
        let mut d = FloatDrag::new((500.0, 110.0), origin());
        assert_eq!(drag_to(&mut d, (504.0, 113.0), 1000.0, 800.0), origin());
        assert!(!d.engaged, "five pixels is a wobble");
        let r = drag_to(&mut d, (507.0, 110.0), 1000.0, 800.0);
        assert!(d.engaged, "seven pixels is a move");
        assert_eq!((r.x, r.y), (407.0, 100.0));
        let r = drag_to(&mut d, (501.0, 111.0), 1000.0, 800.0);
        assert!(d.engaged, "an engaged drag stays engaged");
        assert_eq!((r.x, r.y), (401.0, 101.0));
        assert_eq!((r.w, r.h), (300.0, 300.0), "a move never resizes");
    }

    /// Dragged far past any edge, the square stops against it.
    #[test]
    fn a_float_dragged_past_the_edge_stays_inside_its_pane() {
        let (w, h) = (1000.0, 800.0);
        for to in [
            (-5000.0, -5000.0),
            (5000.0, -5000.0),
            (5000.0, 5000.0),
            (-5000.0, 5000.0),
        ] {
            let mut d = FloatDrag::new((500.0, 110.0), origin());
            let r = drag_to(&mut d, to, w, h);
            assert!(inside(r, w, h), "{to:?} -> {r:?}");
            assert_eq!((r.w, r.h), (300.0, 300.0), "{to:?}: pushed, not squashed");
        }
        let mut d = FloatDrag::new((500.0, 110.0), origin());
        let r = drag_to(&mut d, (5000.0, 110.0), w, h);
        assert_eq!((r.x, r.y), (700.0, 100.0), "against the right edge");
    }

    /// A button paints over its strip, so it is found first; a point between
    /// two zones that share an edge belongs to exactly one of them.
    #[test]
    fn a_strip_button_is_found_over_the_strip_it_sits_on() {
        let zones = [
            FloatZone {
                x: 0.0,
                y: 0.0,
                w: 300.0,
                h: 22.0,
                hit: FloatHit::Strip,
            },
            FloatZone {
                x: 240.0,
                y: 0.0,
                w: 30.0,
                h: 22.0,
                hit: FloatHit::Desktop,
            },
            FloatZone {
                x: 270.0,
                y: 0.0,
                w: 30.0,
                h: 22.0,
                hit: FloatHit::Close,
            },
            FloatZone {
                x: 0.0,
                y: 22.0,
                w: 300.0,
                h: 278.0,
                hit: FloatHit::Body,
            },
        ];
        let hit = |x, y| float_hit_at(&zones, x, y).map(|z| z.hit);
        assert_eq!(hit(10.0, 10.0), Some(FloatHit::Strip));
        assert_eq!(hit(250.0, 10.0), Some(FloatHit::Desktop));
        assert_eq!(hit(270.0, 10.0), Some(FloatHit::Close));
        assert_eq!(hit(269.9, 10.0), Some(FloatHit::Desktop));
        assert_eq!(hit(100.0, 22.0), Some(FloatHit::Body));
        assert_eq!(hit(300.0, 10.0), None);
    }

    #[test]
    fn a_float_is_clamped_inside_a_screen_that_shrank() {
        let r = FloatRect {
            x: 900.0,
            y: 700.0,
            w: 500.0,
            h: 500.0,
        };
        let c = clamp_float(r, 400.0, 300.0);
        assert!(inside(c, 400.0, 300.0), "{c:?}");
        assert_eq!((c.w, c.h), (400.0, 300.0));
        assert!(c.contains(10.0, 10.0) && !c.contains(400.0, 10.0));
    }
}
