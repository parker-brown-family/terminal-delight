//! What of a program's text reaches the core, read the way the program meant
//! it, and bounded where rio-vt is not.
//!
//! **Combining marks.** rio-vt attaches a mark to the cell before the cursor by
//! copying the cell's whole list of marks and interning the copy, so every
//! mark on a cell costs as much as all the marks before it: 1,000 on one cell
//! take milliseconds, 32,000 take seconds, all under the terminal's lock with
//! the window's frame waiting on it. A program does not even have to send them.
//! `e`, U+0301 and `ESC[65535b`, thirteen bytes, repeat the mark 65,535 times:
//! 9.7 s and 2.8 GB in review on 2026-09-25, where alacritty took 1.4 ms. So a
//! mark reaches the core only straight after the character it belongs to, with
//! no cursor movement between, and no more than [`MOST_MARKS`] of them, the
//! Unicode stream-safe limit (UAX #15) with room to spare. A repeat whose
//! character is a mark goes nowhere. Grapheme clustering (DEC 2027) joins whole
//! characters onto a cell the same way, so a request to turn it on goes nowhere
//! either — which also keeps a window agreeing, cell for cell, with a session
//! host that cannot cluster.
//!
//! **A synchronized update starts where the program starts it.** rio-vt holds
//! an update's bytes back only from its next read, so a read that opened one
//! drew the first part of the frame at once, torn. The rest of such a read is
//! handed to the core as a read of its own.
//!
//! Everything else passes through as it came, in as few pieces as it came in.
//! Only an escape sequence or a character cut off by the end of a chunk is held
//! back, until the chunk that completes it: a few bytes that mean nothing yet.

use rio_vt::codepoint_width::codepoint_width;

const ESC: u8 = 0x1b;
const BEL: u8 = 0x07;
const CAN: u8 = 0x18;
const SUB: u8 = 0x1a;

/// Marks one character may carry.
pub(super) const MOST_MARKS: usize = 32;

/// Longer than this, a control sequence is passed as it is rather than read.
const LONGEST_SEQUENCE: usize = 64;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    /// Partway through a character: this many continuation bytes to come.
    Utf8(u8),
    /// After `ESC`.
    Escape,
    /// After `ESC` and intermediate bytes.
    Intermediate,
    /// After `ESC [`, to the final byte.
    Csi,
    /// Inside a string (OSC, DCS, SOS, PM, APC), to its end.
    Str,
}

#[derive(Debug, Default)]
pub(super) struct TextGuard {
    state: State,
    /// The start of a sequence or character the last chunk ended inside,
    /// not yet handed to the core.
    held: Vec<u8>,
    /// The codepoint being assembled.
    codepoint: u32,
    /// Marks passed since the character they belong to.
    marks: usize,
    /// Whether a mark now would land on the character just written.
    attached: bool,
    /// Whether the last codepoint handed to the core was a mark, which is
    /// what a repeat would repeat.
    last_was_mark: bool,
    /// The control sequence being read began in an earlier chunk and was too
    /// long to hold, so it has partly gone to the core already and is passed
    /// as it is.
    oversized: bool,
}

/// What to do with a control sequence once it is whole.
struct Verdict {
    /// What the core gets in its place; `None` for the sequence itself.
    instead: Option<Vec<u8>>,
    /// Whether the core's read ends with it.
    split: bool,
}

impl Verdict {
    const PASS: Verdict = Verdict {
        instead: None,
        split: false,
    };
}

impl TextGuard {
    pub(super) fn feed(&mut self, bytes: &[u8], core: &mut dyn FnMut(&[u8])) {
        if self.held.is_empty() {
            self.run(bytes, core);
        } else {
            let mut joined = std::mem::take(&mut self.held);
            joined.extend_from_slice(bytes);
            self.run(&joined, core);
        }
    }

