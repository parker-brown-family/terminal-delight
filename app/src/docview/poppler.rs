//! poppler, run as child processes: what a PDF holds, and one tile of one of
//! its pages as pixels gpui can draw.
//!
//! # Why a child process
//!
//! A PDF is somebody else's file, and a PDF parser is a large surface. Run in a
//! child, a malformed or hostile file crashes `pdftoppm`, not the window, and a
//! page that would take a minute to draw is killed at [`RENDER_LIMIT`]. Nothing
//! is linked or loaded: the two tools are found on PATH when the first PDF
//! opens, and a machine without them runs TD as before, handing a PDF to the
//! desktop with a sentence, as a machine without Chromium hands it an HTML page.
//!
//! # Two tools, one box each
//!
//! `pdfinfo` reads every page's size and rotation once. `pdftoppm` draws one
//! region of one page, as a PPM on its standard output: three bytes a pixel,
//! turned here into the four, in B G R A order, that a gpui texture takes. Two
//! things about them are easy to get wrong, and both were measured on poppler
//! 26.08 before this was written:
//!
//! - `pdfinfo`'s page size is the **crop box**, the box a reader shows, and
//!   `pdftoppm` draws the **media box** unless told `-cropbox`. Without it, a
//!   page whose crop box is smaller than its media box comes out the wrong
//!   shape and is squeezed into the right one.
//! - `pdfinfo` reports a page's size **before** its rotation, and `pdftoppm`'s
//!   `-scale-to-x`/`-scale-to-y` are read before it too, while `-x -y -W -H`,
//!   the region, is read **after** it, on the page as it is shown. So a page
//!   turned a quarter is asked for with its width and height swapped, and its
//!   tiles are cut from the turned picture.
//!
//! # Absent is not zero
//!
//! A page with no size, a rotation poppler did not report, or a page count
//! that disagrees with the pages listed is a file this module will not guess
//! at: [`parse_info`] refuses it with a sentence, rather than drawing a page of
//! nothing.
//!
//! No `crate::` paths: `rustc --test app/src/docview/poppler.rs` runs these
//! tests on their own.

use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The two tools, found on PATH.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tools {
    pub pdfinfo: PathBuf,
    pub pdftoppm: PathBuf,
}

/// Why there is no poppler to draw with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Missing {
    /// Neither tool, or only one of them, is on PATH: the name of the first
    /// one that is not.
    NotFound(&'static str),
}

impl Missing {
    /// Why, in a sentence, and nothing about what happened instead.
    pub fn reason(&self) -> String {
        match self {
            Missing::NotFound(tool) => {
                format!("No {tool} on PATH (install poppler to read PDFs in TD).")
            }
        }
    }

    /// One sentence for the person who clicked, ending in what happened
    /// instead: the HTML engine's sentence, for a PDF.
    pub fn sentence(&self) -> String {
        format!("{} Opened with the desktop.", self.reason())
    }

    /// The reason in a few words, for the chip on the row that was clicked.
    pub fn short_reason(&self) -> String {
        "no poppler".into()
    }
}

/// Look for both tools in `path_var`, the way a shell would. Pure but for
/// asking the disk whether each candidate is an executable file.
pub fn locate(path_var: Option<&OsStr>) -> Result<Tools, Missing> {
    let dirs: Vec<PathBuf> = path_var
        .map(|p| std::env::split_paths(p).collect())
        .unwrap_or_default();
    let find = |name: &'static str| {
        dirs.iter()
            .map(|d| d.join(name))
            .find(|c| executable(c))
            .ok_or(Missing::NotFound(name))
    };
    Ok(Tools {
        pdfinfo: find("pdfinfo")?,
        pdftoppm: find("pdftoppm")?,
    })
}

fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Tools once found are kept for the life of the process; a lookup that
/// found nothing is made again after this long, so installing poppler does
/// not need a TD restart.
const ASK_AGAIN: Duration = Duration::from_secs(30);

