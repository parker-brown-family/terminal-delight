//! The terminal emulator core, behind a boundary Terminal Delight owns.
//!
//! Everything that turns a program's bytes into a grid of cells lives below
//! this line, and nothing above it names the crate that does the work. Above
//! it, the application reads [`Term`] in TD's own vocabulary ([`types`]); below
//! it, one of two adapters translates a real core into that vocabulary:
//!
//! - **rio-vt** (`rio.rs`), the default since 2026-09-25 — faster to parse,
//!   lighter to hold, and it keeps the pictures programs draw.
//! - **alacritty_terminal** (`alacritty.rs`), compiled only with
//!   `--features core-alacritty`, kept as a fallback until rio-vt has held in
//!   daily use and as the oracle the test suite checks rio-vt against.
//!
//! Around the core, TD owns the rest of a terminal: the read loop
//! ([`pump`]), the pseudoterminal ([`pty`]), the socket a replica reads
//! ([`socket`]), and the lock. The core's parser lives inside [`Term`], inside
//! that lock, which is what lets a session host flush a half-finished
//! synchronized update before it snapshots a pane.
//!
//! The plan this implements, with every alternative it was chosen over, is
//! `docs/plans/core-swap-rio/` in the repository.

use std::sync::Arc;
use std::time::Instant;

mod types;
pub use types::*;

pub mod pty;
pub mod pump;
pub mod socket;

#[cfg(feature = "core-alacritty")]
mod alacritty;
#[cfg(feature = "core-alacritty")]
use alacritty::Core;

#[cfg(not(feature = "core-alacritty"))]
mod compat;
#[cfg(not(feature = "core-alacritty"))]
mod kitty;
#[cfg(not(feature = "core-alacritty"))]
mod rio;
#[cfg(not(feature = "core-alacritty"))]
use rio::Core;

pub use pump::{Msg, Notifier};

/// A terminal as the application shares it: the renderer, the read loop and
/// the session host's fence all take this one lock.
pub type Shared = Arc<parking_lot::Mutex<Term>>;

/// Wrap a terminal for sharing.
pub fn shared(term: Term) -> Shared {
    Arc::new(parking_lot::Mutex::new(term))
}

/// Lines of history a terminal keeps. alacritty's default, which TD has always
/// run with and which its tests assume.
pub const SCROLLBACK: usize = 10_000;

/// Which core this build runs, for logs and `--version`.
pub const CORE_NAME: &str = Core::NAME;

/// What an adapter provides. The narrow half of [`Term`]: everything both
/// cores should agree on — clamping, the resize floor, row width, what a blank
/// cell is — is done once, in [`Term`], and not twice in the adapters.
trait Backend: Send + Sized {
    const NAME: &'static str;
    fn new(size: TermSize, listener: Arc<dyn Listener>, scrollback: usize) -> Self;
    fn advance(&mut self, bytes: &[u8]);
    fn sync_bytes_count(&self) -> usize;
    fn sync_deadline(&self) -> Option<Instant>;
    fn flush_sync(&mut self);
    fn resize(&mut self, size: TermSize);
    fn columns(&self) -> usize;
    fn screen_lines(&self) -> usize;
    fn history_size(&self) -> usize;
    fn display_offset(&self) -> usize;
    /// Append the cells of `line`, which is already clamped into the grid.
    /// May append fewer than `columns()`; [`Term`] pads.
    fn row_into(&self, line: Line, out: &mut Vec<Cell>);
    /// The cursor's grid position, and its shape: `Hidden` when a program has
    /// hidden it, but not merely because the view is scrolled.
    fn cursor(&self) -> Cursor;
    fn mode(&self) -> TermMode;
    fn scroll_display(&mut self, scroll: Scroll);
    fn clear_history(&mut self);
    fn start_selection(&mut self, ty: SelectionType, point: Point, side: Side);
    fn update_selection(&mut self, point: Point, side: Side);
    fn clear_selection(&mut self);
    fn selection_range(&self) -> Option<SelectionRange>;
    fn selection_to_string(&self) -> Option<String>;
    fn semantic_search_left(&self, point: Point) -> Point;
    fn semantic_search_right(&self, point: Point) -> Point;
    #[cfg(test)]
    fn hyperlink(&self, point: Point) -> Option<Hyperlink>;
    fn pictures(&self) -> Vec<Picture>;
    fn picture_data(&self, key: PictureKey) -> Option<PictureData>;
    fn forget_pictures(&mut self);
}

/// One terminal: a core, its parser, and the size it was last told.
///
/// Addressing is alacritty's, and the application's since the start: `Line(0)`
/// is the top of the live screen whatever the view is scrolled to, history
/// runs negative to `-history_size()`, and a viewport row is a `Line` plus the
/// display offset. **Every read is clamped** into the grid, because rio-vt only
/// checks its indices in debug builds and returns the wrong row in a release
/// build rather than panicking.
pub struct Term {
    core: Core,
    size: TermSize,
}

