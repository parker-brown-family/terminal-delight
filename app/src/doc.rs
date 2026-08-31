//! The reader's document model — the seam between *what the text is* and *how a
//! surface lays it out*.
//!
//! # Why this module exists
//!
//! The FOCUS reader used to render a **rendering**. The pane chops logical text
//! into fixed-width grid rows, pads every cell, and forgets which breaks were real
//! — then the reader tried to reverse-engineer the original out of that grid. Every
//! recovery step was a guess (was this break width-induced? is this row a code
//! fence? is that indent structure or a margin?), each new guess existed to patch
//! the previous one's misfire, and content that never reached the screen could not
//! be shown at all.
//!
//! The rule this module enforces is: **render from the source, not from another
//! rendering.** The pane and the reader are two *views* of one [`Document`], and
//! neither is derived from the other's layout decisions.
//!
//! # The three pieces
//!
//! - [`Document`] — WHAT the text is. Logical lines, real breaks only. No columns,
//!   no rows, no wrapping. A surface-independent snapshot.
//! - [`DocumentSource`] — WHERE it comes from. Implemented for a terminal pane
//!   today (viewport plus scrollback); an agent's own transcript is the natural
//!   second implementation, and needs none of the recovery below.
//! - [`layout`] — HOW it sits on one surface, at that surface's width. Pure, and
//!   it reports the document position behind every visual row so selection and copy
//!   keep working.
//!
//! # The quarantine
//!
//! [`Document::from_grid_rows`] is the ONLY place that guesses. When text arrives
//! already chopped — because a TUI hard-wrapped its own output before it ever
//! reached the terminal — the original breaks are genuinely gone and no
//! architecture recovers them. Recovery is therefore best-effort *by nature*, and
//! it is confined here rather than spread through the reader. A source that knows
//! its own structure bypasses it entirely.

use gpui::TextRun;

/// One logical line: real line breaks only, never a width chop. `runs` covers
/// `text` byte for byte.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocLine {
    pub text: String,
    pub runs: Vec<TextRun>,
}

impl DocLine {
    pub fn new(text: String, runs: Vec<TextRun>) -> Self {
        Self { text, runs }
    }
}

/// An ordered run of logical lines — the reader's coordinate space. A position is
/// `(line index, char offset)`, which is stable regardless of the width any
/// surface happens to lay it out at.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Document {
    pub lines: Vec<DocLine>,
}

/// How much of a source's backlog a surface wants. A reader shrinking its glyphs
/// needs proportionally more lines to stay full, so it asks for what it can show
/// rather than accepting whatever one screenful happens to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowBudget {
    /// Maximum logical lines to return, counting back from the newest.
    pub lines: usize,
}

impl RowBudget {
    pub fn of(lines: usize) -> Self {
        Self { lines }
    }
}

/// Anything that can produce a [`Document`]. The reader talks to this, never to a
/// grid — which is what stops pane-width quirks becoming reader bugs.
pub trait DocumentSource {
    fn document(&self, budget: RowBudget) -> Document;
}