static FOUND: Mutex<Option<(Result<Tools, Missing>, Instant)>> = Mutex::new(None);

/// The tools, looked for on this process's PATH.
pub fn tools() -> Result<Tools, Missing> {
    let mut slot = FOUND.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((answer, at)) = &*slot {
        if answer.is_ok() || at.elapsed() < ASK_AGAIN {
            return answer.clone();
        }
    }
    let path = std::env::var_os("PATH");
    let answer = locate(path.as_deref());
    *slot = Some((answer.clone(), Instant::now()));
    answer
}

/// One page as `pdfinfo` reports it: its crop box in points, before its
/// rotation, and the rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageBox {
    pub w: f32,
    pub h: f32,
    /// Clockwise, in degrees: 0, 90, 180 or 270.
    pub rot: u16,
}

impl PageBox {
    /// Whether the page is shown turned a quarter, so its width and height
    /// trade places.
    pub fn turned(&self) -> bool {
        self.rot % 180 == 90
    }

    /// The page as a reader shows it, in points: `(width, height)`.
    pub fn shown(&self) -> (f32, f32) {
        if self.turned() {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        }
    }
}

/// Why a PDF, or one of its pages, could not be drawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Locked with a password, which TD has nowhere to ask for.
    Locked,
    /// poppler could not read it, in poppler's own words.
    Broken(String),
    /// The tool was still working at the limit and was stopped.
    TooSlow,
    /// The tool itself could not be run, or said nothing TD could read.
    Tool(String),
}

impl Refused {
    /// Said in the square, where the document would be.
    pub fn sentence(&self) -> String {
        match self {
            Refused::Locked => "This PDF is locked with a password. Ctrl+click its path \
                                to open it on the desktop, which can ask for it."
                .into(),
            Refused::Broken(why) => format!("poppler could not read this PDF: {why}"),
            Refused::TooSlow => "poppler was still drawing this after twenty seconds, \
                                 and was stopped."
                .into(),
            Refused::Tool(why) => format!("poppler could not be run: {why}"),
        }
    }
}

/// What a failed run said on its standard error, as a [`Refused`]. poppler
/// writes a line per complaint; the last one is the one that stopped it.
pub fn refused_by(stderr: &str) -> Refused {
    if stderr.contains("Incorrect password") {
        return Refused::Locked;
    }
    let last = stderr
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| {
            l.trim_start_matches("Syntax Error:")
                .trim_start_matches("Command Line Error:")
                .trim_start_matches("Internal Error:")
                .trim()
        });
    match last {
        Some(why) if !why.is_empty() => Refused::Broken(why.to_string()),
        _ => Refused::Broken("it said nothing about why".into()),
    }
}

