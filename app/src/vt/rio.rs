//! The core: rio-vt 0.5.28, the emulator inside the Rio terminal, behind TD's
//! boundary.
//!
//! rio-vt began as a fork of alacritty's `Term`, so most of the translation
//! below is renaming: `Pos{row, col}` for `Point{line, column}`, `AnsiColor`
//! for `Color`, `Mode` for `TermMode` (the same bits, plus a few of rio's own,
//! which are masked off). The one real difference in shape is the cell. rio-vt
//! packs a cell into a `u64` and keeps its colours and attributes in a side
//! table, so a cell is assembled here, by value, from the packed word, the
//! style table and the grid's store of combining marks.
//!
//! Four of rio-vt's defaults are changed, each for a reason stated where it is
//! done: grapheme clustering is off, the `pty` feature is off, the `graphics`
//! feature is on, and the grid is always told the real pixel size of a cell.

use std::sync::Arc;
use std::time::Instant;

use rio_vt::ansi::CursorShape as RShape;
use rio_vt::clipboard::ClipboardType as RClipboard;
use rio_vt::config::colors::{AnsiColor, ColorRgb};
use rio_vt::crosswords::grid::{Dimensions, Scroll as RScroll};
use rio_vt::crosswords::pos::{Column as RColumn, Direction, Line as RLine, Pos};
use rio_vt::crosswords::square::Wide;
use rio_vt::crosswords::style::StyleFlags;
use rio_vt::crosswords::{Crosswords, CrosswordsSize};
use rio_vt::event::{EventListener, RioEvent, WindowId};
use rio_vt::performer::handler::Processor;
use rio_vt::selection::{Selection, SelectionType as RType};

#[cfg(test)]
use super::Hyperlink;
use super::{
    Backend, Cell, ClipboardType, Color, Column, Cursor, CursorShape, Event, Flags, Line, Listener,
    NamedColor, Point, Rgb, Scroll, SelectionRange, SelectionType, Side, TermMode, TermSize,
    WindowSize,
};

/// The modes TD reads from rio-vt: bits 0 to 17, which rio numbers exactly as
/// alacritty did.
///
/// Masked off: the five kitty keyboard bits (18 to 22), because TD encodes
/// keys itself and does not speak that protocol — alacritty's core had it
/// switched off and never set them, and these do the same — and everything
/// from bit 23 up, where rio keeps modes TD's numbering does not have (X10
/// mouse reporting, grapheme clustering, three sixel modes). `TermMode::ANY`
/// means the mask has to be explicit: `from_bits_truncate` keeps every bit
/// when one flag is all of them.
const READ_MODES: u32 = (1 << 18) - 1;

/// Primary device attributes, as Terminal Delight answers them: a VT220 with
/// ANSI colour.
///
/// rio-vt's own answer adds sixel and OSC 52 clipboard access, and TD draws no
/// sixel and does not act on OSC 52, so a program believing it would send
/// pictures and clipboard writes into nothing. The answer still has more than
/// three characters of parameters, which is what `kitten icat` looks for;
/// alacritty's `?6c` did not, and icat waited ten seconds in every pane
/// (issue 750).
const DEVICE_ATTRIBUTES: &str = "\x1b[?62;22c";

