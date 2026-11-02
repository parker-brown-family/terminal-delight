//! A synchronized update's bytes, held back from the core until it closes.
//!
//! A program opens an update (DEC 2026, `ESC [ ? 2026 h`, or the older string
//! `ESC P = 1 s ESC \`) before it redraws and closes it after, so that the
//! terminal shows the frame whole or not at all. Up to rio-vt 0.5.28 the core
//! did that by holding the update's bytes back from its grid and replaying them
//! when it closed. From 0.5.29 it parses every byte at once and only records
//! that an update is open, leaving it to the renderer to keep showing the last
//! frame (rio PR 2000).
//!
//! TD keeps the old arrangement, here, in front of the core. Two reasons. Its
//! renderer reads the grid for reasons of its own, its effects among them, so a
//! grid that is half a frame would be drawn. And a window is attached to a
//! session host that is never restarted, and the guard in `gridwire.rs`
//! compares the two grids at the same byte offset: a host built on 0.5.28
//! holds an update's bytes, so a window that did not would disagree with it
//! for as long as any update was open, and take the pane again and again.
//! Holding the bytes here gives the grid, at every offset, the shape it has
//! always had.
//!
//! The rules are rio-vt 0.5.28's (`Processor::advance_sync`, MIT, Raphael
//! Amorim and contributors), kept exactly: the core says when an update opens;
//! the bytes after it are held for at most [`TIMEOUT`] and [`MOST`] bytes; the
//! exact closer, in either form, releases them to the core in one piece; an
//! exact opener among them starts the next update there. Past the limit or the
//! deadline the frame is drawn as it stands.

use std::time::{Duration, Instant};

/// How long an update may hold its bytes back. rio-vt 0.5.28's figure.
pub(super) const TIMEOUT: Duration = Duration::from_millis(150);

/// The most bytes an update may hold. rio-vt 0.5.28's figure, 2 MiB.
pub(super) const MOST: usize = 0x20_0000;

/// The longest opener or closer, and the shortest.
const LONGEST: usize = 8;
const SHORTEST: usize = 7;

const OPEN_CSI: &[u8; LONGEST] = b"\x1b[?2026h";
const OPEN_DCS: &[u8; SHORTEST] = b"\x1bP=1s\x1b\\";
const CLOSE_CSI: &[u8; LONGEST] = b"\x1b[?2026l";
const CLOSE_DCS: &[u8; SHORTEST] = b"\x1bP=2s\x1b\\";

/// What the hold hands bytes to: the core, as far as updates go.
pub(super) trait Core {
    /// Parse `bytes` into the grid.
    fn parse(&mut self, bytes: &[u8]);
    /// Whether the core has an update open, by what it has parsed.
    fn open(&self) -> bool;
    /// End the open update, as a closer would.
    fn close(&mut self);
}

/// The bytes of an open update, and when they must be drawn regardless.
#[derive(Debug, Default)]
pub(super) struct Hold {
    held: Vec<u8>,
    deadline: Option<Instant>,
}

impl Hold {
    /// Bytes held back.
    pub(super) fn len(&self) -> usize {
        self.held.len()
    }

    /// When the open update must be drawn regardless; `None` with none open.
    pub(super) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Hand `bytes` to `core`, holding back those that belong to an open
    /// update.
    pub(super) fn advance(&mut self, core: &mut impl Core, bytes: &[u8]) {
        match self.deadline {
            // An update left open past its deadline by a loop that did not
            // draw it: drawn now, before what follows.
            Some(at) if at <= Instant::now() => {
                self.release(core, None);
                self.parse(core, bytes);
            }
            Some(_) => self.hold(core, bytes),
            None => self.parse(core, bytes),
        }
    }

    /// Draw the open update as it stands, and end it.
    pub(super) fn flush(&mut self, core: &mut impl Core) {
        self.release(core, None);
    }

