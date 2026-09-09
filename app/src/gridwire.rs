//! A terminal's visible truth, as bytes another terminal can eat.
//!
//! When the GUI attaches to a pane the session host is already running, the
//! client's emulator starts empty: it has missed everything the shell ever
//! printed. [`encode_snapshot`] closes that gap by reading the host's grid and
//! re-emitting it as ordinary VT bytes — the same bytes a program could have
//! written — so the client's stock emulator arrives at the same state by the
//! only route it knows. [`grid_hash`] then lets the two sides agree, cheaply
//! and across processes, that they really did.
//!
//! **The encoder never mutates the terminal it reads.** That is a `&Term`, not
//! a convention: the first design of this module reached the primary grid
//! hidden under an alt screen by flipping alacritty's screens twice and calling
//! the pair neutral. It is not neutral — `Term::swap_alt` toggles the mode bit
//! unconditionally, so the second flip runs the destructive branch and its
//! `reset_region` erases the grid it just showed. One snapshot of a pane
//! running vim would have wiped that vim. The type signature is the fix that
//! cannot be argued with.
//!
//! **What a snapshot carries:** every row of scrollback and screen with its
//! colours, attributes, wide characters, combining marks and soft-wrap
//! structure; the cursor's position and visibility; and the input-affecting
//! modes. Rows are painted first and modes restored last, because several
//! modes (insert, origin, wrap) change how the painting itself would land.
//!
//! **What it does not, and why that is stated rather than discovered:** an alt
//! screen has no scrollback of its own and the primary grid beneath it cannot
//! be read without mutating the terminal, so attaching to a pane inside vim
//! shows vim exactly and leaves the history behind it empty until vim exits,
//! when the host re-snapshots. Scroll regions, tab stops, the saved cursor,
//! window title and charset selection are not carried; every program that sets
//! them redraws when the terminal tells it the size, and a pane that lost them
//! is a pane that redraws, not one that lies. None of this loses anything
//! host-side: the authoritative terminal keeps all of it.

use alacritty_terminal::grid::{Dimensions, Grid};
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// Attributes SGR can express, in the form a cell holds them.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Style {
    fg: Color,
    bg: Color,
    flags: Flags,
    underline: Option<Color>,
}

/// The attributes a freshly-reset terminal writes with. Emitting nothing must
/// leave a cell equal to `Cell::default()`, so this has to match it.
impl Default for Style {
    fn default() -> Self {
        Self {
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            underline: None,
        }
    }
}

impl Style {
    fn of(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & SGR_FLAGS,
            underline: cell.underline_color(),
        }
    }
}

/// The flags SGR sets. The rest — wrap, wide-char and its spacers — are
/// consequences of *what* is written and where, not of an attribute, so they
/// are reproduced by the writing itself.
const SGR_FLAGS: Flags = Flags::INVERSE
    .union(Flags::BOLD)
    .union(Flags::ITALIC)
    .union(Flags::DIM)
    .union(Flags::HIDDEN)
    .union(Flags::STRIKEOUT)
    .union(Flags::ALL_UNDERLINES);

/// The modes that change how a client behaves rather than how it looks, and
/// that a program would otherwise have to be asked to set again. Restored
/// after the paint, never before it: `INSERT` would make the paint insert,
/// `ORIGIN` would move where the cursor lands, and clearing `LINE_WRAP` would
/// break every soft-wrapped row.
const RESTORED_MODES: &[(TermMode, &str)] = &[
    (TermMode::APP_CURSOR, "?1"),
    (TermMode::APP_KEYPAD, "?66"),
    (TermMode::MOUSE_REPORT_CLICK, "?1000"),
    (TermMode::MOUSE_DRAG, "?1002"),
    (TermMode::MOUSE_MOTION, "?1003"),
    (TermMode::FOCUS_IN_OUT, "?1004"),
    (TermMode::UTF8_MOUSE, "?1005"),
    (TermMode::SGR_MOUSE, "?1006"),
    (TermMode::ALTERNATE_SCROLL, "?1007"),
    (TermMode::BRACKETED_PASTE, "?2004"),
    (TermMode::LINE_FEED_NEW_LINE, "20"),
    (TermMode::INSERT, "4"),
    (TermMode::ORIGIN, "?6"),
    (TermMode::LINE_WRAP, "?7"),
];

