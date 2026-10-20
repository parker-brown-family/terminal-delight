//! The Terminal Lab's replay harness: bytes in, the screen they leave, and
//! whether it depends on how the bytes arrived.
//!
//! A case is a size and a list of operations — write these bytes, resize to
//! this, look now. [`replay`] runs one through [`Term`], the same boundary the
//! application reads, and records an [`Observation`] at every look: each row's
//! text and styles, the cursor, the modes, the history. Nothing here is a hash:
//! two observations that differ say *where*, which is what makes a failure
//! something a person can read and a minimizer can shrink.
//!
//! Two properties are checked, by the tests at the bottom on whichever core
//! this build carries, and by `scripts/lab/run.py` across both:
//!
//! - **Chunk invariance.** The same bytes, cut at different places, must leave
//!   the same screen. A program's output reaches the parser in whatever pieces
//!   the kernel hands the read loop, and TD's own guards in front of rio-vt
//!   (`kitty.rs`, `text.rs`, `compat.rs`) look for sequences that a cut can
//!   split.
//! - **Agreement between the cores.** rio-vt and alacritty fed the same bytes.
//!   A difference is evidence to classify, not proof that either one is right;
//!   `scripts/lab/known.json` names the ones already understood.
//!
//! Test-only, and it adds no runtime: `cfg(test)` from `vt/mod.rs`. The entry
//! point a script drives is [`replay_from_env`], an ignored test that reads a
//! case file and writes observations as JSON lines.
//!
//! Plan: `docs/plans/terminal-lab/plan.md`, the correctness lane.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    Cell, Color, Column, Event, Flags, Line, Listener, Rgb, Term, TermMode, TermSize, WindowSize,
};

/// The shape of a case file and of the observations written for it. 2 added
/// `sync_open` to observations and expectations (synchronized updates, DEC
/// 2026), and `events`: what the core told its owner (`Ear`).
pub const FORMAT_VERSION: u64 = 2;

/// Limits applied before a single byte reaches a parser. A generated or
/// minimized case that breaks one is refused as a setup failure, never
/// truncated: a truncated case would be a different case under the same name.
pub const MAX_COLUMNS: usize = 400;
pub const MAX_ROWS: usize = 200;
pub const MAX_BYTES: usize = 1 << 20;
pub const MAX_OPS: usize = 4096;
/// History rows an observation carries, nearest the screen first. The total
/// is always reported; only the rows themselves are bounded.
pub const HISTORY_ROWS_OBSERVED: usize = 64;
/// How long one case may run before it is reported as hung.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);

/// One thing a case does.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Write(Vec<u8>),
    Resize {
        columns: usize,
        rows: usize,
    },
    Checkpoint,
    /// A checkpoint that also states what must be there.
    Expect(Expect),
}

/// What a named case says the screen holds at a checkpoint. Each field is
/// optional because a case states only what it is about.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expect {
    #[serde(default)]
    pub rows: Vec<(i32, String)>,
    #[serde(default)]
    pub cursor: Option<(i32, usize)>,
    #[serde(default)]
    pub modes: std::collections::BTreeMap<String, bool>,
    #[serde(default)]
    pub history: Option<usize>,
    /// Whether a synchronized update (DEC 2026) is open: the core is holding
    /// the program's bytes back until it closes.
    #[serde(default)]
    pub sync_open: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawOp {
    Write(String),
    WriteHex(String),
    Resize([usize; 2]),
    Checkpoint,
    Expect(Expect),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    name: String,
    #[serde(default)]
    seed: Option<u64>,
    size: [usize; 2],
    ops: Vec<RawOp>,
}

#[derive(Deserialize)]
struct RawCorpus {
    format_version: u64,
    corpus_version: String,
    #[serde(default)]
    #[allow(dead_code)]
    note: Option<String>,
    cases: Vec<RawCase>,
}

/// A case: a starting size and what happens to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub corpus_version: String,
    pub name: String,
    pub seed: Option<u64>,
    pub columns: usize,
    pub rows: usize,
    pub ops: Vec<Op>,
}

/// Why a case produced no comparable observation, or produced a wrong one.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// The screen at a checkpoint is not what the case says it must be.
    StateMismatch {
        checkpoint: usize,
        detail: String,
    },
    /// The same bytes cut differently left a different screen.
    ChunkMismatch {
        checkpoint: usize,
        detail: String,
    },
    Crash {
        detail: String,
    },
    TimedOut {
        after: Duration,
    },
    SetupFailed {
        reason: String,
    },
}

