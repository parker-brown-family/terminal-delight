//! probe: feed two recordings through libghostty-vt and report what the
//! terminal stored, as JSON on stdout.

use std::cell::RefCell;

use libghostty_vt::{
    Terminal, TerminalOptions,
    alloc::{Allocator, Bytes},
    kitty::graphics::{self, DecodePng, DecodedImage, PlacementIterator},
    screen::CellWide,
    terminal::{Point, PointCoordinate},
};
use serde_json::{Value, json};

const WORKSPACE: &str = "/home/parker/.cache/td-core-research/fluency/ghostty-3";
const COLS: u16 = 120;
const ROWS: u16 = 40;
const CELL_W: u32 = 10;
const CELL_H: u32 = 22;
const SCROLLBACK: usize = 10_000;
const CHUNK: usize = 4096;

/// PNG decoder handed to libghostty. The crate's own `RustPngDecoder` has no
/// public constructor in 0.2.1, so this is written against the `png` crate.
/// libghostty wants 8-bit RGBA, allocated through the allocator it passes in.
struct PngDecoder;

impl DecodePng for PngDecoder {
    fn decode_png<'alloc>(
        &mut self,
        alloc: &'alloc Allocator<'_>,
        data: &[u8],
    ) -> Option<DecodedImage<'alloc>> {
        let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
        // Expand palette / low bit depths and strip 16-bit down to 8-bit.
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder.read_info().ok()?;
        let mut buf = vec![0u8; reader.output_buffer_size()?];
        let info = reader.next_frame(&mut buf).ok()?;
        let src = &buf[..info.buffer_size()];
        let pixels = (info.width as usize) * (info.height as usize);

        let mut out = Bytes::new_with_alloc(alloc, pixels * 4).ok()?;
        match info.color_type {
            png::ColorType::Rgba => out.copy_from_slice(src),
            png::ColorType::Rgb => {
                for (o, i) in out.chunks_exact_mut(4).zip(src.chunks_exact(3)) {
                    o.copy_from_slice(&[i[0], i[1], i[2], 255]);
                }
            }
            png::ColorType::GrayscaleAlpha => {
                for (o, i) in out.chunks_exact_mut(4).zip(src.chunks_exact(2)) {
                    o.copy_from_slice(&[i[0], i[0], i[0], i[1]]);
                }
            }
            png::ColorType::Grayscale => {
                for (o, &g) in out.chunks_exact_mut(4).zip(src.iter()) {
                    o.copy_from_slice(&[g, g, g, 255]);
                }
            }
            // normalize_to_color8 expands palettes, so this should not occur.
            png::ColorType::Indexed => return None,
        }
        Some(DecodedImage {
            width: info.width,
            height: info.height,
            data: out,
        })
    }
}

fn new_terminal<'cb>() -> Terminal<'static, 'cb> {
    let mut term = Terminal::new(TerminalOptions {
        cols: COLS,
        rows: ROWS,
        max_scrollback: SCROLLBACK,
    })
    .expect("create terminal");
    // Cell pixel size only reaches the terminal through resize().
    term.resize(COLS, ROWS, CELL_W, CELL_H).expect("set cell size");
    term
}

fn feed(term: &mut Terminal<'_, '_>, bytes: &[u8]) {
    for chunk in bytes.chunks(CHUNK) {
        term.vt_write(chunk);
    }
}

/// Text of every non-empty row of the viewport, read cell by cell.
fn visible_lines(term: &Terminal<'_, '_>) -> Vec<Value> {
    let rows = term.rows().expect("rows");
    let cols = term.cols().expect("cols");
    let mut out = Vec::new();
    let mut graphemes = vec!['\0'; 32];
    for y in 0..rows {
        let mut line = String::new();
        for x in 0..cols {
            let gref = term
                .grid_ref(Point::Viewport(PointCoordinate { x, y: u32::from(y) }))
                .expect("grid ref");
            let cell = gref.cell().expect("cell");
            match cell.wide().expect("wide") {
                CellWide::SpacerTail | CellWide::SpacerHead => continue,
                CellWide::Narrow | CellWide::Wide => {}
            }
            let n = gref.graphemes(&mut graphemes).expect("graphemes");
            if n == 0 {
                line.push(' ');
            } else {
                line.extend(&graphemes[..n]);
            }
        }
        let trimmed = line.trim_end();
        if !trimmed.is_empty() {
            out.push(json!({ "row": y, "text": trimmed }));
        }
    }
    out
}

