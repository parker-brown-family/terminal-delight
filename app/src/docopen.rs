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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    const JPEG: &[u8] = b"\xff\xd8\xff\xe0\0\x10JFIF\0";

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