impl Failure {
    pub fn kind(&self) -> &'static str {
        match self {
            Failure::StateMismatch { .. } => "state_mismatch",
            Failure::ChunkMismatch { .. } => "chunk_mismatch",
            Failure::Crash { .. } => "crash",
            Failure::TimedOut { .. } => "timed_out",
            Failure::SetupFailed { .. } => "setup_failed",
        }
    }

    pub fn to_json(&self) -> Value {
        let detail = match self {
            Failure::StateMismatch { checkpoint, detail }
            | Failure::ChunkMismatch { checkpoint, detail } => {
                json!({ "checkpoint": checkpoint, "detail": detail })
            }
            Failure::Crash { detail } => json!({ "detail": detail }),
            Failure::TimedOut { after } => json!({ "after_ms": after.as_millis() as u64 }),
            Failure::SetupFailed { reason } => json!({ "reason": reason }),
        };
        json!({ "kind": self.kind(), "detail": detail })
    }
}

/// How a case's writes are cut before they reach the parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkPlan {
    /// Each write in one call.
    Whole,
    /// One byte per call — every boundary a read can fall on.
    EveryByte,
    /// Cut at places drawn from this seed, 1 to 16 bytes apart.
    Seeded(u64),
    /// Every run of consecutive writes in one call: the case's own write
    /// boundaries removed. A cut plan written out as separate writes, then
    /// compared joined against whole, can be shrunk without moving its cuts.
    Joined,
}

impl ChunkPlan {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "whole" => Ok(ChunkPlan::Whole),
            "every-byte" => Ok(ChunkPlan::EveryByte),
            "joined" => Ok(ChunkPlan::Joined),
            other => other
                .strip_prefix("seed:")
                .and_then(|n| n.parse().ok())
                .map(ChunkPlan::Seeded)
                .ok_or_else(|| format!("unknown chunk plan {other:?}")),
        }
    }

    pub fn name(&self) -> String {
        match self {
            ChunkPlan::Whole => "whole".into(),
            ChunkPlan::EveryByte => "every-byte".into(),
            ChunkPlan::Joined => "joined".into(),
            ChunkPlan::Seeded(seed) => format!("seed:{seed}"),
        }
    }

    fn cuts(&self, len: usize, rng: &mut SplitMix) -> Vec<usize> {
        match self {
            ChunkPlan::Whole | ChunkPlan::Joined => vec![len],
            ChunkPlan::EveryByte => (1..=len).collect(),
            ChunkPlan::Seeded(_) => {
                let mut cuts = vec![];
                let mut at = 0;
                while at < len {
                    at = (at + 1 + (rng.next() % 16) as usize).min(len);
                    cuts.push(at);
                }
                cuts
            }
        }
    }
}

/// A fault planted on purpose, to prove the lab notices one. Each imitates a
/// real class of core bug; none exists outside this test module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mutation {
    None,
    /// The core ignores Erase in Line to the right (`CSI K`, `CSI 0 K`): a
    /// wrong-state bug that only a comparison against a sound core can see.
    EraseLineIgnored,
    /// An ESC that ends a read is lost: a parser that drops its state at a
    /// chunk boundary. Invisible when bytes arrive whole, so only the chunk
    /// comparison can see it.
    EscapeLostAtSeam,
}

impl Mutation {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "" | "none" => Ok(Mutation::None),
            "erase-line-ignored" => Ok(Mutation::EraseLineIgnored),
            "escape-lost-at-seam" => Ok(Mutation::EscapeLostAtSeam),
            other => Err(format!("unknown mutation {other:?}")),
        }
    }
}

/// splitmix64: small, fixed, and the same in `scripts/lab/run.py`, so a seed
/// names one case on every machine and in every language.
pub struct SplitMix(u64);

impl SplitMix {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

// --- reading cases ---------------------------------------------------------

fn hex_bytes(text: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !digits.len().is_multiple_of(2) {
        return Err(format!("odd number of hex digits in {text:?}"));
    }
    digits
        .chunks(2)
        .map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
                .ok_or_else(|| format!("not hex: {:?}", String::from_utf8_lossy(pair)))
        })
        .collect()
}

