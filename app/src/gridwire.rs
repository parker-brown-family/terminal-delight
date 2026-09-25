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

use crate::vt::{Cell, Color, Column, Flags, Line, NamedColor, Point, Term, TermMode};

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

/// The mouse-reporting protocols, weakest first: clicks, clicks and drags,
/// all motion.
///
/// xterm treats these as one setting — turning one on turns the others off,
/// and turning any off turns reporting off — and so does rio-vt. alacritty
/// does the first half and not the second: turning one off clears only that
/// one. Written one at a time, as the other modes are, a later `l` undoes an
/// earlier `h` in a core that keeps one setting: a pane reporting drags
/// arrived in a replica reporting nothing.
/// Found by `the_cursor_and_the_modes_come_back` the first time the suite ran
/// on rio-vt.
const MOUSE_PROTOCOLS: [(TermMode, &str); 3] = [
    (TermMode::MOUSE_REPORT_CLICK, "?1000"),
    (TermMode::MOUSE_DRAG, "?1002"),
    (TermMode::MOUSE_MOTION, "?1003"),
];

/// The mouse protocol as one setting: all three off, then each that is on,
/// weakest first. A core that keeps one setting lands on the strongest; a
/// core that keeps three bits gets all three back.
fn restore_mouse_protocol(mode: TermMode, out: &mut String) {
    for (_, code) in MOUSE_PROTOCOLS {
        out.push_str(&format!("\x1b[{code}l"));
    }
    for (bit, code) in MOUSE_PROTOCOLS {
        if mode.contains(bit) {
            out.push_str(&format!("\x1b[{code}h"));
        }
    }
}

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
pub fn encode_snapshot(term: &Term) -> Vec<u8> {
    let cols = term.columns();
    let mode = term.mode();
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
    // Whether the row painted last was soft-wrapped — carried forward rather
    // than read again, because a row is materialised to be read.
    let mut above_wrapped = false;
    let bottom = term.bottommost_line().0;
    term.for_each_row(term.topmost_line(), term.bottommost_line(), |line, row| {
        let line = line.0;
        // A row whose last cell carries WRAPLINE is soft-wrapped: the row below
        // it is a continuation, so it gets no newline and must be painted to
        // its full width, because it is the wrap itself that joins them.
        let wrapped = cols > 0 && row[cols - 1].flags.contains(Flags::WRAPLINE);
        // Whether the row ABOVE is soft-wrapped into this one. Load-bearing
        // twice: it withholds the newline, and it forces a character below.
        let continues = above_wrapped;
        above_wrapped = wrapped;
        if !first_row && !continues {
            out.push_str("\r\n");
        }
        first_row = false;

        let last = if wrapped {
            cols
        } else {
            // Trailing default cells need no bytes: the row is already blank
            // there, and a shorter line is a smaller snapshot.
            let trimmed = (0..cols)
                .rev()
                .find(|&c| !is_blank(&row[c]))
                .map_or(0, |c| c + 1);
            // ...except on a continuation row, where the trim would delete the
            // wrap itself.
            //
            // A wrap is not written into the snapshot, it is CAUSED by it: the
            // row above is painted to its full width, which leaves the cursor
            // in pending-wrap, and the first character painted here is what
            // resolves that into a real line break and sets WRAPLINE above.
            // Trim this row to nothing and nothing resolves it — the cursor
            // sits pending, the next `\r\n` cancels it, and the row above comes
            // back unwrapped.
            //
            // One cell is enough, and it is honest: `is_blank` only says yes to
            // a space in default colours with no marks and no wide-character
            // structure, so painting column 0 of an all-blank row reproduces
            // exactly what is there.
            //
            // Found by `a_wrap_into_a_blank_continuation_survives`. The cost of
            // missing it was not cosmetic: `grid_hash` reads `cell.flags`, so a
            // dropped WRAPLINE is a permanent divergence that the guard's
            // repair cannot fix, because every re-snapshot drops it again — the
            // pane re-ignites on a cycle forever (#356). Wrapped prose ending
            // in a space is the common case, not a corner one.
            if continues && trimmed == 0 {
                1
            } else {
                trimmed
            }
        };

        let mut col = 0;
        while col < last {
            let cell = &row[col];
            // The gap left at a line end where a wide character did not fit is
            // written by the emulator as a consequence of that character, so
            // painting the character reproduces it — but only while the
            // character is still THERE. An erase can clear the row below and
            // leave the gap behind, and then nothing recreates it: the row
            // stops one column short of full width, the pending wrap is never
            // armed, and the soft wrap above is lost.
            //
            // So the condition is the character, not the flag.
            if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
                && line < bottom
                && term
                    .cell(Point::new(Line(line + 1), Column(0)))
                    .flags
                    .contains(Flags::WIDE_CHAR)
            {
                col += 1;
                continue;
            }
            // A wide character's own second cell needs no such test: it is
            // stepped over by WIDTH at the bottom of this loop, which is right
            // whether or not the spacer flag survived.
            let want = Style::of(cell);
            if want != style {
                out.push_str(&sgr(&want));
                style = want;
            }
            out.push(cell.c);
            for &mark in cell.zerowidth().unwrap_or(&[]) {
                out.push(mark);
            }
            // Step over a wide character's second cell by its WIDTH, not by the
            // flag on it. The two are the same until an erase disagrees: `\x1b[J`
            // and friends clear by cell, so they will take the spacer and leave
            // the character, and the grid holds that perfectly happily.
            //
            // Trusting the flag there costs far more than a flag. The orphaned
            // cell reads as an ordinary blank, so it gets painted — one space
            // that was never in the source — and because the emulator has
            // already advanced two columns for the character itself, every
            // cell after it on that row lands one column late. A snapshot that
            // shifts its own content is not a cosmetic defect; it is the
            // replica being wrong about what the pane says.
            col += if cell.flags.contains(Flags::WIDE_CHAR) {
                2
            } else {
                1
            };
        }

        // Say what the rest of the row is, rather than assuming it.
        //
        // The trim above stops at the last non-blank cell on the grounds that
        // the remainder is already default-blank. In a terminal being painted
        // from empty that holds; in this one it does not, because rows here
        // come into existence by SCROLLING — a newline or a wrap at the bottom
        // row — and a scroll fills the row it exposes with whatever background
        // is active at that instant. Paint a row to its full width in colour
        // and the blank row after it is tinted, invisibly, in the replica only.
        //
        // `\x1b[K` from the cursor is the whole remainder, erased in default
        // colours because the reset precedes it. Skipped when the row was
        // painted to its full width: there is no remainder, and the cursor is
        // then sitting in pending-wrap, which is load-bearing for the row below
        // and must not be disturbed.
        //
        // Costs about seven bytes on a row that needs it. The alternative —
        // tracking which fills could have tinted which rows — is the kind of
        // reasoning that was wrong here twice.
        if last < cols {
            if style != Style::default() {
                out.push_str("\x1b[0m");
                style = Style::default();
            }
            out.push_str("\x1b[K");
        }
    });

    // The cursor is placed after the paint, never during it: painting moves it.
    out.push_str("\x1b[0m");
    let cursor = term.cursor().point;
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
        if MOUSE_PROTOCOLS.iter().any(|(mouse, _)| mouse == bit) {
            // Written once, as a setting, where the first of them falls.
            if *bit == MOUSE_PROTOCOLS[0].0 {
                restore_mouse_protocol(mode, &mut out);
            }
            continue;
        }
        let set = if mode.contains(*bit) { "h" } else { "l" };
        out.push_str(&format!("\x1b[{code}{set}"));
    }

    out.into_bytes()
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

