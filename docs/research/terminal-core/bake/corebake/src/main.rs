//! Feed recorded program output to candidate terminal cores and report what each
//! did with the pictures: images stored, placements, cursor, and the text around.
//!
//!     corebake <capture.bin> [<capture.bin> ...]
//!
//! The recordings are the raw bytes image programs wrote to a stand-in terminal
//! that answered as kitty does (fakepty.py, identity `kitty`), at 120x40 cells of
//! 10x22 device pixels. Bytes are fed in 4096-byte chunks, the way a PTY read
//! would hand them over; a streaming parser must give the same result however
//! they are cut.
use std::sync::{Arc, Mutex};

const COLS: usize = 120;
const ROWS: usize = 40;
const CW: u32 = 10;
const CH: u32 = 22;

// ---------------------------------------------------------------- rio-vt

mod rio {
    use super::*;
    use rio_vt::ansi::CursorShape;
    use rio_vt::crosswords::pos::Column;
    use rio_vt::crosswords::{Crosswords, CrosswordsSize};
    use rio_vt::event::{EventListener, RioEvent, WindowId};
    use rio_vt::performer::handler::Processor;

    #[derive(Clone, Default)]
    pub struct Sink {
        pub replies: Arc<Mutex<Vec<String>>>,
        pub images: Arc<Mutex<Vec<(u32, usize, usize)>>>,
    }
    impl EventListener for Sink {
        fn send_event(&self, event: RioEvent, _w: WindowId) {
            match event {
                RioEvent::PtyWrite(_, text) => self.replies.lock().unwrap().push(text),
                RioEvent::UpdateGraphics { queues, .. } => {
                    for (id, g) in &queues.pending_images {
                        self.images.lock().unwrap().push((*id, g.width, g.height));
                    }
                }
                _ => {}
            }
        }
    }

    pub fn run(bytes: &[u8]) -> serde_json::Value {
        let sink = Sink::default();
        let size = CrosswordsSize::new_with_dimensions(COLS, ROWS, COLS as u32 * CW, ROWS as u32 * CH, CW, CH);
        let mut term = Crosswords::new(size, CursorShape::Block, sink.clone(), WindowId::from(0), 0, 10_000);
        let mut parser = Processor::default();
        for chunk in bytes.chunks(4096) {
            parser.advance(&mut term, chunk);
        }
        let g = &term.graphics;
        let mut placements: Vec<serde_json::Value> = g
            .kitty_placements
            .values()
            .map(|p| serde_json::json!({
                "image": p.image_id, "placement": p.placement_id,
                "row": p.dest_row, "col": p.dest_col, "cols": p.columns, "rows": p.rows,
            }))
            .collect();
        placements.sort_by_key(|v| v["row"].as_i64());
        let rows = term.visible_rows();
        let mut placeholder_cells = 0;
        let mut text: Vec<String> = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let mut line = String::new();
            for x in 0..COLS {
                let c = row[Column(x)].c();
                if c == '\u{10EEEE}' {
                    placeholder_cells += 1;
                    line.push('#');
                } else {
                    line.push(c);
                }
            }
            let t = line.trim_end().to_string();
            if !t.is_empty() {
                text.push(format!("{i:>2}|{t}"));
            }
        }
        let cur = term.cursor();
        serde_json::json!({
            "core": "rio-vt 0.5.28",
            "images_stored": g.kitty_images.len(),
            "images_emitted": sink.images.lock().unwrap().len(),
            "image_sizes": sink.images.lock().unwrap().iter().map(|(i, w, h)| format!("{i}:{w}x{h}")).collect::<Vec<_>>(),
            "placements": placements,
            "virtual_placements": g.kitty_virtual_placements.len(),
            "placeholder_cells_on_screen": placeholder_cells,
            "cursor": [cur.pos.row.0, cur.pos.col.0],
            "replies": sink.replies.lock().unwrap().iter().map(|r| r.escape_default().to_string()).collect::<Vec<_>>(),
            "text": text,
        })
    }
}

// ---------------------------------------------------------------- alacritty_terminal (the stay-put baseline)

mod alac {
    use super::*;
    use alacritty_terminal::event::{Event, EventListener};
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::{Config, Term};
    use alacritty_terminal::vte::ansi::Processor;

    #[derive(Clone, Default)]
    pub struct Sink(pub Arc<Mutex<Vec<String>>>);
    impl EventListener for Sink {
        fn send_event(&self, event: Event) {
            if let Event::PtyWrite(t) = event {
                self.0.lock().unwrap().push(t);
            }
        }
    }
    struct Size;
    impl Dimensions for Size {
        fn total_lines(&self) -> usize { ROWS }
        fn screen_lines(&self) -> usize { ROWS }
        fn columns(&self) -> usize { COLS }
    }