/// Read a corpus file's text. Every limit is checked here, before any case
/// can reach a parser.
pub fn parse_corpus(text: &str) -> Result<Vec<Case>, Failure> {
    let setup = |reason: String| Failure::SetupFailed { reason };
    let raw: RawCorpus =
        serde_json::from_str(text).map_err(|e| setup(format!("malformed corpus: {e}")))?;
    if raw.format_version != FORMAT_VERSION {
        return Err(setup(format!(
            "corpus format {} but this harness reads {FORMAT_VERSION}",
            raw.format_version
        )));
    }
    raw.cases
        .into_iter()
        .map(|case| {
            let ops = case
                .ops
                .into_iter()
                .map(|op| {
                    Ok(match op {
                        RawOp::Write(text) => Op::Write(text.into_bytes()),
                        RawOp::WriteHex(hex) => Op::Write(hex_bytes(&hex).map_err(setup)?),
                        RawOp::Resize([columns, rows]) => Op::Resize { columns, rows },
                        RawOp::Checkpoint => Op::Checkpoint,
                        RawOp::Expect(expect) => Op::Expect(expect),
                    })
                })
                .collect::<Result<Vec<_>, Failure>>()?;
            let case = Case {
                corpus_version: raw.corpus_version.clone(),
                name: case.name,
                seed: case.seed,
                columns: case.size[0],
                rows: case.size[1],
                ops,
            };
            check_limits(&case)?;
            Ok(case)
        })
        .collect()
}

pub fn check_limits(case: &Case) -> Result<(), Failure> {
    let refuse = |reason: String| {
        Err(Failure::SetupFailed {
            reason: format!("{}: {reason}", case.name),
        })
    };
    let sizes = std::iter::once((case.columns, case.rows)).chain(case.ops.iter().filter_map(
        |op| match op {
            Op::Resize { columns, rows } => Some((*columns, *rows)),
            _ => None,
        },
    ));
    for (columns, rows) in sizes {
        if columns > MAX_COLUMNS || rows > MAX_ROWS {
            return refuse(format!(
                "{columns}x{rows} is over the {MAX_COLUMNS}x{MAX_ROWS} limit"
            ));
        }
    }
    if case.ops.len() > MAX_OPS {
        return refuse(format!("{} operations, limit {MAX_OPS}", case.ops.len()));
    }
    let bytes: usize = case
        .ops
        .iter()
        .map(|op| match op {
            Op::Write(bytes) => bytes.len(),
            _ => 0,
        })
        .sum();
    if bytes > MAX_BYTES {
        return refuse(format!("{bytes} bytes, limit {MAX_BYTES}"));
    }
    Ok(())
}

// --- observing ---------------------------------------------------------------

/// What a core tells its owner while a case replays, in order: the replies it
/// would write back to the program (answers to DA, DSR, DECRQM, XTGETTCAP,
/// colour and size queries), the title, the bell, the clipboard. A screen can
/// be right while an answer is wrong, and a program waiting on a wrong answer
/// hangs, so each observation carries what was heard since the one before.
///
/// Nothing here goes anywhere: requests that need an answer from the window
/// are answered with fixed values, so both cores format the same input.
#[derive(Default)]
struct Ear(std::sync::Mutex<Vec<Value>>);

/// The colour and cell size a request is answered with.
const ANSWER_RGB: Rgb = Rgb {
    r: 0x12,
    g: 0x34,
    b: 0x56,
};
const ANSWER_SIZE: WindowSize = WindowSize {
    num_lines: 24,
    num_cols: 80,
    cell_width: 8,
    cell_height: 16,
};

