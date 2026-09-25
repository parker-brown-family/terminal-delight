//! The fallback core: alacritty_terminal 0.26 behind TD's boundary.
//!
//! Compiled only with `--features core-alacritty`. It is kept for two reasons,
//! and both have a date on them. It is the oracle: the whole test suite runs
//! on it as well as on rio-vt, and a test that passes here and fails there is
//! a difference between the cores, found. And it is a way back: if rio-vt
//! misbehaves in daily use, a build on the old core is one flag away rather
//! than one revert away. Once rio-vt has held, a follow-up deletes this file.
//!
//! Translation is nearly free, because `vt`'s numbering *is* alacritty's:
//! flags and modes cross by their bits, and named colours by their index.

use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::event::{Event as AEvent, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll as AScroll};
use alacritty_terminal::index::{Column as AColumn, Line as ALine, Point as APoint, Side as ASide};
use alacritty_terminal::selection::{Selection, SelectionType as AType};
use alacritty_terminal::term::{ClipboardType as AClipboard, Config, Term};
use alacritty_terminal::vte::ansi::{
    Color as AColor, CursorShape as AShape, Processor, Rgb as ARgb, StdSyncHandler,
};

#[cfg(test)]
use super::Hyperlink;
use super::{
    Backend, Cell, ClipboardType, Color, Column, Cursor, CursorShape, Event, Flags, Line, Listener,
    NamedColor, Picture, PictureData, PictureKey, Point, Rgb, Scroll, SelectionRange,
    SelectionType, Side, TermMode, TermSize, WindowSize,
};

/// alacritty's view of a size.
struct Size(TermSize);

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.0.screen_lines
    }
    fn screen_lines(&self) -> usize {
        self.0.screen_lines
    }
    fn columns(&self) -> usize {
        self.0.columns
    }
}

/// Carries alacritty's events across into TD's, and no further than TD reads.
struct Relay(Arc<dyn Listener>);

impl EventListener for Relay {
    fn send_event(&self, event: AEvent) {
        let event = match event {
            AEvent::MouseCursorDirty => Event::MouseCursorDirty,
            AEvent::Title(title) => Event::Title(title),
            AEvent::ResetTitle => Event::ResetTitle,
            AEvent::ClipboardStore(ty, text) => Event::ClipboardStore(clipboard(ty), text),
            AEvent::ColorRequest(index, format) => {
                Event::ColorRequest(index, Arc::new(move |c: Rgb| format(rgb_out(c))))
            }
            AEvent::PtyWrite(text) => Event::PtyWrite(text),
            AEvent::TextAreaSizeRequest(format) => {
                Event::TextAreaSizeRequest(Arc::new(move |size: WindowSize| {
                    format(alacritty_terminal::event::WindowSize {
                        num_lines: size.num_lines,
                        num_cols: size.num_cols,
                        cell_width: size.cell_width,
                        cell_height: size.cell_height,
                    })
                }))
            }
            AEvent::CursorBlinkingChange => Event::CursorBlinkingChange,
            AEvent::Bell => Event::Bell,
            // TD's loop sends these itself; alacritty's `Term` only sends
            // `Exit` from `Term::exit`, which TD never calls. An OSC 52 read is
            // never answered: a program reading the clipboard is refused.
            AEvent::Wakeup | AEvent::Exit | AEvent::ChildExit(_) | AEvent::ClipboardLoad(..) => {
                return
            }
        };
        self.0.send_event(event);
    }
}

fn clipboard(ty: AClipboard) -> ClipboardType {
    match ty {
        AClipboard::Clipboard => ClipboardType::Clipboard,
        AClipboard::Selection => ClipboardType::Selection,
    }
}

