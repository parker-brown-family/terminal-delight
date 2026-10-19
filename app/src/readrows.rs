//! The rows the FOCUS reader has read from a terminal's history, kept between
//! reads so a pane streaming output is not re-read ten thousand rows a frame.
//!
//! # Why the rows can be kept
//!
//! A terminal's history changes in only three ways. Rows arrive at its bottom as
//! the screen scrolls; rows fall off its top once it holds its limit; and a
//! resize, a clear or a reset rewrites the lot. The first two are counted —
//! `history_size` plus `lines_evicted` is every row the history has ever taken —
//! so between two reads the reader need only fetch the rows that arrived and drop
//! the ones that fell off. Everything else it cannot see, it does not trust: a new
//! width or height, a new style, a core that keeps no eviction count once its
//! history is full, and rows at either end of what is kept that no longer match
//! the terminal's all mean starting over.
//!
//! Before this the reader walked the whole history on every change, holding the
//! terminal's lock for all of it, which stalled the parser behind it for as long
//! as the walk took — every frame, while a shell streamed.

use std::collections::VecDeque;

use crate::doc::GridRow;
use crate::vt::{self, Cell, Line};

/// A terminal's history rows as the reader last read them.
///
/// `K` is whatever else decides how a row reads — for a pane, the theme inputs
/// that coloured its runs. A different `K` means different rows.
pub struct ReadHistory<K> {
    key: K,
    columns: usize,
    screen_lines: usize,
    /// `lines_evicted` at the last read; `None` when the core does not count.
    evicted: Option<u64>,
    /// `history_size` at the last read.
    held: usize,
    rows: VecDeque<GridRow>,
}

/// What one [`ReadHistory::refresh`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refresh {
    /// Rows read from the grid this time, the checks included.
    pub read: usize,
    /// Whether the kept rows were thrown away and the history read in full.
    pub rebuilt: bool,
}

impl<K: PartialEq + Clone> ReadHistory<K> {
    /// The history, oldest row first.
    pub fn rows(&self) -> &VecDeque<GridRow> {
        &self.rows
    }

    /// Bring `slot` up to date with `term`'s history, reading as little of it as
    /// the counts allow. `read` turns one grid row into the reader's row; it must
    /// give the same row for the same cells, because the checks compare its
    /// output with what is kept.
    pub fn refresh(
        slot: &mut Option<Self>,
        term: &vt::Term,
        key: &K,
        mut read: impl FnMut(&[Cell]) -> GridRow,
    ) -> Refresh {
        let columns = term.columns();
        let screen_lines = term.screen_lines();
        let held = term.history_size();
        let evicted = term.lines_evicted();
        let delta = slot
            .as_ref()
            .and_then(|kept| kept.delta(key, columns, screen_lines, held, evicted));
        if let (Some((arrived, dropped)), Some(kept)) = (delta, slot.as_mut()) {
            kept.rows.drain(..dropped);
            let mut read_now = 0;
            // The rows either side of what is kept must still be the terminal's:
            // a clear followed by as much new output as was cleared passes every
            // count, and would otherwise serve the rows that were cleared.
            let top_ok = match kept.rows.front() {
                Some(first) => {
                    read_now += 1;
                    one_row(term, term.topmost_line(), &mut read) == *first
                }
                None => true,
            };
            let seam_ok = match kept.rows.back() {
                Some(last) if arrived < held => {
                    read_now += 1;
                    one_row(term, Line(-(arrived as i32) - 1), &mut read) == *last
                }
                _ => true,
            };
            if top_ok && seam_ok {
                if arrived > 0 {
                    term.for_each_row(Line(-(arrived as i32)), Line(-1), |_, cells| {
                        kept.rows.push_back(read(cells))
                    });
                }
                if kept.rows.len() == held {
                    kept.held = held;
                    kept.evicted = evicted;
                    return Refresh {
                        read: read_now + arrived,
                        rebuilt: false,
                    };
                }
            }
        }
        let mut rows = VecDeque::with_capacity(held);
        if held > 0 {
            term.for_each_row(term.topmost_line(), Line(-1), |_, cells| {
                rows.push_back(read(cells))
            });
        }
        *slot = Some(Self {
            key: key.clone(),
            columns,
            screen_lines,
            evicted,
            held,
            rows,
        });
        Refresh {
            read: held,
            rebuilt: true,
        }
    }

