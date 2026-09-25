use std::sync::{Arc, Mutex};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::grid::Dimensions;
use rio_vt::crosswords::pos::{Column, Line, Pos};
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{EventListener, RioEvent, WindowId};
use rio_vt::performer::handler::Processor;
use serde_json::{json, Value};

const COLS: usize = 120;
const ROWS: usize = 40;
const CELL_W: u32 = 10;
const CELL_H: u32 = 22;
const SCROLLBACK: usize = 10_000;
const CHUNK: usize = 4096;

/// Collects everything the terminal pushes out. `PtyWrite` is the
/// terminal answering the program; the rest are recorded by name so a
/// reply that arrives through some other event is not silently lost.
#[derive(Clone, Default)]
struct Sink {
    replies: Arc<Mutex<Vec<String>>>,
    other_events: Arc<Mutex<Vec<String>>>,
}

impl EventListener for Sink {
    fn send_event(&self, event: RioEvent, _id: WindowId) {
        match event {
            RioEvent::PtyWrite(_route, text) => self.replies.lock().unwrap().push(text),
            other => self.other_events.lock().unwrap().push(format!("{other:?}")),
        }
    }
}

fn new_term(sink: Sink) -> Crosswords<Sink> {
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

fn feed(term: &mut Crosswords<Sink>, bytes: &[u8]) {
    let mut parser = Processor::default();
    for chunk in bytes.chunks(CHUNK) {
        parser.advance(term, chunk);
    }
}

fn main() {
    let dir = std::env::current_dir().expect("cwd");

    // --- Part 1: the kitty icat recording -------------------------------
    let icat = std::fs::read(dir.join("icat.bin")).expect("read icat.bin");
    let sink = Sink::default();
    let mut term = new_term(sink.clone());
    feed(&mut term, &icat);

    // Placements are anchored in an absolute row space:
    // lines_evicted + history_size + screen row (see place_kitty_overlay).
    let origin = term.lines_evicted() as i64 + term.history_size() as i64;
    let mut placements: Vec<Value> = term
        .graphics
        .kitty_placements
        .values()
        .map(|p| {
            json!({
                "image_id": p.image_id,
                "placement_id": p.placement_id,
                "row": p.dest_row - origin,
                "column": p.dest_col,
                "width_cells": p.columns,
                "height_cells": p.rows,
                "absolute_row": p.dest_row,
            })
        })
        .collect();
    placements.sort_by_key(|v| {
        (
            v["row"].as_i64().unwrap_or(0),
            v["column"].as_u64().unwrap_or(0),
            v["image_id"].as_u64().unwrap_or(0),
        )
    });
    let virtual_placements: Vec<Value> = term
        .graphics
        .kitty_virtual_placements
        .keys()
        .map(|(image, placement)| json!({"image_id": image, "placement_id": placement}))
        .collect();

    let cursor = term.grid.cursor.pos;

    let last_col = Column(term.columns() - 1);
    let mut lines: Vec<Value> = Vec::new();
    for r in 0..term.screen_lines() as i32 {
        let text =
            term.bounds_to_string(Pos::new(Line(r), Column(0)), Pos::new(Line(r), last_col));
        if !text.trim().is_empty() {
            lines.push(json!({"row": r, "text": text}));
        }
    }

    let replies = sink.replies.lock().unwrap().clone();
    let other_events = sink.other_events.lock().unwrap().clone();

    // --- Part 2: scrollback fill with ordinary text ---------------------
    let text = std::fs::read(dir.join("text.bin")).expect("read text.bin");
    let sink2 = Sink::default();
    let mut term2 = new_term(sink2);
    feed(&mut term2, &text);

    // Optional sanity check (stderr only): the last screen lines should
    // match the tail of text.bin, with no U+FFFD from chunk-split UTF-8.
    if std::env::var_os("PROBE_CHECK").is_some() {
        let last_col = Column(term2.columns() - 1);
        let mut bad = 0;
        let top = -(term2.history_size() as i32);
        for r in top..term2.screen_lines() as i32 {
            let t = term2
                .bounds_to_string(Pos::new(Line(r), Column(0)), Pos::new(Line(r), last_col));
            bad += t.matches('\u{FFFD}').count();
            if r >= term2.screen_lines() as i32 - 3 {
                eprintln!("row {r}: {t}");
            }
        }
        eprintln!("replacement chars in all {} rows: {bad}", term2.total_lines());
    }

    let out = json!({
        "crate": "rio-vt 0.5.28",
        "icat": {
            "placements": placements,
            "virtual_placements": virtual_placements,
            "cursor": {"row": cursor.row.0, "column": cursor.col.0},
            "replies": replies,
            "other_events": other_events,
            "visible_lines": lines,
        },
        "text": {
            "total_rows": term2.total_lines(),
            "visible_rows": term2.screen_lines(),
            "history_rows": term2.history_size(),
        },
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