    fn run(&mut self, bytes: &[u8], core: &mut dyn FnMut(&[u8])) {
        // `bytes[pass..i]` has been looked at and goes to the core unchanged;
        // `start` is where the sequence or character being read began.
        let (mut pass, mut i, mut start) = (0, 0, 0);
        while i < bytes.len() {
            let byte = bytes[i];
            match self.state {
                State::Ground => {
                    // Printable ASCII is characters, and nearly everything.
                    if (0x20..0x7f).contains(&byte) {
                        self.character();
                        i += printable_run(&bytes[i..]);
                        continue;
                    }
                    start = i;
                    i += 1;
                    match byte {
                        ESC => self.state = State::Escape,
                        // A bell moves nothing; every other control can.
                        BEL | 0x7f => {}
                        0x00..=0x1f => self.attached = false,
                        0xc2..=0xf4 => {
                            let more = match byte {
                                0xc2..=0xdf => 1,
                                0xe0..=0xef => 2,
                                _ => 3,
                            };
                            let first = byte & (0x7f >> (more + 1));
                            match bytes.get(i..i + more) {
                                // Whole and well formed, as nearly every
                                // character is: read at once, rather than a
                                // byte at a time as `Utf8` reads the rest.
                                Some(rest) if rest.iter().all(|b| (0x80..0xc0).contains(b)) => {
                                    self.codepoint = rest.iter().fold(u32::from(first), |cp, b| {
                                        (cp << 6) | u32::from(b & 0x3f)
                                    });
                                    i += more;
                                    if !self.goes_to_the_core() {
                                        emit(core, &bytes[pass..start]);
                                        pass = i;
                                    }
                                }
                                _ => self.utf8(first, more as u8),
                            }
                        }
                        // Not a character's first byte: the core prints a
                        // replacement character for it.
                        _ => self.character(),
                    }
                }
                State::Utf8(more) => {
                    if !(0x80..0xc0).contains(&byte) {
                        // Cut short: a replacement character, and this byte
                        // starts over.
                        self.state = State::Ground;
                        self.character();
                        continue;
                    }
                    self.codepoint = (self.codepoint << 6) | u32::from(byte & 0x3f);
                    i += 1;
                    if more > 1 {
                        self.state = State::Utf8(more - 1);
                        continue;
                    }
                    self.state = State::Ground;
                    if !self.goes_to_the_core() {
                        emit(core, &bytes[pass..start]);
                        pass = i;
                    }
                }
                State::Escape => {
                    i += 1;
                    match byte {
                        b'[' => {
                            self.state = State::Csi;
                            self.oversized = false;
                        }
                        b']' | b'P' | b'X' | b'^' | b'_' => self.state = State::Str,
                        0x20..=0x2f => self.state = State::Intermediate,
                        ESC => start = i - 1,
                        CAN | SUB => self.state = State::Ground,
                        _ => self.after_escape(byte),
                    }
                }
                State::Intermediate => {
                    i += 1;
                    match byte {
                        0x20..=0x2f => {}
                        ESC => {
                            start = i - 1;
                            self.state = State::Escape;
                        }
                        CAN | SUB => self.state = State::Ground,
                        _ => self.after_escape(byte),
                    }
                }
                State::Csi => {
                    // Parameters and intermediates go by unread; only the
                    // final byte, or what breaks the sequence off, is looked at.
                    let Some(at) = bytes[i..]
                        .iter()
                        .position(|&b| matches!(b, 0x40..=0x7e | ESC | CAN | SUB))
                    else {
                        i = bytes.len();
                        continue;
                    };
                    i += at;
                    let byte = bytes[i];
                    i += 1;
                    match byte {
                        0x40..=0x7e => {
                            self.state = State::Ground;
                            let sequence = &bytes[start..i];
                            let verdict = if std::mem::take(&mut self.oversized)
                                || sequence.len() > LONGEST_SEQUENCE
                            {
                                self.attached = false;
                                Verdict::PASS
                            } else {
                                self.judge(&sequence[2..sequence.len() - 1], byte)
                            };
                            if let Some(instead) = &verdict.instead {
                                emit(core, &bytes[pass..start]);
                                emit(core, instead);
                                pass = i;
                            }
                            if verdict.split {
                                emit(core, &bytes[pass..i]);
                                pass = i;
                            }
                        }
                        ESC => {
                            start = i - 1;
                            self.state = State::Escape;
                        }
                        CAN | SUB => self.state = State::Ground,
                        _ => {}
                    }
                }
                State::Str => {
                    // Nothing in a string is changed; only its end matters.
                    match super::kitty::string_end(&bytes[i..]) {
                        Some(at) => {
                            i += at;
                            if bytes[i] == ESC {
                                start = i;
                                self.state = State::Escape;
                            } else {
                                self.state = State::Ground;
                            }
                            i += 1;
                        }
                        None => i = bytes.len(),
                    }
                }
            }
        }
        // A sequence or character the chunk ended inside waits for the rest,
        // unless it has grown past anything worth reading.
        let open = match self.state {
            State::Utf8(_) | State::Escape | State::Intermediate => true,
            State::Csi => {
                let short = !self.oversized && bytes.len() - start <= LONGEST_SEQUENCE;
                self.oversized = !short;
                short
            }
            State::Ground | State::Str => false,
        };
        if open && start >= pass {
            emit(core, &bytes[pass..start]);
            self.held = bytes[start..].to_vec();
            // Read again from its first byte with the next chunk, from where
            // that byte was read: plain text.
            self.state = State::Ground;
        } else {
            emit(core, &bytes[pass..]);
        }
    }