impl Term {
    pub fn new(size: TermSize, listener: Arc<dyn Listener>) -> Self {
        Self {
            core: Core::new(size, listener, SCROLLBACK),
            size,
        }
    }

    // --- feeding -------------------------------------------------------

    /// Parse output from the program.
    pub fn advance(&mut self, bytes: &[u8]) {
        self.core.advance(bytes);
    }

    /// Bytes held back inside an open synchronized update (DEC 2026).
    pub fn sync_bytes_count(&self) -> usize {
        self.core.sync_bytes_count()
    }

    /// When an open synchronized update must be drawn regardless.
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.core.sync_deadline()
    }

    /// Draw an open synchronized update as it stands.
    pub fn flush_sync(&mut self) {
        if self.core.sync_bytes_count() > 0 || self.core.sync_deadline().is_some() {
            self.core.flush_sync();
        }
    }

    // --- geometry ------------------------------------------------------

    /// Tell the core its new size. The grid never goes below 2 columns by 1
    /// line, whatever it is asked for.
    pub fn resize(&mut self, size: TermSize) {
        let size = TermSize::new(
            size.columns,
            size.screen_lines,
            size.cell_width,
            size.cell_height,
        );
        if size != self.size {
            self.core.resize(size);
            self.size = size;
        }
    }

    pub fn columns(&self) -> usize {
        self.core.columns()
    }

    pub fn screen_lines(&self) -> usize {
        self.core.screen_lines()
    }

    pub fn history_size(&self) -> usize {
        self.core.history_size()
    }

    /// Screen and history together.
    #[cfg(test)]
    pub fn total_lines(&self) -> usize {
        self.screen_lines() + self.history_size()
    }

    /// The oldest line of history still held.
    pub fn topmost_line(&self) -> Line {
        Line(-(self.history_size() as i32))
    }

    /// The bottom line of the live screen.
    pub fn bottommost_line(&self) -> Line {
        Line(self.screen_lines() as i32 - 1)
    }

    pub fn last_column(&self) -> Column {
        Column(self.columns().saturating_sub(1))
    }

    /// How many lines the view is scrolled back from the live screen.
    pub fn display_offset(&self) -> usize {
        self.core.display_offset()
    }

    // --- content -------------------------------------------------------

    fn clamp_line(&self, line: Line) -> Line {
        Line(
            line.0
                .clamp(self.topmost_line().0, self.bottommost_line().0),
        )
    }

    /// One row, by value, exactly `columns()` wide.
    pub fn row(&self, line: Line) -> Row {
        let mut cells = Vec::with_capacity(self.columns());
        self.row_into(line, &mut cells);
        Row(cells)
    }

    /// Fill `out` with one row, reusing its allocation — for walking all of
    /// history without allocating a row per line.
    pub fn row_into(&self, line: Line, out: &mut Vec<Cell>) {
        out.clear();
        let columns = self.columns();
        self.core.row_into(self.clamp_line(line), out);
        out.truncate(columns);
        out.resize(columns, Cell::default());
    }

    /// Visit every row from `first` to `last` inclusive (both clamped), one
    /// buffer reused throughout.
    pub fn for_each_row(&self, first: Line, last: Line, mut visit: impl FnMut(Line, &[Cell])) {
        let (first, last) = (self.clamp_line(first), self.clamp_line(last));
        let mut cells = Vec::with_capacity(self.columns());
        for line in first.0..=last.0 {
            self.row_into(Line(line), &mut cells);
            visit(Line(line), &cells);
        }
    }

    /// One cell, by value.
    pub fn cell(&self, point: Point) -> Cell {
        let column = point.column.0.min(self.last_column().0);
        self.row(point.line)
            .0
            .into_iter()
            .nth(column)
            .unwrap_or_default()
    }

    /// The visible cells, top left to bottom right, each at its grid point:
    /// a caller adds `display_offset()` to a point's line to get its screen row.
    pub fn display_iter(&self) -> std::vec::IntoIter<Indexed> {
        let offset = self.display_offset() as i32;
        let lines = self.screen_lines() as i32;
        let mut out = Vec::with_capacity(self.columns() * self.screen_lines());
        self.for_each_row(Line(-offset), Line(lines - 1 - offset), |line, cells| {
            for (column, cell) in cells.iter().enumerate() {
                out.push(Indexed {
                    point: Point::new(line, Column(column)),
                    cell: cell.clone(),
                });
            }
        });
        out.into_iter()
    }

    /// Everything one frame needs, read at once.
    pub fn renderable_content(&self) -> RenderableContent {
        RenderableContent {
            display_iter: self.display_iter(),
            selection: self.selection_range(),
            cursor: self.renderable_cursor(),
            display_offset: self.display_offset(),
            mode: self.mode(),
        }
    }

    /// The cursor as it is drawn: on a wide character's first half rather
    /// than its spacer, and hidden when a program has hidden it.
    fn renderable_cursor(&self) -> Cursor {
        let mut cursor = self.core.cursor();
        if cursor.point.column.0 > 0
            && self
                .cell(cursor.point)
                .flags
                .contains(Flags::WIDE_CHAR_SPACER)
        {
            cursor.point.column.0 -= 1;
        }
        if !self.mode().contains(TermMode::SHOW_CURSOR) {
            cursor.shape = CursorShape::Hidden;
        }
        cursor
    }

    /// Where the next character will be written, in grid coordinates.
    pub fn cursor(&self) -> Cursor {
        self.core.cursor()
    }

    pub fn mode(&self) -> TermMode {
        self.core.mode()
    }

    // --- the application's own changes --------------------------------

    pub fn scroll_display(&mut self, scroll: Scroll) {
        self.core.scroll_display(scroll);
    }

    /// Forget the history, keep the screen.
    pub fn clear_history(&mut self) {
        self.core.clear_history();
    }

    /// Begin a selection. The core keeps it, because output that scrolls the
    /// screen has to move the selection with the text.
    pub fn start_selection(&mut self, ty: SelectionType, point: Point, side: Side) {
        let point = self.clamp_point(point);
        self.core.start_selection(ty, point, side);
    }

    /// Move the selection's free end. Nothing happens without a selection.
    pub fn update_selection(&mut self, point: Point, side: Side) {
        let point = self.clamp_point(point);
        self.core.update_selection(point, side);
    }

    pub fn clear_selection(&mut self) {
        self.core.clear_selection();
    }

    pub fn selection_range(&self) -> Option<SelectionRange> {
        self.core.selection_range()
    }

    pub fn selection_to_string(&self) -> Option<String> {
        self.core.selection_to_string()
    }

    /// The start of the word at `point`.
    pub fn semantic_search_left(&self, point: Point) -> Point {
        self.core.semantic_search_left(self.clamp_point(point))
    }

    /// The end of the word at `point`.
    pub fn semantic_search_right(&self, point: Point) -> Point {
        self.core.semantic_search_right(self.clamp_point(point))
    }

    fn clamp_point(&self, point: Point) -> Point {
        Point::new(
            self.clamp_line(point.line),
            Column(point.column.0.min(self.last_column().0)),
        )
    }

    // --- pictures ------------------------------------------------------

    /// The pictures a program has drawn that are on the screen now, lowest
    /// first. Cheap: no pixels, only where each one is.
    pub fn pictures(&self) -> Vec<Picture> {
        self.core.pictures()
    }

    /// One image's pixels, if the key still names what the core holds — asked
    /// only by a renderer that has no texture for it yet.
    pub fn picture_data(&self, key: PictureKey) -> Option<PictureData> {
        self.core.picture_data(key)
    }

    /// Forget every picture — from the screen and from the core. Pictures in
    /// Terminal Delight are attentional: they are there while their pane is
    /// looked at, and gone once it has been hidden.
    pub fn forget_pictures(&mut self) {
        self.core.forget_pictures();
    }

    /// The OSC 8 hyperlink on a cell, which only the tests read.
    #[cfg(test)]
    pub fn hyperlink(&self, point: Point) -> Option<Hyperlink> {
        self.core.hyperlink(self.clamp_point(point))
    }
}