    /// Bytes outside any update go straight to the core, which says whether
    /// they opened one.
    fn parse(&mut self, core: &mut impl Core, bytes: &[u8]) {
        core.parse(bytes);
        if self.deadline.is_none() && core.open() {
            self.deadline = Some(Instant::now() + TIMEOUT);
        }
    }

    fn hold(&mut self, core: &mut impl Core, bytes: &[u8]) {
        if self.held.len() + bytes.len() >= MOST - 1 {
            self.release(core, None);
            self.parse(core, bytes);
            return;
        }
        self.held.extend_from_slice(bytes);

        // Only the new bytes, and an escape the last read cut short, can hold
        // an opener or a closer. Read from the end back: the last closer ends
        // the update, and an opener after it begins the next one.
        let len = self.held.len();
        let from = (len - bytes.len()).saturating_sub(LONGEST - 1);
        let to = len.saturating_sub(SHORTEST - 1);
        let mut opener = None;
        for at in memchr::memchr_iter(0x1b, &self.held[from..to]).rev() {
            let at = from + at;
            let is = |what: &[u8]| self.held.get(at..at + what.len()) == Some(what);
            if is(OPEN_CSI) || is(OPEN_DCS) {
                self.deadline = Some(Instant::now() + TIMEOUT);
                opener = Some(at);
            } else if is(CLOSE_CSI) || is(CLOSE_DCS) {
                self.release(core, opener);
                break;
            }
        }
    }

    /// Hand the held bytes to the core: all of them, or those before an
    /// `opener` that begins the next update, which stays held.
    fn release(&mut self, core: &mut impl Core, opener: Option<usize>) {
        let held = std::mem::take(&mut self.held);
        let end = opener.unwrap_or(held.len());
        core.parse(&held[..end]);
        self.held = held;
        match opener {
            Some(at) => {
                self.held.drain(..at);
                // Replaying may have read a closer inline; this opener is
                // still waiting for its own.
                self.deadline = Some(Instant::now() + TIMEOUT);
            }
            None => {
                core.close();
                self.deadline = None;
                self.held.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A core that records what it parsed and keeps an update open the way
    /// rio-vt 0.5.29 does: from an opener to a closer, in either form.
    #[derive(Default)]
    struct Recorder {
        parsed: Vec<u8>,
        pieces: usize,
        open: bool,
    }

    impl Core for Recorder {
        fn parse(&mut self, bytes: &[u8]) {
            if bytes.is_empty() {
                return;
            }
            self.pieces += 1;
            self.parsed.extend_from_slice(bytes);
            let ends = |what: &[u8]| memchr::memmem::rfind(bytes, what);
            let opened = ends(OPEN_CSI).max(ends(OPEN_DCS));
            let closed = ends(CLOSE_CSI).max(ends(CLOSE_DCS));
            match (opened, closed) {
                (Some(o), Some(c)) => self.open = o > c,
                (Some(_), None) => self.open = true,
                (None, Some(_)) => self.open = false,
                (None, None) => {}
            }
        }
        fn open(&self) -> bool {
            self.open
        }
        fn close(&mut self) {
            self.open = false;
        }
    }

    #[test]
    fn an_update_reaches_the_core_whole_when_it_closes() {
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"\x1b[?2026h");
        assert!(hold.deadline().is_some(), "the opener arms the deadline");
        hold.advance(&mut core, b"frame, ");
        hold.advance(&mut core, b"in parts");
        assert_eq!(core.parsed, b"\x1b[?2026h", "nothing of the frame yet");
        assert_eq!(hold.len(), 15);
        hold.advance(&mut core, b"\x1b[?2026lafter");
        assert_eq!(core.parsed, b"\x1b[?2026hframe, in parts\x1b[?2026lafter");
        assert_eq!((hold.len(), hold.deadline()), (0, None));
    }

    #[test]
    fn the_older_form_holds_and_releases_the_same_way() {
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"\x1bP=1s\x1b\\");
        hold.advance(&mut core, b"frame");
        assert_eq!(hold.len(), 5);
        hold.advance(&mut core, b"\x1bP=2s\x1b\\");
        assert_eq!(core.parsed, b"\x1bP=1s\x1b\\frame\x1bP=2s\x1b\\");
        assert_eq!(hold.len(), 0);
    }

    #[test]
    fn a_closer_cut_between_reads_is_still_found() {
        let stream = b"frame\x1b[?2026lafter";
        for cut in 0..=stream.len() {
            let (mut hold, mut core) = (Hold::default(), Recorder::default());
            hold.advance(&mut core, b"\x1b[?2026h");
            hold.advance(&mut core, &stream[..cut]);
            hold.advance(&mut core, &stream[cut..]);
            assert_eq!(
                core.parsed, b"\x1b[?2026hframe\x1b[?2026lafter",
                "cut at {cut}"
            );
            assert_eq!(hold.len(), 0, "cut at {cut}");
        }
    }

    #[test]
    fn an_opener_after_the_closer_holds_the_next_frame() {
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"\x1b[?2026h");
        hold.advance(&mut core, b"one\x1b[?2026l\x1b[?2026htwo");
        assert_eq!(core.parsed, b"\x1b[?2026hone\x1b[?2026l");
        assert_eq!(hold.held, b"\x1b[?2026htwo");
        assert!(hold.deadline().is_some());
        hold.advance(&mut core, b"\x1b[?2026l");
        assert_eq!(
            core.parsed,
            b"\x1b[?2026hone\x1b[?2026l\x1b[?2026htwo\x1b[?2026l"
        );
        assert_eq!(hold.len(), 0);
    }