/// One SGR sequence carrying a whole style: a reset, then everything that is
/// not the default. Emitted only where the style changes, so a run of text in
/// one colour costs one sequence.
fn sgr(style: &Style) -> String {
    let mut params = vec!["0".to_string()];
    let f = style.flags;
    if f.contains(Flags::BOLD) {
        params.push("1".into());
    }
    if f.contains(Flags::DIM) {
        params.push("2".into());
    }
    if f.contains(Flags::ITALIC) {
        params.push("3".into());
    }
    // The underline family is one attribute with five shapes; a cell carries
    // at most one of them.
    if f.contains(Flags::DOUBLE_UNDERLINE) {
        params.push("4:2".into());
    } else if f.contains(Flags::UNDERCURL) {
        params.push("4:3".into());
    } else if f.contains(Flags::DOTTED_UNDERLINE) {
        params.push("4:4".into());
    } else if f.contains(Flags::DASHED_UNDERLINE) {
        params.push("4:5".into());
    } else if f.contains(Flags::UNDERLINE) {
        params.push("4".into());
    }
    if f.contains(Flags::INVERSE) {
        params.push("7".into());
    }
    if f.contains(Flags::HIDDEN) {
        params.push("8".into());
    }
    if f.contains(Flags::STRIKEOUT) {
        params.push("9".into());
    }
    if let Some(p) = color_params(style.fg, false) {
        params.push(p);
    }
    if let Some(p) = color_params(style.bg, true) {
        params.push(p);
    }
    if let Some(u) = style.underline {
        params.push(underline_color_params(u));
    }
    format!("\x1b[{}m", params.join(";"))
}

