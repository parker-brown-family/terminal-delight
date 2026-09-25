//! Feeds recorded program output through libghostty-vt and reports what the
//! terminal ended up holding.

use std::cell::RefCell;
use std::error::Error;

use libghostty_vt::{
    Terminal, TerminalOptions,
    alloc::{Allocator, Bytes},
    kitty::graphics::{self, DecodePng, DecodedImage, PlacementIterator},
    screen::CellWide,
    terminal::{Point, PointCoordinate, PointSpace},
};
use serde_json::{Value, json};

const COLS: u16 = 120;
const ROWS: u16 = 40;
const CELL_W: u32 = 10;
const CELL_H: u32 = 22;
const SCROLLBACK: usize = 10_000;
const CHUNK: usize = 4096;

/// PNG decoder handed to libghostty. The crate ships a `RustPngDecoder`
/// behind its `png` feature, but it has no public constructor, so we write
/// our own on top of the `png` crate. libghostty wants 8-bit RGBA.
struct PngDecoder;

impl DecodePng for PngDecoder {
    fn decode_png<'alloc>(
        &mut self,
        alloc: &'alloc Allocator<'_>,
        data: &[u8],
    ) -> Option<DecodedImage<'alloc>> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
        decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
        let mut reader = decoder.read_info().ok()?;
        let mut buf = vec![0u8; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let buf = &buf[..info.buffer_size()];

        let pixels = (info.width as usize) * (info.height as usize);
        let mut rgba = Vec::with_capacity(pixels * 4);
        match info.color_type {
            png::ColorType::Rgba => rgba.extend_from_slice(buf),
            png::ColorType::Rgb => {
                for p in buf.chunks_exact(3) {
                    rgba.extend_from_slice(&[p[0], p[1], p[2], 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for p in buf.chunks_exact(2) {
                    rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]);
                }
            }
            png::ColorType::Grayscale => {
                for p in buf {
                    rgba.extend_from_slice(&[*p, *p, *p, 255]);
                }
            }
            // EXPAND turns indexed images into RGB(A), so this should not occur.
            png::ColorType::Indexed => return None,
        }
        if rgba.len() != pixels * 4 {
            return None;
        }

        let mut bytes = Bytes::new_with_alloc(alloc, rgba.len()).ok()?;
        bytes.copy_from_slice(&rgba);
        Some(DecodedImage {
            width: info.width,
            height: info.height,
            data: bytes,
        })
    }
}

fn new_terminal<'cb>(
    replies: &'cb RefCell<Vec<u8>>,
    max_scrollback: usize,
) -> Result<Terminal<'static, 'cb>, Box<dyn Error>> {
    let mut term = Terminal::new(TerminalOptions {
        cols: COLS,
        rows: ROWS,
        max_scrollback,
    })?;
    // The options struct has no pixel size; resize() is the only way to set it.
    term.resize(COLS, ROWS, CELL_W, CELL_H)?;
    // Kitty graphics: raise the lib's 10 MB default and allow the file medium,
    // because kitten icat sent the picture as t=f (a path, not inline bytes).
    term.set_kitty_image_storage_limit(320 * 1000 * 1000)?;
    term.set_kitty_image_from_file_allowed(true)?;
    term.on_pty_write(move |_, data| replies.borrow_mut().extend_from_slice(data))?;
    Ok(term)
}

fn feed(term: &mut Terminal<'_, '_>, bytes: &[u8]) {
    for chunk in bytes.chunks(CHUNK) {
        term.vt_write(chunk);
    }
}

fn visible_lines(term: &Terminal<'_, '_>) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut lines = Vec::new();
    let mut graphemes = ['\0'; 32];
    for y in 0..u32::from(ROWS) {
        let mut text = String::new();
        for x in 0..COLS {
            let gref = term.grid_ref(Point::Viewport(PointCoordinate { x, y }))?;
            match gref.cell()?.wide()? {
                CellWide::SpacerTail | CellWide::SpacerHead => continue,
                CellWide::Narrow | CellWide::Wide => {}
            }
            let n = gref.graphemes(&mut graphemes)?;
            if n == 0 {
                text.push(' ');
            } else {
                text.extend(&graphemes[..n]);
            }
        }
        let trimmed = text.trim_end();
        if !trimmed.is_empty() {
            lines.push(json!({ "row": y, "text": trimmed }));
        }
    }
    Ok(lines)
}