    pub fn run(bytes: &[u8]) -> serde_json::Value {
        let sink = Sink::default();
        let mut term = Term::new(Config::default(), &Size, sink.clone());
        let mut parser: Processor = Processor::new();
        for chunk in bytes.chunks(4096) {
            parser.advance(&mut term, chunk);
        }
        let grid = term.grid();
        let mut text = Vec::new();
        let mut placeholder_cells = 0;
        for i in 0..ROWS {
            let mut line = String::new();
            for x in 0..COLS {
                let c = grid[Line(i as i32)][Column(x)].c;
                if c == '\u{10EEEE}' { placeholder_cells += 1; line.push('#'); } else { line.push(c); }
            }
            let t = line.trim_end().to_string();
            if !t.is_empty() { text.push(format!("{i:>2}|{t}")); }
        }
        let cur = grid.cursor.point;
        serde_json::json!({
            "core": "alacritty_terminal 0.26",
            "images_stored": 0,
            "placements": [],
            "placeholder_cells_on_screen": placeholder_cells,
            "cursor": [cur.line.0, cur.column.0],
            "replies": sink.0.lock().unwrap().iter().map(|r| r.escape_default().to_string()).collect::<Vec<_>>(),
            "text": text,
        })
    }
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

/// Memory first, in a process that has done nothing else: `panes` terminals, each fed the
/// whole text stream and then one picture, all kept alive. Then speed: the text stream into
/// a fresh terminal, five times.
fn perf<T>(core: &str, make: impl Fn() -> T, feed: impl Fn(&mut T, &[u8]), rows_kept: impl Fn(&T) -> usize,
           text: &[u8], pic: &[u8], panes: usize) -> serde_json::Value {
    let before = rss();
    let mut terms: Vec<T> = (0..panes).map(|_| make()).collect();
    let empty = rss();
    for t in terms.iter_mut() {
        for c in text.chunks(4096) { feed(t, c); }
        for c in pic.chunks(4096) { feed(t, c); }
    }
    let full = rss();
    let kept = rows_kept(&terms[0]);
    drop(terms);
    let mut runs = Vec::new();
    for _ in 0..5 {
        let mut t = make();
        let t0 = std::time::Instant::now();
        for c in text.chunks(4096) { feed(&mut t, c); }
        runs.push((t0.elapsed().as_secs_f64() * 1e4).round() / 10.0);
    }
    let mut sorted = runs.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let per_pane = |a: Option<u64>, b: Option<u64>| a.zip(b).map(|(a, b)| (a.saturating_sub(b)) / panes as u64);
    serde_json::json!({
        "core": core, "panes": panes, "text_bytes": text.len(), "picture_bytes": pic.len(),
        "rss_kb": {"start": before.0, "after_create": empty.0, "after_fill": full.0, "high_water": full.1},
        "kb_per_pane_empty": per_pane(empty.0, before.0),
        "kb_per_pane_full": per_pane(full.0, before.0),
        "rows_kept": kept,
        "text_ms_runs": runs, "text_ms_median": sorted[2],
        "text_mb_per_s": (text.len() as f64 / 1048576.0) / (sorted[2] / 1000.0),
    })
}

fn perf_main(args: &[String]) {
    let core = args[0].as_str();
    let text = std::fs::read(&args[1]).expect("text stream");
    let pic = std::fs::read(&args[2]).expect("picture recording");
    let panes: usize = args[3].parse().expect("panes");
    let v = match core {
        "rio" => {
            use rio_vt::ansi::CursorShape;
            use rio_vt::crosswords::{Crosswords, CrosswordsSize};
            use rio_vt::event::WindowId;
            use rio_vt::performer::handler::Processor;
            perf("rio-vt 0.5.28",
                 || {
                     let size = CrosswordsSize::new_with_dimensions(COLS, ROWS, COLS as u32 * CW, ROWS as u32 * CH, CW, CH);
                     (Crosswords::new(size, CursorShape::Block, rio::Sink::default(), WindowId::from(0), 0, 10_000), Processor::default())
                 },
                 |t: &mut (Crosswords<rio::Sink>, Processor), c| t.1.advance(&mut t.0, c),
                 |t| t.0.history_size() + ROWS,
                 &text, &pic, panes)
        }
        "alacritty" => {
            use alacritty_terminal::grid::Dimensions;
            use alacritty_terminal::term::{Config, Term};
            use alacritty_terminal::vte::ansi::Processor;
            struct Size;
            impl Dimensions for Size {
                fn total_lines(&self) -> usize { ROWS }
                fn screen_lines(&self) -> usize { ROWS }
                fn columns(&self) -> usize { COLS }
            }
            perf("alacritty_terminal 0.26",
                 || (Term::new(Config::default(), &Size, alac::Sink::default()), Processor::<alacritty_terminal::vte::ansi::StdSyncHandler>::new()),
                 |t: &mut (Term<alac::Sink>, Processor), c| t.1.advance(&mut t.0, c),
                 |t| t.0.grid().history_size() + ROWS,
                 &text, &pic, panes)
        }
        other => panic!("unknown core {other}"),
    };
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
        // A full-screen program leaves the alternate screen when it exits, and
        // leaving it rightly clears that screen's pictures. To see what the core
        // held while the program was running, stop just before the last exit.
        let exit = b"\x1b[?1049l";
        let cut = bytes.windows(exit.len()).rposition(|w| w == exit);
        if let Some(i) = cut {
            bytes.truncate(i);
        }
        let name = std::path::Path::new(&path).file_stem().unwrap().to_string_lossy().to_string();
        let t0 = std::time::Instant::now();
        let r = rio::run(&bytes);
        let rio_ms = t0.elapsed().as_secs_f64() * 1e3;
        let t1 = std::time::Instant::now();
        let a = alac::run(&bytes);
        let alac_ms = t1.elapsed().as_secs_f64() * 1e3;
        out.insert(name, serde_json::json!({
            "bytes": bytes.len(),
            "cut_before_alt_screen_exit": cut.is_some(),
            "rio": r, "rio_ms": (rio_ms * 10.0).round() / 10.0,
            "alacritty": a, "alacritty_ms": (alac_ms * 10.0).round() / 10.0,
        }));
    }
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