/// SGR parameters for a foreground or background colour, or `None` when it is
/// already the default a reset leaves behind.
fn color_params(color: Color, background: bool) -> Option<String> {
    let base = if background { 40 } else { 30 };
    let bright = if background { 100 } else { 90 };
    let extended = if background { 48 } else { 38 };
    let default = if background {
        NamedColor::Background
    } else {
        NamedColor::Foreground
    };
    Some(match color {
        Color::Named(n) if n == default => return None,
        Color::Named(n) => {
            let i = n as usize;
            if i < 8 {
                format!("{}", base + i)
            } else if i < 16 {
                format!("{}", bright + i - 8)
            } else {
                // Dim/bright-foreground/cursor variants: a cell cannot reach
                // them through SGR (the emulator stores what SGR set, and
                // brightening is a rendering decision), so this arm is
                // unreachable from parsed input. Say what we mean anyway
                // rather than dropping the colour on the floor.
                format!("{}", if background { 49 } else { 39 })
            }
        }
        Color::Indexed(i) => format!("{extended};5;{i}"),
        Color::Spec(rgb) => format!("{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b),
    })
}

/// SGR 58 — the underline's own colour, which is not the text colour.
fn underline_color_params(color: Color) -> String {
    match color {
        Color::Indexed(i) => format!("58;5;{i}"),
        Color::Spec(rgb) => format!("58;2;{};{};{}", rgb.r, rgb.g, rgb.b),
        // A named underline colour has no SGR spelling; 59 restores the
        // default, which is what the text colour already gives.
        Color::Named(_) => "59".to_string(),
    }
}

/// Bytes that reproduce `term`'s scrollback, screen, cursor and modes in a
/// terminal of the same size, fed through an ordinary parser.
///
/// Reads only. See the module docs for what a snapshot carries and what it
/// deliberately does not.
pub fn encode_snapshot<T>(term: &Term<T>) -> Vec<u8> {
    let grid = term.grid();
    let cols = grid.columns();
    let mode = *term.mode();
    let mut out = String::new();

    // Land in a known state before painting: an alt screen is entered first so
    // the paint lands on the right grid, then hard reset attributes, put G0
    // back to ASCII (the grid holds characters already mapped through whatever
    // charset wrote them), and guarantee the three modes the paint itself
    // depends on.
    if mode.contains(TermMode::ALT_SCREEN) {
        out.push_str("\x1b[?1049h");
    }
    out.push_str("\x1b[0m\x1b(B\x1b[?7h\x1b[4l\x1b[?6l");
    out.push_str("\x1b[H\x1b[2J\x1b[3J");

    let mut style = Style::default();
    let mut first_row = true;
    for line in grid.topmost_line().0..=grid.bottommost_line().0 {
        let row = &grid[Line(line)];
        // A row whose last cell carries WRAPLINE is soft-wrapped: the row below
        // it is a continuation, so it gets no newline and must be painted to
        // its full width, because it is the wrap itself that joins them.
        let wrapped = cols > 0 && row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        if !first_row && !wrapped_before(grid, line, cols) {
            out.push_str("\r\n");
        }
        first_row = false;

        let last = if wrapped {
            cols
        } else {
            // Trailing default cells need no bytes: the row is already blank
            // there, and a shorter line is a smaller snapshot.
            (0..cols)
                .rev()
                .find(|&c| !is_blank(&row[Column(c)]))
                .map_or(0, |c| c + 1)
        };

        let mut col = 0;
        while col < last {
            let cell = &row[Column(col)];
            // The second half of a wide character, and the gap left at a line
            // end where a wide character did not fit: both are written by the
            // emulator as a consequence of the character itself, so emitting
            // the character reproduces them.
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                col += 1;
                continue;
            }
            let want = Style::of(cell);
            if want != style {
                out.push_str(&sgr(&want));
                style = want;
            }
            out.push(cell.c);
            for &mark in cell.zerowidth().unwrap_or(&[]) {
                out.push(mark);
            }
            col += 1;
        }
    }

    // The cursor is placed after the paint, never during it: painting moves it.
    out.push_str("\x1b[0m");
    let cursor = grid.cursor.point;
    out.push_str(&format!(
        "\x1b[{};{}H",
        cursor.line.0 + 1,
        cursor.column.0 + 1
    ));
    out.push_str(if mode.contains(TermMode::SHOW_CURSOR) {
        "\x1b[?25h"
    } else {
        "\x1b[?25l"
    });

    // Modes last, for the reasons in RESTORED_MODES.
    for (bit, code) in RESTORED_MODES {
        let set = if mode.contains(*bit) { "h" } else { "l" };
        out.push_str(&format!("\x1b[{code}{set}"));
    }

    out.into_bytes()
}

/// Whether the row *above* `line` is soft-wrapped into it.
fn wrapped_before(grid: &Grid<Cell>, line: i32, cols: usize) -> bool {
    if cols == 0 || line <= grid.topmost_line().0 {
        return false;
    }
    grid[Line(line - 1)][Column(cols - 1)]
        .flags
        .contains(Flags::WRAPLINE)
}

/// A cell that costs nothing to leave unwritten: a blank in default colours.
fn is_blank(cell: &Cell) -> bool {
    cell.c == ' '
        && Style::of(cell) == Style::default()
        && cell.zerowidth().is_none_or(|z| z.is_empty())
        && !cell
            .flags
            .intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER)
}

