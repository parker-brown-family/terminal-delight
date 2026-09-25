//! What a page engine is asked, and what it answers.
//!
//! An engine turns an HTML file into pixels and geometry. It never writes the
//! file and never owns the notes: TD's notes layer sits on top of whatever
//! engine drew the page, so swapping the engine cannot break a brief. The
//! first engine is a snapshot of the page taken by headless Chromium
//! ([`super::snapshot`]); a live engine later answers the same calls.
//!
//! Every call blocks. The view makes them from gpui's background pool, never
//! from the foreground, and everything returned by one [`PageEngine::open`]
//! comes from one page instance and one layout pass: a brief can lay out 17–29
//! CSS px taller on a later load, so anchors from one load and pixels from
//! another would disagree at the seams.
//!
//! **Absent is not zero.** A rect the browser reports as 0 × 0 — an element
//! inside a closed dialog — arrives here as `None`, never as a rectangle at the
//! origin.
//!
//! No `crate::` paths, so the engine tests can compile this file on its own.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

/// Pixels and geometry for an HTML page.
pub trait PageEngine: Send + Sync {
    fn name(&self) -> &'static str;
    /// Load and lay out once. Everything returned comes from that one pass.
    fn open(&self, req: &PageRequest) -> Result<PageLayout, EngineError>;
    /// A band of the page as laid out by `open`, full layout width, as PNG.
    /// `Err(Stale)` when `generation` is not the page's current one.
    fn tile(&self, page: PageId, generation: u64, band: Band) -> Result<Tile, EngineError>;
    /// One of the page's own `<dialog>`s, opened by id rather than through
    /// the page's wiring: six briefs ship dialog buttons with no script
    /// behind them, and the button-to-dialog mapping is plain markup.
    fn dialog(&self, page: PageId, generation: u64, id: &str) -> Result<DialogRender, EngineError>;
    fn close(&self, page: PageId);
    /// Stop whatever the engine runs, now: TD is quitting.
    fn shutdown(&self) {}
    /// Load the file as it is on disk now, in a page of its own, and report
    /// what the brief's own script shows: which anchors carry notes and
    /// stamps, and where every anchor is. The page is closed before this
    /// returns. What a save checks itself against.
    fn read_back(&self, path: &Path, geometry: Geometry) -> Result<PageLayout, EngineError> {
        let bytes = std::fs::read(path).map_err(|e| EngineError::Page(e.to_string()))?;
        let layout = self.open(&PageRequest {
            path: path.to_path_buf(),
            geometry,
            expect: layout_hash(&bytes),
        })?;
        if let Some(page) = layout.page {
            self.close(page);
        }
        Ok(layout)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct PageId(pub u64);

/// The page's CSS width and viewport height, and device pixels per CSS pixel.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Geometry {
    pub css_width: u32,
    pub viewport_css_height: u32,
    /// The window's scale factor (times a zoom, when there is one).
    pub scale: f32,
}

impl Geometry {
    /// The scale as the number it was meant to be. A window's 1.6 arrives as
    /// the f32 nearest 1.6, which is 1.60000002; multiplied out to 25,000
    /// rows that is a row too many, and a row too many is a band nobody
    /// captured.
    fn exact_scale(&self) -> f64 {
        (f64::from(self.scale.max(0.01)) * 10_000.0).round() / 10_000.0
    }

    /// A band of `height_dev` device rows starting at `top_dev`, in CSS px.
    pub fn band_css(&self, band: Band) -> (f64, f64) {
        let s = self.exact_scale();
        (f64::from(band.top_dev) / s, f64::from(band.height_dev) / s)
    }

    /// The page's height in device pixels, rounded up so the last row of
    /// the page is never cut off.
    pub fn height_dev(&self, height_css: f32) -> u32 {
        (f64::from(height_css) * self.exact_scale() - 1e-6)
            .ceil()
            .max(0.0) as u32
    }
}

/// Which bytes the anchors were laid out from.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct LayoutHash(pub u64);

pub struct PageRequest {
    pub path: PathBuf,
    pub geometry: Geometry,
    /// The hash of the bytes the caller read. The engine re-reads the file
    /// after the load and answers `FileChanged` if they differ, so a render
    /// is never paired with bytes it was not made from.
    pub expect: LayoutHash,
}

/// A horizontal band of the page, in device pixels.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub struct Band {
    pub top_dev: u32,
    pub height_dev: u32,
}

/// The spike's tile height: 2,048 device rows, 12.1 MiB of BGRA at 1,549 wide.
/// Tall enough that a viewport spans two or three, short enough that five
/// resident stay near 61 MiB.
pub const TILE_DEV: u32 = 2048;