    /// How many rows arrived at the bottom of the history and how many fell off
    /// its top since the last read, or `None` when the counts cannot tell.
    fn delta(
        &self,
        key: &K,
        columns: usize,
        screen_lines: usize,
        held: usize,
        evicted: Option<u64>,
    ) -> Option<(usize, usize)> {
        if self.key != *key || self.columns != columns || self.screen_lines != screen_lines {
            return None;
        }
        let (arrived, dropped) = match (self.evicted, evicted) {
            (Some(was), Some(now)) if now >= was => {
                let before = was.checked_add(self.held as u64)?;
                let after = now.checked_add(held as u64)?;
                (
                    usize::try_from(after.checked_sub(before)?).ok()?,
                    usize::try_from(now - was).ok()?,
                )
            }
            // A core that keeps no eviction count can only be followed until
            // its history is full: after that, a row arriving and a row leaving
            // leave `history_size` exactly where it was.
            (None, None) if held < vt::SCROLLBACK && held >= self.held => (held - self.held, 0),
            _ => return None,
        };
        (dropped <= self.rows.len() && arrived <= held).then_some((arrived, dropped))
    }
}

/// The live screen's rows, top to bottom.
pub fn screen_rows(term: &vt::Term, mut read: impl FnMut(&[Cell]) -> GridRow) -> Vec<GridRow> {
    let mut out = Vec::with_capacity(term.screen_lines());
    term.for_each_row(Line(0), term.bottommost_line(), |_, cells| {
        out.push(read(cells))
    });
    out
}