    /// A character that takes a cell: marks may follow it.
    fn character(&mut self) {
        self.marks = 0;
        self.attached = true;
        self.last_was_mark = false;
    }

    /// Counts the character just read, `self.codepoint`, and says whether the
    /// core gets it: not a mark with nothing of its own to sit on, nor one past
    /// the limit.
    fn goes_to_the_core(&mut self) -> bool {
        match codepoint_width(self.codepoint) {
            Some(0) => {
                if !self.attached || self.marks >= MOST_MARKS {
                    return false;
                }
                self.marks += 1;
                self.last_was_mark = true;
            }
            Some(_) => self.character(),
            None => {}
        }
        true
    }

    fn utf8(&mut self, first: u8, more: u8) {
        self.codepoint = u32::from(first);
        self.state = State::Utf8(more);
    }

    /// An escape sequence other than a control sequence or a string has ended
    /// at `byte`. Most move the cursor or reset something.
    fn after_escape(&mut self, byte: u8) {
        self.state = State::Ground;
        self.attached = false;
        if byte == b'c' {
            // A full reset forgets what the last character was.
            self.last_was_mark = false;
        }
    }

    /// The verdict on a whole control sequence: `body` is what came between
    /// `ESC [` and `last`.
    fn judge(&mut self, body: &[u8], last: u8) -> Verdict {
        let private = body.first() == Some(&b'?');
        let plain = !body.iter().any(|b| matches!(b, 0x20..=0x2f | 0x3c..=0x3f));
        if plain && last == b'm' {
            // Colours and attributes: nothing moves.
            return Verdict::PASS;
        }
        self.attached = false;
        if plain && last == b'b' && self.last_was_mark {
            // A repeat repeats the last character the core printed, and when
            // that was a mark, every copy lands on one cell.
            return Verdict {
                instead: Some(Vec::new()),
                split: false,
            };
        }
        if !(private && matches!(last, b'h' | b'l')) {
            return Verdict::PASS;
        }
        let Some(modes) = std::str::from_utf8(&body[1..]).ok().and_then(|text| {
            text.split(';')
                .map(|mode| mode.parse::<u16>().ok())
                .collect::<Option<Vec<u16>>>()
        }) else {
            return Verdict::PASS;
        };
        let kept: Vec<u16> = modes.iter().copied().filter(|&mode| mode != 2027).collect();
        let instead = (kept.len() != modes.len()).then(|| {
            if kept.is_empty() {
                return Vec::new();
            }
            let kept: Vec<String> = kept.iter().map(u16::to_string).collect();
            let mut again = format!("\x1b[?{}", kept.join(";")).into_bytes();
            again.push(last);
            again
        });
        Verdict {
            instead,
            split: last == b'h' && kept.contains(&2026),
        }
    }
}