impl Document {
    /// Recover logical lines from grid rows — **the one place that guesses.**
    ///
    /// `src_cols` is the width the rows were chopped at. Rows arrive space-padded
    /// (a grid row is always full width) so padding is trimmed first: leave it on
    /// and every row reads as full to the width test below, gluing the whole screen
    /// into one line.
    ///
    /// Then the COMMON left indent is stripped before structure is judged.
    /// [`wrap_join`] refuses to merge an indented row, because an indented row is
    /// normally a code block or a list continuation — but a TUI agent indents its
    /// *entire* transcript, so every row trips that guard and nothing joins. A
    /// margin shared by every row is not structure, it is a margin. Removing it
    /// restores the guard to what it is for: rows indented *relative to their
    /// neighbours*. A block with no shared margin dedents by zero and is untouched.
    pub fn from_grid_rows(rows: &[DocLine], src_cols: usize) -> Document {
        let trimmed: Vec<DocLine> = rows
            .iter()
            .map(|l| {
                let keep = l.text.trim_end_matches(' ').len();
                DocLine::new(l.text[..keep].to_string(), slice_runs(&l.runs, 0, keep))
            })
            .collect();
        let indent = trimmed
            .iter()
            .filter(|l| !l.text.trim().is_empty())
            .map(|l| l.text.len() - l.text.trim_start_matches(' ').len())
            .min()
            .unwrap_or(0);
        let trimmed: Vec<DocLine> = if indent == 0 {
            trimmed
        } else {
            trimmed
                .into_iter()
                .map(|l| {
                    let cut = indent.min(l.text.len());
                    let end = l.text.len();
                    DocLine::new(l.text[cut..].to_string(), slice_runs(&l.runs, cut, end))
                })
                .collect()
        };
        // The width test must judge against the width text actually had once that
        // margin is gone, or a dedented full row reads as short of the edge.
        let src_cols = src_cols.saturating_sub(indent).max(1);

        let mut lines: Vec<DocLine> = Vec::with_capacity(trimmed.len());
        // The logical line under construction, plus the char-width of the last raw
        // row appended — the row the width test is applied against.
        let mut cur: Option<(DocLine, usize)> = None;
        for row in &trimmed {
            let row_len = row.text.chars().count();
            if row.text.trim().is_empty() {
                if let Some((line, _)) = cur.take() {
                    lines.push(line);
                }
                lines.push(row.clone());
                continue;
            }
            match cur.take() {
                None => cur = Some((row.clone(), row_len)),
                Some((mut acc, prev_len)) => {
                    match wrap_join(&acc.text, prev_len, &row.text, src_cols) {
                        WrapJoin::Break => {
                            lines.push(acc);
                            cur = Some((row.clone(), row_len));
                        }
                        join => {
                            if matches!(join, WrapJoin::Space) {
                                acc.text.push(' ');
                                // the inserted space wears the style it follows
                                match acc.runs.last_mut() {
                                    Some(last) => last.len += 1,
                                    None => acc.runs.extend(slice_runs(&row.runs, 0, 1)),
                                }
                            }
                            acc.text.push_str(&row.text);
                            acc.runs.extend(row.runs.iter().cloned());
                            cur = Some((acc, row_len));
                        }
                    }
                }
            }
        }
        if let Some((line, _)) = cur.take() {
            lines.push(line);
        }
        Document { lines }
    }
}

/// One laid-out visual row, plus where it came from in the [`Document`].
/// `doc_line` indexes `Document::lines`; `doc_col0` is the char offset within that
/// logical line. Selection and copy work in those coordinates, so they never see a
/// grid cell.
#[derive(Debug, Clone)]
pub struct VisualRow {
    pub text: String,
    pub runs: Vec<TextRun>,
    pub doc_line: usize,
    pub doc_col0: usize,
    /// Glyph count painted on this visual row (for hit-clamping a click).
    pub cols: usize,
}

/// Lay a [`Document`] out at `fit_cols` glyphs per line. Pure and width-driven:
/// the same document at a narrower glyph yields more text per row, which is the
/// whole point — a smaller font must show MORE, not merely smaller.
pub fn layout(doc: &Document, fit_cols: usize) -> Vec<VisualRow> {
    let mut out = Vec::new();
    for (i, line) in doc.lines.iter().enumerate() {
        wrap_doc_line(i, &line.text, &line.runs, fit_cols, &mut out);
    }
    out
}

/// Break one logical line into visual rows at `fit_cols`, preferring a word
/// boundary and falling back to a hard cap so an over-long token still cannot
/// spill off the edge.
fn wrap_doc_line(
    doc_line: usize,
    text: &str,
    runs: &[TextRun],
    fit_cols: usize,
    out: &mut Vec<VisualRow>,
) {
    let fit_cols = fit_cols.max(1);
    let keep = text.trim_end_matches(' ').len();
    let chars: Vec<(usize, char)> = text[..keep].char_indices().collect();
    let n = chars.len();
    if n == 0 {
        out.push(VisualRow {
            text: String::new(),
            runs: Vec::new(),
            doc_line,
            doc_col0: 0,
            cols: 0,
        });
        return;
    }
    let mut i = 0usize;
    while i < n {
        let mut end = (i + fit_cols).min(n);
        if end < n {
            if let Some(sp) = (i + 1..=end).rev().find(|&k| chars[k].1 == ' ') {
                end = sp;
            }
        }
        let byte_start = chars[i].0;
        let byte_end = if end < n { chars[end].0 } else { keep };
        out.push(VisualRow {
            text: text[byte_start..byte_end].to_string(),
            runs: slice_runs(runs, byte_start, byte_end),
            doc_line,
            doc_col0: i,
            cols: end - i,
        });
        i = end;
        while i < n && chars[i].1 == ' ' {
            i += 1; // swallow the break space(s) so the next row's head is a glyph
        }
    }
}