/// The cell flags the guard deliberately does not compare: the two spacers that
/// mark where a double-width character sits.
///
/// They are bookkeeping, not content. A renderer draws a wide character from its
/// `WIDE_CHAR` cell and never consults the spacer beside it, so a grid that has
/// lost a spacer is pixel-for-pixel the grid that has one. `WIDE_CHAR` itself is
/// still compared — a replica that lost the character, or placed it a column
/// over, is a real divergence and is still caught.
///
/// They are excluded because an erase can take the second half of a wide
/// character and leave the first, and the grid can hold that while printing
/// cannot produce it: printing the character always writes both halves. So a
/// host that has been erased across a wide character can never be matched by a
/// client that was painted from a snapshot, no matter how careful the encoder
/// is — the guard would report a divergence forever and re-snapshot into the
/// same one every time, which is a pane that re-ignites on a cycle and loses
/// its appearance each time it does.
///
/// This is the same principle already applied to the scroll position and the
/// selection below, one step further: **compare what a viewer could see.** A
/// guard that fires on a difference nobody can observe, and that no repair can
/// remove, is worse than no guard, because the repair is not free.
const DERIVED_FLAGS: Flags = Flags::WIDE_CHAR_SPACER.union(Flags::LEADING_WIDE_CHAR_SPACER);

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
pub fn grid_hash(term: &Term) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = OFFSET;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(PRIME);
        }
    };

    let cols = term.columns();
    term.for_each_row(term.topmost_line(), term.bottommost_line(), |_, row| {
        for cell in &row[..cols] {
            eat(&(cell.c as u32).to_le_bytes());
            eat(&(cell.flags & !DERIVED_FLAGS).bits().to_le_bytes());
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
    });
    let cursor = term.cursor().point;
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
// ---------------------------------------------------------------------------
// The divergence guard
// ---------------------------------------------------------------------------

/// The probe a client answers is the one the host sends, spelled once, on the
/// wire, where both ends can see it: [`crate::hostproto::GridCheck`].
pub use crate::hostproto::GridCheck;

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
    Mismatch {
        host: u64,
        replica: u64,
    },
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
    term: crate::vt::Shared,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
    consumed: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl ReplicaGuard {
    pub fn new(
        term: crate::vt::Shared,
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
        self.consumed.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Answer a probe.
    ///
    /// The fence is the same one the host takes and for the same reason: the
    /// read loop counts and parses each chunk under the terminal's lock, in one
    /// step (`vt::pump`), so the grid read under that lock contains exactly the
    /// bytes counted so far — never some of a chunk.
    pub fn check(&self, probe: &GridCheck) -> GuardVerdict {
        use std::sync::atomic::Ordering;
        // Read the counters under the fence, not before it: sampling first and
        // hashing after would compare a grid to a byte count taken at a
        // different moment, which is the very confusion this exists to avoid.
        let term = self.term.lock();
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
        let replica = grid_hash(&term);
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
/// The seventh, turning on insert mode before the paint, survives and is
/// expected to: inserting into blank cells and overwriting them leave the same
/// grid, so nothing observable changed.
#[cfg(test)]
mod roundtrip {
    use std::sync::{Arc, Mutex};

    use crate::vt::{
        Cell, Column, Event as TermEvent, Line, Listener, Point, Scroll, Term, TermMode,
    };

    use super::{encode_snapshot, grid_hash};
    use crate::term::GridSize;

    #[derive(Clone, Default)]
    struct Silent(Arc<Mutex<Vec<TermEvent>>>);
    impl Listener for Silent {
        fn send_event(&self, event: TermEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    /// A terminal that has been told `bytes`, exactly as the PTY reader would.
    fn term_fed(cols: usize, rows: usize, bytes: &[u8]) -> Term {
        let size = GridSize { cols, rows }.with_cell(8, 16);
        let mut term = Term::new(size, Arc::new(Silent::default()));
        feed(&mut term, bytes);
        term
    }

    /// Feed bytes into a core exactly as the real read loop does.
    fn feed(term: &mut Term, bytes: &[u8]) {
        term.advance(bytes);
    }

    /// Screens programs commonly leave behind, each with the hash a session
    /// host built before the core swap gives it: `grid_hash` at 70c6233, on
    /// alacritty_terminal 0.26, fed these bytes.
    ///
    /// The divergence guard compares a host's hash with its window's, and the
    /// host is the process that is never restarted, so a window from this
    /// build spends its first weeks attached to a host from the last one. For
    /// the same bytes the two have to agree, or the guard takes a quiet pane
    /// again and again over a difference nobody can see. This pins that
    /// agreement, and because the suite runs on both cores it also pins the
    /// cores agreeing with each other.
    ///
    /// Measured before it was pinned, 2026-09-25: every case run through the
    /// old build and through this one on both cores. The measurement found
    /// rio-vt's blanks reading differently after a scroll and after an erase,
    /// which `blank_background` in `vt/rio.rs` now answers.
    #[rustfmt::skip]
    const AS_THE_OLD_HOST_HASHED: &[(&str, usize, usize, &[u8], u64)] = &[
        ("plain text", 20, 5, b"hello world\r\nsecond line", 0x37fbc118d8a5eff0),
        ("sgr colours", 40, 5, b"\x1b[31mred\x1b[32mgreen\x1b[0m \x1b[1;33mbold yellow\x1b[0m", 0x3291f322c85b8194),
        ("bright colours", 40, 5, b"\x1b[91mbright\x1b[0m\x1b[101mbg\x1b[0m\x1b[97;100mx\x1b[0m", 0xb3bebd9fae529bf0),
        ("256 colours", 40, 5, b"\x1b[38;5;208morange\x1b[48;5;17mblue bg\x1b[38;5;3mlow\x1b[0m", 0x0efbbba38767c2c2),
        ("truecolor", 40, 5, b"\x1b[38;2;10;20;30mrgb\x1b[48;2;200;100;50mbg\x1b[0m", 0xcb01c661b61994ce),
        ("attributes", 60, 5, b"\x1b[1mb\x1b[2md\x1b[3mi\x1b[4mu\x1b[5mblink\x1b[7mrev\x1b[8mhid\x1b[9mstrike\x1b[0m", 0x31ac80a49f768bd4),
        ("attribute resets", 60, 5, b"\x1b[1;3;4;7;9mall\x1b[22;23;24;27;29mnone\x1b[1;2mdb\x1b[22mn", 0x55ed358b73e6b48a),
        ("underline styles", 60, 5, b"\x1b[4:2mdouble\x1b[4:3mcurl\x1b[4:4mdot\x1b[4:5mdash\x1b[4:0mnone\x1b[21mdbl\x1b[0m", 0x159340a5f8978296),
        ("underline colour", 40, 5, b"\x1b[4;58;5;196mindexed\x1b[58;2;1;2;3mrgb\x1b[59mplain\x1b[0m", 0xf745b33d4d5c4b72),
        ("default colours explicit", 40, 5, b"\x1b[31;41mx\x1b[39;49my\x1b[7mz\x1b[0m", 0x16f7830cb6e8ecc8),
        ("wide chars", 20, 5, "\u{4f60}\u{597d} \u{1f642} ok".as_bytes(), 0x4cac8696944c6dd8),
        ("wide char at the edge", 10, 3, "123456789\u{4f60}after".as_bytes(), 0x8c2a89980c3f422c),
        ("combining marks", 20, 5, "e\u{301}a\u{308}\u{303} n\u{303}".as_bytes(), 0xb0c2ef14361dd8f5),
        ("zero width joiner", 20, 5, "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467} x".as_bytes(), 0x6809918d94e9217d),
        ("variation selector", 20, 5, "\u{2764}\u{fe0f} \u{263a}\u{fe0e} x".as_bytes(), 0xe849a6b5ba0bc7f1),
        ("soft wrap", 10, 5, b"abcdefghijklmnopqrstuvwxyz", 0xf2308f533a1e9089),
        ("exactly full line", 10, 5, b"0123456789", 0x5b781b96fb766302),
        ("exactly full line then cr", 10, 5, b"0123456789\r", 0xf9c10d0f0b8413fb),
        ("full line then newline", 10, 5, b"0123456789\r\nnext", 0xabd261c8969b604f),
        ("scrollback", 20, 5, b"l01\r\nl02\r\nl03\r\nl04\r\nl05\r\nl06\r\nl07\r\nl08\r\nl09\r\nl10\r\nl11\r\nl12", 0xaad89c4bb4a7eab6),
        ("scrollback with colour", 20, 4, b"\x1b[32ma\r\nb\r\n\x1b[41mc\r\nd\r\ne\r\nf\x1b[0m\r\ng", 0xa5e5c3a207f97ffe),
        ("cursor moves", 30, 8, b"\x1b[5;10Hhere\x1b[2;3Hthere\x1b[8;30H", 0x05d55397045da1da),
        ("cursor relative", 30, 8, b"\x1b[3;3Hx\x1b[2Ay\x1b[5Cz\x1b[1Bw\x1b[3Dv\x1b[2Eu\x1b[1Ft", 0x4e36515ad0fc136e),
        ("erase in line", 20, 5, b"abcdefghij\x1b[5G\x1b[K\r\nklmnopqrst\x1b[5G\x1b[1K\r\nuvwxyz\x1b[2K", 0xac8d19006485144c),
        ("erase in display", 20, 5, b"row1\r\nrow2\r\nrow3\x1b[2;2H\x1b[J", 0x3db2241575d0236b),
        ("erase above", 20, 5, b"row1\r\nrow2\r\nrow3\x1b[2;2H\x1b[1J", 0xb1f7bcb29f4c121b),
        ("erase with colour", 20, 5, b"\x1b[44mblue\x1b[K\r\n\x1b[2Kmore\x1b[0m", 0x9131a4d666d161a8),
        ("erase characters", 20, 5, b"abcdefghij\x1b[3G\x1b[4X", 0xc2e7088461e1bb5f),
        ("clear screen", 20, 5, b"stuff\r\nmore\x1b[H\x1b[2J\x1b[3Jfresh", 0x6fb74098921b31e5),
        ("scroll region", 20, 6, b"a\r\nb\r\nc\r\nd\r\ne\r\nf\x1b[2;4r\x1b[4;1H\n\n\nx\x1b[r", 0xe7ccf40a3623e26e),
        ("scroll up and down", 20, 6, b"1\r\n2\r\n3\r\n4\r\n5\x1b[2S\x1b[1T", 0xd68024b28b60d614),
        ("reverse index at top", 20, 4, b"a\r\nb\r\nc\x1b[H\x1bMtop", 0x394fb40c25413f2a),
        ("index and next line", 20, 4, b"a\x1bDb\x1bEc", 0x49c4c4a3a70dfb8f),
        ("tabs", 40, 5, b"a\tb\tc\x1b[3g\r\nd\te\x1bH\r\n\tf", 0x621e68f04a66b38c),
        ("insert and delete characters", 20, 5, b"abcdefgh\x1b[3G\x1b[2@XY\x1b[6G\x1b[2P", 0x715ac97e71250c63),
        ("insert and delete lines", 20, 6, b"1\r\n2\r\n3\r\n4\r\n5\x1b[2;1H\x1b[2L\x1b[4;1H\x1b[1M", 0x0d0af58bd87a1d13),
        ("insert mode", 20, 5, b"abcdef\x1b[3G\x1b[4hXY\x1b[4l", 0xe87df326370fed18),
        ("alt screen", 20, 5, b"main\x1b[?1049halt screen\x1b[5;5Hx", 0xffa3f782fcce54aa),
        ("alt screen left", 20, 5, b"main\x1b[?1049halt\x1b[?1049lback", 0xc81a5dacd3715c02),
        ("alt screen 47 and 1047", 20, 5, b"m\x1b[?47ha\x1b[?47l\x1b[?1047hb\x1b[?1047l", 0xb1538a8bcd6be27f),
        ("save and restore cursor", 20, 5, b"\x1b[3;4H\x1b7\x1b[31m\x1b[1;1Hx\x1b8y\x1b[s\x1b[5;5H\x1b[uz", 0x70be3196024ef6ea),
        ("modes", 20, 5, b"\x1b[?1h\x1b=\x1b[?2004h\x1b[?1004h\x1b[?25l", 0x88324b02110f223f),
        ("modes back off", 20, 5, b"\x1b[?1h\x1b=\x1b[?2004h\x1b[?1004h\x1b[?25l\x1b[?1l\x1b>\x1b[?2004l\x1b[?1004l\x1b[?25h", 0xe360fe422afb3be0),
        ("mouse one protocol", 20, 5, b"\x1b[?1002h\x1b[?1006h", 0x4fbfdd331944d420),
        ("mouse click only", 20, 5, b"\x1b[?1000h\x1b[?1006h", 0xe3e0dd7a15f18e48),
        ("mouse three protocols", 20, 5, b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h", 0xe5607b21d6d48580),
        ("mouse on then off", 20, 5, b"\x1b[?1000h\x1b[?1002h\x1b[?1006h\x1b[?1002l\x1b[?1000l\x1b[?1006l", 0xe360fe422afb3be0),
        ("mouse utf8 and sgr", 20, 5, b"\x1b[?1000h\x1b[?1005h\x1b[?1006h", 0xe3e0dd7a15f18e48),
        ("alternate scroll off", 20, 5, b"\x1b[?1007l", 0x8ea37e3fb6fbe660),
        ("alternate scroll on", 20, 5, b"\x1b[?1007h", 0xe360fe422afb3be0),
        ("origin mode", 20, 6, b"\x1b[2;5r\x1b[?6h\x1b[1;1Hx\x1b[?6l\x1b[r", 0x042a97c3ec8380ac),
        ("line feed new line", 20, 5, b"\x1b[20ha\nb\x1b[20l\nc", 0x660b83d71344dd51),
        ("autowrap off", 10, 5, b"\x1b[?7labcdefghijklmnop\x1b[?7h", 0xeb2e81c33ef0df48),
        ("full reset", 20, 5, b"junk\x1b[?1049h\x1b[31m\x1b[?1h\x1bcafter reset", 0xaf7bb4cefe7a91f6),
        ("soft reset", 20, 5, b"\x1b[31m\x1b[4h\x1b[?6h\x1b[!pafter", 0x3bf596abee0c3bbf),
        ("dec line drawing", 20, 5, b"\x1b(0lqqk\r\nx  x\r\nmqqj\x1b(B", 0xd7fa0682957e6b74),
        ("hyperlink", 30, 5, b"\x1b]8;;http://example.com\x07link\x1b]8;;\x07 plain", 0x8d5d8d4974c94fac),
        ("title", 20, 5, b"\x1b]0;a title\x07text\x1b]2;other\x1b\\", 0xa4817cc2837d3539),
        ("sync update closed", 20, 5, b"\x1b[?2026hinside\x1b[?2026l after", 0x9d18d4167d37180a),
        ("decaln", 10, 4, b"\x1b#8", 0xf7b97ac47a1a3bd4),
        ("cursor shapes", 20, 5, b"\x1b[5 qbeam\x1b[3 qunder\x1b[2 qblock\x1b[0 q", 0x3d2c6ceddbe83a46),
        ("bell and controls", 20, 5, b"a\x07b\x08c\x0bd\x0ce", 0x3e68d162891baa07),
        ("repeat character", 20, 5, b"x\x1b[5b", 0xe5ccd15c7814bd06),
        ("backspace over wide char", 20, 5, "\u{4f60}\x08\x08ab".as_bytes(), 0x93427296c35455cb),
        ("overwrite half a wide char", 20, 5, "\u{4f60}\u{597d}\x1b[1;2Hx".as_bytes(), 0xc0aeb69f7274e774),
        ("dim and bright bold", 30, 5, b"\x1b[1;30mbold black\x1b[0m\x1b[2;37mdim white\x1b[0m", 0x1d3e2d03c099ed48),
        ("claude-like frame", 60, 12, "\x1b[?2026h\x1b[2K\x1b[1G\x1b[38;2;215;119;87m\u{256d}\u{2500}\u{2500}\u{2500}\u{256e}\x1b[39m\r\n\x1b[38;2;215;119;87m\u{2502}\x1b[39m \x1b[1m> \x1b[22mtype here \x1b[2m(shift+tab)\x1b[22m\r\n\x1b[38;2;215;119;87m\u{2570}\u{2500}\u{2500}\u{2500}\u{256f}\x1b[39m\r\n\x1b[2m  ? for shortcuts\x1b[22m\x1b[?2026l".as_bytes(), 0x3a0307257d3995eb),
        ("prompt with rprompt", 40, 5, b"\x1b[1;32muser@host\x1b[0m:\x1b[1;34m~/w\x1b[0m$ \x1b[s\x1b[40G\x1b[7D\x1b[33m12:00:00\x1b[u", 0xe7bd58dbec81e0c3),
        ("progress bar redraw", 40, 5, b"[#####     ] 50%\r[##########] 100%\r\n", 0x400c353e41c99fe5),
        ("resized content", 20, 5, b"\x1b[8;5;20t\x1b[18t", 0xe360fe422afb3be0),
        ("device queries", 20, 5, b"\x1b[c\x1b[>c\x1b[5n\x1b[6n\x1b[?u\x1b[>0q\x1b[16t\x1b[14t", 0xe360fe422afb3be0),
        ("kitty graphics no cursor move", 20, 5, b"\x1b_Gi=8,s=2,v=1,a=T,t=d,f=24,c=4,r=2,C=1;/wAAAP8A\x1b\\after", 0xc3744af53ab0e06f),
        ("sixel", 20, 5, b"\x1bPq#0;2;0;0;0#1;2;100;100;0#1~~@@vv@@~~$\x1b\\after", 0xc3744af53ab0e06f),
        ("unterminated osc", 20, 5, b"\x1b]0;never ends text", 0xe360fe422afb3be0),
        ("c1 controls", 20, 5, b"a\xc2\x9b31mb\xc2\x9b0mc", 0x8089feec9fbb4a50),
        ("invalid utf8", 20, 5, b"a\xff\xfeb\xc3c", 0xa4abc847b76bd698),
        ("pen through a scroll", 6, 2, b"\x1b[4;7;32;41ma\r\nb\r\nc", 0x5b7c339581d69d7e),
        ("pen through scroll up", 6, 3, b"x\x1b[4;7;32;41m\x1b[1S", 0x3e4ef2ce6ff24c07),
        ("pen through scroll down", 6, 3, b"x\x1b[4;7;32;41m\x1b[1T", 0xef0e56e2d99bba91),
        ("pen through insert lines", 6, 3, b"x\r\ny\x1b[1;1H\x1b[4;7;32;41m\x1b[1L", 0xa280e644bcaef45b),
        ("pen through delete lines", 6, 3, b"x\r\ny\x1b[1;1H\x1b[4;7;32;41m\x1b[1M", 0x46fcb65ae83f24e9),
        ("pen through insert chars", 6, 2, b"abc\x1b[1;1H\x1b[4;7;32;41m\x1b[2@", 0x0e18a130e8b495b6),
        ("pen through delete chars", 6, 2, b"abcdef\x1b[1;1H\x1b[4;7;32;41m\x1b[2P", 0x7815fdd31af1cf86),
        ("pen through erase chars", 6, 2, b"abcdef\x1b[1;1H\x1b[4;7;32;41m\x1b[2X", 0x3d66cd3818931b66),
        ("pen through erase line", 6, 2, b"abcdef\x1b[1;3H\x1b[4;7;32;41m\x1b[K", 0xf2aa12c5024f3f21),
        ("pen through erase display", 6, 2, b"abcdef\x1b[4;7;32;41m\x1b[2J", 0x8b3f065a09491a62),
        ("pen through reverse index", 6, 2, b"x\x1b[1;1H\x1b[4;7;32;41m\x1bM", 0x63910a330a62c332),
        ("erase with a high palette background", 6, 2, b"ab\x1b[48;5;200m\x1b[K", 0xf5658eed8af2da29),
        ("erase with an rgb background", 6, 2, b"ab\x1b[48;2;1;2;3m\x1b[K\r\n\x1b[2K", 0x7cfe7dafd50ca786),
        ("erase with a bright background", 6, 2, b"ab\x1b[103m\x1b[K", 0x04b2b3804842a101),
        ("vim-like clear", 20, 5, b"\x1b[38;5;252;48;5;235m\x1b[H\x1b[2J\x1b[1;1H~\x1b[2;1H~\x1b[5;1H\x1b[7m-- INSERT --\x1b[27m", 0x0e7face4e64b17b1),
        ("styled wide char", 20, 3, "\x1b[1;31;44m\u{4f60}\x1b[0m\u{597d}".as_bytes(), 0xc78a1e14fb3f688d),
    ];

    /// Where rio-vt parts from a host built before the swap, on purpose, with
    /// the old host's hash. The fallback core still matches it; rio-vt does
    /// not, for a reason each can name:
    ///
    /// - a Kitty picture, which the old host drops in its parser and rio-vt
    ///   places, moving the cursor below it;
    /// - a palette index under 16 set with `48;5;n` and then erased, which
    ///   rio-vt stores without saying whether the colour was named, and which
    ///   TD reads back as named because `4n` then erase is far commoner.
    #[rustfmt::skip]
    const WHERE_RIO_PARTS: &[(&str, usize, usize, &[u8], u64)] = &[
        ("kitty graphics inline", 20, 5, b"\x1b_Gi=7,s=2,v=1,a=T,t=d,f=24,c=4,r=2;/wAAAP8A\x1b\\after", 0xc3744af53ab0e06f),
        ("erase with a palette background", 6, 2, b"ab\x1b[48;5;4m\x1b[K", 0x41f18ce63e45a129),
    ];

    #[test]
    fn a_screen_hashes_as_a_host_from_before_the_core_swap_hashed_it() {
        let wrong: Vec<String> = AS_THE_OLD_HOST_HASHED
            .iter()
            .filter_map(|(name, cols, rows, bytes, old)| {
                let got = grid_hash(&term_fed(*cols, *rows, bytes));
                (got != *old).then(|| format!("{name}: {got:#018x}, the old host {old:#018x}"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "on {}, a window would disagree with a host from before the swap about:\n{}",
            crate::vt::CORE_NAME,
            wrong.join("\n")
        );
    }

    /// A host built before the core swap restores mouse reporting as a run of
    /// three settings written one at a time, on or off, and a window has to
    /// come out of that run reporting what the host reports. On rio-vt it came
    /// out reporting nothing whenever clicks or drags were on — every reset
    /// turns reporting off there — until `vt/compat.rs`.
    #[test]
    fn a_snapshot_from_a_host_before_the_swap_keeps_its_mouse_reporting() {
        for (run, reporting) in [
            (
                "\x1b[?1000h\x1b[?1002l\x1b[?1003l",
                TermMode::MOUSE_REPORT_CLICK,
            ),
            ("\x1b[?1000l\x1b[?1002h\x1b[?1003l", TermMode::MOUSE_DRAG),
            ("\x1b[?1000l\x1b[?1002l\x1b[?1003h", TermMode::MOUSE_MOTION),
            ("\x1b[?1000l\x1b[?1002l\x1b[?1003l", TermMode::empty()),
        ] {
            let term = term_fed(20, 5, run.as_bytes());
            assert_eq!(term.mode() & TermMode::MOUSE_MODE, reporting, "{run:?}");
        }
    }

    #[test]
    fn where_rio_parts_from_a_host_before_the_swap_is_on_purpose() {
        for (name, cols, rows, bytes, old) in WHERE_RIO_PARTS {
            let got = grid_hash(&term_fed(*cols, *rows, bytes));
            if cfg!(feature = "core-alacritty") {
                assert_eq!(got, *old, "{name}: the fallback is the old host's core");
            } else {
                assert_ne!(
                    got, *old,
                    "{name}: rio-vt agrees with the old host now; move it to AS_THE_OLD_HOST_HASHED"
                );
            }
        }
    }

    /// Encode `source`, replay it into a fresh terminal of the same size, and
    /// return that terminal. The two are then compared by [`assert_same`].
    fn replay(source: &Term) -> Term {
        let (cols, rows) = (source.columns(), source.screen_lines());
        term_fed(cols, rows, &encode_snapshot(source))
    }

    /// Every difference the two terminals could have, named. Compares the
    /// semantic projection of each cell rather than the struct, because a cell
    /// with no extras and a cell with empty extras are the same cell.
    fn assert_same(a: &Term, b: &Term, what: &str) {
        let (ga, gb) = (a, b);
        assert_eq!(
            ga.history_size(),
            gb.history_size(),
            "{what}: scrollback depth"
        );
        assert_eq!(ga.columns(), gb.columns(), "{what}: width");
        assert_eq!(ga.screen_lines(), gb.screen_lines(), "{what}: height");
        for line in ga.topmost_line().0..=ga.bottommost_line().0 {
            for col in 0..ga.columns() {
                let (ca, cb) = (
                    &ga.cell(Point::new(Line(line), Column(col))),
                    &gb.cell(Point::new(Line(line), Column(col))),
                );
                let render = |c: &Cell| {
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
        assert_eq!(ga.cursor().point, gb.cursor().point, "{what}: cursor");
        assert_eq!(
            a.mode().contains(TermMode::SHOW_CURSOR),
            b.mode().contains(TermMode::SHOW_CURSOR),
            "{what}: cursor visibility"
        );
        assert_eq!(grid_hash(a), grid_hash(b), "{what}: hashes disagree");
    }

    /// What reading a whole terminal costs: the hash the guard computes and
    /// the snapshot an attach sends, over a full 10,000-line history.
    ///
    /// An instrument, not a gate. rio-vt keeps a cell as a packed word and TD
    /// reads cells by value, so a reader walking all of history assembles a
    /// million cells; this is how the two cores compare at doing so. Prints
    /// one JSON line per measurement.
    ///
    /// ```text
    /// cargo test --release --bin terminal-delight reading_a_full_history -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "timing instrument; run with --ignored --nocapture"]
    fn reading_a_full_history_costs() {
        let mut bytes = String::new();
        for i in 0..12_000 {
            bytes.push_str(&format!(
                "\x1b[3{}mline {i:05} \x1b[1mbold\x1b[0m {}\r\n",
                i % 8,
                "text ".repeat(16)
            ));
        }
        let source = term_fed(100, 30, bytes.as_bytes());
        assert!(source.history_size() >= 9_000, "a full history");
        let mut best = (u128::MAX, u128::MAX);
        let mut sink = 0usize;
        for _ in 0..5 {
            let t = std::time::Instant::now();
            sink ^= grid_hash(&source) as usize;
            best.0 = best.0.min(t.elapsed().as_micros());
            let t = std::time::Instant::now();
            sink ^= encode_snapshot(&source).len();
            best.1 = best.1.min(t.elapsed().as_micros());
        }
        println!(
            r#"{{"core":"{}","lines":{},"hash_us":{},"snapshot_us":{},"sink":{}}}"#,
            crate::vt::CORE_NAME,
            source.history_size() + source.screen_lines(),
            best.0,
            best.1,
            sink % 7
        );
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
        assert!(source.history_size() > 250, "test needs real history");
        assert_same(&source, &replay(&source), "plain scrollback");
    }

    /// A deterministic sweep over generated terminal content, checking the one
    /// property the whole split rests on: encode then replay is the identity.
    ///
    /// This exists because the hand-written fixtures around it all passed while
    /// a real pane diverged on every re-snapshot. Each of them was written by
    /// someone who had already thought of the case it covers, which is exactly
    /// the set of cases that were never going to be the bug. The generator has
    /// no such blind spot: it combines wraps, blanks, wide characters, erases,
    /// colours and scrolling in orders nobody chose.
    ///
    /// Deliberately no proptest/quickcheck dependency — this crate's dependency
    /// list is short on purpose and CI gates it. A 64-bit xorshift and a match
    /// statement buy the same thing here, and the seed is printed on failure,
    /// which is the part that actually matters for a fix.
    ///
    /// **Runs on rio-vt; ignored on the alacritty fallback, where it fails on
    /// a real defect rather than a flaky one.** It found four in the sitting it
    /// was written, all fixed and each pinned as its own named fixture beside
    /// this one; the fifth is a `BOLD` flag reaching a trailing blank the
    /// source has plain, at 11x4 seed 107, and it happens only on alacritty.
    /// Why rio-vt never produces that grid has not been established; what has
    /// been measured is that all 1,200 seeds pass on it (2026-09-25), so the
    /// sweep went back into the suite the day the core moved — a bug-finding
    /// tool nobody runs finds nothing.
    ///
    /// ```text
    /// cargo test --bin terminal-delight --features core-alacritty encode_then_replay -- --ignored --nocapture
    /// ```
    #[test]
    #[cfg_attr(
        feature = "core-alacritty",
        ignore = "fails on alacritty's erase (11x4 seed 107); passes on rio-vt"
    )]
    fn encode_then_replay_is_the_identity_over_generated_content() {
        // Widths that are small enough for a wrap to be likely and odd enough
        // that a wide character can straddle the margin.
        for &(cols, rows) in &[(20usize, 5usize), (11, 4), (37, 7)] {
            for seed in 0..400u64 {
                let bytes = generate(seed, cols);
                if faithful(cols, rows, &bytes) {
                    continue;
                }
                // No exceptions. There was one, for grids holding a wide
                // character an erase had torn in half; `DERIVED_FLAGS` removed
                // the need for it by not comparing what no renderer draws.
                // A skip list on a property test is a place for bugs to live.
                // Shrink before reporting. A generated stream is ~300 bytes of
                // which three matter, and a failure nobody can read gets
                // reported as "flaky" and then muted.
                let small = shrink(cols, rows, &bytes);
                let source = term_fed(cols, rows, &small);
                let client = replay(&source);
                panic!(
                    "{cols}x{rows} seed {seed}, shrunk to {} bytes: {:?}\n  {}",
                    small.len(),
                    String::from_utf8_lossy(&small),
                    // Reported the way the GUARD compares, not the way
                    // `assert_same` does. The two differ by `DERIVED_FLAGS`,
                    // and pointing at a cell the guard ignores sends whoever
                    // picks this up to the wrong place.
                    guard_difference(&source, &client)
                        .unwrap_or_else(|| "no cell differs — the cursor or the modes do".into()),
                );
            }
        }
    }

    /// The first cell the divergence guard would object to, described.
    ///
    /// Masks the same flags `grid_hash` masks, so the answer is the one the
    /// guard would give rather than a stricter one.
    fn guard_difference(a: &Term, b: &Term) -> Option<String> {
        let (ga, gb) = (a, b);
        if ga.history_size() != gb.history_size() {
            return Some(format!(
                "scrollback depth: source {} vs client {}",
                ga.history_size(),
                gb.history_size()
            ));
        }
        for line in ga.topmost_line().0..=ga.bottommost_line().0 {
            for col in 0..ga.columns() {
                let (ca, cb) = (
                    &ga.cell(Point::new(Line(line), Column(col))),
                    &gb.cell(Point::new(Line(line), Column(col))),
                );
                let seen = |c: &Cell| {
                    (
                        c.c,
                        c.flags & !super::DERIVED_FLAGS,
                        c.fg,
                        c.bg,
                        c.underline_color(),
                        c.zerowidth().unwrap_or(&[]).to_vec(),
                    )
                };
                if seen(ca) != seen(cb) {
                    return Some(format!(
                        "line {line} column {col}: source {:?} vs client {:?}",
                        seen(ca),
                        seen(cb)
                    ));
                }
            }
        }
        if ga.cursor().point != gb.cursor().point {
            return Some(format!(
                "cursor: source {:?} vs client {:?}",
                ga.cursor().point,
                gb.cursor().point
            ));
        }
        None
    }

    /// Does encode-then-replay reproduce this content exactly?
    fn faithful(cols: usize, rows: usize, bytes: &[u8]) -> bool {
        let source = term_fed(cols, rows, bytes);
        grid_hash(&source) == grid_hash(&replay(&source))
    }

    /// A wide character whose trailing spacer has been erased out from under it.
    ///
    /// `\x1b[J` and friends erase by cell, so they will happily take the second
    /// half of a double-width character and leave the first. The grid can hold
    /// that; printing cannot produce it, because printing the character always
    /// writes both halves. So the encoder — which reproduces a grid by printing
    /// into one — cannot reach this state by any sequence of characters, and no
    /// amount of care in the trimming or the wrap handling changes that.
    ///
    /// Visually the two are the same: the renderer draws from the WIDE_CHAR
    /// cell either way. Only `grid_hash` can tell them apart, which is what
    /// makes this a guard problem rather than a rendering one.
    ///
    /// Skipped here and tracked separately rather than hidden by narrowing the
    /// generator, so the sweep keeps its coverage of erases and wide characters
    /// everywhere else. Minimal reproduction is pinned in
    /// `a_wide_char_whose_spacer_was_erased_cannot_be_reprinted`.
    fn torn_wide_char(term: &Term) -> bool {
        use crate::vt::Flags;
        let grid = term;
        let cols = grid.columns();
        if cols == 0 {
            return false;
        }
        let flags = |line: i32, col: usize| grid.cell(Point::new(Line(line), Column(col))).flags;
        for line in grid.topmost_line().0..=grid.bottommost_line().0 {
            for col in 0..cols {
                let f = flags(line, col);
                // A wide character missing the spacer that belongs to it.
                if f.contains(Flags::WIDE_CHAR)
                    && (col + 1 >= cols || !flags(line, col + 1).contains(Flags::WIDE_CHAR_SPACER))
                {
                    return true;
                }
                // A spacer with nothing in front of it to be the spacer for.
                if f.contains(Flags::WIDE_CHAR_SPACER)
                    && (col == 0 || !flags(line, col - 1).contains(Flags::WIDE_CHAR))
                {
                    return true;
                }
                // The gap left at a line end where a wide character did not
                // fit, after the character it made room for has been erased off
                // the row below.
                if f.contains(Flags::LEADING_WIDE_CHAR_SPACER)
                    && (line >= grid.bottommost_line().0
                        || !flags(line + 1, 0).contains(Flags::WIDE_CHAR))
                {
                    return true;
                }
            }
        }
        false
    }

    ///
    /// On rio-vt the premise itself fails: its erase never leaves half a wide
    /// character behind, so the grid this test is about cannot occur there.
    /// It stays, ignored, for the alacritty fallback.
    #[test]
    #[ignore = "known gap on alacritty: a torn wide character cannot be reprinted; rio-vt never tears one"]
    fn a_wide_char_whose_spacer_was_erased_cannot_be_reprinted() {
        // Twenty-five bytes, found by the sweep and shrunk by it. `\x1b[J`
        // erases from the cursor, which is parked on the emoji's second half,
        // so the source keeps WIDE_CHAR at column 18 and loses the spacer at
        // 19. Replaying prints the emoji, which writes both halves back.
        //
        //   source |…🙂 | flags |..................W.|
        //   client |…🙂 | flags |..................Ws|
        //
        // Un-ignore when the encoder learns to erase the spacer after printing,
        // or when the guard stops reading a flag no renderer distinguishes.
        let source = term_fed(20, 5, "abcdefghijklmnop  \u{1f642}\x1b[J".as_bytes());
        assert!(torn_wide_char(&source), "the premise: the source is torn");
        assert_same(&source, &replay(&source), "torn wide character");
    }

    /// The shortest still-failing version of a failing stream.
    ///
    /// Two passes, both greedy and both cheap: cut the tail down as far as it
    /// will go, then walk the head forward. Byte-wise rather than token-wise,
    /// so it can land mid-escape — which is fine, because the only thing asked
    /// of the result is that it still fails, and a truncated escape sequence is
    /// a thing real streams contain anyway.
    fn shrink(cols: usize, rows: usize, bytes: &[u8]) -> Vec<u8> {
        let mut best = bytes.to_vec();
        let mut step = best.len() / 2;
        while step > 0 {
            while best.len() > step && !faithful(cols, rows, &best[..best.len() - step]) {
                best.truncate(best.len() - step);
            }
            step /= 2;
        }
        let mut start = 0;
        let mut step = best.len() / 2;
        while step > 0 {
            while start + step < best.len() && !faithful(cols, rows, &best[start + step..]) {
                start += step;
            }
            step /= 2;
        }
        best[start..].to_vec()
    }

    /// Replay a snapshot captured from a REAL host and check it against the
    /// hash that host computed for its own grid.
    ///
    /// Every other test in this module is the encoder marking its own homework:
    /// it builds a terminal, encodes it, replays it, and compares the two
    /// terminals it made. This one compares against a number produced by a
    /// different process, from a real pseudoterminal, over content a shell
    /// actually printed. It is the only test here that could catch the encoder
    /// and the hash being wrong in the same direction.
    ///
    /// Capture with `scripts/probe-capture.py`, which stands up a throwaway
    /// session host, feeds a pane the shapes the generated sweep found, and
    /// writes `<name>.bin` and `<name>.json` beside each other.
    ///
    /// ```text
    /// TD_SNAPSHOT=/path/to/capture cargo test --bin terminal-delight \
    ///     a_real_hosts_snapshot -- --ignored --nocapture
    /// ```
    ///
    /// `TD_SNAPSHOT_UNMASKED=1` hashes spacer flags in, which is what
    /// `grid_hash` did before `DERIVED_FLAGS` — set it when the capture came
    /// from a host built before that change, or the comparison is between two
    /// different hash functions and means nothing.
    #[test]
    #[ignore = "needs a capture from a real host; see the doc comment"]
    fn a_real_hosts_snapshot_replays_to_the_hash_that_host_reported() {
        let Ok(base) = std::env::var("TD_SNAPSHOT") else {
            panic!("set TD_SNAPSHOT to the capture prefix (without .bin/.json)");
        };
        let bytes = std::fs::read(format!("{base}.bin")).expect("the captured bytes");
        let meta = std::fs::read_to_string(format!("{base}.json")).expect("the capture metadata");
        let field = |k: &str| -> u64 {
            let at = meta
                .find(&format!("\"{k}\""))
                .unwrap_or_else(|| panic!("no {k}"));
            let rest = &meta[at + k.len() + 3..];
            let digits: String = rest
                .trim_start_matches([':', ' '])
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits
                .parse()
                .unwrap_or_else(|_| panic!("{k} is not a number"))
        };
        let (cols, rows) = (field("cols") as usize, field("rows") as usize);
        let reported = field("hash");
        assert_eq!(
            field("bytes_read"),
            field("stream_offset"),
            "the capture is not aligned with the host's stream — the pane was still printing, \
             so its hash describes a grid these bytes do not produce"
        );

        let replica = term_fed(cols, rows, &bytes);
        let unmasked = std::env::var("TD_SNAPSHOT_UNMASKED").is_ok();
        let ours = if unmasked {
            grid_hash_including_spacers(&replica)
        } else {
            grid_hash(&replica)
        };
        assert_eq!(
            ours,
            reported,
            "replaying {} bytes of a real host's snapshot at {cols}x{rows} produced a grid the \
             host would call divergent (ours {ours:#x}, host {reported:#x}){}",
            bytes.len(),
            if unmasked { " [unmasked]" } else { "" }
        );
    }

    /// `grid_hash` as it was before `DERIVED_FLAGS` — spacer flags hashed in.
    ///
    /// Exists so a capture taken from an older host can be checked against the
    /// hash function that host was actually running. Comparing it to today's
    /// would be comparing two different functions and calling the difference a
    /// bug.
    fn grid_hash_including_spacers(term: &Term) -> u64 {
        const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut h = OFFSET;
        let mut eat = |bytes: &[u8]| {
            for b in bytes {
                h ^= *b as u64;
                h = h.wrapping_mul(PRIME);
            }
        };
        let grid = term;
        let cols = grid.columns();
        for line in grid.topmost_line().0..=grid.bottommost_line().0 {
            let row = grid.row(Line(line));
            for col in 0..cols {
                let cell = &row[Column(col)];
                eat(&(cell.c as u32).to_le_bytes());
                eat(&cell.flags.bits().to_le_bytes());
                eat(&super::hash_color(cell.fg).to_le_bytes());
                eat(&super::hash_color(cell.bg).to_le_bytes());
                eat(&match cell.underline_color() {
                    Some(c) => super::hash_color(c),
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
        let cursor = grid.cursor().point;
        eat(&cursor.line.0.to_le_bytes());
        eat(&cursor.column.0.to_le_bytes());
        let modes = super::RESTORED_MODES
            .iter()
            .map(|(bit, _)| *bit)
            .fold(TermMode::SHOW_CURSOR | TermMode::ALT_SCREEN, |a, b| a | b);
        eat(&(term.mode().intersection(modes)).bits().to_le_bytes());
        h
    }

    #[test]
    #[ignore = "diagnostic: prints the encoder's bytes for a failing case"]
    fn dump_the_snapshot_for_a_failing_case() {
        let input = "abcdefghijklmnop  \u{1f642}\x1b[J".as_bytes();
        let source = term_fed(20, 5, input);
        let snap = encode_snapshot(&source);
        println!(
            "--- encoder output ---\n{:?}",
            String::from_utf8_lossy(&snap)
        );
        let client = term_fed(20, 5, &snap);
        for (name, t) in [("source", &source), ("client", &client)] {
            println!("--- {name} ---");
            let g = t;
            for line in g.topmost_line().0..=g.bottommost_line().0 {
                let row = g.row(Line(line));
                let text: String = (0..g.columns()).map(|c| row[Column(c)].c).collect();
                let flags: String = (0..g.columns())
                    .map(|c| {
                        let f = row[Column(c)].flags;
                        use crate::vt::Flags as F;
                        if f.contains(F::WIDE_CHAR) {
                            'W'
                        } else if f.contains(F::WIDE_CHAR_SPACER) {
                            's'
                        } else if f.contains(F::LEADING_WIDE_CHAR_SPACER) {
                            'L'
                        } else if f.contains(F::WRAPLINE) {
                            '>'
                        } else {
                            '.'
                        }
                    })
                    .collect();
                println!("  {line:>3} |{text}| flags |{flags}|");
            }
        }
    }

    /// One pseudo-random terminal session. Same seed, same bytes, forever.
    fn generate(seed: u64, cols: usize) -> Vec<u8> {
        let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut out = String::new();
        for _ in 0..24 {
            match next() % 11 {
                // A run of text, often landing exactly on the margin — which is
                // where wrapping decisions are made.
                0..=2 => {
                    let n = (next() as usize) % (cols * 2 + 2);
                    for i in 0..n {
                        out.push((b'a' + (i % 26) as u8) as char);
                    }
                }
                // A run that ends in spaces. The case that was broken: a wrap
                // whose continuation row holds nothing but blanks.
                3..=4 => {
                    let n = (next() as usize) % (cols + 3);
                    for i in 0..n {
                        out.push((b'a' + (i % 26) as u8) as char);
                    }
                    for _ in 0..(next() as usize % 4 + 1) {
                        out.push(' ');
                    }
                }
                5 => out.push_str("\r\n"),
                6 => out.push_str(match next() % 6 {
                    0 => "\x1b[1m",
                    1 => "\x1b[4:3m",
                    2 => "\x1b[31;44m",
                    3 => "\x1b[38;2;10;20;30m",
                    4 => "\x1b[7m",
                    _ => "\x1b[0m",
                }),
                // Wide characters and a combining mark: the spacer cells the
                // encoder skips on purpose.
                7 => out.push_str(match next() % 3 {
                    0 => "漢字",
                    1 => "e\u{0301}x",
                    _ => "🙂",
                }),
                // Erases, which paint the current background into cells that
                // then must not be trimmed as blank.
                8 => out.push_str(match next() % 3 {
                    0 => "\x1b[K",
                    1 => "\x1b[2K",
                    _ => "\x1b[J",
                }),
                // Cursor moves, so content is overwritten rather than appended.
                9 => {
                    let r = next() as usize % 6 + 1;
                    let c = next() as usize % cols + 1;
                    out.push_str(&format!("\x1b[{r};{c}H"));
                }
                _ => out.push_str("\r\n\r\n"),
            }
        }
        out.into_bytes()
    }

    #[test]
    fn a_wrap_into_a_blank_continuation_survives() {
        // The encoder reproduces a soft wrap by *causing* one: a wrapped row is
        // painted to its full width, the newline is withheld, and the first
        // character of the next row triggers the wrap that sets WRAPLINE.
        //
        // That depends on the next row having a character to paint. A
        // continuation row holding nothing but spaces is entirely `is_blank`,
        // so the trailing-blank trim emits zero bytes for it, nothing triggers
        // the pending wrap, and the wrap is lost. `grid_hash` reads
        // `cell.flags`, so a dropped WRAPLINE is a divergence — and a
        // divergence the guard can never repair, because every re-snapshot
        // drops it again.
        //
        // Twenty characters exactly fill the row; the space after them is the
        // continuation. Any wrapped line ending in a space does this, which is
        // most wrapped prose.
        let source = term_fed(20, 5, b"abcdefghijklmnopqrst ");
        assert!(
            source
                .cell(Point::new(Line(0), Column(19)))
                .flags
                .contains(crate::vt::Flags::WRAPLINE),
            "the test's premise is that the source row is wrapped"
        );
        assert_same(&source, &replay(&source), "wrap into a blank continuation");
    }

    #[test]
    fn a_wide_character_wrapped_to_the_next_row_survives() {
        // A wide character that does not fit in the last column leaves a
        // LEADING_WIDE_CHAR_SPACER behind and moves to the next row. The
        // encoder skips both spacer kinds on the grounds that writing the
        // character reproduces them — which is true only if the character is
        // still written in the same place.
        let source = term_fed(20, 5, "abcdefghijklmnopqrs漢字".as_bytes());
        assert_same(&source, &replay(&source), "wide char wrapped at the margin");
    }

    #[test]
    fn an_erased_row_keeps_the_colour_it_was_erased_with() {
        // `\x1b[K` erases with the *current* background, so the trailing cells
        // of this row are not default and must not be trimmed as blank.
        let source = term_fed(20, 5, b"\x1b[44mtinted\x1b[K\r\nplain");
        assert_same(&source, &replay(&source), "erase to end of line in colour");
    }

    #[test]
    fn a_wrap_crossing_the_history_boundary_survives() {
        // The wrap machinery is driven off `wrapped_before`, which refuses to
        // look above `topmost_line`. A wrap whose two halves straddle the
        // history/screen boundary is the case where that refusal is load-
        // bearing, and nothing else exercises it.
        let mut bytes = String::new();
        for i in 0..40 {
            bytes.push_str(&format!("line {i:02} padding to twenty\r\n"));
        }
        bytes.push_str("abcdefghijklmnopqrst continues past the margin");
        let source = term_fed(20, 5, bytes.as_bytes());
        assert!(source.history_size() > 30, "test needs real history");
        assert_same(&source, &replay(&source), "wrap across the history edge");
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
        let wrapped = source
            .cell(Point::new(Line(0), Column(9)))
            .flags
            .contains(crate::vt::Flags::WRAPLINE);
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
        let last = &source.cell(Point::new(Line(0), Column(9)));
        assert!(
            last.c == ' ' && last.flags.contains(crate::vt::Flags::WRAPLINE),
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
            source.history_size() > client.history_size(),
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
        assert_eq!(source.cell(Point::new(Line(0), Column(0))).c, 'a');
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
            source.cursor().point,
            client.cursor().point,
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
