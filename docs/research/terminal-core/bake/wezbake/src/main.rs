//! The same bake-off as corebake and ghostbake, against wezterm-term (git b09b56c29c):
//! feed recorded program output and report pictures, cursor, replies and the screen text.
//!
//!     wezbake <capture.bin> [...]
//!
//! wezterm-term keeps no placement map. A placed picture is sliced into one ImageCell
//! per covered cell, so this harness rebuilds each placement from the cells: the
//! smallest rectangle holding every slice with the same image and placement id.
use std::collections::BTreeMap;
use std::io::Write;
use std::sync::{Arc, Mutex};
use wezterm_term::color::ColorPalette;
use wezterm_term::{Terminal, TerminalConfiguration, TerminalSize};

const COLS: usize = 120;
const ROWS: usize = 40;
const CW: usize = 10;
const CH: usize = 22;

#[derive(Debug)]
struct Config;
impl TerminalConfiguration for Config {
    fn color_palette(&self) -> ColorPalette {
        ColorPalette::default()
    }
    // Off by default in wezterm-term; a terminal that wants pictures turns it on.
    fn enable_kitty_graphics(&self) -> bool {
        true
    }
    fn scrollback_size(&self) -> usize {
        10_000
    }
}

#[derive(Clone, Default)]
struct Replies(Arc<Mutex<Vec<u8>>>);
impl Write for Replies {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn run(bytes: &[u8]) -> serde_json::Value {
    let replies = Replies::default();
    let size = TerminalSize { rows: ROWS, cols: COLS, pixel_width: COLS * CW, pixel_height: ROWS * CH, dpi: 96 };
    let mut term = Terminal::new(size, Arc::new(Config), "wezbake", "0", Box::new(replies.clone()));
    for chunk in bytes.chunks(4096) {
        term.advance_bytes(chunk);
    }
    // Replies go through a writer thread (ThreadedWriter); give it time to drain.
    std::thread::sleep(std::time::Duration::from_millis(300));

    let screen = term.screen();
    let history = screen.scrollback_rows();
    let lines = screen.lines_in_phys_range(0..history);
    let visible_from = history.saturating_sub(ROWS);
    // (image id, placement id) -> [min row, min col, max row, max col, visible?]
    let mut rects: BTreeMap<(u32, u32), [i64; 5]> = BTreeMap::new();
    let mut hashes = std::collections::BTreeSet::new();
    let mut image_cells = 0usize;
    let mut text = Vec::new();
    for (phys, line) in lines.iter().enumerate() {
        let row = phys as i64 - visible_from as i64;
        for cell in line.visible_cells() {
            if let Some(imgs) = cell.attrs().images() {
                for ic in imgs {
                    image_cells += 1;
                    hashes.insert(ic.image_data().hash());
                    let key = (ic.image_id().unwrap_or(0), ic.placement_id().unwrap_or(0));
                    let col = cell.cell_index() as i64;
                    let e = rects.entry(key).or_insert([row, col, row, col, 0]);
                    e[0] = e[0].min(row);
                    e[1] = e[1].min(col);
                    e[2] = e[2].max(row);
                    e[3] = e[3].max(col);
                    if row >= 0 {
                        e[4] = 1;
                    }
                }
            }
        }
        if row >= 0 {
            let s = line.as_str().trim_end().replace('\u{10EEEE}', "#");
            if !s.trim().is_empty() {
                text.push(format!("{row:>2}|{s}"));
            }
        }
    }
    let placements: Vec<_> = rects
        .iter()
        .map(|((img, pl), r)| {
            serde_json::json!({ "image": img, "placement": pl, "row": r[0], "col": r[1],
                "rows": r[2] - r[0] + 1, "cols": r[3] - r[1] + 1, "on_screen": r[4] == 1 })
        })
        .collect();
    let cur = term.cursor_pos();
    let out = replies.0.lock().unwrap().clone();
    serde_json::json!({
        "core": "wezterm-term git b09b56c29c",
        "placements": placements,
        "images_distinct": hashes.len(),
        "image_cells": image_cells,
        "cursor": [cur.y, cur.x],
        "replies": [String::from_utf8_lossy(&out).escape_default().to_string()],
        "text": text,
        "history_rows": history,
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

fn make() -> Terminal {
    let size = TerminalSize { rows: ROWS, cols: COLS, pixel_width: COLS * CW, pixel_height: ROWS * CH, dpi: 96 };
    Terminal::new(size, Arc::new(Config), "wezbake", "0", Box::new(Replies::default()))
}

/// Memory first, in a process that has done nothing else, then speed; the same
/// procedure as corebake's --perf. Each wezterm-term Terminal also starts a writer
/// thread for its replies, and that is part of what a pane costs.
fn perf_main(args: &[String]) {
    let text = std::fs::read(&args[0]).expect("text stream");
    let pic = std::fs::read(&args[1]).expect("picture recording");
    let panes: usize = args[2].parse().expect("panes");
    let before = rss();
    let mut terms: Vec<_> = (0..panes).map(|_| make()).collect();
    let empty = rss();
    for t in terms.iter_mut() {
        for c in text.chunks(4096) { t.advance_bytes(c); }
        for c in pic.chunks(4096) { t.advance_bytes(c); }
    }
    let full = rss();
    let kept = terms[0].screen().scrollback_rows();
    drop(terms);
    let mut runs = Vec::new();
    for _ in 0..5 {
        let mut t = make();
        let t0 = std::time::Instant::now();
        for c in text.chunks(4096) { t.advance_bytes(c); }
        runs.push((t0.elapsed().as_secs_f64() * 1e4).round() / 10.0);
    }
    let mut sorted = runs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let per_pane = |a: Option<u64>, b: Option<u64>| a.zip(b).map(|(a, b)| a.saturating_sub(b) / panes as u64);
    let v = serde_json::json!({
        "core": "wezterm-term git b09b56c29c", "panes": panes, "text_bytes": text.len(), "picture_bytes": pic.len(),
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
        let w = run(&bytes);
        let ms = t0.elapsed().as_secs_f64() * 1e3 - 300.0;
        out.insert(name, serde_json::json!({ "bytes": bytes.len(), "cut_before_alt_screen_exit": cut.is_some(), "wezterm": w, "wezterm_ms": (ms * 10.0).round() / 10.0 }));
    }
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