/// What TD says a program asked, in place of what rio-vt would say about Rio.
///
/// `None` drops the answer. Matched on the answers' shapes rather than their
/// exact text, and each is pinned by a test below, so a rio-vt that changes
/// its wording is caught rather than obeyed.
fn as_terminal_delight(answer: String) -> Option<String> {
    // Primary device attributes: CSI ? … c.
    if answer.starts_with("\x1b[?") && answer.ends_with('c') {
        return Some(DEVICE_ATTRIBUTES.to_string());
    }
    // XTVERSION: DCS > | name ST. A program that read "Rio" here would pick
    // the picture protocol Rio prefers, which TD does not draw.
    if answer.starts_with("\x1bP>|") {
        return Some(format!(
            "\x1bP>|terminal-delight {}\x1b\\",
            env!("CARGO_PKG_VERSION")
        ));
    }
    // The kitty keyboard protocol's `CSI ? flags u`. Answering it tells a
    // program TD will encode keys that way, and TD will not.
    let body = answer
        .strip_prefix("\x1b[?")
        .and_then(|rest| rest.strip_suffix('u'));
    if body.is_some_and(|flags| !flags.is_empty() && flags.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    Some(answer)
}

/// rio-vt's size, with the real pixels of a cell. A cell of 0×0 is what
/// rio-vt's own constructor gives, and it silently drops every picture a
/// program sends, having already told the program the picture was received.
fn dimensions(size: TermSize) -> CrosswordsSize {
    let (cell_w, cell_h) = (size.cell_width as u32, size.cell_height as u32);
    CrosswordsSize::new_with_dimensions(
        size.columns,
        size.screen_lines,
        size.columns as u32 * cell_w,
        size.screen_lines as u32 * cell_h,
        cell_w,
        cell_h,
    )
}

/// Carries rio-vt's events across into TD's, and no further than TD reads.
///
/// rio-vt calls this on the parsing thread with the terminal's lock held; it
/// only translates and forwards, as `Listener` requires.
struct Relay(Arc<dyn Listener>);

impl EventListener for Relay {
    fn send_event(&self, event: RioEvent, _window: WindowId) {
        let event = match event {
            RioEvent::PtyWrite(_, text) => match as_terminal_delight(text) {
                Some(text) => Event::PtyWrite(text),
                None => return,
            },
            // rio-vt hands its formatter the text area in total pixels; TD's
            // callers answer from the pseudoterminal's cell size, and the
            // conversion is theirs to trust, so it happens here once.
            RioEvent::TextAreaSizeRequest(_, format) => {
                Event::TextAreaSizeRequest(Arc::new(move |size: WindowSize| {
                    format(rio_vt::event::WindowSize {
                        rows: size.num_lines,
                        cols: size.num_cols,
                        width: size.num_cols.saturating_mul(size.cell_width),
                        height: size.num_lines.saturating_mul(size.cell_height),
                    })
                }))
            }
            RioEvent::ColorRequest(_, index, format) => Event::ColorRequest(
                index,
                Arc::new(move |c: Rgb| {
                    format(ColorRgb {
                        r: c.r,
                        g: c.g,
                        b: c.b,
                    })
                }),
            ),
            RioEvent::Title(_, title) => Event::Title(title),
            RioEvent::ResetTitle => Event::ResetTitle,
            RioEvent::Bell(_) => Event::Bell,
            RioEvent::ClipboardStore(ty, text) => Event::ClipboardStore(clipboard(ty), text),
            RioEvent::CursorBlinkingChange | RioEvent::CursorBlinkingChangeOnRoute(_) => {
                Event::CursorBlinkingChange
            }
            RioEvent::MouseCursorDirty => Event::MouseCursorDirty,
            // Render requests, damage, graphics queues, desktop
            // notifications, progress, the glyph protocol, and a clipboard
            // read (which TD refuses): nothing TD reads. Dropped here so they
            // cannot move a pane's content generation.
            _ => return,
        };
        self.0.send_event(event);
    }
}

fn clipboard(ty: RClipboard) -> ClipboardType {
    match ty {
        RClipboard::Selection => ClipboardType::Selection,
        _ => ClipboardType::Clipboard,
    }
}

fn color(c: AnsiColor) -> Color {
    match c {
        AnsiColor::Named(n) => {
            Color::Named(NamedColor::from_index(n as usize).unwrap_or(NamedColor::Foreground))
        }
        AnsiColor::Spec(rgb) => Color::Spec(Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }),
        AnsiColor::Indexed(i) => Color::Indexed(i),
    }
}

fn pos(p: Point) -> Pos {
    Pos::new(RLine(p.line.0), RColumn(p.column.0))
}

fn point(p: Pos) -> Point {
    Point::new(Line(p.row.0), Column(p.col.0))
}

fn side(side: Side) -> Direction {
    match side {
        Side::Left => Direction::Left,
        Side::Right => Direction::Right,
    }
}

/// A cell's attributes in TD's flags, from rio-vt's three places for them: the
/// style (SGR), the wide-character state, and the soft-wrap mark.
fn flags(style: StyleFlags, wide: Wide, wrapline: bool) -> Flags {
    const PAIRS: [(StyleFlags, Flags); 11] = [
        (StyleFlags::INVERSE, Flags::INVERSE),
        (StyleFlags::BOLD, Flags::BOLD),
        (StyleFlags::ITALIC, Flags::ITALIC),
        (StyleFlags::DIM, Flags::DIM),
        (StyleFlags::HIDDEN, Flags::HIDDEN),
        (StyleFlags::STRIKEOUT, Flags::STRIKEOUT),
        (StyleFlags::UNDERLINE, Flags::UNDERLINE),
        (StyleFlags::DOUBLE_UNDERLINE, Flags::DOUBLE_UNDERLINE),
        (StyleFlags::UNDERCURL, Flags::UNDERCURL),
        (StyleFlags::DOTTED_UNDERLINE, Flags::DOTTED_UNDERLINE),
        (StyleFlags::DASHED_UNDERLINE, Flags::DASHED_UNDERLINE),
    ];
    let mut out = Flags::empty();
    for (from, to) in PAIRS {
        if style.contains(from) {
            out |= to;
        }
    }
    out |= match wide {
        Wide::Narrow => Flags::empty(),
        Wide::Wide => Flags::WIDE_CHAR,
        Wide::Spacer => Flags::WIDE_CHAR_SPACER,
        Wide::LeadingSpacer => Flags::LEADING_WIDE_CHAR_SPACER,
    };
    if wrapline {
        out |= Flags::WRAPLINE;
    }
    out
}

