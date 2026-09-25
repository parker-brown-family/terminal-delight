//! The same bake-off as corebake, against libghostty-vt: feed recorded program
//! output and report images, placements, cursor, replies and the screen text.
//!
//!     ghostbake <capture.bin> [...]
use libghostty_vt::fmt::{Format, Formatter, FormatterOptions};
use libghostty_vt::kitty::graphics::PlacementIterator;
use libghostty_vt::terminal::{Options, Terminal};
use std::cell::RefCell;
use std::rc::Rc;

/// libghostty-vt leaves PNG decoding to the embedder (`set_png_decoder`). Its own
/// `RustPngDecoder` has no public constructor in 0.2.1, so this is the embedder's
/// decoder: the `png` crate, expanded to 8-bit RGBA.
struct PngToRgba;
impl libghostty_vt::kitty::graphics::DecodePng for PngToRgba {
    fn decode_png<'alloc>(
        &mut self,
        alloc: &'alloc libghostty_vt::alloc::Allocator<'_>,
        data: &[u8],
    ) -> Option<libghostty_vt::kitty::graphics::DecodedImage<'alloc>> {
        let mut dec = png::Decoder::new(std::io::Cursor::new(data));
        dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16 | png::Transformations::ALPHA);
        let mut reader = dec.read_info().ok()?;
        let mut buf = vec![0; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let rgba: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
            png::ColorType::GrayscaleAlpha => buf[..info.buffer_size()].chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
            _ => return None,
        };
        let mut bytes = libghostty_vt::alloc::Bytes::new_with_alloc(alloc, rgba.len()).ok()?;
        bytes.copy_from_slice(&rgba);
        Some(libghostty_vt::kitty::graphics::DecodedImage { width: info.width, height: info.height, data: bytes })
    }
}

const COLS: u16 = 120;
const ROWS: u16 = 40;
const CW: u32 = 10;
const CH: u32 = 22;

fn run(bytes: &[u8]) -> serde_json::Value {
    let replies: Rc<RefCell<Vec<String>>> = Rc::default();
    let mut term = Terminal::new(Options { cols: COLS, rows: ROWS, max_scrollback: 10_000 }).expect("terminal");
    term.resize(COLS, ROWS, CW, CH).expect("resize");
    // Pictures sent as a path, a temp file or shared memory are refused by
    // default; allow them, as a terminal serving local programs would.
    let _ = term.set_kitty_image_from_file_allowed(true);
    let _ = term.set_kitty_image_from_temp_file_allowed(true);
    let _ = term.set_kitty_image_from_shared_mem_allowed(true);
    // The bindings' default budget is 10 MB; use kitty's 320 MB so the two cores
    // are compared on the protocol, not on a default.
    let _ = term.set_kitty_image_storage_limit(320 * 1024 * 1024);
    let r2 = replies.clone();
    let _ = term.on_pty_write(move |_t, data: &[u8]| {
        r2.borrow_mut().push(String::from_utf8_lossy(data).escape_default().to_string());
    });
    for chunk in bytes.chunks(4096) {
        term.vt_write(chunk);
    }
    let mut placements = Vec::new();
    let mut images = std::collections::BTreeSet::new();
    let mut virtuals = 0;
    if let Ok(graphics) = term.kitty_graphics() {
        let mut it = PlacementIterator::new().expect("iterator");
        if let Ok(mut iter) = it.update(&graphics) {
            while let Some(p) = iter.next() {
                let id = p.image_id().unwrap_or(0);
                images.insert(id);
                if p.is_virtual().unwrap_or(false) {
                    virtuals += 1;
                    continue;
                }
                if let Some(img) = graphics.image(id) {
                    let pos = p.viewport_pos(&img, &term).ok().flatten();
                    let size = p.grid_size(&img, &term).ok();
                    placements.push(serde_json::json!({
                        "image": id, "placement": p.placement_id().unwrap_or(0),
                        "row": pos.map(|v| v.row), "col": pos.map(|v| v.col),
                        "cols": size.map(|s| s.cols), "rows": size.map(|s| s.rows),
                        "image_px": [img.width().unwrap_or(0), img.height().unwrap_or(0)],
                    }));
                }
            }
        }
    }
    let text = Formatter::new(&term, FormatterOptions::new().with_format(Format::Plain).with_trim(true))
        .and_then(|mut f| f.format_alloc(None).map(|b| String::from_utf8_lossy(&b).to_string()))
        .unwrap_or_else(|e| format!("formatter error: {e:?}"));
    let lines: Vec<String> = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| format!("{i:>2}|{}", l.replace('\u{10EEEE}', "#")))
        .collect();
    serde_json::json!({
        "core": "libghostty-vt 0.2.1 (ghostty a887df42, Zig 0.15.2)",
        "placements": placements,
        "images_referenced": images.len(),
        "virtual_placements": virtuals,
        "cursor": [term.cursor_y().unwrap_or(u16::MAX), term.cursor_x().unwrap_or(u16::MAX)],
        "replies": replies.borrow().clone(),
        "text": lines,
        "storage_limit": term.kitty_image_storage_limit().ok(),
    })
}