impl Ear {
    fn take(&self) -> Vec<Value> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

impl Listener for Ear {
    fn send_event(&self, event: Event) {
        let heard = match event {
            Event::PtyWrite(text) => json!(["reply", text]),
            Event::TextAreaSizeRequest(answer) => json!(["reply", answer(ANSWER_SIZE)]),
            Event::ColorRequest(index, answer) => json!(["reply", answer(ANSWER_RGB), index]),
            Event::Title(title) => json!(["title", title]),
            Event::ResetTitle => json!(["title-reset"]),
            Event::Bell => json!(["bell"]),
            Event::ClipboardStore(kind, text) => json!(["clipboard", format!("{kind:?}"), text]),
            Event::CursorBlinkingChange => json!(["cursor-blinking"]),
            // Redraw hints and the pump's own messages: not the core's word.
            Event::MouseCursorDirty | Event::Wakeup | Event::ChildExit(_) | Event::Exit => return,
        };
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(heard);
    }
}

fn colour(color: Color) -> String {
    match color {
        Color::Named(named) => format!("{named:?}"),
        Color::Spec(rgb) => format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => format!("i{index}"),
    }
}

fn style(cell: &Cell) -> Value {
    json!([
        colour(cell.fg),
        colour(cell.bg),
        cell.flags.bits(),
        cell.underline_color().map(colour),
    ])
}

/// One row: its text (wide-character spacers skipped, combining marks kept,
/// trailing blanks trimmed) and, separately, every run of cells whose style is
/// not a blank's — `[first column, length, [fg, bg, flags, underline]]`. The
/// runs carry the wide and wrap flags, so the text needs no column padding.
fn row(term: &Term, line: Line) -> Value {
    let cells = term.row(line);
    let blank = style(&Cell::default());
    let mut text = String::new();
    for cell in cells.iter() {
        if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        text.push(cell.c);
        if let Some(marks) = cell.zerowidth() {
            text.extend(marks);
        }
    }
    let text = text.trim_end_matches(' ').to_string();
    let mut runs: Vec<Value> = vec![];
    let mut column = 0;
    while column < cells.len() {
        let current = style(&cells[Column(column)]);
        let mut end = column + 1;
        while end < cells.len() && style(&cells[Column(end)]) == current {
            end += 1;
        }
        if current != blank {
            runs.push(json!([column, end - column, current]));
        }
        column = end;
    }
    json!({ "text": text, "runs": runs })
}

fn mode_names(mode: TermMode) -> Vec<String> {
    mode.iter_names()
        .map(|(name, _)| name.to_string())
        .filter(|name| {
            !["MOUSE_MODE", "KITTY_KEYBOARD_PROTOCOL", "ANY", "NONE"].contains(&name.as_str())
        })
        .collect()
}

/// Everything a checkpoint records, in TD's own vocabulary.
pub fn observe(term: &Term, heard: Vec<Value>) -> Value {
    let screen: Vec<Value> = (0..term.screen_lines() as i32)
        .map(|line| row(term, Line(line)))
        .collect();
    let shown = term.history_size().min(HISTORY_ROWS_OBSERVED) as i32;
    let history: Vec<Value> = (1..=shown).map(|back| row(term, Line(-back))).collect();
    let cursor = term.cursor();
    json!({
        "size": [term.columns(), term.screen_lines()],
        "cursor": {
            "line": cursor.point.line.0,
            "column": cursor.point.column.0,
            "shape": format!("{:?}", cursor.shape),
        },
        "modes": mode_names(term.mode()),
        "history_size": term.history_size(),
        // An open synchronized update holds the program's bytes back, so the
        // screen here is the frame from before it, and whether one is open
        // is state of its own. Both cores set its deadline exactly while one
        // is open. The count of bytes held is not compared: TD's text guard
        // holds an unfinished sequence in front of rio-vt, alacritty holds it
        // in its own buffer, and neither is wrong.
        "sync_open": term.sync_deadline().is_some(),
        // What the core told its owner since the last checkpoint (`Ear`).
        "events": heard,
        "screen": screen,
        "history": history,
    })
}

fn check_expect(term: &Term, expect: &Expect) -> Result<(), String> {
    let mut wrong = vec![];
    for (line, want) in &expect.rows {
        let got = row(term, Line(*line));
        let got = got["text"].as_str().unwrap_or_default();
        if got != want {
            wrong.push(format!("row {line}: expected {want:?}, got {got:?}"));
        }
    }
    if let Some((line, column)) = expect.cursor {
        let cursor = term.cursor().point;
        if (cursor.line.0, cursor.column.0) != (line, column) {
            wrong.push(format!(
                "cursor: expected [{line}, {column}], got [{}, {}]",
                cursor.line.0, cursor.column.0
            ));
        }
    }
    let modes = mode_names(term.mode());
    for (mode, want) in &expect.modes {
        if TermMode::from_name(mode).is_none() {
            wrong.push(format!("mode {mode:?} names no mode"));
        } else if modes.contains(mode) != *want {
            wrong.push(format!("mode {mode}: expected {want}"));
        }
    }
    if let Some(history) = expect.history {
        if term.history_size() != history {
            wrong.push(format!(
                "history: expected {history} rows, got {}",
                term.history_size()
            ));
        }
    }
    if let Some(open) = expect.sync_open {
        if term.sync_deadline().is_some() != open {
            wrong.push(format!("synchronized update: expected open {open}"));
        }
    }
    if wrong.is_empty() {
        Ok(())
    } else {
        Err(wrong.join("; "))
    }
}

// --- replaying ---------------------------------------------------------------

fn erase_line_ignored(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        if rest.starts_with(b"\x1b[K") {
            at += 3;
        } else if rest.starts_with(b"\x1b[0K") {
            at += 4;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    out
}

/// Replay a case on this build's core, with no time limit. Each checkpoint
/// yields one observation; a broken expectation is a failure, carrying the
/// checkpoint it broke at.
pub fn replay_unguarded(
    case: &Case,
    plan: ChunkPlan,
    mutation: Mutation,
) -> Result<Vec<Value>, Failure> {
    check_limits(case)?;
    let ear = Arc::new(Ear::default());
    let mut term = Term::new(TermSize::new(case.columns, case.rows, 8, 16), ear.clone());
    let mut rng = SplitMix::new(match plan {
        ChunkPlan::Seeded(seed) => seed,
        _ => 0,
    });
    let mut observations = vec![];
    let mut joined: Vec<u8> = vec![];
    for op in &case.ops {
        if !matches!(op, Op::Write(_)) && !joined.is_empty() {
            term.advance(&joined);
            joined.clear();
        }
        match op {
            Op::Write(bytes) => {
                let bytes = match mutation {
                    Mutation::EraseLineIgnored => erase_line_ignored(bytes),
                    _ => bytes.clone(),
                };
                if plan == ChunkPlan::Joined {
                    joined.extend_from_slice(&bytes);
                    continue;
                }
                let mut from = 0;
                for to in plan.cuts(bytes.len(), &mut rng) {
                    let mut chunk = &bytes[from..to];
                    if mutation == Mutation::EscapeLostAtSeam
                        && to < bytes.len()
                        && chunk.last() == Some(&0x1b)
                    {
                        chunk = &chunk[..chunk.len() - 1];
                    }
                    term.advance(chunk);
                    from = to;
                }
            }
            Op::Resize { columns, rows } => {
                term.resize(TermSize::new(*columns, *rows, 8, 16));
            }
            Op::Checkpoint => observations.push(observe(&term, ear.take())),
            Op::Expect(expect) => {
                if let Err(detail) = check_expect(&term, expect) {
                    return Err(Failure::StateMismatch {
                        checkpoint: observations.len(),
                        detail: format!("{} (expected state): {detail}", case.name),
                    });
                }
                observations.push(observe(&term, ear.take()));
            }
        }
    }
    if !joined.is_empty() {
        term.advance(&joined);
    }
    observations.push(observe(&term, ear.take()));
    Ok(observations)
}

/// Run `work` on its own thread: a panic becomes [`Failure::Crash`] and a run
/// past `timeout` becomes [`Failure::TimedOut`]. A hung thread is abandoned,
/// not killed — the process ends soon after, and nothing it holds is shared.
pub fn guarded<T: Send + 'static>(
    timeout: Duration,
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = std::thread::Builder::new()
        .name("lab-case".into())
        .spawn(move || {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
            let _ = tx.send(outcome);
        })
        .map_err(|e| Failure::SetupFailed {
            reason: format!("could not start a case thread: {e}"),
        })?;
    match rx.recv_timeout(timeout) {
        Ok(Ok(result)) => {
            let _ = handle.join();
            result
        }
        Ok(Err(panic)) => {
            let detail = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "a panic with no message".into());
            Err(Failure::Crash { detail })
        }
        Err(_) => Err(Failure::TimedOut { after: timeout }),
    }
}