pub(super) struct Core {
    term: Crosswords<Relay>,
    parser: Processor,
}

impl Backend for Core {
    const NAME: &'static str = "rio-vt 0.5.28";

    fn new(size: TermSize, listener: Arc<dyn Listener>, scrollback: usize) -> Self {
        let mut term = Crosswords::new(
            dimensions(size),
            RShape::Block,
            Relay(listener),
            WindowId::from(0),
            0,
            scrollback,
        );
        // Off, where rio-vt defaults it on. With DEC 2027 on, rio-vt sizes a
        // cell by grapheme cluster rather than by wcwidth, which is right for
        // Rio's renderer and wrong for everything TD shares a grid with: the
        // programs that lay out their own screens by wcwidth, the session host
        // and window that must agree cell for cell, and the tests. A program
        // can still turn it on with DECSET 2027.
        term.set_grapheme_clustering(false);
        Self {
            term,
            parser: Processor::default(),
        }
    }

    fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    fn sync_bytes_count(&self) -> usize {
        self.parser.sync_bytes_count()
    }

    fn sync_deadline(&self) -> Option<Instant> {
        self.parser.sync_timeout().sync_timeout()
    }

    fn flush_sync(&mut self) {
        self.parser.stop_sync(&mut self.term);
    }

    fn resize(&mut self, size: TermSize) {
        self.term.resize(dimensions(size));
    }

    fn columns(&self) -> usize {
        self.term.columns()
    }

    fn screen_lines(&self) -> usize {
        self.term.screen_lines()
    }

    fn history_size(&self) -> usize {
        self.term.history_size()
    }

    fn display_offset(&self) -> usize {
        self.term.display_offset()
    }

    fn row_into(&self, line: Line, out: &mut Vec<Cell>) {
        let grid = &self.term.grid;
        let row = &grid[RLine(line.0)];
        // A history row keeps the width it was written at when the terminal
        // later grows wider; `Term` pads what is missing with blanks.
        let width = row.len().min(grid.columns());
        for column in 0..width {
            let square = row[RColumn(column)];
            let style = grid.style_of(&square);
            // An erased or never-written cell holds NUL. TD's blank is a space,
            // as alacritty's was, and word selection, trimming and the
            // snapshot all read it that way.
            let c = match square.c() {
                '\0' => ' ',
                c => c,
            };
            let mut cell = Cell::new(
                c,
                color(style.fg),
                color(style.bg),
                flags(style.flags, square.wide(), square.wrapline()),
            );
            let underline = style.underline_color.map(color);
            if square.has_grapheme() || underline.is_some() {
                let marks: Vec<char> = if square.has_grapheme() {
                    grid.cell_text(Pos::new(RLine(line.0), RColumn(column)))
                        .skip(1)
                        .collect()
                } else {
                    Vec::new()
                };
                cell = cell.with_extra(&marks, underline);
            }
            out.push(cell);
        }
    }

    fn cursor(&self) -> Cursor {
        let shape = match self.term.cursor_shape {
            RShape::Block => CursorShape::Block,
            RShape::Underline => CursorShape::Underline,
            RShape::Beam => CursorShape::Beam,
            RShape::Hidden => CursorShape::Hidden,
        };
        Cursor {
            point: point(self.term.grid.cursor.pos),
            shape,
        }
    }

    fn mode(&self) -> TermMode {
        TermMode::from_bits_retain(self.term.mode().bits() & READ_MODES)
    }

    fn scroll_display(&mut self, scroll: Scroll) {
        self.term.scroll_display(match scroll {
            Scroll::Delta(lines) => RScroll::Delta(lines),
            Scroll::PageUp => RScroll::PageUp,
            Scroll::PageDown => RScroll::PageDown,
            Scroll::Top => RScroll::Top,
            Scroll::Bottom => RScroll::Bottom,
        });
    }

    fn clear_history(&mut self) {
        self.term.clear_saved_history();
    }