/// Every band of a page `height_dev` rows tall, top to bottom.
pub fn bands(height_dev: u32) -> Vec<Band> {
    let mut out = Vec::new();
    let mut top = 0;
    while top < height_dev {
        out.push(Band {
            top_dev: top,
            height_dev: TILE_DEV.min(height_dev - top),
        });
        top += TILE_DEV;
    }
    out
}

pub struct Tile {
    pub band: Band,
    pub png: Vec<u8>,
    pub width_dev: u32,
    pub height_dev: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageLayout {
    /// `None` when read from the cache: no live page behind it.
    pub page: Option<PageId>,
    pub generation: u64,
    pub rendered: LayoutHash,
    pub geometry: Geometry,
    pub height_css: f32,
    /// notes.js's FILE: `window.NOTES_FILE`, else the location's basename.
    /// `None` when no notes.js ran.
    pub notes_file: Option<String>,
    pub capability: NotesCapability,
    /// Document order, as notes.js tags them. Collected in the same pass as
    /// the tiles for the notes layer, which reads them; this slice draws none.
    pub anchors: Vec<Anchor>,
    pub links: Vec<Link>,
    pub openers: Vec<Opener>,
    /// Every content `<dialog>` by id: all but notes.js's own `#d-note` and
    /// `#d-export`.
    pub dialogs: Vec<String>,
    /// Page errors, dead openers, dialogs that overflowed.
    pub diagnostics: Vec<String>,
}

/// A rectangle in CSS px: page coordinates, or relative to a dialog's box.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct RectCss {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl RectCss {
    /// Whether this is a place at all: laid out, with some area.
    pub fn is_place(&self) -> bool {
        self.w > 0.0 && self.h > 0.0 && self.x.is_finite() && self.y.is_finite()
    }
}

/// A 0 × 0 rect is where the browser puts an element it did not lay out —
/// inside a closed dialog, or `display: none`. That is not a place, so it is
/// read as none rather than as a rectangle at the origin.
fn rect_or_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<RectCss>, D::Error> {
    let raw: Option<RectCss> = Option::deserialize(d)?;
    Ok(raw.filter(RectCss::is_place))
}

/// One element notes.js made annotatable.
#[derive(Clone, Debug, Serialize, Deserialize)]
// The notes layer reads these; this slice only carries them from the page.
#[allow(dead_code)]
pub struct Anchor {
    pub nid: String,
    pub title: String,
    pub tag: String,
    #[serde(default)]
    pub dialog: Option<String>,
    #[serde(default, deserialize_with = "rect_or_none")]
    pub rect: Option<RectCss>,
    /// The page's own `.note-btn`, hidden with `visibility`, so it keeps its box.
    #[serde(default, deserialize_with = "rect_or_none")]
    pub button: Option<RectCss>,
    /// Only where this brief's notes.js made a concur zone.
    #[serde(default, deserialize_with = "rect_or_none")]
    pub concur_zone: Option<RectCss>,
    /// Whether the brief's own notes.js marked it as having notes, as the
    /// page shows it. `None` from a probe too old to ask: unknown, not no.
    #[serde(default)]
    pub has_note: Option<bool>,
    /// Whether the brief's own notes.js drew a stamp on it.
    #[serde(default)]
    pub has_concur: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Link {
    /// The attribute as written.
    pub href: String,
    /// What the browser resolved it to, against the page's own base.
    pub resolved: String,
    #[serde(default, deserialize_with = "rect_or_none")]
    pub rect: Option<RectCss>,
    #[serde(default)]
    pub dialog: Option<String>,
    /// For a link to a fragment of this page: where its target starts, in
    /// page CSS px. `None` when the fragment names nothing.
    #[serde(default)]
    pub fragment_top_css: Option<f32>,
}

/// A `[data-dlg]` button: a press opens the dialog it names.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Opener {
    pub dialog: String,
    #[serde(default, deserialize_with = "rect_or_none")]
    pub rect: Option<RectCss>,
    /// The dialog this button sits in, if it is inside one.
    #[serde(default)]
    pub inside: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
// The notes layer reads these; this slice only carries them from the page.
#[allow(dead_code)]
pub struct NotesCapability {
    /// How many `report-notes` islands the file has. 0 = read-only image.
    pub notes_islands: u32,
    pub concurs_island: bool,
    /// How many elements notes.js tagged.
    pub tagged: u32,
    pub concur: ConcurSupport,
}

/// Never a bare bool: "this brief cannot take a concur" and "nothing ran that
/// could tell" are different answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ConcurSupport {
    Supported,
    NotSupported,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogRender {
    pub id: String,
    pub width_dev: u32,
    pub height_dev: u32,
    /// Whether the viewport had to grow before nothing inside scrolled.
    pub grew_viewport: bool,
    /// Rects below are relative to the dialog's own box, in CSS px.
    #[allow(dead_code)] // the notes layer reads these
    pub anchors: Vec<Anchor>,
    pub links: Vec<Link>,
    pub openers: Vec<Opener>,
    /// The dialog's own close buttons (`[data-close]`).
    pub closers: Vec<RectCss>,
    /// Cached as a file beside the JSON, never inside it.
    #[serde(skip)]
    pub png: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineError {
    /// The browser would not start, with the tail of what it said.
    Launch(String),
    Timeout(&'static str),
    /// The file changed between the read and the render.
    FileChanged,
    /// The page was re-laid out since this generation.
    Stale,
    /// No live page by that id: it was closed, or the browser was shut down
    /// for being idle. The caller opens a new one.
    Closed,
    Page(String),
    Protocol(String),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::Launch(why) => write!(f, "Chromium would not start: {why}"),
            EngineError::Timeout(what) => write!(f, "Chromium took too long to {what}"),
            EngineError::FileChanged => write!(f, "the file changed while it was being drawn"),
            EngineError::Stale => write!(f, "the page was laid out again meanwhile"),
            EngineError::Closed => write!(f, "the page is no longer open"),
            EngineError::Page(why) => write!(f, "the page could not be drawn: {why}"),
            EngineError::Protocol(why) => write!(f, "Chromium answered unexpectedly: {why}"),
        }
    }
}

/// Why there is no engine to ask. Each says what happened instead.
#[derive(Debug, Clone, PartialEq)]
pub enum Unavailable {
    /// `documents.toml` says `engine = "off"`.
    Off,
    /// `documents.toml` names an engine this build does not have.
    UnknownEngine(String),
    /// `documents.toml` could not be read.
    Prefs(String),
    NoBrowser {
        /// Every place looked, in order: the configured path, or the four
        /// names on PATH.
        searched: Vec<PathBuf>,
        /// Whether `searched` is the one configured path rather than PATH.
        configured: bool,
    },
}

impl Unavailable {
    /// One sentence for the person who clicked, ending in what happened
    /// instead.
    pub fn sentence(&self) -> String {
        format!("{} Opened with the desktop.", self.reason())
    }

