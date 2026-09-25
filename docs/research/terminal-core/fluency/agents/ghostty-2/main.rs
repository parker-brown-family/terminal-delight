//! Probe: feed recorded terminal output into libghostty-vt and report what it stored.

use std::cell::RefCell;
use std::error::Error;
use std::rc::Rc;

use libghostty_vt::alloc::{Allocator, Bytes};
use libghostty_vt::fmt::Format;
use libghostty_vt::kitty::graphics::{self, DecodePng, DecodedImage, PlacementIterator};
use libghostty_vt::selection::{FormatOptions, Selection};
use libghostty_vt::terminal::{Point, PointCoordinate, PointSpace};
use libghostty_vt::{Terminal, TerminalOptions};
use serde_json::{Value, json};

const COLS: u16 = 120;
const ROWS: u16 = 40;
const CELL_W: u32 = 10;
const CELL_H: u32 = 22;
const SCROLLBACK: usize = 10_000;
const CHUNK: usize = 4096;

const WORKSPACE: &str = "/home/parker/.cache/td-core-research/fluency/ghostty-2";

/// PNG decoder handed to libghostty-vt. The library wants 8-bit RGBA.
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
        let px = &buf[..info.buffer_size()];
        let rgba: Vec<u8> = match info.color_type {
            png::ColorType::Rgba => px.to_vec(),
            png::ColorType::Rgb => px.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
            png::ColorType::GrayscaleAlpha => {
                px.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
            }
            png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            // EXPAND turns indexed images into RGB(A), so this should not happen.
            png::ColorType::Indexed => return None,
        };
        let mut bytes = Bytes::new_with_alloc(alloc, rgba.len()).ok()?;
        bytes.copy_from_slice(&rgba);
        Some(DecodedImage { width: info.width, height: info.height, data: bytes })
    }
}

/// Build a terminal with the probe's fixed settings, recording every pty reply.
fn new_terminal(replies: Rc<RefCell<Vec<Vec<u8>>>>) -> Result<Terminal<'static, 'static>, Box<dyn Error>> {
    let mut term = Terminal::new(TerminalOptions { cols: COLS, rows: ROWS, max_scrollback: SCROLLBACK })?;
    // Cell pixel size is not part of TerminalOptions; it is only settable via resize().
    term.resize(COLS, ROWS, CELL_W, CELL_H)?;
    // libghostty-vt defaults to the direct medium only; the recording sends the
    // picture by file path (t=f) and queries the temp-file and shm media, as a
    // real terminal would accept.
    term.set_kitty_image_from_file_allowed(true)?
        .set_kitty_image_from_temp_file_allowed(true)?
        .set_kitty_image_from_shared_mem_allowed(true)?;
    term.on_pty_write(move |_t, data| replies.borrow_mut().push(data.to_vec()))?;
    Ok(term)
}

fn feed(term: &mut Terminal<'_, '_>, bytes: &[u8]) {
    for chunk in bytes.chunks(CHUNK) {
        term.vt_write(chunk);
    }
}

fn placements(term: &Terminal<'_, '_>) -> Result<Vec<Value>, Box<dyn Error>> {
    let graphics = term.kitty_graphics()?;
    let mut iter = PlacementIterator::new()?;
    let mut it = iter.update(&graphics)?;
    let mut out = Vec::new();
    while let Some(p) = it.next() {
        let image_id = p.image_id()?;
        let Some(image) = graphics.image(image_id) else {
            out.push(json!({ "image_id": image_id, "error": "placement refers to a missing image" }));
            continue;
        };
        let grid = p.grid_size(&image, term)?;
        let viewport = p.viewport_pos(&image, term)?;
        // Screen-space (scrollback-inclusive) origin via the placement's bounding rect.
        let rect = p.rect(&image, term)?;
        let screen = term.point_from_grid_ref(&rect.start(), PointSpace::Screen)?;
        out.push(json!({
            "image_id": image_id,
            "placement_id": p.placement_id()?,
            "row": viewport.map(|v| v.row),
            "column": viewport.map(|v| v.col),
            "screen_row": screen.map(|s| s.y),
            "screen_column": screen.map(|s| s.x),
            "width_cells": grid.cols,
            "height_cells": grid.rows,
            "virtual": p.is_virtual()?,
            "z": p.z()?,
            "image_px": [image.width()?, image.height()?],
        }));
    }
    Ok(out)
}

/// Non-empty rows of the visible viewport, one selection per row so the row index is exact.
fn visible_lines(term: &Terminal<'_, '_>) -> Result<Vec<Value>, Box<dyn Error>> {
    let cols = term.cols()?;
    let rows = term.rows()?;
    let mut out = Vec::new();
    for y in 0..u32::from(rows) {
        let start = term.grid_ref(Point::Viewport(PointCoordinate { x: 0, y }))?;
        let end = term.grid_ref(Point::Viewport(PointCoordinate { x: cols - 1, y }))?;
        let sel = Selection::new(start, end, false);
        let opts = FormatOptions::new()
            .with_emit_format(Format::Plain)
            .with_trim(true)
            .with_unwrap(false)
            .with_selection(&sel);
        let text = match term.format_selection_alloc(None, opts)? {
            Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            None => String::new(),
        };
        if !text.trim().is_empty() {
            out.push(json!({ "row": y, "text": text }));
        }
    }
    Ok(out)
}

fn main() -> Result<(), Box<dyn Error>> {
    graphics::set_png_decoder(Some(Box::new(PngDecoder)))?;

    // Steps 1-3: the kitty icat recording.
    let icat = std::fs::read(format!("{WORKSPACE}/icat.bin"))?;
    let replies = Rc::new(RefCell::new(Vec::new()));
    let mut term = new_terminal(Rc::clone(&replies))?;
    feed(&mut term, &icat);

    let placements = placements(&term)?;
    let lines = visible_lines(&term)?;
    let replies_json: Vec<Value> = replies
        .borrow()
        .iter()
        .map(|r| json!({ "text": String::from_utf8_lossy(r), "escaped": r.escape_ascii().to_string() }))
        .collect();
    let icat_report = json!({
        "input_bytes": icat.len(),
        "placements": placements,
        "cursor": { "row": term.cursor_y()?, "column": term.cursor_x()? },
        "replies": replies_json,
        "visible_lines": lines,
        "kitty_image_storage_limit_bytes": term.kitty_image_storage_limit()?,
    });
    drop(term);

    // Step 4: a fresh terminal, same settings, fed ordinary text.
    let text = std::fs::read(format!("{WORKSPACE}/text.bin"))?;
    let text_replies = Rc::new(RefCell::new(Vec::new()));
    let mut term = new_terminal(Rc::clone(&text_replies))?;
    feed(&mut term, &text);
    let newlines = text.iter().filter(|&&b| b == b'\n').count();
    let text_report = json!({
        "input_bytes": text.len(),
        "input_newlines": newlines,
        "max_scrollback_requested": SCROLLBACK,
        "total_rows": term.total_rows()?,
        "scrollback_rows": term.scrollback_rows()?,
        "viewport_rows": term.rows()?,
    });

    let report = json!({
        "crate": "libghostty-vt 0.2.1",
        "terminal": { "cols": COLS, "rows": ROWS, "cell_px": [CELL_W, CELL_H], "max_scrollback": SCROLLBACK },
        "icat": icat_report,
        "text": text_report,
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