/// FNV-1a over a canonical reading of the grid: every scrollback and screen
/// cell's character, colours, SGR flags, combining marks and wide-character
/// structure, plus the cursor and the modes a snapshot restores.
///
/// Its caller is the divergence guard, which compares a host's grid against a
/// client's and forces a fresh snapshot when they disagree — so it arrives
/// with the client, in the attach slice. It is written and tested here because
/// a guard whose comparison is wrong is worse than no guard at all.
///
/// Deliberately **excludes** the scroll position and the selection. Those are
/// the viewer's, not the terminal's — a client may be scrolled back or holding
/// a selection and still be a faithful copy — and a hash that moved when a
/// reader scrolled would report divergence for looking.
#[allow(dead_code)]
pub fn grid_hash<T>(term: &Term<T>) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(PRIME);
        }
    };

    let grid = term.grid();
    let cols = grid.columns();
    for line in grid.topmost_line().0..=grid.bottommost_line().0 {
        let row = &grid[Line(line)];
        for col in 0..cols {
            let cell = &row[Column(col)];
            eat(&(cell.c as u32).to_le_bytes());
            eat(&cell.flags.bits().to_le_bytes());
            eat(&hash_color(cell.fg).to_le_bytes());
            eat(&hash_color(cell.bg).to_le_bytes());
            eat(&match cell.underline_color() {
                Some(c) => hash_color(c),
                // Absent is its own value, and must not hash as "colour 0".
                None => u32::MAX,
            }
            .to_le_bytes());
            for &mark in cell.zerowidth().unwrap_or(&[]) {
                eat(&(mark as u32).to_le_bytes());
            }
            eat(b"|");
        }
        eat(b"\n");
    }
    let cursor = grid.cursor.point;
    eat(&cursor.line.0.to_le_bytes());
    eat(&cursor.column.0.to_le_bytes());
    let modes = RESTORED_MODES
        .iter()
        .map(|(bit, _)| *bit)
        .fold(TermMode::SHOW_CURSOR | TermMode::ALT_SCREEN, |a, b| a | b);
    eat(&(term.mode().intersection(modes)).bits().to_le_bytes());
    h
}

/// A colour as one number, with the three kinds kept apart so an indexed 4 and
/// a named 4 cannot collide.
#[allow(dead_code)]
fn hash_color(color: Color) -> u32 {
    match color {
        Color::Named(n) => 0x0100_0000 | n as u32,
        Color::Indexed(i) => 0x0200_0000 | i as u32,
        Color::Spec(rgb) => {
            0x0300_0000 | ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | rgb.b as u32
        }
    }
}

/// The round trip, driven the way the real reader thread drives the emulator:
/// build a terminal by feeding it bytes, encode it, feed the encoding to a
/// second terminal of the same size, and require the two to be indistinguishable
/// cell by cell.
///
/// This is where the encoder earns the right to run against a live session. It
/// is the piece of the split with no second chance — a snapshot that quietly
/// drops an attribute produces a client that looks right and is wrong, which is
/// the failure this whole design is built to avoid.
///
/// These tests were themselves tested, by breaking the encoder seven ways on
/// purpose and checking the suite noticed: dropping the wide-character spacer
/// skip, trimming wrapped rows, dropping the underline colour, dropping
/// combining marks, adding a newline after the final row, and forcing line-wrap
/// off before the paint were all caught — two of them only after the tests were
/// strengthened, because the first versions passed against a broken encoder.
// ---------------------------------------------------------------------------
// The divergence guard
// ---------------------------------------------------------------------------

/// One integrity probe: what the authoritative terminal looked like, and how
/// far into a client's stream that moment was.
///
/// Both numbers are required and neither is sufficient. A hash alone cannot be
/// acted on, because a client that is merely behind would look exactly like a
/// client that is wrong; an offset alone says nothing about content. Together
/// they say: *when you have taken this many bytes, your grid must hash to
/// this*, which is a claim a client can check by itself and either satisfy,
/// not-yet-satisfy, or fail.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct GridCheck {
    /// Which pane, in the host's own numbering.
    pub pane: u64,
    /// The host's count of bytes enqueued to this client's stream at the moment
    /// the hash was taken. The socket is ordered, so it is the same clock the
    /// client's [`crate::socketpty::CountingReader`] ticks.
    pub stream_offset: u64,
    pub hash: u64,
}

/// What a client concludes about a probe.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GuardVerdict {
    /// Nothing can be concluded from this probe, for a stated reason.
    ///
    /// **Not a mismatch.** Unknown is not wrong, and a guard that reported
    /// divergence whenever the two ends were merely at different points in the
    /// stream would fire constantly and then be switched off — which is how a
    /// check becomes decoration.
    NotYet(Unsettled),
    Match,
    /// The two terminals have genuinely disagreed. The repair is loud: throw
    /// the replica away and ask for a fresh snapshot, because a client that has
    /// diverged cannot reason its way back.
    Mismatch { host: u64, replica: u64 },
}