/// Every page `pdfinfo -f 1 -l <last>` listed, first page first.
///
/// A page is read from its `Page N size:` and `Page N rot:` lines. When a page
/// is listed twice, the last listing counts: the document's own metadata is
/// printed first, and a title written to look like a page line must not win.
/// Refused when a page has no size or no rotation, when the pages listed do
/// not run from 1 without a gap, or when there are none.
pub fn parse_info(text: &str) -> Result<Vec<PageBox>, Refused> {
    let mut sizes: std::collections::BTreeMap<usize, (f32, f32)> = Default::default();
    let mut rots: std::collections::BTreeMap<usize, u16> = Default::default();
    let mut count: Option<usize> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("Pages:") {
            count = rest.trim().parse().ok();
            continue;
        }
        let Some(rest) = line.strip_prefix("Page ") else {
            continue;
        };
        let rest = rest.trim_start();
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        let Ok(n) = rest[..digits].parse::<usize>() else {
            continue;
        };
        let rest = rest[digits..].trim_start();
        if let Some(size) = rest.strip_prefix("size:") {
            // "  612 x 792 pts (letter)"
            let mut words = size.split_whitespace();
            let w = words.next().and_then(|s| s.parse::<f32>().ok());
            let x = words.next();
            let h = words.next().and_then(|s| s.parse::<f32>().ok());
            if let (Some(w), Some("x"), Some(h)) = (w, x, h) {
                sizes.insert(n, (w, h));
            }
        } else if let Some(rot) = rest.strip_prefix("rot:") {
            if let Ok(r) = rot.trim().parse::<i32>() {
                rots.insert(n, r.rem_euclid(360) as u16);
            }
        }
    }
    if count == Some(0) || sizes.is_empty() {
        return Err(Refused::Broken("it has no pages".into()));
    }
    let mut pages = Vec::with_capacity(sizes.len());
    for (i, (&n, &(w, h))) in sizes.iter().enumerate() {
        if n != i + 1 {
            return Err(Refused::Tool(format!("pdfinfo skipped page {}", i + 1)));
        }
        if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            return Err(Refused::Broken(format!("page {n} has no size")));
        }
        let rot = *rots
            .get(&n)
            .ok_or_else(|| Refused::Tool(format!("pdfinfo gave page {n} no rotation")))?;
        if rot % 90 != 0 {
            return Err(Refused::Broken(format!("page {n} is turned {rot}°")));
        }
        pages.push(PageBox { w, h, rot });
    }
    if let Some(c) = count {
        if c != pages.len() {
            return Err(Refused::Tool(format!(
                "pdfinfo counted {c} pages and listed {}",
                pages.len()
            )));
        }
    }
    Ok(pages)
}

/// A path as an argument no tool can mistake for an option.
fn arg_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(".").join(path)
    }
}

/// How long `pdfinfo` may take over a whole document.
pub const INFO_LIMIT: Duration = Duration::from_secs(20);

/// Every page of the PDF at `path`, read by `pdfinfo`. Blocks: called from
/// gpui's background pool.
pub fn info(tools: &Tools, path: &Path, still: &AtomicBool) -> Result<Vec<PageBox>, Refused> {
    let mut cmd = Command::new(&tools.pdfinfo);
    cmd.args(["-f", "1", "-l", "2147483647"])
        .arg(arg_path(path));
    let (out, err, status) = run(&mut cmd, INFO_LIMIT, 16 << 20, still)?;
    if !status.success() {
        return Err(refused_by(&err));
    }
    parse_info(&String::from_utf8_lossy(&out))
}