    #[test]
    fn past_the_limit_the_frame_is_drawn_as_it_stands() {
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"\x1b[?2026h");
        let block = vec![b'x'; MOST / 2];
        hold.advance(&mut core, &block);
        assert_eq!(hold.len(), block.len());
        hold.advance(&mut core, &block);
        assert_eq!(hold.len(), 0, "released");
        assert_eq!(core.parsed.len(), 8 + 2 * block.len(), "and nothing lost");
        assert!(!core.open, "the update is over");
        assert_eq!(hold.deadline(), None);
    }

    #[test]
    fn a_flush_draws_the_frame_and_ends_the_update() {
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"\x1b[?2026h");
        hold.advance(&mut core, b"half a frame");
        hold.flush(&mut core);
        assert_eq!(core.parsed, b"\x1b[?2026hhalf a frame");
        assert!(!core.open);
        assert_eq!((hold.len(), hold.deadline()), (0, None));
        // And the next bytes go straight through.
        hold.advance(&mut core, b"next");
        assert_eq!(hold.len(), 0);
    }

    #[test]
    fn an_update_past_its_deadline_is_drawn_before_what_follows() {
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"\x1b[?2026h");
        hold.advance(&mut core, b"stale");
        hold.deadline = Instant::now().checked_sub(Duration::from_millis(1));
        hold.advance(&mut core, b"fresh");
        assert_eq!(core.parsed, b"\x1b[?2026hstalefresh");
        assert_eq!((hold.len(), hold.deadline()), (0, None));
    }

    #[test]
    fn text_that_shares_the_openers_read_goes_through_as_the_core_left_it() {
        // The opener mid-read is the core's to handle (`vt/text.rs` splits the
        // read there, so in TD it ends one); the hold starts with the next.
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"before\x1b[?2026hsame read");
        assert_eq!(core.parsed, b"before\x1b[?2026hsame read");
        hold.advance(&mut core, b"next read");
        assert_eq!(hold.len(), 9);
    }

    #[test]
    fn a_closer_with_no_update_open_is_just_bytes() {
        let (mut hold, mut core) = (Hold::default(), Recorder::default());
        hold.advance(&mut core, b"\x1b[?2026lplain");
        assert_eq!(core.parsed, b"\x1b[?2026lplain");
        assert_eq!((hold.len(), hold.deadline(), core.pieces), (0, None, 1));
    }
}
