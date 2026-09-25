use std::sync::{Arc, Mutex};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::grid::Dimensions;
use rio_vt::crosswords::pos::{Column, Line, Pos};
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{EventListener, RioEvent, WindowId};
use rio_vt::performer::handler::Processor;
use serde_json::{Value, json};

const COLS: usize = 120;
const ROWS: usize = 40;
const CELL_W: u32 = 10;
const CELL_H: u32 = 22;
const SCROLLBACK: usize = 10_000;
const CHUNK: usize = 4096;

/// Collects everything the terminal wants to write back to the program.
#[derive(Clone, Default)]
struct Sink {
    replies: Arc<Mutex<Vec<String>>>,
    /// Reply requests that need a frontend callback to answer (colour
    /// queries, text-area size) — recorded so they are not silently lost.
    deferred: Arc<Mutex<Vec<String>>>,
}

impl Sink {
    fn record(&self, event: RioEvent) {
        match event {
            RioEvent::PtyWrite(_route, text) => self.replies.lock().unwrap().push(text),
            RioEvent::ColorRequest(route, idx, _) => self
                .deferred
                .lock()
                .unwrap()
                .push(format!("ColorRequest(route={route}, index={idx})")),
            RioEvent::TextAreaSizeRequest(route, _) => self
                .deferred
                .lock()
                .unwrap()
                .push(format!("TextAreaSizeRequest(route={route})")),
            RioEvent::ClipboardLoad(route, ty, _) => self
                .deferred
                .lock()
                .unwrap()
                .push(format!("ClipboardLoad(route={route}, {ty:?})")),
            _ => {}
        }
    }
}

impl EventListener for Sink {
    fn send_event(&self, event: RioEvent, _id: WindowId) {
        self.record(event);
    }
    fn send_event_with_high_priority(&self, event: RioEvent, _id: WindowId) {
        self.record(event);
    }
    fn send_global_event(&self, event: RioEvent) {
        self.record(event);
    }
}

fn new_terminal(sink: Sink) -> Crosswords<Sink> {
    let size = CrosswordsSize::new_with_dimensions(
        COLS,
        ROWS,
        COLS as u32 * CELL_W,
        ROWS as u32 * CELL_H,
        CELL_W,
        CELL_H,
    );
    Crosswords::new(
        size,
        CursorShape::Block,
        sink,
        WindowId::from(0),
        0,
        SCROLLBACK,
    )
}

fn feed(term: &mut Crosswords<Sink>, parser: &mut Processor, bytes: &[u8]) {
    for chunk in bytes.chunks(CHUNK) {
        parser.advance(term, chunk);
    }
    // Flush a synchronized update left open at end of input, if any.
    if parser.sync_bytes_count() > 0 {
        parser.stop_sync(term);
    }
}

fn workspace_file(name: &str) -> Vec<u8> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("reading {name}: {e}"))
}

fn picture_report() -> Value {
    let sink = Sink::default();
    let mut term = new_terminal(sink.clone());
    let mut parser = Processor::default();
    feed(&mut term, &mut parser, &workspace_file("icat.bin"));

    // Placement rows are stored in an absolute, scrollback-aware space:
    // lines ever evicted + current history + screen row.
    let top_abs = term.lines_evicted() as i64 + term.grid.history_size() as i64;

    let mut placements: Vec<Value> = term
        .graphics
        .kitty_placements
        .values()
        .map(|p| {
            json!({
                "image_id": p.image_id,
                "placement_id": p.placement_id,
                "row": p.dest_row - top_abs,
                "absolute_row": p.dest_row,
                "column": p.dest_col,
                "width_cells": p.columns,
                "height_cells": p.rows,
                "pixel_width": p.pixel_width,
                "pixel_height": p.pixel_height,
                "z_index": p.z_index,
            })
        })
        .collect();
    placements.sort_by_key(|v| (v["absolute_row"].as_i64(), v["column"].as_u64()));

    let virtual_placements: Vec<Value> = term
        .graphics
        .kitty_virtual_placements
        .keys()
        .map(|(image_id, placement_id)| json!({"image_id": image_id, "placement_id": placement_id}))
        .collect();

    let mut images: Vec<Value> = term
        .graphics
        .kitty_images
        .iter()
        .map(|(id, s)| json!({"image_id": id, "width_px": s.data.width, "height_px": s.data.height}))
        .collect();
    images.sort_by_key(|v| v["image_id"].as_u64());

    let cursor = term.grid.cursor.pos;

    let last_col = Column(term.columns() - 1);
    let mut lines = Vec::new();
    for row in 0..term.screen_lines() as i32 {
        let text = term.bounds_to_string(Pos::new(Line(row), Column(0)), Pos::new(Line(row), last_col));
        if !text.trim().is_empty() {
            lines.push(json!({"row": row, "text": text}));
        }
    }

    let replies = sink.replies.lock().unwrap().clone();
    let deferred = sink.deferred.lock().unwrap().clone();

    json!({
        "placements": placements,
        "virtual_placements": virtual_placements,
        "sixel_or_iterm2_placements": term.graphics.atlas_placements.len(),
        "stored_images": images,
        "cursor": {"row": cursor.row.0, "column": cursor.col.0},
        "replies": replies,
        "reply_requests_needing_frontend": deferred,
        "visible_lines": lines,
    })
}

fn scrollback_report() -> Value {
    let sink = Sink::default();
    let mut term = new_terminal(sink);
    let mut parser = Processor::default();
    let bytes = workspace_file("text.bin");
    feed(&mut term, &mut parser, &bytes);
    json!({
        "input_bytes": bytes.len(),
        "input_newlines": bytes.iter().filter(|&&b| b == b'\n').count(),
        "rows_held_total": term.grid.total_lines(),
        "history_rows": term.grid.history_size(),
        "visible_rows": term.screen_lines(),
    })
}

fn main() {
    let out = json!({
        "crate": "rio-vt 0.5.28",
        "terminal": {"columns": COLS, "rows": ROWS, "cell_px": [CELL_W, CELL_H], "scrollback": SCROLLBACK, "chunk_bytes": CHUNK},
        "icat": picture_report(),
        "text": scrollback_report(),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