/// One region of one page, in device pixels of the page as shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Region {
    /// Counted from 0.
    pub page: usize,
    /// The whole page's size as it is being drawn, shown the way up a reader
    /// shows it.
    pub full: (u32, u32),
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// `pdftoppm`'s arguments for `region` of a page rotated `rot`, before the
/// file's path. Pure. See the module notes for why a turned page is asked for
/// with its sides swapped.
pub fn render_args(region: &Region, rot: u16) -> Vec<String> {
    let (fw, fh) = region.full;
    let (sx, sy) = if rot % 180 == 90 { (fh, fw) } else { (fw, fh) };
    let n = (region.page + 1).to_string();
    let mut a: Vec<String> = ["-f", &n, "-l", &n, "-cropbox", "-singlefile"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    for (flag, v) in [
        ("-scale-to-x", sx),
        ("-scale-to-y", sy),
        ("-x", region.x),
        ("-y", region.y),
        ("-W", region.w),
        ("-H", region.h),
    ] {
        a.push(flag.into());
        a.push(v.to_string());
    }
    a
}

/// How long one tile may take. A letter page at reading size takes about
/// 85 ms, and a 2,048-pixel tile of a page drawn twice that wide about 120.
pub const RENDER_LIMIT: Duration = Duration::from_secs(20);

/// Pixels in the order a gpui texture takes them: B G R A, opaque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// Draw `region` of the PDF at `path`. Blocks: called from gpui's background
/// pool. Stops, and answers [`Refused::TooSlow`], when `still` is lowered —
/// the square was closed, or the file changed — as well as at the limit.
pub fn render(
    tools: &Tools,
    path: &Path,
    region: &Region,
    rot: u16,
    still: &AtomicBool,
) -> Result<Picture, Refused> {
    let mut cmd = Command::new(&tools.pdftoppm);
    cmd.args(render_args(region, rot)).arg(arg_path(path));
    // A tile is at most TILE × TILE × 3 bytes and a header; anything much
    // longer is not a picture of the size asked for.
    let most = (region.w as usize + 1) * (region.h as usize + 1) * 3 + 64;
    let (out, err, status) = run(&mut cmd, RENDER_LIMIT, most, still)?;
    if !status.success() {
        return Err(refused_by(&err));
    }
    let picture = ppm_to_bgra(&out).map_err(Refused::Tool)?;
    // poppler clips a region running past the page's edge, and may round
    // the page itself a pixel either way; more than that is not the tile
    // that was asked for.
    if picture.width > region.w + 1 || picture.height > region.h + 1 {
        return Err(Refused::Tool(format!(
            "asked for {}×{} and was given {}×{}",
            region.w, region.h, picture.width, picture.height
        )));
    }
    Ok(picture)
}

/// A binary PPM (`P6`, one byte a channel) as B G R A. Pure.
pub fn ppm_to_bgra(bytes: &[u8]) -> Result<Picture, String> {
    let mut at = 0;
    let mut field = || -> Result<&[u8], String> {
        loop {
            while bytes.get(at).is_some_and(u8::is_ascii_whitespace) {
                at += 1;
            }
            if bytes.get(at) == Some(&b'#') {
                while bytes.get(at).is_some_and(|b| *b != b'\n') {
                    at += 1;
                }
                continue;
            }
            break;
        }
        let start = at;
        while bytes.get(at).is_some_and(|b| !b.is_ascii_whitespace()) {
            at += 1;
        }
        if start == at {
            return Err("the picture ended in its header".into());
        }
        Ok(&bytes[start..at])
    };
    if field()? != b"P6" {
        return Err("the picture is not a colour PPM".into());
    }
    let mut number = |what: &str| -> Result<u32, String> {
        std::str::from_utf8(field()?)
            .ok()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("the picture's {what} is not a number"))
    };
    let (w, h, max) = (number("width")?, number("height")?, number("depth")?);
    if max != 255 {
        return Err(format!("the picture has {max} levels a channel, not 255"));
    }
    if w == 0 || h == 0 {
        return Err("the picture has no pixels".into());
    }
    // One whitespace byte ends the header.
    let body = bytes.get(at + 1..).unwrap_or_default();
    let n = w as usize * h as usize;
    if body.len() < n * 3 {
        return Err(format!(
            "the picture is {}×{} and carries {} of its {} bytes",
            w,
            h,
            body.len(),
            n * 3
        ));
    }
    let mut bgra = Vec::with_capacity(n * 4);
    for px in body[..n * 3].as_chunks::<3>().0 {
        bgra.extend_from_slice(&[px[2], px[1], px[0], 0xff]);
    }
    Ok(Picture {
        width: w,
        height: h,
        bgra,
    })
}

/// Run `cmd` to its end, reading what it writes, and kill it at `limit` or as
/// soon as `still` is lowered. Both pipes are read on threads of their own: a
/// broken file can make poppler write more complaints than a pipe holds, and
/// a child blocked writing its errors never finishes its picture.
fn run(
    cmd: &mut Command,
    limit: Duration,
    most: usize,
    still: &AtomicBool,
) -> Result<(Vec<u8>, String, ExitStatus), Refused> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Refused::Tool(e.to_string()))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>, keep: usize| {
        std::thread::spawn(move || {
            let mut kept = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = (&mut pipe).take(keep as u64).read_to_end(&mut kept);
                let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            }
            kept
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        most,
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
        64 << 10,
    );
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() >= deadline || !still.load(Ordering::SeqCst) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Refused::TooSlow);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(4)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Refused::Tool(e.to_string()));
            }
        }
    };
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    Ok((out, String::from_utf8_lossy(&err).into_owned(), status?))
}