/// Convert a viewport position into a grid point: the viewport's row `line`
/// is `display_offset` lines above the live screen's.
pub fn viewport_to_point(display_offset: usize, point: Point) -> Point {
    Point::new(Line(point.line.0 - display_offset as i32), point.column)
}

#[cfg(test)]
mod boundary {
    //! Nothing outside `vt/` may name a core crate. The boundary is only worth
    //! having if it holds, and it erodes one convenient import at a time.
    use std::path::Path;

    fn scan(dir: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("read src") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|name| name == "vt") {
                    continue;
                }
                scan(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).expect("read file");
                for (number, line) in text.lines().enumerate() {
                    // Spelled in two halves so this file's own source does not
                    // match the scan it runs.
                    let names = [["alacritty", "_terminal::"], ["rio", "_vt::"]];
                    if names.iter().any(|[a, b]| line.contains(&format!("{a}{b}"))) {
                        found.push(format!(
                            "{}:{}: {}",
                            path.display(),
                            number + 1,
                            line.trim()
                        ));
                    }
                }
            }
        }
    }

    #[test]
    fn no_file_outside_vt_names_a_core_crate() {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut found = vec![];
        scan(&src, &mut found);
        assert!(
            found.is_empty(),
            "the core is reached through crate::vt, never directly:\n{}",
            found.join("\n")
        );
    }
}
