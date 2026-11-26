//! A PDF page in the theme's colours, with its pictures as printed.
//!
//! Parker, 2026-11-26, pointing at the Halo library's reader: *"we want it to
//! inherit the theme and go dark mode by default (click button for paper) …
//! inherit theme via colour mapping + print pictures."* So a page is drawn
//! dark by default and the **Paper** chip (or `p`) shows it as printed.
//!
//! # The colour mapping
//!
//! The same three steps the Halo reader's SVG filter takes, done on the tile's
//! pixels once poppler has drawn them, in sRGB as the filter does:
//!
//! 1. Invert, then turn the hue half way round: a colour's lightness flips and
//!    its hue stays, so black text goes light, white paper goes dark, and a red
//!    link stays red rather than turning cyan.
//! 2. Stretch each channel into the theme's range: what was paper lands on the
//!    theme's surface, what was ink on its text colour.
//! 3. The paper is 65% opaque and the ink nearly solid, so the glass behind
//!    the square shows faintly through the page, as it does on the site.
//!
//! # Pictures
//!
//! A photograph run through that mapping is a negative, so the places the PDF
//! paints an image are left as poppler drew them. poppler does not say where
//! those are cheaply (`pdftohtml -xml` decodes every image to report them, two
//! minutes for a 48-page paper), so MuPDF's `mutool trace` is asked once per
//! reading of the file: it lists each image's placement without decoding it,
//! under a second for the same paper. It runs in a child process, as poppler
//! does, so a hostile file crashes it and not the window. `mutool` is
//! optional: without it, the page still goes dark and pictures go with it.
//!
//! An image that covers nearly the whole page is the page — a scan — and is
//! recoloured with the text, not kept white.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

/// The paper's opacity in the theme's look; ink is drawn nearly solid.
pub const GROUND_ALPHA: f32 = 0.65;

/// An image covering this much of its page or more is the page itself.
pub const WHOLE_PAGE: f32 = 0.85;

/// How long `mutool trace` may take over a whole document.
pub const TRACE_LIMIT: Duration = Duration::from_secs(20);

/// The two colours a themed page is drawn between, sRGB, 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Where the paper was.
    pub page: [f32; 3],
    /// Where the ink was.
    pub ink: [f32; 3],
}

impl Look {
    /// The theme's surface for paper and its text colour for ink, rounded to
    /// whole 8-bit steps: every tile is drawn again when the look changes, so
    /// a drift too small to see must not count as a change.
    pub fn of(th: &crate::theme::Theme) -> Self {
        let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0;
        let c = |h: gpui::Hsla| {
            let r = h.to_rgb();
            [q(r.r), q(r.g), q(r.b)]
        };
        Self {
            page: c(th.surface),
            ink: c(th.text),
        }
    }
}

/// A picture's place on its page, as fractions of the page as shown, from
/// the top left: `x0 <= x1`, `y0 <= y1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spot {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Spot {
    fn area(&self) -> f32 {
        (self.x1 - self.x0).max(0.0) * (self.y1 - self.y0).max(0.0)
    }
}

/// Every page's pictures, counted from page 0. A page `mutool` listed with
/// no image has an empty list; a page it never reached is absent, which is
/// not the same thing, and is drawn with nothing kept.
pub type Spots = std::collections::BTreeMap<usize, Vec<Spot>>;

/// The SVG `hueRotate(180)` matrix, rows of R G B.
const HUE_180: [[f32; 3]; 3] = [
    [-0.574, 1.430, 0.144],
    [0.426, 0.430, 0.144],
    [0.426, 1.430, -0.856],
];

/// One pixel through the mapping: sRGB in, sRGB and alpha out, 0 to 1.
pub fn map_pixel(rgb: [f32; 3], look: &Look) -> ([f32; 3], f32) {
    let inv = [1.0 - rgb[0], 1.0 - rgb[1], 1.0 - rgb[2]];
    let mut h = [0.0f32; 3];
    for (i, row) in HUE_180.iter().enumerate() {
        h[i] = (row[0] * inv[0] + row[1] * inv[1] + row[2] * inv[2]).clamp(0.0, 1.0);
    }
    let alpha = (GROUND_ALPHA + 0.0744 * h[0] + 0.2503 * h[1] + 0.0253 * h[2]).clamp(0.0, 1.0);
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        out[i] = (look.page[i] + (look.ink[i] - look.page[i]) * h[i]).clamp(0.0, 1.0);
    }
    (out, alpha)
}