fn placements(term: &Terminal<'_, '_>) -> Result<Vec<Value>, Box<dyn Error>> {
    let graphics = term.kitty_graphics()?;
    let mut iter = PlacementIterator::new()?;
    let mut it = iter.update(&graphics)?;
    let mut out = Vec::new();
    while let Some(p) = it.next() {
        let image_id = p.image_id()?;
        let Some(image) = graphics.image(image_id) else {
            out.push(json!({ "image_id": image_id, "error": "image not found" }));
            continue;
        };
        let grid = p.grid_size(&image, term)?;
        let pos = p.viewport_pos(&image, term)?;
        let rect = p.rect(&image, term)?;
        let screen = term.point_from_grid_ref(&rect.start(), PointSpace::Screen)?;
        out.push(json!({
            "image_id": image_id,
            "placement_id": p.placement_id()?,
            "row": pos.map(|v| v.row),
            "col": pos.map(|v| v.col),
            "screen_row": screen.map(|v| v.y),
            "screen_col": screen.map(|v| v.x),
            "width_cells": grid.cols,
            "height_cells": grid.rows,
            "image_px": [image.width()?, image.height()?],
            "virtual": p.is_virtual()?,
        }));
    }
    Ok(out)
}

/// Feed `text` into a fresh terminal whose scrollback budget is `budget`
/// and report (total rows, history rows, last visible line).
fn run_text(text: &[u8], budget: usize) -> Result<(usize, usize, Option<Value>), Box<dyn Error>> {
    let replies = RefCell::new(Vec::new());
    let mut term = new_terminal(&replies, budget)?;
    feed(&mut term, text);
    let last = visible_lines(&term)?.pop();
    Ok((term.total_rows()?, term.scrollback_rows()?, last))
}

/// `max_scrollback` is documented as a line count but libghostty treats it
/// as a byte budget. There is no lines-based knob, so find the smallest byte
/// budget that retains at least `lines` rows of history for plain 120-column
/// text, by bisection on a synthetic stream (not on text.bin).
fn bytes_for_history_lines(lines: usize) -> Result<usize, Box<dyn Error>> {
    let mut synthetic = Vec::new();
    for i in 0..(lines + 2000) {
        synthetic.extend_from_slice(format!("calibration line {i}\r\n").as_bytes());
    }
    let (mut lo, mut hi) = (1usize, 256 * 1024 * 1024);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let (_, history, _) = run_text(&synthetic, mid)?;
        if history >= lines {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    Ok(lo)
}

fn main() -> Result<(), Box<dyn Error>> {
    graphics::set_png_decoder(Some(Box::new(PngDecoder)))?;

    // Part 1: kitten icat recording.
    let icat = std::fs::read("icat.bin")?;
    let replies = RefCell::new(Vec::new());
    let mut term = new_terminal(&replies, SCROLLBACK)?;
    feed(&mut term, &icat);

    let placements = placements(&term)?;
    let cursor = json!({ "row": term.cursor_y()?, "col": term.cursor_x()? });
    let lines = visible_lines(&term)?;
    drop(term);
    let reply_bytes = replies.into_inner();

    // Part 2: plain text, fresh terminal with the same settings.
    // As documented: max_scrollback = 10_000 ("lines").
    let text = std::fs::read("text.bin")?;
    let (total_rows, scrollback_rows, last_line) = run_text(&text, SCROLLBACK)?;
    // Workaround: a byte budget calibrated to hold >= 10,000 history lines.
    let budget = bytes_for_history_lines(SCROLLBACK)?;
    let (cal_total, cal_history, _) = run_text(&text, budget)?;

    let out = json!({
        "crate": "libghostty-vt 0.2.1",
        "icat": {
            "placements": placements,
            "cursor": cursor,
            "replies": {
                "escaped": reply_bytes.escape_ascii().to_string(),
                "len": reply_bytes.len(),
            },
            "visible_lines": lines,
        },
        "text": {
            "bytes_fed": text.len(),
            "max_scrollback_as_documented": {
                "max_scrollback": SCROLLBACK,
                "total_rows": total_rows,
                "scrollback_rows": scrollback_rows,
                "last_visible_line": last_line,
            },
            "max_scrollback_calibrated_to_10000_lines": {
                "max_scrollback_bytes": budget,
                "total_rows": cal_total,
                "scrollback_rows": cal_history,
            },
        },
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
