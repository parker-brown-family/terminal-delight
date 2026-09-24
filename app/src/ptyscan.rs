//! The size answers' pure half: what a pseudoterminal is told about its cell,
//! and the one size question the emulator cannot hear.
//!
//! # `CSI 16 t`, which vte drops
//!
//! A program drawing pixels into a terminal (chafa, a video player, anything
//! speaking a graphics protocol) needs one cell's size in pixels, and xterm's
//! way of asking is `CSI 16 t`, answered `CSI 6 ; height ; width t`. vte 0.15
//! parses the window operations 14, 18, 22 and 23 and nothing else
//! (`vte-0.15.0/src/ansi.rs`, the `('t', [])` arm), so the question is
//! swallowed before alacritty ever sees it, and a program that asks waits for a
//! reply that is never coming.
//!
//! Patching vte is the only way to hear it from inside the parser. The session
//! host already watches every byte a pane prints — its tee copies the output to
//! the attached window — so [`CellSizeQuery`] watches the same bytes for the
//! question, and the host answers it from the geometry it holds.
//!
//! The price is ordering. The scanner sees a read before the parser does, so a
//! program that writes another question and then `CSI 16 t` in one go is
//! answered `16 t` first. The usual probe sends the size question first and a
//! device-attributes request last, as a sentinel, which this answers in order.
//!
//! # Device pixels
//!
//! [`device_cell`] is what a window tells the pseudoterminal its cell is. The
//! kernel multiplies it by the grid into `ws_xpixel` / `ws_ypixel`, and the
//! same numbers answer `14 t` and `16 t`, so they have to be the pixels a
//! program drawing an image actually has: the logical cell times the window's
//! scale factor.
//!
//! # Why this module imports nothing
//!
//! Like `keylayer.rs`: it compiles standalone, so `rustc --test` runs these
//! tests in about a second, which is what makes mutating the scanner cheap
//! enough to actually do.

/// The bytes of the question, in the only form the scanner answers.
///
/// Exactly these five. `CSI 16 ; t` is left alone on purpose: xterm may read a
/// trailing empty parameter as `16 t` as well, and nobody here has checked.
const CELL_SIZE_QUESTION: &[u8] = b"\x1b[16t";

/// How much of `CSI 16 t` the output has shown so far.
///
/// Kept across reads, because a read can end anywhere and a question split
/// across two of them is still one question.
#[derive(Default, Debug)]
pub struct CellSizeQuery {
    /// Bytes of [`CELL_SIZE_QUESTION`] matched, `0..5`.
    seen: usize,
}

impl CellSizeQuery {
    /// Feed one chunk of a pane's output; answers how many complete
    /// `CSI 16 t` it finished.
    ///
    /// Runs on the host's reader thread for every chunk of every pane, so it
    /// allocates nothing and looks at each byte once. The question's only
    /// escape byte is its first, so a mismatch restarts at zero, or at one when
    /// the byte that broke the match is itself an escape — which is how vte
    /// treats an escape in the middle of a sequence: the old one is abandoned
    /// and a new one begins.
    pub fn feed(&mut self, bytes: &[u8]) -> usize {
        let mut finished = 0;
        for &byte in bytes {
            if byte == CELL_SIZE_QUESTION[self.seen] {
                self.seen += 1;
                if self.seen == CELL_SIZE_QUESTION.len() {
                    finished += 1;
                    self.seen = 0;
                }
            } else {
                self.seen = usize::from(byte == CELL_SIZE_QUESTION[0]);
            }
        }
        finished
    }
}

/// A cell in device pixels, for the pseudoterminal: the logical cell times the
/// window's scale factor, rounded.
///
/// Rounded rather than truncated. `cell_w as u16` told a 6.3 × 14.7 cell on a
/// 1.6× monitor that it was 6 × 14 — neither the logical size nor the device
/// one; this says 10 × 24. `floor` would say 10 × 23: programs size an image
/// from `ws_ypixel / rows`, and rounding can overstate a cell by half a pixel
/// where flooring understates it by up to one.
///
/// A float cast saturates, so a nonsense measurement cannot wrap: a negative
/// or NaN input comes out as 0, which a winsize reads as "not known".
pub fn device_cell(cell_w: f32, cell_h: f32, scale: f32) -> (u16, u16) {
    (
        (cell_w * scale).round() as u16,
        (cell_h * scale).round() as u16,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scanner_finds_a_cell_size_question_split_across_two_reads() {
        let mut scan = CellSizeQuery::default();
        assert_eq!(scan.feed(b"prompt\x1b[1"), 0, "half a question is not one");
        assert_eq!(
            scan.feed(b"6t"),
            1,
            "the rest of it arrived in the next read"
        );
        // Split at every other point too: a read can end anywhere.
        let question = b"\x1b[16t";
        for cut in 1..question.len() {
            let mut scan = CellSizeQuery::default();
            let (head, tail) = question.split_at(cut);
            assert_eq!(
                scan.feed(head) + scan.feed(tail),
                1,
                "split after byte {cut}"
            );
        }
    }

    #[test]
    fn the_scanner_counts_two_questions_in_one_read() {
        let mut scan = CellSizeQuery::default();
        assert_eq!(scan.feed(b"\x1b[16t\x1b[14t\x1b[16t"), 2);
        assert_eq!(scan.feed(b"\x1b[16t\x1b[16t"), 2, "back to back");
    }

    #[test]
    fn the_scanner_ignores_other_window_ops_and_lookalikes() {
        for other in [
            &b"\x1b[14t"[..], // the text area: alacritty answers it
            b"\x1b[18t",      // the text area in cells: alacritty answers it
            b"\x1b[116t",     // not 16
            b"\x1b[?16t",     // a private marker makes it another question
            b"16t",           // text, no introducer
            b"\x1b16t",       // an escape, but not a CSI
            b"\x1b[1;6t",     // parameters 1 and 6
        ] {
            let mut scan = CellSizeQuery::default();
            assert_eq!(scan.feed(other), 0, "{:?}", String::from_utf8_lossy(other));
        }
        // An abandoned sequence does not swallow the question behind it.
        let mut scan = CellSizeQuery::default();
        assert_eq!(scan.feed(b"\x1b[1\x1b[16t"), 1);
    }

    #[test]
    fn the_pty_is_told_its_cell_in_device_pixels() {
        assert_eq!(device_cell(6.3, 14.7, 1.6), (10, 24));
        assert_eq!(device_cell(6.3, 14.7, 1.0), (6, 15));
    }
}