/// [`replay_unguarded`] under [`guarded`].
pub fn replay(
    case: &Case,
    plan: ChunkPlan,
    mutation: Mutation,
    timeout: Duration,
) -> Result<Vec<Value>, Failure> {
    let case = case.clone();
    guarded(timeout, move || replay_unguarded(&case, plan, mutation))
}

/// The first checkpoint at which two runs of one case differ, and how.
pub fn first_difference(a: &[Value], b: &[Value]) -> Option<(usize, String)> {
    if a.len() != b.len() {
        return Some((
            a.len().min(b.len()),
            format!("{} checkpoints against {}", a.len(), b.len()),
        ));
    }
    for (index, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            let fields = [
                "size",
                "cursor",
                "modes",
                "history_size",
                "sync_open",
                "events",
                "screen",
                "history",
            ];
            let differing: Vec<&str> = fields
                .iter()
                .copied()
                .filter(|field| x[*field] != y[*field])
                .collect();
            let mut detail = format!("differs in {}", differing.join(", "));
            if let (Some(xs), Some(ys)) = (x["screen"].as_array(), y["screen"].as_array()) {
                if let Some((line, (p, q))) =
                    xs.iter().zip(ys).enumerate().find(|(_, (p, q))| p != q)
                {
                    detail.push_str(&format!("; first at screen row {line}: {p} vs {q}"));
                }
            }
            return Some((index, detail));
        }
    }
    None
}