/// Slice the styled runs covering bytes `[start, end)`, clamping the two boundary
/// runs so styling follows the text it belongs to.
pub fn slice_runs(runs: &[TextRun], start: usize, end: usize) -> Vec<TextRun> {
    let mut out = Vec::new();
    let mut acc = 0usize;
    for r in runs {
        let (r0, r1) = (acc, acc + r.len);
        acc = r1;
        if r1 <= start {
            continue;
        }
        if r0 >= end {
            break;
        }
        let (s, e) = (r0.max(start), r1.min(end));
        if e > s {
            let mut nr = r.clone();
            nr.len = e - s;
            out.push(nr);
        }
    }
    out
}

/// True when a row begins with whitespace — an indented code/list continuation
/// that must never be merged into the row above.
pub fn starts_indented(s: &str) -> bool {
    matches!(s.chars().next(), Some(' ') | Some('\t'))
}

/// True when a row is *structure*, not flowing text: a code fence, or a rule /
/// box-drawing separator (≥60% of its ink is `─│┌┐…`, `-`, or `=`). Such rows
/// bound a paragraph and are never rejoined.
pub fn is_structural(s: &str) -> bool {
    let t = s.trim();
    let n = t.chars().count();
    if n < 3 {
        return false;
    }
    if t.starts_with("```") {
        return true;
    }
    let rule = t
        .chars()
        .filter(|c| matches!(c, '\u{2500}'..='\u{257F}' | '-' | '=' | '·' | '•'))
        .count();
    rule * 5 >= n * 3
}

/// How a width-wrapped row rejoins the logical line above it.
pub enum WrapJoin {
    /// Mid-token wrap (previous row filled to `cols`) — concatenate, no space.
    Glue,
    /// Word-boundary wrap at the pane width — concatenate with one space.
    Space,
    /// A genuine line break — keep it.
    Break,
}

