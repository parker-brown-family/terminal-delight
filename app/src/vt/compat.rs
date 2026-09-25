//! A snapshot from a session host built before the core swap, read the way
//! that host meant it.
//!
//! Such a host restores a pane's mouse reporting by writing the three
//! protocols one at a time, each on or off, in one run: `ESC[?1000h
//! ESC[?1002l ESC[?1003l` for a pane reporting clicks. Its core, alacritty,
//! turned off only the protocol a reset named. rio-vt, like xterm, turns
//! reporting off whichever one is named — so the last `l` switched off the
//! clicks the first `h` had just switched on, and every pane reporting clicks
//! or drags came into a new window reporting nothing: vim, htop and tmux stop
//! seeing the mouse. The divergence guard cannot repair it, because each repair
//! is the same snapshot. Found in review, 2026-09-25, before the cutover.
//!
//! [`MouseRun`] watches for exactly that run of three and, where it ends with
//! reporting turned off by a protocol that was not the one on, turns the one
//! that was on back on — what alacritty was left with. Every byte passes
//! through as it came, when it came; the correction follows the run. Once no
//! session host from before the swap is running this has nothing left to do,
//! and it goes with the alacritty fallback (issue 826).

const ESC: u8 = 0x1b;

/// The run a host from before the swap writes, `#` standing for `h` or `l`.
const RUN: &[u8; 24] = b"\x1b[?1000#\x1b[?1002#\x1b[?1003#";

/// The protocols' codes, in the run's order: clicks, drags, all motion.
const CODES: [&str; 3] = ["1000", "1002", "1003"];

/// The watch, one per terminal, fed every chunk in order.
#[derive(Debug, Default)]
pub(super) struct MouseRun {
    /// Bytes of the run matched so far, across chunks.
    matched: usize,
    /// The protocols the run has set so far.
    on: [bool; 3],
}

impl MouseRun {
    /// Hand `bytes` to `core` as they came, and after any run that ended with
    /// reporting off, the setting alacritty would have kept.
    pub(super) fn feed(&mut self, bytes: &[u8], core: &mut dyn FnMut(&[u8])) {
        let (mut pass, mut i) = (0, 0);
        while i < bytes.len() {
            if self.matched == 0 {
                // Only an ESC can begin the run.
                match bytes[i..].iter().position(|&b| b == ESC) {
                    Some(at) => i += at,
                    None => break,
                }
            }
            let byte = bytes[i];
            let fits = match RUN[self.matched] {
                b'#' => byte == b'h' || byte == b'l',
                wanted => byte == wanted,
            };
            if !fits {
                // Not the run. This byte may begin one itself.
                let restart = self.matched > 0 && byte == ESC;
                self.matched = 0;
                if !restart {
                    i += 1;
                }
                continue;
            }
            if RUN[self.matched] == b'#' {
                self.on[self.matched / 8] = byte == b'h';
            }
            self.matched += 1;
            i += 1;
            if self.matched == RUN.len() {
                self.matched = 0;
                let on = std::mem::take(&mut self.on);
                // Left on by the run in rio-vt: the last protocol, if it was
                // set. Left on in alacritty: the last one set. They differ only
                // when the run ends with a reset of one that was not on.
                if !on[2] {
                    if let Some(code) = CODES.iter().zip(on).rev().find(|(_, on)| *on) {
                        emit(core, &bytes[pass..i]);
                        core(format!("\x1b[?{}h", code.0).as_bytes());
                        pass = i;
                    }
                }
            }
        }
        emit(core, &bytes[pass..]);
    }
}

fn emit(core: &mut dyn FnMut(&[u8]), bytes: &[u8]) {
    if !bytes.is_empty() {
        core(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn through(stream: &[u8], cuts: &[usize]) -> (Vec<u8>, usize) {
        let mut run = MouseRun::default();
        let (mut out, mut calls) = (Vec::new(), 0);
        let mut at = 0;
        for &cut in cuts.iter().chain(std::iter::once(&stream.len())) {
            run.feed(&stream[at..cut], &mut |bytes| {
                out.extend_from_slice(bytes);
                calls += 1;
            });
            at = cut;
        }
        (out, calls)
    }

    #[test]
    fn output_without_the_run_passes_whole_at_once() {
        let stream = b"x\x1b[?1000h\x1b[?1006h\x1b[?1002l y \x1b[?25l\x1b";
        let (out, calls) = through(stream, &[]);
        assert_eq!(out, stream.to_vec());
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_program_turning_reporting_on_is_passed_on_at_once_not_held() {
        // The run's first third is a whole sequence of its own, and a program
        // that ends its output with it must not wait for more output.
        let (out, _) = through(b"\x1b[?1000h", &[]);
        assert_eq!(out, b"\x1b[?1000h".to_vec());
    }

    #[test]
    fn the_run_is_followed_by_what_alacritty_kept_wherever_the_chunks_fall() {
        for (run, kept) in [
            (&b"\x1b[?1000h\x1b[?1002l\x1b[?1003l"[..], Some("1000")),
            (b"\x1b[?1000l\x1b[?1002h\x1b[?1003l", Some("1002")),
            (b"\x1b[?1000h\x1b[?1002h\x1b[?1003l", Some("1002")),
            (b"\x1b[?1000l\x1b[?1002l\x1b[?1003h", None),
            (b"\x1b[?1000l\x1b[?1002l\x1b[?1003l", None),
        ] {
            let mut stream = b"a".to_vec();
            stream.extend_from_slice(run);
            stream.push(b'b');
            let mut want = b"a".to_vec();
            want.extend_from_slice(run);
            if let Some(code) = kept {
                want.extend_from_slice(format!("\x1b[?{code}h").as_bytes());
            }
            want.push(b'b');
            for cut in 0..=stream.len() {
                assert_eq!(through(&stream, &[cut]).0, want, "{kept:?}, cut at {cut}");
            }
        }
    }

    #[test]
    fn a_broken_run_restarts_at_the_escape_that_broke_it() {
        let stream = b"\x1b[?1000h\x1b[?1002l\x1b\x1b[?1000h\x1b[?1002l\x1b[?1003l";
        let (out, _) = through(stream, &[]);
        assert!(
            out.ends_with(b"\x1b[?1003l\x1b[?1000h"),
            "{:?}",
            String::from_utf8_lossy(&out)
        );
    }
}