/// A table for each channel would not do — the hue turn mixes them — but a
/// page is mostly paper and ink, so the last pixel's answer is kept and
/// reused while the colour repeats.
pub fn recolour(
    bgra: &mut [u8],
    width: u32,
    height: u32,
    look: &Look,
    keep: &[(u32, u32, u32, u32)],
) {
    let (w, h) = (width as usize, height as usize);
    let mut last: Option<([u8; 4], [u8; 4])> = None;
    for y in 0..h {
        // The spans of this row that are pictures, left as drawn.
        let mut spans: Vec<(usize, usize)> = keep
            .iter()
            .filter(|&&(_, y0, _, y1)| (y0 as usize) <= y && y < y1 as usize)
            .map(|&(x0, _, x1, _)| (x0 as usize, (x1 as usize).min(w)))
            .collect();
        spans.sort_unstable();
        let row = &mut bgra[y * w * 4..(y + 1) * w * 4];
        let mut x = 0;
        let mut next = spans.iter().peekable();
        while x < w {
            if let Some(&&(s0, s1)) = next.peek() {
                if x >= s0 {
                    x = x.max(s1);
                    next.next();
                    continue;
                }
            }
            let px = &mut row[x * 4..x * 4 + 4];
            let was = [px[0], px[1], px[2], px[3]];
            let now = match last {
                Some((from, to)) if from == was => to,
                _ => {
                    let rgb = [
                        was[2] as f32 / 255.0,
                        was[1] as f32 / 255.0,
                        was[0] as f32 / 255.0,
                    ];
                    let (c, a) = map_pixel(rgb, look);
                    let b = |v: f32| (v * 255.0).round() as u8;
                    let to = [b(c[2]), b(c[1]), b(c[0]), b(a * was[3] as f32 / 255.0)];
                    last = Some((was, to));
                    to
                }
            };
            px.copy_from_slice(&now);
            x += 1;
        }
    }
}

/// The pictures of `page` that meet a tile, in the tile's own pixels:
/// `(x0, y0, x1, y1)`, end exclusive. `full` is the whole page's size as it
/// is being drawn, and the tile starts at `(tx, ty)` and is `tw` × `th`.
pub fn spots_in_tile(
    spots: &[Spot],
    full: (u32, u32),
    (tx, ty, tw, th): (u32, u32, u32, u32),
) -> Vec<(u32, u32, u32, u32)> {
    let (fw, fh) = (full.0 as f32, full.1 as f32);
    spots
        .iter()
        .filter(|s| s.area() < WHOLE_PAGE)
        .filter_map(|s| {
            let x0 = ((s.x0 * fw).floor() - tx as f32).max(0.0) as u32;
            let y0 = ((s.y0 * fh).floor() - ty as f32).max(0.0) as u32;
            let x1 = ((s.x1 * fw).ceil() - tx as f32).clamp(0.0, tw as f32) as u32;
            let y1 = ((s.y1 * fh).ceil() - ty as f32).clamp(0.0, th as f32) as u32;
            (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
        })
        .collect()
}

/// Read `mutool trace`'s output: every `fill_image` on every page, as a
/// fraction of that page's size as shown (`shown`, in points, from
/// `pdfinfo`). MuPDF lays a page out from its top left, turned and cropped as
/// a reader shows it, which is the frame poppler draws in, so an image's
/// unit square under its `transform` lands where poppler painted it.
/// `fill_image_mask` is a stencil painted in the ink colour, and is ink.
pub fn parse_trace(text: &str, shown: &[(f32, f32)]) -> Spots {
    let mut spots = Spots::new();
    let mut page: Option<usize> = None;
    for line in text.lines() {
        let l = line.trim_start();
        if l.starts_with("<page ") {
            page = attr(l, "number")
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));
            if let Some(p) = page {
                spots.entry(p).or_default();
            }
            continue;
        }
        if !l.starts_with("<fill_image ") {
            continue;
        }
        let (Some(p), Some(m)) = (page, attr(l, "transform")) else {
            continue;
        };
        let Some(&(pw, ph)) = shown.get(p) else {
            continue;
        };
        let m: Vec<f32> = m
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        let [a, b, c, d, e, f] = m[..] else {
            continue;
        };
        let xs = [e, a + e, c + e, a + c + e];
        let ys = [f, b + f, d + f, b + d + f];
        let lo = |v: [f32; 4]| v.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = |v: [f32; 4]| v.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if pw <= 0.0 || ph <= 0.0 {
            continue;
        }
        let s = Spot {
            x0: (lo(xs) / pw).clamp(0.0, 1.0),
            y0: (lo(ys) / ph).clamp(0.0, 1.0),
            x1: (hi(xs) / pw).clamp(0.0, 1.0),
            y1: (hi(ys) / ph).clamp(0.0, 1.0),
        };
        if s.area() > 0.0 {
            spots.entry(p).or_default().push(s);
        }
    }
    spots
}