/// A PDF built by hand for the tests: two pages, each drawn so that getting
/// one of the module notes' two traps wrong shows in its pixels.
///
/// Page 1's media box is 600 × 800 points and its crop box the 300 × 400 in
/// the middle, filled blue edge to edge: drawn from the crop box it is blue
/// to its corners, and from the media box it is white there. Page 2 is
/// 400 × 200, turned 90°, with its left half red: shown, it is 200 wide and
/// 400 tall, red above and white below.
#[cfg(test)]
pub fn two_page_pdf() -> Vec<u8> {
    let blue = "0 0 1 rg 100 100 300 400 re f";
    let red = "1 0 0 rg 0 0 200 200 re f";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] \
         /CropBox [100 100 400 500] /Contents 4 0 R >>"
            .into(),
        format!("<< /Length {} >>\nstream\n{blue}\nendstream", blue.len()),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 200] /Rotate 90 \
         /Contents 6 0 R >>"
            .into(),
        format!("<< /Length {} >>\nstream\n{red}\nendstream", red.len()),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).bytes());
    for o in offsets {
        out.extend(format!("{o:010} 00000 n \n").bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .bytes(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real poppler over [`two_page_pdf`]: the sizes `pdfinfo` reads, and
    /// the pixels `pdftoppm` draws from them, on both pages and in a region.
    ///
    /// Runs wherever poppler is installed. CI installs it; elsewhere, without
    /// it, this says so on its error stream and passes, except under `CI`,
    /// where a missing poppler fails rather than passing unseen.
    #[test]
    fn poppler_draws_the_crop_box_and_turns_the_page() {
        let tools = match locate(std::env::var_os("PATH").as_deref()) {
            Ok(t) => t,
            Err(why) if std::env::var_os("CI").is_none() => {
                eprintln!("skipped: {}", why.reason());
                return;
            }
            Err(why) => panic!("{}", why.reason()),
        };
        let dir = std::env::temp_dir().join(format!("td-poppler-real-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("two pages.pdf");
        std::fs::write(&path, two_page_pdf()).unwrap();
        let still = AtomicBool::new(true);

        let pages = info(&tools, &path, &still).unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].shown(), (300.0, 400.0), "the crop box");
        assert_eq!(pages[1].rot, 90);
        assert_eq!(pages[1].shown(), (200.0, 400.0), "turned");

        let at = |p: &Picture, x: u32, y: u32| {
            let i = ((y * p.width + x) * 4) as usize;
            [p.bgra[i], p.bgra[i + 1], p.bgra[i + 2], p.bgra[i + 3]]
        };
        const BLUE: [u8; 4] = [255, 0, 0, 255];
        const RED: [u8; 4] = [0, 0, 255, 255];
        const WHITE: [u8; 4] = [255, 255, 255, 255];

        let whole = |page: usize, full: (u32, u32)| Region {
            page,
            full,
            x: 0,
            y: 0,
            w: full.0,
            h: full.1,
        };
        let p1 = render(&tools, &path, &whole(0, (30, 40)), 0, &still).unwrap();
        assert_eq!((p1.width, p1.height), (30, 40));
        assert_eq!(at(&p1, 0, 0), BLUE, "blue to the corner: the crop box");
        assert_eq!(at(&p1, 29, 39), BLUE);

        let p2 = render(&tools, &path, &whole(1, (20, 40)), 90, &still).unwrap();
        assert_eq!((p2.width, p2.height), (20, 40), "shown the way up");
        assert_eq!(at(&p2, 10, 5), RED, "the left half turned to the top");
        assert_eq!(at(&p2, 10, 35), WHITE);

        let lower = Region {
            page: 1,
            full: (20, 40),
            x: 0,
            y: 20,
            w: 20,
            h: 20,
        };
        let p3 = render(&tools, &path, &lower, 90, &still).unwrap();
        assert_eq!((p3.width, p3.height), (20, 20));
        assert_eq!(
            at(&p3, 10, 10),
            WHITE,
            "a region is cut from the page as shown"
        );

        std::fs::write(dir.join("locked.pdf"), b"%PDF-1.4\nnot really").unwrap();
        assert!(matches!(
            info(&tools, &dir.join("locked.pdf"), &still),
            Err(Refused::Broken(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // The title holds a newline, and its second line forges page 1: pdfinfo
    // prints metadata as it finds it.
    const INFO: &str = "Title:           A forged\n\
        Page    1 size:  1 x 1 pts\n\
        Author:          Somebody\n\
        Pages:           3\n\
        Encrypted:       no\n\
        Page    1 size:  612 x 792 pts (letter)\n\
        Page    1 rot:   0\n\
        Page    2 size:  612 x 792 pts (letter)\n\
        Page    2 rot:   90\n\
        Page    3 size:  595.276 x 841.89 pts (A4)\n\
        Page    3 rot:   180\n\
        File size:       1150637 bytes\n";

    /// Every page, in order, with its rotation; and a title written to look
    /// like a page line is outvoted by the real line after it.
    #[test]
    fn pdfinfo_lists_every_page_with_its_size_and_rotation() {
        let pages = parse_info(INFO).unwrap();
        assert_eq!(pages.len(), 3);
        assert_eq!(
            pages[0],
            PageBox {
                w: 612.0,
                h: 792.0,
                rot: 0
            }
        );
        assert_eq!(pages[1].rot, 90);
        assert!(pages[1].turned());
        assert_eq!(pages[1].shown(), (792.0, 612.0));
        assert_eq!(pages[2].rot, 180);
        assert!(!pages[2].turned());
        assert!((pages[2].w - 595.276).abs() < 1e-3);
    }

    /// A page with no size, a gap in the pages, a page count that disagrees
    /// with the list, and a file with none are all refused, never drawn as a
    /// page of nothing.
    #[test]
    fn pdfinfo_output_that_does_not_add_up_is_refused() {
        assert_eq!(
            parse_info("Pages: 0\n"),
            Err(Refused::Broken("it has no pages".into()))
        );
        assert!(parse_info("").is_err());
        let gap =
            "Page 1 size: 10 x 10 pts\nPage 1 rot: 0\nPage 3 size: 10 x 10 pts\nPage 3 rot: 0\n";
        assert!(matches!(parse_info(gap), Err(Refused::Tool(_))));
        let flat = "Page 1 size: 0 x 10 pts\nPage 1 rot: 0\n";
        assert!(matches!(parse_info(flat), Err(Refused::Broken(_))));
        let unturned = "Page 1 size: 10 x 10 pts\n";
        assert!(matches!(parse_info(unturned), Err(Refused::Tool(_))));
        let miscounted = "Pages: 2\nPage 1 size: 10 x 10 pts\nPage 1 rot: 0\n";
        assert!(matches!(parse_info(miscounted), Err(Refused::Tool(_))));
        let negative = "Page 1 size: 10 x 20 pts\nPage 1 rot: -90\n";
        assert_eq!(parse_info(negative).unwrap()[0].rot, 270);
    }

    /// A locked file says so, in words that tell the reader what to do; any
    /// other failure keeps poppler's last complaint without its label.
    #[test]
    fn what_poppler_complains_of_becomes_a_sentence() {
        assert_eq!(
            refused_by("Command Line Error: Incorrect password\n"),
            Refused::Locked
        );
        assert_eq!(
            refused_by(
                "Syntax Warning: May not be a PDF file (continuing anyway)\n\
                 Syntax Error: Couldn't find trailer dictionary\n\
                 Syntax Error: Couldn't read xref table\n"
            ),
            Refused::Broken("Couldn't read xref table".into())
        );
        assert!(matches!(refused_by(""), Refused::Broken(_)));
        assert!(Refused::Locked.sentence().contains("password"));
    }

    /// The page and its region, with `-cropbox`, and a page turned a quarter
    /// asked for with its sides swapped while its region is not.
    #[test]
    fn a_region_is_asked_for_on_the_page_as_shown() {
        let r = Region {
            page: 1,
            full: (1280, 1656),
            x: 0,
            y: 2048,
            w: 1280,
            h: 2048,
        };
        let a = render_args(&r, 0);
        let pairs: Vec<(&str, &str)> = a
            .windows(2)
            .map(|w| (w[0].as_str(), w[1].as_str()))
            .collect();
        for want in [
            ("-f", "2"),
            ("-l", "2"),
            ("-scale-to-x", "1280"),
            ("-scale-to-y", "1656"),
            ("-x", "0"),
            ("-y", "2048"),
            ("-W", "1280"),
            ("-H", "2048"),
        ] {
            assert!(pairs.contains(&want), "{want:?} in {a:?}");
        }
        assert!(a.iter().any(|s| s == "-cropbox"));
        assert!(a.iter().any(|s| s == "-singlefile"));
        let turned = render_args(&r, 270);
        let pairs: Vec<(&str, &str)> = turned
            .windows(2)
            .map(|w| (w[0].as_str(), w[1].as_str()))
            .collect();
        assert!(pairs.contains(&("-scale-to-x", "1656")), "{turned:?}");
        assert!(pairs.contains(&("-scale-to-y", "1280")), "{turned:?}");
        assert!(pairs.contains(&("-W", "1280")), "the region is not swapped");
    }

    /// Three bytes a pixel become four, blue first, opaque; a header with a
    /// comment in it still reads; a short or foreign picture is refused.
    #[test]
    fn a_ppm_becomes_bgra() {
        let ppm = b"P6\n# poppler\n2 1\n255\n\x10\x20\x30\x40\x50\x60";
        let p = ppm_to_bgra(ppm).unwrap();
        assert_eq!((p.width, p.height), (2, 1));
        assert_eq!(p.bgra, vec![0x30, 0x20, 0x10, 0xff, 0x60, 0x50, 0x40, 0xff]);
        assert!(ppm_to_bgra(b"P6\n2 1\n255\n\x10\x20\x30").is_err(), "short");
        assert!(ppm_to_bgra(b"P5\n1 1\n255\n\x10").is_err(), "grey");
        assert!(
            ppm_to_bgra(b"P6\n1 1\n65535\n\0\0\0\0\0\0").is_err(),
            "deep"
        );
        assert!(ppm_to_bgra(b"P6\n0 0\n255\n").is_err(), "empty");
        assert!(ppm_to_bgra(b"P6\n1").is_err(), "cut off");
    }

    /// Both tools have to be there; the first missing is named.
    #[test]
    fn both_tools_are_looked_for_on_path() {
        let dir = std::env::temp_dir().join(format!("td-poppler-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let make = |name: &str| {
            use std::os::unix::fs::PermissionsExt;
            let p = dir.join(name);
            std::fs::write(&p, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        let path = std::ffi::OsString::from(dir.as_os_str());
        assert_eq!(locate(Some(&path)), Err(Missing::NotFound("pdfinfo")));
        make("pdfinfo");
        assert_eq!(locate(Some(&path)), Err(Missing::NotFound("pdftoppm")));
        make("pdftoppm");
        assert_eq!(
            locate(Some(&path)),
            Ok(Tools {
                pdfinfo: dir.join("pdfinfo"),
                pdftoppm: dir.join("pdftoppm"),
            })
        );
        assert_eq!(locate(None), Err(Missing::NotFound("pdfinfo")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A run is stopped the moment nobody wants it any more, and says so,
    /// rather than waiting out its limit.
    #[test]
    fn a_run_nobody_wants_is_stopped() {
        let still = AtomicBool::new(false);
        let at = Instant::now();
        let got = run(
            Command::new("sleep").arg("5"),
            Duration::from_secs(10),
            64,
            &still,
        );
        assert_eq!(got.err(), Some(Refused::TooSlow));
        assert!(at.elapsed() < Duration::from_secs(2), "{:?}", at.elapsed());
    }
}