/// Why a probe could not be turned into an answer. Each of these is a fact
/// worth having: a guard that keeps landing on `Ahead` is being probed too
/// slowly, and one that keeps landing on `Moving` is being probed during a
/// flood rather than at rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unsettled {
    /// The replica has not yet taken every byte the probe describes.
    Behind { consumed: u64, expected: u64 },
    /// The replica has taken bytes the probe does not describe. The probe is
    /// stale — the pane printed more after the host read its grid — and the
    /// next one will be comparable.
    Ahead { consumed: u64, expected: u64 },
    /// An event landed while the grid was being read, so the grid that was
    /// hashed is not the one the probe describes.
    Moving,
}

/// A client's side of the guard: everything needed to answer a probe, and
/// nothing else.
///
/// Deliberately not part of [`crate::term::Session`]. The pane never sees this
/// type, so a replica stays byte-for-byte the same shape as a locally-spawned
/// terminal and no rendering path can accidentally start depending on being
/// attached.
pub struct ReplicaGuard {
    term: std::sync::Arc<alacritty_terminal::sync::FairMutex<Term<crate::term::EventProxy>>>,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    consumed: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl ReplicaGuard {
    pub fn new(
        term: std::sync::Arc<alacritty_terminal::sync::FairMutex<Term<crate::term::EventProxy>>>,
        generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
        consumed: std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Self {
        Self {
            term,
            generation,
            consumed,
        }
    }

    /// Bytes this replica's parser has taken off the socket.
    pub fn consumed(&self) -> u64 {
        self.consumed
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Answer a probe.
    ///
    /// The fence is the same one the host takes and for the same reason: the
    /// reader thread holds alacritty's *lease* for a whole read-and-parse cycle
    /// (`event_loop.rs:117`), so a lease taken here cannot land inside one, and
    /// the grid read under it contains exactly the bytes counted so far. The
    /// unfair lock is the one to pair with a lease — the fair `lock` would try
    /// to take the lease this thread is already holding.
    pub fn check(&self, probe: &GridCheck) -> GuardVerdict {
        use std::sync::atomic::Ordering;
        // Read the counters under the fence, not before it: sampling first and
        // hashing after would compare a grid to a byte count taken at a
        // different moment, which is the very confusion this exists to avoid.
        let _lease = self.term.lease();
        let term = self.term.lock_unfair();
        let consumed = self.consumed.load(Ordering::Relaxed);
        let before = self.generation.load(Ordering::Relaxed);
        if consumed != probe.stream_offset {
            let (consumed, expected) = (consumed, probe.stream_offset);
            return GuardVerdict::NotYet(if consumed < expected {
                Unsettled::Behind { consumed, expected }
            } else {
                Unsettled::Ahead { consumed, expected }
            });
        }
        let replica = grid_hash(&*term);
        // A generation that moved while we held the fence would mean an event
        // landed mid-read; the grid we hashed is then not the grid the probe
        // describes. Say nothing rather than something wrong.
        if self.generation.load(Ordering::Relaxed) != before {
            return GuardVerdict::NotYet(Unsettled::Moving);
        }
        if replica == probe.hash {
            GuardVerdict::Match
        } else {
            GuardVerdict::Mismatch {
                host: probe.hash,
                replica,
            }
        }
    }
}

/// The seventh, turning on insert mode before the paint, survives and is
/// expected to: inserting into blank cells and overwriting them leave the same
/// grid, so nothing observable changed.
#[cfg(test)]
mod roundtrip {
    use std::sync::{Arc, Mutex};

    use alacritty_terminal::event::{Event as TermEvent, EventListener};
    use alacritty_terminal::grid::{Dimensions, Scroll};
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::{Config, Term, TermMode};
    use alacritty_terminal::vte::ansi::Processor;

    use super::{encode_snapshot, grid_hash};
    use crate::term::GridSize;

    #[derive(Clone, Default)]
    struct Silent(Arc<Mutex<Vec<TermEvent>>>);
    impl EventListener for Silent {
        fn send_event(&self, event: TermEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    /// A terminal that has been told `bytes`, exactly as the PTY reader would.
    fn term_fed(cols: usize, rows: usize, bytes: &[u8]) -> Term<Silent> {
        let size = GridSize { cols, rows };
        let mut term = Term::new(Config::default(), &size, Silent::default());
        feed(&mut term, bytes);
        term
    }

    /// Feed bytes into an emulator exactly as the real reader thread does.
    fn feed(term: &mut Term<Silent>, bytes: &[u8]) {
        let mut parser: Processor = Processor::new();
        parser.advance(term, bytes);
    }

    /// Encode `source`, replay it into a fresh terminal of the same size, and
    /// return that terminal. The two are then compared by [`assert_same`].
    fn replay(source: &Term<Silent>) -> Term<Silent> {
        let (cols, rows) = (source.grid().columns(), source.grid().screen_lines());
        term_fed(cols, rows, &encode_snapshot(source))
    }

    /// Every difference the two terminals could have, named. Compares the
    /// semantic projection of each cell rather than the struct, because a cell
    /// with no extras and a cell with empty extras are the same cell.
    fn assert_same(a: &Term<Silent>, b: &Term<Silent>, what: &str) {
        let (ga, gb) = (a.grid(), b.grid());
        assert_eq!(
            ga.history_size(),
            gb.history_size(),
            "{what}: scrollback depth"
        );
        assert_eq!(ga.columns(), gb.columns(), "{what}: width");
        assert_eq!(ga.screen_lines(), gb.screen_lines(), "{what}: height");
        for line in ga.topmost_line().0..=ga.bottommost_line().0 {
            for col in 0..ga.columns() {
                let (ca, cb) = (&ga[Line(line)][Column(col)], &gb[Line(line)][Column(col)]);
                let render = |c: &alacritty_terminal::term::cell::Cell| {
                    (
                        c.c,
                        c.flags,
                        c.fg,
                        c.bg,
                        c.underline_color(),
                        c.zerowidth().unwrap_or(&[]).to_vec(),
                    )
                };
                assert_eq!(
                    render(ca),
                    render(cb),
                    "{what}: cell at line {line} column {col}"
                );
            }
        }
        assert_eq!(ga.cursor.point, gb.cursor.point, "{what}: cursor");
        assert_eq!(
            a.mode().contains(TermMode::SHOW_CURSOR),
            b.mode().contains(TermMode::SHOW_CURSOR),
            "{what}: cursor visibility"
        );
        assert_eq!(grid_hash(a), grid_hash(b), "{what}: hashes disagree");
    }

    #[test]
    fn scrollback_survives_the_trip() {
        // More lines than the screen holds: most of this only exists in history,
        // which is exactly the content a client attaching late has never seen.
        let mut bytes = String::new();
        for i in 0..300 {
            bytes.push_str(&format!("line {i:04}\r\n"));
        }
        let source = term_fed(20, 5, bytes.as_bytes());
        assert!(source.grid().history_size() > 250, "test needs real history");
        assert_same(&source, &replay(&source), "plain scrollback");
    }

    #[test]
    fn colour_and_attribute_extremes_survive_the_trip() {
        let source = term_fed(
            40,
            6,
            concat!(
                "\x1b[1mbold\x1b[0m \x1b[2mdim\x1b[0m \x1b[3mital\x1b[0m\r\n",
                "\x1b[4munder\x1b[0m \x1b[4:2mdouble\x1b[0m \x1b[4:3mcurl\x1b[0m\r\n",
                "\x1b[4:4mdotted\x1b[0m \x1b[4:5mdashed\x1b[0m \x1b[9mstrike\x1b[0m\r\n",
                "\x1b[7minverse\x1b[0m \x1b[8mhidden\x1b[0m \x1b[1;3;4;7mall\x1b[0m\r\n",
                "\x1b[31mred\x1b[0m \x1b[91mbright\x1b[0m \x1b[38;5;208mindexed\x1b[0m\r\n",
                "\x1b[38;2;12;34;56mtruecolor\x1b[48;2;200;100;50mon-rgb\x1b[0m",
                "\x1b[4;58;5;33munderline-coloured\x1b[0m",
            )
            .as_bytes(),
        );
        assert_same(&source, &replay(&source), "sgr extremes");
    }

    #[test]
    fn wide_characters_and_combining_marks_survive_the_trip() {
        // A wide character occupies two cells and the emulator writes the second
        // itself; a combining mark occupies none. Both are reproduced by writing
        // the character, never by copying the cells.
        let source = term_fed(
            12,
            4,
            "日本語テスト\r\ne\u{0301}a\u{0301}o\u{0308} mixed\r\n漢字abc漢\r\n".as_bytes(),
        );
        assert_same(&source, &replay(&source), "wide and combining");
    }

    #[test]
    fn a_soft_wrapped_line_comes_back_soft_wrapped() {
        // The wrap flag is not an attribute that can be copied — it is set by
        // the emulator when text runs off the edge. So the encoder has to make
        // the text run off the edge again, which means never trimming a wrapped
        // row's trailing blanks.
        let source = term_fed(10, 4, "abcdefghijklmnopqrs\r\nshort\r\n".as_bytes());
        let wrapped = source.grid()[Line(0)][Column(9)]
            .flags
            .contains(alacritty_terminal::term::cell::Flags::WRAPLINE);
        assert!(wrapped, "test needs a genuinely wrapped row");
        assert_same(&source, &replay(&source), "soft wrap");
    }

    #[test]
    fn a_wrapped_row_whose_tail_is_spaces_still_wraps() {
        // The trap the previous test walks past: a wrapped row can end in
        // written spaces, and spaces in default colours look exactly like the
        // blanks it is safe to trim. Trim them and the text no longer runs off
        // the edge, so the wrap never happens and two rows silently become one.
        // Found by breaking the encoder on purpose and watching the suite stay
        // green.
        let source = term_fed(10, 4, "abc       tail\r\n".as_bytes());
        let last = &source.grid()[Line(0)][Column(9)];
        assert!(
            last.c == ' '
                && last
                    .flags
                    .contains(alacritty_terminal::term::cell::Flags::WRAPLINE),
            "test needs a wrapped row ending in a written space"
        );
        assert_same(&source, &replay(&source), "wrapped row with a blank tail");
    }

    #[test]
    fn a_resnapshot_lands_on_a_client_that_is_not_fresh() {
        // The heal is not a fresh terminal: it is the live client, already
        // carrying a previous snapshot's content and modes. A snapshot has to
        // land on that correctly — which is why the paint starts by clearing
        // the screen AND the scrollback and by forcing the three modes that
        // decide where written characters go, rather than trusting whatever the
        // last snapshot left behind.
        let first = term_fed(
            20,
            5,
            b"old line one\r\nold line two\r\n\x1b[4h\x1b[?7l\x1b[?25l",
        );
        let mut client = replay(&first);
        assert!(client.mode().contains(TermMode::INSERT), "client is dirty");

        // The host has moved on: different content, and a wrapped row that only
        // survives if the client's wrap mode is put back before the paint.
        let second = term_fed(
            20,
            5,
            b"\x1b[H\x1b[2J\x1b[3Jfresh\r\nthis line is long enough to wrap around\r\ntail",
        );
        feed(&mut client, &encode_snapshot(&second));
        assert_same(&second, &client, "re-snapshot onto a live client");
    }

    #[test]
    fn an_alt_screen_pane_comes_back_showing_its_alt_screen() {
        // The vim shape: history, then a full-screen application. The alt screen
        // itself must be exact — it is what the person is looking at.
        let source = term_fed(
            20,
            5,
            concat!(
                "history one\r\nhistory two\r\nhistory three\r\n",
                "\x1b[?1049h\x1b[H\x1b[2J",
                "~ VIM ~\r\n~\r\n~\r\n",
                "\x1b[?25l",
            )
            .as_bytes(),
        );
        assert!(source.mode().contains(TermMode::ALT_SCREEN));
        assert_same(&source, &replay(&source), "alt screen");
    }

    #[test]
    fn the_history_under_an_alt_screen_is_empty_and_heals_on_exit() {
        // The documented degradation, pinned so it stays a decision rather than
        // becoming a surprise: attaching mid-vim shows vim exactly and an empty
        // scrollback behind it. Leaving the alt screen is what the host watches
        // for, and the re-snapshot it sends then carries the history in full.
        let source = term_fed(
            20,
            5,
            concat!(
                "history one\r\nhistory two\r\nhistory three\r\nhistory four\r\n",
                "history five\r\nhistory six\r\n",
                "\x1b[?1049h\x1b[H\x1b[2J~ VIM ~",
            )
            .as_bytes(),
        );
        let client = replay(&source);
        assert_same(&source, &client, "alt screen paint");

        // While on the alt screen the client's primary history is empty — the
        // host's is not, and that is the whole point: nothing was lost, only
        // deferred.
        let mut client = client;
        let mut source = source;
        feed(&mut client, b"\x1b[?1049l");
        feed(&mut source, b"\x1b[?1049l");
        assert!(
            source.grid().history_size() > client.grid().history_size(),
            "the test's premise is that the host knows more here"
        );

        // The heal: on that transition the host re-snapshots, and the client
        // catches up completely.
        let healed = replay(&source);
        assert_same(&source, &healed, "after the alt-exit re-snapshot");
    }

    #[test]
    fn encoding_touches_nothing() {
        // The falsified design mutated the terminal it read. This is the pin:
        // encoding twice is byte-identical and leaves the hash where it was.
        // `&Term` makes it true at the type level; the test makes it visible.
        let source = term_fed(
            20,
            5,
            b"scrollback\r\n\x1b[?1049h\x1b[H\x1b[2Jalt screen contents",
        );
        let before = grid_hash(&source);
        let once = encode_snapshot(&source);
        let twice = encode_snapshot(&source);
        assert_eq!(once, twice, "a second encode differs from the first");
        assert_eq!(before, grid_hash(&source), "encoding changed the terminal");
        // and the thing it was protecting: the alt screen is still there
        assert!(source.mode().contains(TermMode::ALT_SCREEN));
        assert_eq!(source.grid()[Line(0)][Column(0)].c, 'a');
    }

    #[test]
    fn the_cursor_and_the_modes_come_back() {
        let source = term_fed(
            30,
            6,
            concat!(
                "some text\r\nmore text\r\n",
                "\x1b[?1h\x1b[?2004h\x1b[?1006h\x1b[?1002h\x1b[?25l",
                "\x1b[4;7H",
            )
            .as_bytes(),
        );
        let client = replay(&source);
        assert_eq!(
            source.grid().cursor.point,
            client.grid().cursor.point,
            "cursor position"
        );
        for bit in [
            TermMode::APP_CURSOR,
            TermMode::BRACKETED_PASTE,
            TermMode::SGR_MOUSE,
            TermMode::MOUSE_DRAG,
            TermMode::SHOW_CURSOR,
            TermMode::LINE_WRAP,
        ] {
            assert_eq!(
                source.mode().contains(bit),
                client.mode().contains(bit),
                "mode {bit:?}"
            );
        }
    }

    #[test]
    fn the_hash_ignores_what_belongs_to_the_viewer() {
        // A client scrolled back is still a faithful copy. If scrolling moved
        // the hash, the divergence guard would cry every time someone read
        // their own scrollback — and a guard that cries wolf gets switched off.
        let mut source = term_fed(20, 5, b"a\r\nb\r\nc\r\nd\r\ne\r\nf\r\ng\r\nh\r\n");
        let settled = grid_hash(&source);
        source.scroll_display(Scroll::PageUp);
        assert_eq!(settled, grid_hash(&source), "scrolling moved the hash");
        source.scroll_display(Scroll::Bottom);
        assert_eq!(settled, grid_hash(&source));
    }

    #[test]
    fn the_hash_notices_a_single_changed_cell() {
        // The other half of the guard's contract: it has to actually fire.
        let a = term_fed(20, 5, b"hello world\r\n");
        let b = term_fed(20, 5, b"hello worlds\r\n");
        assert_ne!(grid_hash(&a), grid_hash(&b));
        let c = term_fed(20, 5, b"\x1b[31mhello world\x1b[0m\r\n");
        assert_ne!(
            grid_hash(&a),
            grid_hash(&c),
            "a colour change must move the hash"
        );
    }
}