/// Replay a case whole and then cut by each plan; the first cut that leaves a
/// different screen is a [`Failure::ChunkMismatch`]. Returns the whole run's
/// observations when every cut agrees with it.
pub fn check_chunks(
    case: &Case,
    plans: &[ChunkPlan],
    mutation: Mutation,
    timeout: Duration,
) -> Result<Vec<Value>, Failure> {
    let whole = replay(case, ChunkPlan::Whole, mutation, timeout)?;
    for plan in plans {
        let cut = replay(case, *plan, mutation, timeout)?;
        if let Some((checkpoint, detail)) = first_difference(&whole, &cut) {
            return Err(Failure::ChunkMismatch {
                checkpoint,
                detail: format!("{} cut {}: {detail}", case.name, plan.name()),
            });
        }
    }
    Ok(whole)
}

/// The script's door. Reads `TD_LAB_IN` (a corpus file), cuts each write by
/// `TD_LAB_CHUNKS` (`whole`, `every-byte`, `seed:N`), plants
/// `TD_LAB_MUTATE` if set, and writes one JSON line per case to `TD_LAB_OUT`:
/// `{"case", "observations"}` or `{"case", "failure"}`, after a header line
/// naming the core. Ignored, so a plain `cargo test` never runs it.
#[test]
#[ignore = "driven by scripts/lab/run.py with TD_LAB_IN and TD_LAB_OUT"]
fn replay_from_env() {
    use std::io::Write;
    let var = |name: &str| std::env::var(name).unwrap_or_default();
    let input = var("TD_LAB_IN");
    let output = var("TD_LAB_OUT");
    assert!(
        !input.is_empty() && !output.is_empty(),
        "set TD_LAB_IN and TD_LAB_OUT"
    );
    let plan = ChunkPlan::parse(&if var("TD_LAB_CHUNKS").is_empty() {
        "whole".into()
    } else {
        var("TD_LAB_CHUNKS")
    })
    .expect("TD_LAB_CHUNKS");
    let mutation = Mutation::parse(&var("TD_LAB_MUTATE")).expect("TD_LAB_MUTATE");
    let timeout = var("TD_LAB_TIMEOUT_MS")
        .parse()
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_TIMEOUT);

    let mut out = std::io::BufWriter::new(std::fs::File::create(&output).expect("TD_LAB_OUT"));
    let header = json!({
        "format_version": FORMAT_VERSION,
        "core": super::CORE_NAME,
        "chunks": plan.name(),
        "mutation": var("TD_LAB_MUTATE"),
    });
    writeln!(out, "{header}").expect("write header");
    let text = std::fs::read_to_string(&input).expect("TD_LAB_IN");
    let cases = match parse_corpus(&text) {
        Ok(cases) => cases,
        Err(failure) => {
            writeln!(
                out,
                "{}",
                json!({ "case": null, "failure": failure.to_json() })
            )
            .expect("write");
            return;
        }
    };
    for case in &cases {
        let line = match replay(case, plan, mutation, timeout) {
            Ok(observations) => json!({ "case": case.name, "observations": observations }),
            Err(failure) => json!({ "case": case.name, "failure": failure.to_json() }),
        };
        writeln!(out, "{line}").expect("write case");
        out.flush().expect("flush");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named() -> Vec<Case> {
        parse_corpus(include_str!("lab/named.json")).expect("the named corpus parses")
    }

    fn run(case: &Case, plan: ChunkPlan, mutation: Mutation) -> Result<Vec<Value>, Failure> {
        replay(case, plan, mutation, DEFAULT_TIMEOUT)
    }

    /// Every hand-written case meets its own expectations on this core,
    /// whole and cut every way. One test for all of them, so a failure lists
    /// every case that broke rather than the first.
    #[test]
    fn named_cases_hold_and_do_not_depend_on_where_reads_end() {
        let plans = [
            ChunkPlan::EveryByte,
            ChunkPlan::Seeded(1),
            ChunkPlan::Seeded(2),
            ChunkPlan::Seeded(3),
        ];
        let broken: Vec<String> = named()
            .iter()
            .filter_map(|case| {
                check_chunks(case, &plans, Mutation::None, DEFAULT_TIMEOUT)
                    .err()
                    .map(|failure| format!("{}: {failure:?}", case.name))
            })
            .collect();
        assert!(
            broken.is_empty(),
            "on {}:\n{}",
            super::super::CORE_NAME,
            broken.join("\n")
        );
    }

    /// The planted erase fault is seen by the case that erases, and by an
    /// expectation rather than by luck: the controls are what make a green
    /// lab mean something.
    #[test]
    fn a_core_that_ignores_erase_in_line_is_caught() {
        let case = named()
            .into_iter()
            .find(|c| c.name == "colour-and-erase")
            .expect("colour-and-erase");
        let failure = run(&case, ChunkPlan::Whole, Mutation::EraseLineIgnored)
            .expect_err("the planted fault must break an expectation");
        assert_eq!(failure.kind(), "state_mismatch", "{failure:?}");
    }

    /// The seam fault is invisible to whole writes and visible to cut ones —
    /// which is the whole argument for replaying every case in pieces.
    #[test]
    fn a_parser_that_loses_an_escape_at_a_read_boundary_is_caught_only_by_cutting() {
        let case = named()
            .into_iter()
            .find(|c| c.name == "cursor-movement")
            .expect("cursor-movement");
        let whole = run(&case, ChunkPlan::Whole, Mutation::EscapeLostAtSeam)
            .expect("whole writes have no seam to lose an escape at");
        let clean = run(&case, ChunkPlan::Whole, Mutation::None).expect("clean");
        assert_eq!(first_difference(&whole, &clean), None);
        let caught = match check_chunks(
            &case,
            &[ChunkPlan::EveryByte],
            Mutation::EscapeLostAtSeam,
            DEFAULT_TIMEOUT,
        ) {
            Err(failure) => ["chunk_mismatch", "state_mismatch"].contains(&failure.kind()),
            Ok(_) => false,
        };
        assert!(caught, "a lost escape at a seam went unnoticed");
    }

    #[test]
    fn an_observation_carries_what_the_core_said_since_the_last_one() {
        let case = Case {
            corpus_version: "test".into(),
            name: "events".into(),
            seed: None,
            columns: 8,
            rows: 2,
            ops: vec![
                Op::Write(b"ab\x1b[6n\x07".to_vec()),
                Op::Checkpoint,
                Op::Checkpoint,
            ],
        };
        let observations = run(&case, ChunkPlan::Whole, Mutation::None).expect("replays");
        assert_eq!(
            observations[0]["events"],
            json!([["reply", "\x1b[1;3R"], ["bell"]]),
            "the cursor report and the bell, in order"
        );
        assert_eq!(observations[1]["events"], json!([]), "each heard once");
    }

    #[test]
    fn an_observation_says_whether_a_synchronized_update_is_open() {
        let case = Case {
            corpus_version: "test".into(),
            name: "sync".into(),
            seed: None,
            columns: 8,
            rows: 1,
            ops: vec![
                Op::Write(b"a\x1b[?2026hb".to_vec()),
                Op::Checkpoint,
                Op::Write(b"\x1b[?2026l".to_vec()),
                Op::Checkpoint,
            ],
        };
        let observations = run(&case, ChunkPlan::Whole, Mutation::None).expect("replays");
        assert_eq!(observations[0]["sync_open"], true);
        assert_eq!(observations[0]["screen"][0]["text"], "a", "held back");
        assert_eq!(observations[1]["sync_open"], false);
        assert_eq!(observations[1]["screen"][0]["text"], "ab");
    }

    /// A UTF-8 character split across reads, completed by a read that holds
    /// more text after it. The lab found alacritty's parser losing that text
    /// (vte 0.15.0, alacritty/vte#159: fixed on vte master, in no release).
    /// rio-vt keeps it. On the fallback core this pins the loss, so the day a
    /// vte release fixes it this fails and says what to delete.
    #[test]
    fn a_character_split_across_reads_keeps_the_one_after_it() {
        let case = Case {
            corpus_version: "test".into(),
            name: "split-utf8".into(),
            seed: None,
            columns: 8,
            rows: 1,
            ops: vec![
                Op::Write(vec![0xc3]),
                Op::Write(vec![0xa9, b'a', 0xc3]),
                Op::Write(vec![0xa9, b'b']),
            ],
        };
        let observations = run(&case, ChunkPlan::Whole, Mutation::None).expect("replays");
        let text = observations[0]["screen"][0]["text"].clone();
        if cfg!(feature = "core-alacritty") {
            assert_eq!(
                text, "ééb",
                "vte now keeps the character after a split one: a release carries \
                 alacritty/vte#162. Delete this arm and the \
                 vte-split-utf8-drops-the-next-character entry in scripts/lab/known.json"
            );
        } else {
            assert_eq!(text, "éaéb");
        }
    }

    #[test]
    fn chunk_plans_cover_every_byte_exactly_once() {
        for plan in [
            ChunkPlan::Whole,
            ChunkPlan::EveryByte,
            ChunkPlan::Seeded(7),
            ChunkPlan::Joined,
        ] {
            for len in [0, 1, 2, 17, 100] {
                let cuts = plan.cuts(len, &mut SplitMix::new(7));
                let mut from = 0;
                for to in &cuts {
                    assert!(*to > from || len == 0, "{plan:?} {len}: empty chunk");
                    from = *to;
                }
                assert_eq!(from, len, "{plan:?} {len}: bytes left over");
            }
        }
    }

    /// The same numbers `scripts/lab/run.py` produces; if they part, a seed
    /// names two different cases.
    #[test]
    fn splitmix_matches_the_python_generator() {
        let mut rng = SplitMix::new(0);
        assert_eq!(rng.next(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next(), 0x6E78_9E6A_A1B9_65F4);
    }

    #[test]
    fn a_case_over_a_limit_is_refused_before_any_parser_sees_it() {
        let big = Case {
            corpus_version: "test".into(),
            name: "too-wide".into(),
            seed: None,
            columns: MAX_COLUMNS + 1,
            rows: 4,
            ops: vec![Op::Write(b"x".to_vec())],
        };
        let failure = run(&big, ChunkPlan::Whole, Mutation::None).expect_err("refused");
        assert_eq!(failure.kind(), "setup_failed");

        let flood = Case {
            name: "too-many-bytes".into(),
            columns: 10,
            ops: vec![Op::Write(vec![b'x'; MAX_BYTES + 1])],
            ..big
        };
        assert_eq!(
            run(&flood, ChunkPlan::Whole, Mutation::None)
                .expect_err("refused")
                .kind(),
            "setup_failed"
        );
    }

    #[test]
    fn a_malformed_corpus_is_a_setup_failure_not_an_empty_one() {
        for text in [
            "",
            "{",
            r#"{"format_version": 1, "corpus_version": "x", "cases": []}"#,
            r#"{"format_version": 2, "corpus_version": "x", "cases": [{"name": "a", "size": [5, 5], "ops": [{"write_hex": "1"}]}]}"#,
            r#"{"format_version": 2, "corpus_version": "x", "cases": [{"name": "a", "size": [5, 5], "ops": [{"wirte": "x"}]}]}"#,
            r#"{"format_version": 2, "corpus_version": "x", "cases": [{"name": "a", "size": [5, 5], "ops": [{"expect": {"sync_opne": true}}]}]}"#,
        ] {
            let failure = parse_corpus(text).expect_err(text);
            assert_eq!(failure.kind(), "setup_failed", "{text}");
        }
    }

    #[test]
    fn a_panic_is_a_crash_and_a_hang_is_a_timeout() {
        let crash = guarded(Duration::from_secs(5), || -> Result<(), Failure> {
            panic!("boom")
        });
        assert_eq!(
            crash,
            Err(Failure::Crash {
                detail: "boom".into()
            })
        );
        let hang = guarded(Duration::from_millis(50), || -> Result<(), Failure> {
            std::thread::sleep(Duration::from_secs(2));
            Ok(())
        });
        assert_eq!(
            hang,
            Err(Failure::TimedOut {
                after: Duration::from_millis(50)
            })
        );
    }
}
