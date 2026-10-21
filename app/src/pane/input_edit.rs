//! Translate a visible input range into the same caret motion and DELs as typing.
use crate::vt::{Cell, Flags, Line, Point, Term};

fn wraps(term: &Term, line: Line) -> bool {
    term.row(line)
        .iter()
        .any(|c| c.flags.contains(Flags::WRAPLINE))
}

/// Soft wraps are one editable line even when its beginning is above the viewport.
fn logical_line(term: &Term, caret: Line) -> (Line, Line) {
    let (mut first, mut last) = (caret, caret);
    while first > term.topmost_line() && wraps(term, Line(first.0 - 1)) {
        first.0 -= 1;
    }
    while last < term.bottommost_line() && wraps(term, last) {
        last.0 += 1;
    }
    (first, last)
}

struct Rows {
    cells: Vec<Cell>,
    spans: Vec<(Line, usize, usize)>,
}

impl Rows {
    fn read(term: &Term, first: Line, last: Line) -> Option<Self> {
        if first < term.topmost_line() || last > term.bottommost_line() || first > last {
            return None;
        }
        let caret = term.cursor().point;
        let mut out = Self {
            cells: Vec::new(),
            spans: Vec::new(),
        };
        for row in first.0..=last.0 {
            let line = Line(row);
            let cells = term.row(line);
            let wrapped = cells.iter().any(|c| c.flags.contains(Flags::WRAPLINE));
            let len = if wrapped {
                cells.len()
            } else {
                let printed = cells
                    .iter()
                    .rposition(|c| !matches!(c.c, ' ' | '\0'))
                    .map_or(0, |i| i + 1);
                printed.max(if line == caret.line {
                    caret.column.0
                } else {
                    0
                })
            };
            let start = out.cells.len();
            out.cells.extend_from_slice(&cells.0[..len]);
            out.spans.push((line, start, out.cells.len()));
            if !wrapped && line < last {
                // An actual newline is one caret step, unlike a soft wrap.
                let mut newline = Cell::default();
                newline.c = '\n';
                out.cells.push(newline);
            }
        }
        Some(out)
    }

    fn at(&self, point: Point) -> Option<usize> {
        self.spans
            .iter()
            .find(|(line, _, _)| *line == point.line)
            .map(|(_, start, end)| start + point.column.0.min(end - start))
    }
}

pub(super) fn selection_keys(term: &Term) -> Option<Vec<u8>> {
    let range = term.selection_range()?;
    let caret = term.cursor().point;
    if range.is_block && range.start.line != range.end.line {
        return None;
    }
    let (first, last) = logical_line(term, caret.line);
    let in_line = range.start.line >= first && range.end.line <= last;
    let contains_caret_row = range.start.line <= caret.line && range.end.line >= caret.line;
    if !in_line && !contains_caret_row {
        return None; // unrelated history is not input
    }
    let rows = Rows::read(
        term,
        range.start.line.min(caret.line),
        range.end.line.max(caret.line),
    )?;
    super::erase_run_keys(
        &rows.cells,
        rows.at(range.start)?,
        rows.at(range.end)?,
        rows.at(caret)?,
        term.mode().contains(crate::vt::TermMode::APP_CURSOR),
    )
}

pub(super) fn line_keys(term: &Term) -> Option<Vec<u8>> {
    let caret = term.cursor().point;
    let (first, last) = logical_line(term, caret.line);
    let rows = Rows::read(term, first, last)?;
    Some(
        super::erase_run_keys(
            &rows.cells,
            0,
            rows.cells.len().saturating_sub(1),
            rows.at(caret)?,
            term.mode().contains(crate::vt::TermMode::APP_CURSOR),
        )
        .unwrap_or_default(),
    )
}