fn one_row(term: &vt::Term, line: Line, read: &mut impl FnMut(&[Cell]) -> GridRow) -> GridRow {
    let mut out = GridRow::default();
    term.for_each_row(line, line, |_, cells| out = read(cells));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::DocLine;
    use crate::vt::{Event, Flags, Listener, TermSize};
    use std::sync::Arc;

    struct Silent;
    impl Listener for Silent {
        fn send_event(&self, _: Event) {}
    }

    fn term(cols: usize, rows: usize) -> vt::Term {
        vt::Term::new(TermSize::new(cols, rows, 8, 16), Arc::new(Silent))
    }

    /// The text of a row, and its soft-wrap flag: enough to compare rows by.
    fn read(cells: &[Cell]) -> GridRow {
        let text: String = cells
            .iter()
            .filter(|c| !c.flags.contains(Flags::WIDE_CHAR_SPACER))
            .map(|c| if c.c == '\0' { ' ' } else { c.c })
            .collect();
        GridRow {
            line: DocLine::new(text.trim_end().to_string(), Vec::new()),
            wrapped: cells.iter().any(|c| c.flags.contains(Flags::WRAPLINE)),
        }
    }

    fn lines(t: &mut vt::Term, from: usize, to: usize) {
        for i in from..to {
            t.advance(format!("line {i}\r\n").as_bytes());
        }
    }

    fn texts(h: &ReadHistory<()>) -> Vec<String> {
        h.rows().iter().map(|r| r.line.text.clone()).collect()
    }

    /// A streaming shell costs the reader the rows that arrived, not the whole
    /// history: after the first full read, eight new lines are eight rows read
    /// plus the two checks, and the kept rows are exactly the terminal's.
    #[test]
    fn a_refresh_reads_only_the_rows_that_arrived() {
        let mut t = term(20, 5);
        lines(&mut t, 0, 40);
        let mut slot = None;
        let first = ReadHistory::refresh(&mut slot, &t, &(), read);
        assert!(first.rebuilt);
        assert_eq!(first.read, t.history_size());

        lines(&mut t, 40, 48);
        let next = ReadHistory::refresh(&mut slot, &t, &(), read);
        assert!(
            !next.rebuilt,
            "nothing but new rows arrived, so nothing is re-read"
        );
        assert_eq!(
            next.read,
            8 + 2,
            "eight arrived, plus the top and seam checks"
        );

        let mut fresh = None;
        ReadHistory::refresh(&mut fresh, &t, &(), read);
        assert_eq!(
            texts(slot.as_ref().unwrap()),
            texts(fresh.as_ref().unwrap()),
            "the kept rows are the ones a full read would give"
        );
    }

    /// Nothing new means nothing read beyond the two checks.
    #[test]
    fn an_unchanged_history_costs_two_rows() {
        let mut t = term(20, 5);
        lines(&mut t, 0, 12);
        let mut slot = None;
        ReadHistory::refresh(&mut slot, &t, &(), read);
        let again = ReadHistory::refresh(&mut slot, &t, &(), read);
        assert_eq!(
            again,
            Refresh {
                read: 2,
                rebuilt: false
            }
        );
    }

    /// A new width, height or style rewrites the rows, so the reader starts over.
    #[test]
    fn a_resize_or_a_new_style_starts_over() {
        let mut t = term(20, 5);
        lines(&mut t, 0, 12);
        let mut slot = None;
        ReadHistory::refresh(&mut slot, &t, &0u8, read);
        t.resize(TermSize::new(30, 5, 8, 16));
        assert!(
            ReadHistory::refresh(&mut slot, &t, &0u8, read).rebuilt,
            "a new width"
        );
        assert!(
            ReadHistory::refresh(&mut slot, &t, &1u8, read).rebuilt,
            "a new style"
        );
    }

    /// A clear followed by more output than was cleared never serves a cleared row.
    ///
    /// The two cores get there differently, and both must. rio-vt counts the
    /// cleared rows as evicted, so the counts are honest and the reader drops
    /// them and fetches what arrived. alacritty counts nothing, so the history
    /// merely looks longer than it was — and only the check on the rows at
    /// either end of what is kept catches that the old rows are gone. Removing
    /// those checks fails this test on alacritty.
    #[test]
    fn a_clear_never_serves_the_rows_it_cleared() {
        let mut t = term(20, 5);
        lines(&mut t, 0, 12);
        let mut slot = None;
        ReadHistory::refresh(&mut slot, &t, &(), read);
        t.clear_history();
        lines(&mut t, 100, 125);
        ReadHistory::refresh(&mut slot, &t, &(), read);
        let mut fresh = None;
        ReadHistory::refresh(&mut fresh, &t, &(), read);
        assert_eq!(
            texts(slot.as_ref().unwrap()),
            texts(fresh.as_ref().unwrap())
        );
        assert!(
            !texts(slot.as_ref().unwrap()).contains(&"line 0".to_string()),
            "nothing from before the clear"
        );
    }

    /// Once history is full, rows leave its top as rows arrive at its bottom. A
    /// core that counts evictions keeps following along; one that does not
    /// starts over, because its history size no longer moves.
    #[test]
    fn a_full_history_is_followed_only_by_a_core_that_counts_evictions() {
        let mut t = term(20, 5);
        lines(&mut t, 0, vt::SCROLLBACK + 50);
        let mut slot = None;
        ReadHistory::refresh(&mut slot, &t, &(), read);
        lines(&mut t, vt::SCROLLBACK + 50, vt::SCROLLBACK + 60);
        let next = ReadHistory::refresh(&mut slot, &t, &(), read);
        assert_eq!(
            next.rebuilt,
            t.lines_evicted().is_none(),
            "rio-vt counts evictions and follows; alacritty cannot, and says so"
        );
        let mut fresh = None;
        ReadHistory::refresh(&mut fresh, &t, &(), read);
        assert_eq!(
            texts(slot.as_ref().unwrap()),
            texts(fresh.as_ref().unwrap())
        );
    }
}