    /// Why, and nothing about what happened instead: what a document pane
    /// says when nobody asked for the file just now, as when a saved pane
    /// comes back after a restart, and no desktop was asked to open it.
    pub fn reason(&self) -> String {
        match self {
            Unavailable::Off => {
                "HTML is set to open with the desktop (engine = \"off\" in documents.toml).".into()
            }
            Unavailable::UnknownEngine(name) => format!(
                "documents.toml asks for an HTML engine called \"{name}\", and the only one TD has is \"snapshot\"."
            ),
            Unavailable::Prefs(why) => format!("documents.toml could not be read ({why})."),
            Unavailable::NoBrowser {
                searched,
                configured: true,
            } => format!(
                "No Chromium at {} (the chromium setting in documents.toml).",
                searched
                    .first()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
            Unavailable::NoBrowser { searched, .. } => format!(
                "No Chromium found (looked for {} on PATH).",
                searched
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    /// The reason in a few words, for the chip in the pane that was clicked.
    /// Whole sentences go to the log and to a `ctl doc` caller; a pane can be
    /// narrower than any of them.
    pub fn short_reason(&self) -> String {
        match self {
            Unavailable::Off => "HTML engine off in documents.toml".into(),
            Unavailable::UnknownEngine(name) => format!("no HTML engine called \"{name}\""),
            Unavailable::Prefs(_) => "documents.toml could not be read".into(),
            Unavailable::NoBrowser {
                configured: true, ..
            } => "no Chromium where documents.toml says".into(),
            Unavailable::NoBrowser { .. } => "no Chromium found".into(),
        }
    }
}

/// The part of a PNG that says how big it is: width and height from IHDR.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    if png.len() < 24 || !png.starts_with(b"\x89PNG\r\n\x1a\n") || &png[12..16] != b"IHDR" {
        return None;
    }
    let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
    Some((be(&png[16..20]), be(&png[20..24])))
}

/// FNV-1a, 64 bits: stable across runs and builds, which a cache key has to
/// be and `DefaultHasher` does not promise.
pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Which bytes a render was made from, with the notes left out: every byte
/// but the notes island's open tag and text, the concurs island and the last
/// `READER NOTES` mirror ([`super::notes::notes_regions`]). A save changes
/// only those, so a saved note keeps its render, its cache entry and its
/// anchors; any other byte changing is a new render. Least-confident
/// decision 2 rests on it, and the notes-write slice measured it.
pub fn layout_hash(bytes: &[u8]) -> LayoutHash {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for piece in super::notes::outside_notes(bytes) {
        for &b in piece {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    LayoutHash(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_rect_is_none_not_zero() {
        let a: Anchor = serde_json::from_str(
            r#"{"nid":"sub-x","title":"X","tag":"h4","dialog":"d-evidence",
                "rect":{"x":0,"y":0,"w":0,"h":0},"button":null}"#,
        )
        .unwrap();
        assert_eq!(a.rect, None, "a 0x0 rect is an element nobody laid out");
        assert_eq!(a.button, None);
        assert_eq!(a.concur_zone, None, "an absent key is absent too");
        let b: Anchor = serde_json::from_str(
            r#"{"nid":"fig-1","title":"One","tag":"figure",
                "rect":{"x":10,"y":3000.5,"w":640,"h":0.5}}"#,
        )
        .unwrap();
        assert_eq!(
            b.rect,
            Some(RectCss {
                x: 10.0,
                y: 3000.5,
                w: 640.0,
                h: 0.5
            })
        );
        let l: Link = serde_json::from_str(
            r##"{"href":"#top","resolved":"file:///r.html#top","rect":{"x":5,"y":5,"w":0,"h":14}}"##,
        )
        .unwrap();
        assert_eq!(l.rect, None, "zero width is not a place to click");
        assert_eq!(l.fragment_top_css, None);
    }

    #[test]
    fn a_page_is_cut_into_bands_of_whole_tiles_and_a_short_last_one() {
        assert!(bands(0).is_empty());
        assert_eq!(
            bands(5000),
            vec![
                Band {
                    top_dev: 0,
                    height_dev: 2048
                },
                Band {
                    top_dev: 2048,
                    height_dev: 2048
                },
                Band {
                    top_dev: 4096,
                    height_dev: 904
                },
            ]
        );
        // The tallest brief in the corpus: 25,902 device rows, 13 bands.
        assert_eq!(bands(25_902).len(), 13);
        let g = Geometry {
            css_width: 968,
            viewport_css_height: 1400,
            scale: 1.6,
        };
        assert_eq!(g.height_dev(16_188.7), 25_902);
        // Exact: 3,125 CSS px at 1.6 is 5,000 rows, not the 5,001 an f32
        // scale multiplied out would give.
        assert_eq!(g.height_dev(3125.0), 5000);
        let (top, h) = g.band_css(Band {
            top_dev: 2048,
            height_dev: 2048,
        });
        assert!(
            (top - 1280.0).abs() < 1e-9 && (h - 1280.0).abs() < 1e-9,
            "{top} {h}"
        );
    }

    /// The chip's form of each reason names its cause and fits beside a
    /// line: the longest configured path cannot stretch it, because it
    /// carries no path at all.
    #[test]
    fn every_reason_has_a_form_short_enough_for_a_chip() {
        let long = PathBuf::from(format!("/opt/{}/chromium", "deep/".repeat(40)));
        let cases = [
            (Unavailable::Off, "off"),
            (Unavailable::UnknownEngine("servo".into()), "servo"),
            (Unavailable::Prefs("EACCES".into()), "documents.toml"),
            (
                Unavailable::NoBrowser {
                    searched: vec![PathBuf::from("chromium")],
                    configured: false,
                },
                "Chromium",
            ),
            (
                Unavailable::NoBrowser {
                    searched: vec![long],
                    configured: true,
                },
                "Chromium",
            ),
        ];
        for (why, names) in cases {
            let short = why.short_reason();
            assert!(short.contains(names), "{short} should name {names}");
            assert!(
                short.chars().count() <= 40,
                "{short} is too long for a chip"
            );
        }
    }

    #[test]
    fn a_png_says_its_own_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&1549u32.to_be_bytes());
        png.extend_from_slice(&2048u32.to_be_bytes());
        assert_eq!(png_size(&png), Some((1549, 2048)));
        assert_eq!(png_size(b"GIF89a"), None);
    }

    #[test]
    fn a_missing_chromium_says_where_it_looked_and_what_happened_instead() {
        let s = Unavailable::NoBrowser {
            searched: [
                "chromium",
                "chromium-browser",
                "google-chrome-stable",
                "google-chrome",
            ]
            .iter()
            .map(PathBuf::from)
            .collect(),
            configured: false,
        }
        .sentence();
        assert_eq!(
            s,
            "No Chromium found (looked for chromium, chromium-browser, google-chrome-stable, google-chrome on PATH). Opened with the desktop."
        );
        let c = Unavailable::NoBrowser {
            searched: vec![PathBuf::from("/opt/chrome/chrome")],
            configured: true,
        }
        .sentence();
        assert!(
            c.contains("/opt/chrome/chrome") && c.ends_with("Opened with the desktop."),
            "{c}"
        );
        // What a pane says when nothing was opened instead — a saved pane
        // back after a restart — claims nothing about the desktop.
        let off = Unavailable::Off;
        assert!(!off.reason().contains("desktop."), "{}", off.reason());
        assert_eq!(
            off.sentence(),
            format!("{} Opened with the desktop.", off.reason())
        );
    }
}
