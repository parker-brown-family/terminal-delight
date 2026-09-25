use std::sync::{Arc, Mutex};

use rio_vt::ansi::CursorShape;
use rio_vt::crosswords::pos::{Column, Line, Pos};
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{EventListener, RioEvent, WindowId};
use rio_vt::performer::handler::Processor;
use serde_json::json;

const COLS: usize = 120;
const ROWS: usize = 40;
const CELL_W: u32 = 10;
const CELL_H: u32 = 22;
const SCROLLBACK: usize = 10_000;
const CHUNK: usize = 4096;

/// Records every reply the terminal wants written back to the program.
#[derive(Clone, Default)]
struct Replies(Arc<Mutex<Vec<String>>>);

impl EventListener for Replies {
    fn send_event(&self, event: RioEvent, _window: WindowId) {
        if let RioEvent::PtyWrite(_route, text) = event {
            self.0.lock().unwrap().push(text);
        }
    }
}

fn new_term(listener: Replies) -> Crosswords<Replies> {
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
        listener,
        WindowId::from(0),
        0,
        SCROLLBACK,
    )
}

fn feed(term: &mut Crosswords<Replies>, bytes: &[u8]) {
    let mut parser = Processor::default();
    for chunk in bytes.chunks(CHUNK) {
        parser.advance(term, chunk);
    }
}

fn main() {
    let icat = std::fs::read("icat.bin").expect("read icat.bin");
    let text = std::fs::read("text.bin").expect("read text.bin");

    // Part 1: the kitty graphics recording.
    let replies = Replies::default();
    let mut term = new_term(replies.clone());
    feed(&mut term, &icat);

    // Placements store an absolute row: lines evicted off the ring +
    // history size + screen row. Convert back to a screen row.
    let top_abs = term.lines_evicted() as i64 + term.history_size() as i64;
    let mut placements: Vec<_> = term
        .graphics
        .kitty_placements
        .values()
        .map(|p| {
            json!({
                "image_id": p.image_id,
                "placement_id": p.placement_id,
                "row": p.dest_row - top_abs,
                "column": p.dest_col,
                "width_cells": p.columns,
                "height_cells": p.rows,
            })
        })
        .collect();
    placements.sort_by_key(|v| (v["row"].as_i64(), v["column"].as_u64()));

    let cursor = term.grid.cursor.pos;

    let mut lines = Vec::new();
    for row in 0..ROWS as i32 {
        let s = term.bounds_to_string(
            Pos::new(Line(row), Column(0)),
            Pos::new(Line(row), Column(COLS - 1)),
        );
        let s = s.trim_end().to_string();
        if !s.is_empty() {
            lines.push(json!({ "row": row, "text": s }));
        }
    }

    let replies_out: Vec<String> = replies.0.lock().unwrap().clone();

    // Part 2: plain text in a fresh terminal.
    let mut term2 = new_term(Replies::default());
    feed(&mut term2, &text);
    let history = term2.history_size();
    let screen = term2.screen_lines();

    let out = json!({
        "crate": "rio-vt 0.5.28",
        "icat": {
            "placements": placements,
            "cursor": { "row": cursor.row.0, "column": cursor.col.0 },
            "replies": replies_out,
            "visible_lines": lines,
        },
        "text": {
            "history_rows": history,
            "visible_rows": screen,
            "total_rows": history + screen,
        },
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