/// Decide how `raw` attaches to the accumulated logical line `acc` whose last raw
/// row had char-width `prev_len`. A join happens only when that row was at least
/// half-full, neither side is indented or structural, and the first word of `raw`
/// could not have fit after it (the width test).
pub fn wrap_join(acc: &str, prev_len: usize, raw: &str, cols: usize) -> WrapJoin {
    let joinable = prev_len * 2 >= cols
        && !starts_indented(raw)
        && !starts_indented(acc)
        && !is_structural(raw)
        && !is_structural(acc);
    if !joinable {
        return WrapJoin::Break;
    }
    if prev_len >= cols {
        return WrapJoin::Glue;
    }
    let first_word = raw
        .split_whitespace()
        .next()
        .map_or(0, |w| w.chars().count());
    if prev_len + 1 + first_word > cols {
        WrapJoin::Space
    } else {
        WrapJoin::Break
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Font, FontFeatures, FontStyle, FontWeight, Hsla};

    fn font(family: &str) -> Font {
        Font {
            family: family.into(),
            features: FontFeatures::default(),
            weight: FontWeight::default(),
            style: FontStyle::Normal,
            fallbacks: None,
        }
    }

    fn run(len: usize) -> TextRun {
        TextRun {
            len,
            font: font("monospace"),
            color: Hsla {
                h: 0.,
                s: 0.,
                l: 0.,
                a: 1.,
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    fn line(s: &str) -> DocLine {
        DocLine::new(s.to_string(), vec![run(s.len())])
    }

    /// The command that failed to paste four times on 2026-08-31: two grid rows,
    /// one logical line.
    #[test]
    fn recovery_heals_a_width_break_and_accounts_for_every_byte() {
        let rows = [
            line("cd ~/.claude && jq --slurpfile e /tmp/x/automode-env.json"),
            line("'.autoMode.environment = $e[0]' settings.json > s.tmp"),
        ];
        let doc = Document::from_grid_rows(&rows, 60);
        assert_eq!(doc.lines.len(), 1, "the width-break is healed");
        assert!(
            doc.lines[0]
                .text
                .contains("automode-env.json '.autoMode.environment"),
            "got {:?}",
            doc.lines[0].text
        );
        let bytes: usize = doc.lines[0].runs.iter().map(|r| r.len).sum();
        assert_eq!(bytes, doc.lines[0].text.len(), "runs cover every byte");
    }

    /// A TUI agent indents its whole transcript; that shared margin must not veto
    /// every join. But a row indented RELATIVE to its neighbours is still structure.
    #[test]
    fn recovery_dedents_a_shared_margin_but_keeps_relative_indent() {
        let shared = [
            line("  Target: https://github.com/parker-brown-fami"),
            line("  ly/terminal-delight/pull/212"),
        ];
        let doc = Document::from_grid_rows(&shared, 48);
        assert_eq!(
            doc.lines.len(),
            1,
            "a shared margin must not block the join"
        );
        assert!(
            !doc.lines[0].text.starts_with(' '),
            "the margin is stripped"
        );

        let relative = [
            line("  this line runs right up to edge"),
            line("      let x = code_block();"),
        ];
        assert_eq!(Document::from_grid_rows(&relative, 32).lines.len(), 2);
    }

    /// Structure bounds a paragraph: fences, box-drawing rules, blank rows.
    #[test]
    fn recovery_leaves_structure_alone() {
        let ruled = [
            line("a full width heading line here"),
            line("──────────────────────────────"),
            line("body paragraph text follows on"),
        ];
        assert_eq!(Document::from_grid_rows(&ruled, 30).lines.len(), 3);

        let blank = [line("git status"), line(""), line("git log")];
        assert_eq!(Document::from_grid_rows(&blank, 80).lines.len(), 3);
    }

    /// Grid rows arrive space-padded. Untrimmed, every row reads as full-width to
    /// the width test and the whole screen glues into one line.
    #[test]
    fn recovery_trims_grid_padding_first() {
        let cols = 20;
        let padded = [
            DocLine::new(format!("{:cols$}", "git status"), vec![run(cols)]),
            DocLine::new(format!("{:cols$}", "git log"), vec![run(cols)]),
        ];
        let doc = Document::from_grid_rows(&padded, cols);
        assert_eq!(doc.lines.len(), 2, "padded rows must not glue together");
        assert_eq!(doc.lines[0].text, "git status");
        let bytes: usize = doc.lines[0].runs.iter().map(|r| r.len).sum();
        assert_eq!(bytes, doc.lines[0].text.len());
    }

    /// THE POINT OF THE WHOLE MODULE: the same document laid out narrower fits
    /// MORE text per row, because layout runs on logical lines rather than on
    /// somebody else's chopped rows.
    #[test]
    fn a_narrower_glyph_shows_more_text_per_row_not_merely_smaller_text() {
        let doc = Document {
            lines: vec![line(
                "the quick brown fox jumps over the lazy dog and keeps running well past the edge",
            )],
        };
        let wide = layout(&doc, 80);
        let narrow = layout(&doc, 20);
        assert_eq!(wide.len(), 1, "80 cols holds it in one row");
        assert!(narrow.len() > wide.len(), "20 cols needs more rows");
        // and every visual row still knows where it came from
        assert!(narrow.iter().all(|r| r.doc_line == 0));
        assert_eq!(narrow[0].doc_col0, 0);
        assert!(narrow[1].doc_col0 > 0, "the second row starts further in");
    }

    #[test]
    fn layout_breaks_at_words_and_trims_trailing_blanks() {
        // trailing grid blanks are trimmed; a line that fits stays one row
        let doc = Document {
            lines: vec![line("abcdef     ")],
        };
        let rows = layout(&doc, 10);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "abcdef");
        assert_eq!(
            (rows[0].doc_line, rows[0].doc_col0, rows[0].cols),
            (0, 0, 6)
        );

        // word-boundary wrap: the break space is swallowed, not carried over
        let doc = Document {
            lines: vec![line("ab cd ef")],
        };
        let rows = layout(&doc, 4);
        let got: Vec<(&str, usize)> = rows.iter().map(|r| (r.text.as_str(), r.doc_col0)).collect();
        assert_eq!(got, vec![("ab", 0), ("cd", 3), ("ef", 6)]);
    }

    #[test]
    fn layout_hard_breaks_long_tokens_and_never_overflows() {
        // an unbreakable token longer than the width splits at the cap, so a row
        // can NEVER exceed fit_cols — the reader never scrolls sideways
        let fit = 5usize;
        let doc = Document {
            lines: vec![line("0123456789abc")],
        };
        let rows = layout(&doc, fit);
        assert_eq!(
            rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(),
            vec!["01234", "56789", "abc"]
        );
        assert!(
            rows.iter().all(|r| r.cols <= fit),
            "no row exceeds fit_cols"
        );
        // document columns stay contiguous, so a click on any row maps back right
        assert_eq!(
            rows.iter().map(|r| r.doc_col0).collect::<Vec<_>>(),
            vec![0, 5, 10]
        );

        // a blank line survives as one empty visual row (paragraph spacing)
        let doc = Document {
            lines: vec![line("   ")],
        };
        let rows = layout(&doc, 8);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cols, 0);
    }
}