// ---------------------------------------------------------------- performance: twenty panes of text

/// VmRSS and VmHWM from /proc/self/status, in kB. None when the kernel does not say.
fn rss() -> (Option<u64>, Option<u64>) {
    let s = std::fs::read_to_string("/proc/self/status").ok();
    let get = |k: &str| {
        s.as_deref()?.lines().find(|l| l.starts_with(k))?.split_whitespace().nth(1)?.parse().ok()
    };
    (get("VmRSS:"), get("VmHWM:"))
}

/// The same terminal run() builds, without the reply callback.
///
/// max_scrollback is documented as lines, but on 2026-09-25 a value of 10,000 kept 489 rows
/// against 10,040 for the other three cores, so it behaves like Ghostty's byte limit.
/// GHOSTBAKE_SCROLLBACK sets it, so the perf runs can match the others' 10,040 rows kept.
fn make() -> Terminal<'static, 'static> {
    let sb: usize = std::env::var("GHOSTBAKE_SCROLLBACK").ok().and_then(|v| v.parse().ok()).unwrap_or(10_000);
    let mut term = Terminal::new(Options { cols: COLS, rows: ROWS, max_scrollback: sb }).expect("terminal");
    term.resize(COLS, ROWS, CW, CH).expect("resize");
    let _ = term.set_kitty_image_from_file_allowed(true);
    let _ = term.set_kitty_image_from_temp_file_allowed(true);
    let _ = term.set_kitty_image_from_shared_mem_allowed(true);
    let _ = term.set_kitty_image_storage_limit(320 * 1024 * 1024);
    term
}

/// Memory first, in a process that has done nothing else, then speed; the same
/// procedure as corebake's --perf.
fn perf_main(args: &[String]) {
    let text = std::fs::read(&args[0]).expect("text stream");
    let pic = std::fs::read(&args[1]).expect("picture recording");
    let panes: usize = args[2].parse().expect("panes");
    let before = rss();
    let mut terms: Vec<_> = (0..panes).map(|_| make()).collect();
    let empty = rss();
    for t in terms.iter_mut() {
        for c in text.chunks(4096) { t.vt_write(c); }
        for c in pic.chunks(4096) { t.vt_write(c); }
    }
    let full = rss();
    let kept = terms[0].total_rows().ok();
    drop(terms);
    let mut runs = Vec::new();
    for _ in 0..5 {
        let mut t = make();
        let t0 = std::time::Instant::now();
        for c in text.chunks(4096) { t.vt_write(c); }
        runs.push((t0.elapsed().as_secs_f64() * 1e4).round() / 10.0);
    }
    let mut sorted = runs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let per_pane = |a: Option<u64>, b: Option<u64>| a.zip(b).map(|(a, b)| a.saturating_sub(b) / panes as u64);
    let v = serde_json::json!({
        "core": "libghostty-vt 0.2.1 (ghostty a887df42, Zig 0.15.2)", "panes": panes, "text_bytes": text.len(), "picture_bytes": pic.len(),
        "rss_kb": {"start": before.0, "after_create": empty.0, "after_fill": full.0, "high_water": full.1},
        "kb_per_pane_empty": per_pane(empty.0, before.0),
        "kb_per_pane_full": per_pane(full.0, before.0),
        "rows_kept": kept,
        "text_ms_runs": runs, "text_ms_median": sorted[2],
        "text_mb_per_s": (text.len() as f64 / 1048576.0) / (sorted[2] / 1000.0),
    });
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
}

fn main() {
    libghostty_vt::kitty::graphics::set_png_decoder(Some(Box::new(PngToRgba))).expect("png decoder");
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|a| a == "--perf").unwrap_or(false) {
        return perf_main(&args[1..]);
    }
    let mut out = serde_json::Map::new();
    for path in std::env::args().skip(1) {
        let mut bytes = std::fs::read(&path).expect("read capture");
        let exit = b"\x1b[?1049l";
        let cut = bytes.windows(exit.len()).rposition(|w| w == exit);
        if let Some(i) = cut {
            bytes.truncate(i);
        }
        let name = std::path::Path::new(&path).file_stem().unwrap().to_string_lossy().to_string();
        let t0 = std::time::Instant::now();
        let g = run(&bytes);
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        out.insert(name, serde_json::json!({ "bytes": bytes.len(), "cut_before_alt_screen_exit": cut.is_some(), "ghostty": g, "ghostty_ms": (ms * 10.0).round() / 10.0 }));
    }
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