fn rgb_out(c: Rgb) -> ARgb {
    ARgb {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

fn color(c: AColor) -> Color {
    match c {
        AColor::Named(n) => {
            Color::Named(NamedColor::from_index(n as usize).unwrap_or(NamedColor::Foreground))
        }
        AColor::Spec(rgb) => Color::Spec(Rgb {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
        }),
        AColor::Indexed(i) => Color::Indexed(i),
    }
}

fn point_in(p: Point) -> APoint {
    APoint::new(ALine(p.line.0), AColumn(p.column.0))
}

fn point_out(p: APoint) -> Point {
    Point::new(Line(p.line.0), Column(p.column.0))
}

fn side_in(side: Side) -> ASide {
    match side {
        Side::Left => ASide::Left,
        Side::Right => ASide::Right,
    }
}

pub(super) struct Core {
    term: Term<Relay>,
    parser: Processor<StdSyncHandler>,
    listener: Arc<dyn Listener>,
    size: TermSize,
    /// Hears `CSI 16 t`, which vte drops before alacritty sees it, so that the
    /// core answers it the way rio-vt does: from the cell size it was given.
    cell_query: crate::ptyscan::CellSizeQuery,
}

impl Backend for Core {
    const NAME: &'static str = "alacritty_terminal 0.26";

    fn new(size: TermSize, listener: Arc<dyn Listener>, scrollback: usize) -> Self {
        let config = Config {
            scrolling_history: scrollback,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &Size(size), Relay(listener.clone())),
            parser: Processor::new(),
            listener,
            size,
            cell_query: crate::ptyscan::CellSizeQuery::default(),
        }
    }

    fn advance(&mut self, bytes: &[u8]) {
        // Heard before the parser has seen this chunk, as the host's tee did,
        // so this reply can overtake the answer to a question earlier in the
        // same read. See `ptyscan` for why that ordering is acceptable.
        for _ in 0..self.cell_query.feed(bytes) {
            self.listener.send_event(Event::PtyWrite(format!(
                "\x1b[6;{};{}t",
                self.size.cell_height, self.size.cell_width
            )));
        }
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
        self.size = size;
        self.term.resize(Size(size));
    }

    fn columns(&self) -> usize {
        self.term.grid().columns()
    }

    fn screen_lines(&self) -> usize {
        self.term.grid().screen_lines()
    }

    fn history_size(&self) -> usize {
        self.term.grid().history_size()
    }

    fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    fn row_into(&self, line: Line, out: &mut Vec<Cell>) {
        let row = &self.term.grid()[ALine(line.0)];
        for column in 0..self.term.grid().columns() {
            let cell = &row[AColumn(column)];
            let flags = Flags::from_bits_truncate(cell.flags.bits());
            let mut out_cell = Cell::new(cell.c, color(cell.fg), color(cell.bg), flags);
            let zerowidth = cell.zerowidth().unwrap_or(&[]);
            let underline = cell.underline_color().map(color);
            if !zerowidth.is_empty() || underline.is_some() {
                out_cell = out_cell.with_extra(zerowidth, underline);
            }
            out.push(out_cell);
        }
    }

    fn cursor(&self) -> Cursor {
        let shape = match self.term.cursor_style().shape {
            // A hollow block is how alacritty draws an unfocused block; TD
            // draws focus itself, so it is a block here.
            AShape::Block | AShape::HollowBlock => CursorShape::Block,
            AShape::Underline => CursorShape::Underline,
            AShape::Beam => CursorShape::Beam,
            AShape::Hidden => CursorShape::Hidden,
        };
        Cursor {
            point: point_out(self.term.grid().cursor.point),
            shape,
        }
    }

    fn mode(&self) -> TermMode {
        TermMode::from_bits_truncate(self.term.mode().bits())
    }

    fn scroll_display(&mut self, scroll: Scroll) {
        self.term.scroll_display(match scroll {
            Scroll::Delta(lines) => AScroll::Delta(lines),
            Scroll::PageUp => AScroll::PageUp,
            Scroll::PageDown => AScroll::PageDown,
            Scroll::Top => AScroll::Top,
            Scroll::Bottom => AScroll::Bottom,
        });
    }

    fn clear_history(&mut self) {
        self.term.grid_mut().clear_history();
    }

    fn start_selection(&mut self, ty: SelectionType, point: Point, side: Side) {
        let ty = match ty {
            SelectionType::Simple => AType::Simple,
            SelectionType::Semantic => AType::Semantic,
            SelectionType::Lines => AType::Lines,
        };
        self.term.selection = Some(Selection::new(ty, point_in(point), side_in(side)));
    }

    fn update_selection(&mut self, point: Point, side: Side) {
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point_in(point), side_in(side));
        }
    }

    fn clear_selection(&mut self) {
        self.term.selection = None;
    }

    fn selection_range(&self) -> Option<SelectionRange> {
        let range = self.term.selection.as_ref()?.to_range(&self.term)?;
        Some(SelectionRange::new(
            point_out(range.start),
            point_out(range.end),
            range.is_block,
        ))
    }

    fn selection_to_string(&self) -> Option<String> {
        self.term.selection_to_string()
    }

    fn semantic_search_left(&self, point: Point) -> Point {
        point_out(self.term.semantic_search_left(point_in(point)))
    }

    fn semantic_search_right(&self, point: Point) -> Point {
        point_out(self.term.semantic_search_right(point_in(point)))
    }

    fn pictures(&self) -> Vec<Picture> {
        // alacritty drops Kitty graphics in its parser, so there is never a
        // picture to show. That is why it is the fallback.
        Vec::new()
    }

    fn picture_data(&self, _key: PictureKey) -> Option<PictureData> {
        None
    }

    fn forget_pictures(&mut self) {}

    #[cfg(test)]
    fn hyperlink(&self, point: Point) -> Option<Hyperlink> {
        let link = self.term.grid()[point_in(point)].hyperlink()?;
        Some(Hyperlink::new(link.id(), link.uri()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::cell::Flags as AFlags;
    use alacritty_terminal::term::TermMode as AMode;

    /// The translation above is `from_bits_truncate`, which is only lossless
    /// while `vt` knows every bit alacritty uses. This is where an alacritty
    /// bump that added one would be caught.
    #[test]
    fn vt_knows_every_bit_alacritty_uses() {
        assert_eq!(Flags::all().bits(), AFlags::all().bits());
        assert_eq!(
            TermMode::all().bits() & AMode::all().bits(),
            AMode::all().bits()
        );
    }
}