    fn start_selection(&mut self, ty: SelectionType, point: Point, direction: Side) {
        let ty = match ty {
            SelectionType::Simple => RType::Simple,
            SelectionType::Semantic => RType::Semantic,
            SelectionType::Lines => RType::Lines,
        };
        self.term.selection = Some(Selection::new(ty, pos(point), side(direction)));
    }

    fn update_selection(&mut self, point: Point, direction: Side) {
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(pos(point), side(direction));
        }
    }

    fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    fn selection_range(&self) -> Option<SelectionRange> {
        let range = self.term.selection.as_ref()?.to_range(&self.term)?;
        Some(SelectionRange::new(
            point(range.start),
            point(range.end),
            range.is_block,
        ))
    }

    fn selection_to_string(&self) -> Option<String> {
        self.term.selection_to_string()
    }

    fn semantic_search_left(&self, at: Point) -> Point {
        point(self.term.semantic_search_left(pos(at)))
    }

    fn semantic_search_right(&self, at: Point) -> Point {
        point(self.term.semantic_search_right(pos(at)))
    }

    #[cfg(test)]
    fn hyperlink(&self, at: Point) -> Option<Hyperlink> {
        let link = self
            .term
            .cell_hyperlink(RLine(at.line.0), RColumn(at.column.0))?;
        Some(Hyperlink::new(link.id(), link.uri()))
    }
}

#[cfg(test)]
mod tests {
    //! What rio-vt does that alacritty did not, and what TD makes of it. The
    //! shared behaviour is pinned by the correctness matrix in `term.rs`, which
    //! runs on both cores; these are the places the cores part.
    use std::sync::{Arc, Mutex};

    use crate::vt::{Event, Listener, Term, TermMode, TermSize};

    #[derive(Default)]
    struct Heard(Mutex<Vec<String>>);
    impl Listener for Heard {
        fn send_event(&self, event: Event) {
            if let Event::PtyWrite(text) = event {
                self.0.lock().unwrap().push(text);
            }
        }
    }

    fn replies_to(question: &[u8]) -> (Vec<String>, TermMode) {
        let heard = Arc::new(Heard::default());
        let mut term = Term::new(TermSize::new(80, 24, 8, 16), heard.clone());
        term.advance(question);
        let replies = heard.0.lock().unwrap().clone();
        (replies, term.mode())
    }

    #[test]
    fn device_attributes_describe_terminal_delight_and_satisfy_icat() {
        let (replies, _) = replies_to(b"\x1b[c");
        assert_eq!(replies, vec!["\x1b[?62;22c".to_string()]);
        // icat's detector wants more than three characters of parameters.
        let params = replies[0]
            .trim_start_matches("\x1b[?")
            .trim_end_matches('c');
        assert!(params.len() > 3, "{params:?}");
    }

    #[test]
    fn xtversion_names_terminal_delight_not_rio() {
        let (replies, _) = replies_to(b"\x1b[>0q");
        assert_eq!(replies.len(), 1, "{replies:?}");
        assert!(
            replies[0].starts_with("\x1bP>|terminal-delight "),
            "{:?}",
            replies[0]
        );
        assert!(!replies[0].contains("Rio"), "{:?}", replies[0]);
    }

    #[test]
    fn the_kitty_keyboard_protocol_is_neither_offered_nor_taken() {
        // Asked: no answer, so a program keeps to legacy keys.
        let (replies, _) = replies_to(b"\x1b[?u");
        assert!(replies.is_empty(), "{replies:?}");
        // Pushed regardless: TD's modes do not move, because TD's key encoder
        // is what would have to honour them.
        let (_, mode) = replies_to(b"\x1b[>1u");
        assert!(
            !mode.intersects(TermMode::KITTY_KEYBOARD_PROTOCOL),
            "{mode:?}"
        );
    }

    #[test]
    fn a_cell_size_question_is_answered_from_the_cell_the_core_was_given() {
        let (replies, _) = replies_to(b"\x1b[16t");
        assert_eq!(replies, vec!["\x1b[6;16;8t".to_string()]);
    }

    #[test]
    fn a_kitty_picture_is_received_and_acknowledged() {
        // One red pixel, RGB, sent directly. rio-vt answers the way a
        // terminal that keeps pictures answers; alacritty said nothing, and
        // the program's text overwrote the space the picture should have had.
        let (replies, _) = replies_to(b"\x1b_Gi=31,s=1,v=1,a=T,t=d,f=24;/wAA\x1b\\");
        assert_eq!(replies, vec!["\x1b_Gi=31;OK\x1b\\".to_string()]);
    }
}