/// One attribute's value from a line of `mutool trace`'s XML.
fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let at = line.find(&key)? + key.len();
    let end = line[at..].find('"')?;
    Some(&line[at..at + end])
}

/// `mutool`, looked for on this process's PATH. `None` is a machine without
/// MuPDF's tools, where pictures are recoloured with the page.
pub fn mutool() -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("mutool"))
        .find(|c| {
            use std::os::unix::fs::PermissionsExt;
            std::fs::metadata(c).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        })
}

/// Where the pictures are on every page of the PDF at `path`. Blocks: called
/// from gpui's background pool. `None` when `mutool` is not there, failed or
/// took too long: nothing is known, so nothing is kept.
pub fn trace(
    mutool: &Path,
    path: &Path,
    shown: &[(f32, f32)],
    still: &AtomicBool,
) -> Option<Spots> {
    let mut cmd = std::process::Command::new(mutool);
    cmd.arg("trace").arg(super::poppler::arg_path(path));
    let (out, _, status) = super::poppler::run(&mut cmd, TRACE_LIMIT, 256 << 20, still).ok()?;
    status
        .success()
        .then(|| parse_trace(&String::from_utf8_lossy(&out), shown))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOOK: Look = Look {
        page: [0.08, 0.10, 0.08],
        ink: [0.83, 0.82, 0.75],
    };

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.01)
    }

    /// White paper becomes the theme's surface, at the ground's opacity;
    /// black ink becomes its text colour, solid.
    #[test]
    fn paper_becomes_the_surface_and_ink_the_text() {
        let (c, a) = map_pixel([1.0, 1.0, 1.0], &LOOK);
        assert!(close(c, LOOK.page), "{c:?}");
        assert!((a - GROUND_ALPHA).abs() < 0.01, "{a}");
        let (c, a) = map_pixel([0.0, 0.0, 0.0], &LOOK);
        assert!(close(c, LOOK.ink), "{c:?}");
        assert!(a > 0.99, "{a}");
    }

    /// A coloured link keeps its hue: red text stays the reddest channel,
    /// rather than inverting to cyan.
    #[test]
    fn a_colour_keeps_its_hue() {
        let grey = Look {
            page: [0.0; 3],
            ink: [1.0; 3],
        };
        let (c, _) = map_pixel([0.8, 0.1, 0.1], &grey);
        assert!(c[0] > c[1] && c[0] > c[2], "still red: {c:?}");
    }

    fn page_of(w: u32, h: u32, bgra: [u8; 4]) -> Vec<u8> {
        bgra.repeat((w * h) as usize)
    }

    /// A tile is recoloured everywhere but its pictures, which keep the
    /// pixels poppler drew.
    #[test]
    fn a_picture_is_left_as_printed() {
        let (w, h) = (8, 6);
        let mut px = page_of(w, h, [255, 255, 255, 255]);
        let photo = [10, 200, 30, 255];
        for y in 2..4 {
            for x in 3..6 {
                let i = ((y * w + x) * 4) as usize;
                px[i..i + 4].copy_from_slice(&photo);
            }
        }
        recolour(&mut px, w, h, &LOOK, &[(3, 2, 6, 4)]);
        let at = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2], px[i + 3]]
        };
        assert_eq!(at(4, 3), photo, "the picture is untouched");
        let paper = at(0, 0);
        assert_eq!(paper[3], (GROUND_ALPHA * 255.0).round() as u8);
        assert_eq!(
            paper[2],
            (LOOK.page[0] * 255.0).round() as u8,
            "R from the surface"
        );
        assert_eq!(at(6, 3), paper, "just past the picture is paper again");
    }

    /// A picture's place is cut to the tile it meets, in the tile's pixels;
    /// one wholly off the tile is dropped; a page-sized scan is never kept.
    #[test]
    fn spots_are_cut_to_the_tile() {
        let s = Spot {
            x0: 0.25,
            y0: 0.5,
            x1: 0.75,
            y1: 1.0,
        };
        assert_eq!(
            spots_in_tile(&[s], (400, 800), (0, 0, 400, 512)),
            vec![(100, 400, 300, 512)]
        );
        assert_eq!(
            spots_in_tile(&[s], (400, 800), (0, 512, 400, 288)),
            vec![(100, 0, 300, 288)]
        );
        assert!(spots_in_tile(&[s], (400, 800), (0, 0, 400, 300)).is_empty());
        let scan = Spot {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
        };
        assert!(spots_in_tile(&[scan], (400, 800), (0, 0, 400, 800)).is_empty());
    }

    /// `mutool trace` lines become places on each page, as fractions of the
    /// page as shown; a stencil mask is ink, and a listed page with no image
    /// is known to have none.
    #[test]
    fn a_trace_lists_each_pages_pictures() {
        let text = r#"<document filename="x.pdf">
<page number="1" mediabox="0 0 612 792">
	<fill_image alpha="1" colorspace="DeviceRGB" transform="306 0 -0 198 153 99" width="10" height="10"/>
	<fill_image_mask transform="10 0 0 10 0 0" width="1" height="1"/>
</page>
<page number="2" mediabox="0 0 612 792">
</page>
</document>"#;
        let spots = parse_trace(text, &[(612.0, 792.0), (612.0, 792.0)]);
        let p0 = &spots[&0];
        assert_eq!(p0.len(), 1, "the mask is ink");
        assert!((p0[0].x0 - 0.25).abs() < 1e-4 && (p0[0].x1 - 0.75).abs() < 1e-4);
        assert!((p0[0].y0 - 0.125).abs() < 1e-4 && (p0[0].y1 - 0.375).abs() < 1e-4);
        assert_eq!(
            spots.get(&1),
            Some(&Vec::new()),
            "page 2 has none, and says so"
        );
        assert_eq!(spots.get(&2), None, "page 3 was never listed");
    }

    /// The real tool on a PDF with a picture, where the machine has MuPDF.
    #[test]
    fn mutool_finds_a_picture_where_it_is_installed() {
        let Some(m) = mutool() else {
            eprintln!("no mutool on PATH; skipped");
            return;
        };
        let dir = std::env::temp_dir().join(format!("td-pdf-look-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pic.pdf");
        std::fs::write(&path, super::super::poppler::picture_pdf()).unwrap();
        let still = AtomicBool::new(true);
        let spots = trace(&m, &path, &[(200.0, 200.0)], &still).expect("traced");
        let p0 = &spots[&0];
        assert_eq!(p0.len(), 1, "{spots:?}");
        let s = p0[0];
        assert!(
            (s.x0 - 0.25).abs() < 0.01 && (s.x1 - 0.75).abs() < 0.01,
            "{s:?}"
        );
        assert!(
            (s.y0 - 0.25).abs() < 0.01 && (s.y1 - 0.75).abs() < 0.01,
            "{s:?}"
        );
    }

    /// Not a check: draws one real page the way the square does, themed, and
    /// writes it as a PNG, so a person can look. Run by hand:
    /// `TD_PDF_LOOK_SAMPLE=paper.pdf TD_PDF_LOOK_PAGE=2 TD_PDF_LOOK_OUT=out.png
    /// cargo test sample_page -- --ignored`
    #[test]
    #[ignore]
    fn sample_page() {
        use super::super::poppler::{self, Region};
        let (Some(pdf), Some(out)) = (
            std::env::var_os("TD_PDF_LOOK_SAMPLE"),
            std::env::var_os("TD_PDF_LOOK_OUT"),
        ) else {
            return;
        };
        let pdf = std::path::PathBuf::from(pdf);
        let page: usize = std::env::var("TD_PDF_LOOK_PAGE")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(1)
            - 1;
        let tools = poppler::tools().expect("poppler");
        let still = AtomicBool::new(true);
        let pages = poppler::info(&tools, &pdf, &still).expect("pdfinfo");
        let shown: Vec<(f32, f32)> = pages.iter().map(|p| p.shown()).collect();
        let spots = mutool()
            .and_then(|m| trace(&m, &pdf, &shown, &still))
            .unwrap_or_default();
        let (w, h) = shown[page];
        let full = (900, (900.0 * h / w).round() as u32);
        let region = Region {
            page,
            full,
            x: 0,
            y: 0,
            w: full.0,
            h: full.1,
        };
        let mut p =
            poppler::render(&tools, &pdf, &region, pages[page].rot, &still).expect("render");
        let keep = spots_in_tile(
            spots.get(&page).map_or(&[][..], |v| v),
            full,
            (0, 0, p.width, p.height),
        );
        recolour(&mut p.bgra, p.width, p.height, &LOOK, &keep);
        // Over a dark glass ground, as the square's would be, then BGRA → RGBA.
        let ground = [12u8, 14, 12];
        let mut rgb = Vec::with_capacity(p.bgra.len());
        for px in p.bgra.chunks(4) {
            let a = px[3] as f32 / 255.0;
            for (c, g) in [px[2], px[1], px[0]].into_iter().zip(ground) {
                rgb.push((c as f32 * a + g as f32 * (1.0 - a)).round() as u8);
            }
            rgb.push(255);
        }
        ::image::RgbaImage::from_raw(p.width, p.height, rgb)
            .unwrap()
            .save(out)
            .unwrap();
        eprintln!("kept {} pictures on page {}", keep.len(), page + 1);
    }
}