fn emit(core: &mut dyn FnMut(&[u8]), bytes: &[u8]) {
    if !bytes.is_empty() {
        core(bytes);
    }
}

/// How many bytes at the start of `bytes` are printable ASCII, `0x20` to `0x7e`.
///
/// Read eight at a time, because nearly all of a terminal's output is printable
/// ASCII; a byte at a time, the guards' searches cost the core a third of its
/// throughput (`what_the_guards_cost` in `vt/rio.rs`). A byte below `0x20`
/// borrows when `0x20` is taken from it, and one above `0x7e` has its top bit
/// set once one is added. Neither disturbs the bytes before it, so the lowest
/// byte flagged is the first that is not printable.
fn printable_run(bytes: &[u8]) -> usize {
    const ONES: u64 = u64::from_ne_bytes([1; 8]);
    const TOPS: u64 = ONES << 7;
    let (words, rest) = bytes.as_chunks::<8>();
    for (at, word) in words.iter().enumerate() {
        let x = u64::from_le_bytes(*word);
        let below = x.wrapping_sub(ONES * 0x20) & !x & TOPS;
        let above = (x.wrapping_add(ONES) | x) & TOPS;
        let flagged = below | above;
        if flagged != 0 {
            return at * 8 + flagged.trailing_zeros() as usize / 8;
        }
    }
    words.len() * 8
        + rest
            .iter()
            .position(|&b| !(0x20..0x7f).contains(&b))
            .unwrap_or(rest.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_of_printable_ascii_ends_where_a_byte_at_a_time_says() {
        // Every byte value, at every place in a word and in the tail after the
        // last whole word, among neighbours at both edges of printable.
        let slow = |bytes: &[u8]| {
            bytes
                .iter()
                .position(|&b| !(0x20..0x7f).contains(&b))
                .unwrap_or(bytes.len())
        };
        for fill in [b'a', 0x20, 0x7e] {
            for len in 0..=24 {
                let mut bytes = vec![fill; len];
                for at in 0..len {
                    for byte in 0..=255u8 {
                        bytes[at] = byte;
                        assert_eq!(
                            printable_run(&bytes),
                            slow(&bytes),
                            "{byte:#04x} at {at} of {len} in {fill:#04x}"
                        );
                        if (0x20..0x7f).contains(&byte) {
                            continue;
                        }
                        // A later byte that borrows or carries in its turn
                        // cannot move where the run ends.
                        for later in at + 1..len {
                            for after in [0x00, 0x1f, 0x7f, 0x80, 0xff] {
                                bytes[later] = after;
                                assert_eq!(printable_run(&bytes), at, "{after:#04x} at {later}");
                            }
                            bytes[later] = fill;
                        }
                    }
                    bytes[at] = fill;
                }
            }
        }
    }

    /// Feed `stream` split at `cuts`; what the core was handed, piece by piece.
    fn pieces(stream: &[u8], cuts: &[usize]) -> Vec<Vec<u8>> {
        let mut guard = TextGuard::default();
        let mut out = Vec::new();
        let mut at = 0;
        for &cut in cuts.iter().chain(std::iter::once(&stream.len())) {
            guard.feed(&stream[at..cut], &mut |bytes| out.push(bytes.to_vec()));
            at = cut;
        }
        out
    }

    fn guarded(stream: &[u8]) -> Vec<u8> {
        pieces(stream, &[]).concat()
    }

    fn marks(n: usize) -> String {
        "\u{301}".repeat(n)
    }

    #[test]
    fn ordinary_output_reaches_the_core_unchanged_and_in_one_piece() {
        let stream = "plain \x1b[31mred\x1b[0m caf\u{e9} e\u{301} \u{4f60} \x1b]0;t\u{301}itle\x07 \x1b[2J\x1b[?1049h \u{1f642}\r\n".as_bytes();
        assert_eq!(pieces(stream, &[]), vec![stream.to_vec()]);
        for cut in 0..stream.len() {
            assert_eq!(
                pieces(stream, &[cut]).concat(),
                stream.to_vec(),
                "cut at {cut}"
            );
        }
    }

    #[test]
    fn a_character_carries_no_more_than_the_limit_of_marks() {
        let stream = format!("e{}x", marks(MOST_MARKS + 8));
        let out = String::from_utf8(guarded(stream.as_bytes())).unwrap();
        assert_eq!(out, format!("e{}x", marks(MOST_MARKS)));
    }

    #[test]
    fn a_repeated_mark_goes_nowhere_and_a_repeated_character_goes_through() {
        let attack = "e\u{301}\x1b[65535b".as_bytes();
        assert_eq!(guarded(attack), "e\u{301}".as_bytes().to_vec());
        let ruler = b"-\x1b[40b";
        assert_eq!(guarded(ruler), ruler.to_vec());
    }

    #[test]
    fn a_mark_after_the_cursor_moves_has_nothing_to_sit_on() {
        // Moving back to the character and adding more is how the limit
        // would be walked around.
        let moved = format!("e\x1b[D{}", marks(3));
        assert_eq!(guarded(moved.as_bytes()), b"e\x1b[D".to_vec());
        // Colours move nothing, so marks after them still belong.
        let coloured = format!("e\x1b[31m{}", marks(3));
        assert_eq!(guarded(coloured.as_bytes()), coloured.as_bytes().to_vec());
    }

    #[test]
    fn grapheme_clustering_is_never_turned_on() {
        assert_eq!(guarded(b"a\x1b[?2027hb"), b"ab".to_vec());
        assert_eq!(guarded(b"\x1b[?2027;1004h"), b"\x1b[?1004h".to_vec());
    }

    #[test]
    fn a_sequence_or_string_cut_off_gives_way_to_what_follows() {
        // An `ESC` starts a sequence over; a cancel ends one, or a string, and
        // what follows is text again. Each is read that way at every cut.
        for (stream, want) in [
            ("\x1b[3\x1b[?2027h".to_string(), "\x1b[3".to_string()),
            (
                format!("\x1b[?20\x18e{}", marks(MOST_MARKS + 8)),
                format!("\x1b[?20\x18e{}", marks(MOST_MARKS)),
            ),
            (
                format!("\x1b]0;t\x1ae{}", marks(MOST_MARKS + 8)),
                format!("\x1b]0;t\x1ae{}", marks(MOST_MARKS)),
            ),
        ] {
            for cut in 0..=stream.len() {
                assert_eq!(
                    pieces(stream.as_bytes(), &[cut]).concat(),
                    want.as_bytes().to_vec(),
                    "{stream:?}, cut at {cut}"
                );
            }
        }
    }

    #[test]
    fn a_synchronized_update_begins_a_read_of_its_own() {
        let out = pieces(b"before\x1b[?2026hframe\x1b[?2026l", &[]);
        assert_eq!(
            out,
            vec![b"before\x1b[?2026h".to_vec(), b"frame\x1b[?2026l".to_vec()]
        );
    }

    #[test]
    fn a_rewrite_is_the_same_wherever_the_chunks_fall() {
        let stream = format!(
            "a\x1b[?2027h e{} \x1b[31m\u{301}\x1b[3b\x1b[?6h z",
            marks(MOST_MARKS + 2)
        );
        let whole = guarded(stream.as_bytes());
        for cut in 0..stream.len() {
            assert_eq!(
                pieces(stream.as_bytes(), &[cut]).concat(),
                whole,
                "cut at {cut}"
            );
        }
    }
}