/// Every placement in the active screen's Kitty image storage.
fn placements(term: &Terminal<'_, '_>) -> Vec<Value> {
    let storage = term.kitty_graphics().expect("kitty graphics storage");
    let mut iter = PlacementIterator::new().expect("placement iterator");
    let mut it = iter.update(&storage).expect("iterate placements");
    let mut out = Vec::new();
    while let Some(p) = it.next() {
        let image_id = p.image_id().expect("image id");
        let Some(image) = storage.image(image_id) else {
            out.push(json!({ "image_id": image_id, "error": "placement refers to a missing image" }));
            continue;
        };
        let grid = p.grid_size(&image, term).expect("grid size");
        let pos = p.viewport_pos(&image, term).expect("viewport pos");
        out.push(json!({
            "image_id": image_id,
            "placement_id": p.placement_id().expect("placement id"),
            "virtual": p.is_virtual().expect("virtual"),
            // Viewport-relative; null when the placement is off-screen or virtual.
            "row": pos.map(|v| v.row),
            "column": pos.map(|v| v.col),
            "width_cells": grid.cols,
            "height_cells": grid.rows,
            "image_px": [image.width().expect("w"), image.height().expect("h")],
        }));
    }
    out
}

fn read(name: &str) -> Vec<u8> {
    let path = format!("{WORKSPACE}/{name}");
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn main() {
    graphics::set_png_decoder(Some(Box::new(PngDecoder))).expect("install png decoder");

    // Step 1-3: the icat recording.
    let replies: RefCell<Vec<Vec<u8>>> = RefCell::new(Vec::new());
    let icat = {
        let mut term = new_terminal();
        // Replies to queries only come out if a pty-write handler is set.
        // (No device-attributes handler is needed: the C layer answers CSI c
        // with its VT220 default by itself.)
        term.on_pty_write(|_, data| replies.borrow_mut().push(data.to_vec()))
            .expect("pty write handler");
        // Kitty graphics are off until a storage limit is set, and only the
        // direct medium is allowed by default. The recording sends its picture
        // as a file path (t=f), so the file medium has to be switched on.
        term.set_kitty_image_storage_limit(320 * 1024 * 1024)
            .expect("enable kitty graphics")
            .set_kitty_image_from_file_allowed(true)
            .expect("allow file medium");

        feed(&mut term, &read("icat.bin"));

        json!({
            "placements": placements(&term),
            "cursor": { "row": term.cursor_y().expect("cursor y"), "column": term.cursor_x().expect("cursor x") },
            "replies": replies.borrow().iter().map(|r| String::from_utf8_lossy(r).into_owned()).collect::<Vec<_>>(),
            "visible_lines": visible_lines(&term),
        })
    };

    // Step 4: a fresh terminal with the same settings, fed ordinary text.
    let text = {
        let mut term = new_terminal();
        feed(&mut term, &read("text.bin"));
        let total = term.total_rows().expect("total rows");
        let history = term.scrollback_rows().expect("scrollback rows");
        json!({
            "total_rows": total,
            "scrollback_rows": history,
            "viewport_rows": term.rows().expect("rows"),
            // Same reader as the icat step; proves an empty list there is real.
            "non_empty_visible_lines": visible_lines(&term).len(),
        })
    };

    let out = json!({
        "crate": "libghostty-vt 0.2.1",
        "terminal": { "cols": COLS, "rows": ROWS, "cell_px": [CELL_W, CELL_H], "max_scrollback": SCROLLBACK },
        "icat": icat,
        "text": text,
    });
    println!("{}", serde_json::to_string_pretty(&out).expect("serialize"));
}
