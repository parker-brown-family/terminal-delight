//! Terminal Delight's vocabulary for a terminal's state.
//!
//! These are the words the rest of the application already reads a terminal
//! in — `Line`, `Column`, `Point`, `Flags::WIDE_CHAR`, `TermMode::ALT_SCREEN`,
//! `Color::Named(NamedColor::Foreground)` — defined here, owned here, and
//! filled in by whichever core is compiled in. They were alacritty's words
//! first, and rio-vt forked alacritty and kept most of them under other names,
//! so both cores translate into this module rather than the application
//! translating out of either.
//!
//! **The numbering is frozen, and it is a wire format.** `Flags`, `TermMode`
//! and `NamedColor` carry alacritty_terminal 0.26's exact values. The session
//! host's divergence guard hashes those numbers ([`crate::gridwire::grid_hash`]),
//! and a window running this build must agree with a host that is still running
//! the last one — the host is the process that is never restarted. A test at
//! the bottom pins every value, so the numbers cannot drift with a crate.

use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::process::ExitStatus;
use std::sync::Arc;

use bitflags::bitflags;

/// A row of the grid. `Line(0)` is the top of the live screen whatever the
/// view is scrolled to; history is negative, down to `-history_size`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Line(pub i32);

/// A column of the grid, from zero.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Column(pub usize);

impl From<i32> for Line {
    fn from(line: i32) -> Self {
        Line(line)
    }
}

impl Add<i32> for Line {
    type Output = Line;
    fn add(self, rhs: i32) -> Line {
        Line(self.0 + rhs)
    }
}

impl Sub<i32> for Line {
    type Output = Line;
    fn sub(self, rhs: i32) -> Line {
        Line(self.0 - rhs)
    }
}

impl Add<usize> for Line {
    type Output = Line;
    fn add(self, rhs: usize) -> Line {
        Line(self.0 + rhs as i32)
    }
}

impl Sub<usize> for Line {
    type Output = Line;
    fn sub(self, rhs: usize) -> Line {
        Line(self.0 - rhs as i32)
    }
}

impl AddAssign<i32> for Line {
    fn add_assign(&mut self, rhs: i32) {
        self.0 += rhs;
    }
}

impl SubAssign<i32> for Line {
    fn sub_assign(&mut self, rhs: i32) {
        self.0 -= rhs;
    }
}

impl PartialEq<i32> for Line {
    fn eq(&self, other: &i32) -> bool {
        self.0 == *other
    }
}

impl Add<usize> for Column {
    type Output = Column;
    fn add(self, rhs: usize) -> Column {
        Column(self.0 + rhs)
    }
}

impl Sub<usize> for Column {
    type Output = Column;
    fn sub(self, rhs: usize) -> Column {
        Column(self.0.saturating_sub(rhs))
    }
}

impl AddAssign<usize> for Column {
    fn add_assign(&mut self, rhs: usize) {
        self.0 += rhs;
    }
}

impl SubAssign<usize> for Column {
    fn sub_assign(&mut self, rhs: usize) {
        self.0 = self.0.saturating_sub(rhs);
    }
}

/// A cell's address. Ordered top to bottom, then left to right.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    pub line: Line,
    pub column: Column,
}

impl Point {
    pub fn new(line: Line, column: Column) -> Self {
        Self { line, column }
    }
}

/// Which half of a cell a pointer is over. A selection that starts on the
/// right half of a character does not include it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

/// A request to move the view through history. It never changes what the
/// terminal holds, only which part of it is on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scroll {
    Delta(i32),
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// How a selection grows from where it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionType {
    Simple,
    Semantic,
    Lines,
}

/// A selection resolved to the cells it covers, start before end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelectionRange {
    pub start: Point,
    pub end: Point,
    pub is_block: bool,
}

impl SelectionRange {
    pub fn new(start: Point, end: Point, is_block: bool) -> Self {
        Self {
            start,
            end,
            is_block,
        }
    }

    /// Whether `point` is inside the selection.
    pub fn contains(&self, point: Point) -> bool {
        self.start.line <= point.line
            && self.end.line >= point.line
            && (self.start.column <= point.column
                || (self.start.line != point.line && !self.is_block))
            && (self.end.column >= point.column || (self.end.line != point.line && !self.is_block))
    }
}

bitflags! {
    /// What a cell is, beyond its character and colours. alacritty 0.26's
    /// values, frozen (see the module note).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Flags: u16 {
        const INVERSE                   = 0b0000_0000_0000_0001;
        const BOLD                      = 0b0000_0000_0000_0010;
        const ITALIC                    = 0b0000_0000_0000_0100;
        const BOLD_ITALIC               = 0b0000_0000_0000_0110;
        const UNDERLINE                 = 0b0000_0000_0000_1000;
        const WRAPLINE                  = 0b0000_0000_0001_0000;
        const WIDE_CHAR                 = 0b0000_0000_0010_0000;
        const WIDE_CHAR_SPACER          = 0b0000_0000_0100_0000;
        const DIM                       = 0b0000_0000_1000_0000;
        const DIM_BOLD                  = 0b0000_0000_1000_0010;
        const HIDDEN                    = 0b0000_0001_0000_0000;
        const STRIKEOUT                 = 0b0000_0010_0000_0000;
        const LEADING_WIDE_CHAR_SPACER  = 0b0000_0100_0000_0000;
        const DOUBLE_UNDERLINE          = 0b0000_1000_0000_0000;
        const UNDERCURL                 = 0b0001_0000_0000_0000;
        const DOTTED_UNDERLINE          = 0b0010_0000_0000_0000;
        const DASHED_UNDERLINE          = 0b0100_0000_0000_0000;
        const ALL_UNDERLINES            = Self::UNDERLINE.bits() | Self::DOUBLE_UNDERLINE.bits()
                                        | Self::UNDERCURL.bits() | Self::DOTTED_UNDERLINE.bits()
                                        | Self::DASHED_UNDERLINE.bits();
    }
}

bitflags! {
    /// The modes a program has switched on. alacritty 0.26's values, frozen
    /// (see the module note); a mode one core has and the other does not is
    /// simply never set here.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct TermMode: u32 {
        const NONE                    = 0;
        const SHOW_CURSOR             = 1;
        const APP_CURSOR              = 1 << 1;
        const APP_KEYPAD              = 1 << 2;
        const MOUSE_REPORT_CLICK      = 1 << 3;
        const BRACKETED_PASTE         = 1 << 4;
        const SGR_MOUSE               = 1 << 5;
        const MOUSE_MOTION            = 1 << 6;
        const LINE_WRAP               = 1 << 7;
        const LINE_FEED_NEW_LINE      = 1 << 8;
        const ORIGIN                  = 1 << 9;
        const INSERT                  = 1 << 10;
        const FOCUS_IN_OUT            = 1 << 11;
        const ALT_SCREEN              = 1 << 12;
        const MOUSE_DRAG              = 1 << 13;
        const UTF8_MOUSE              = 1 << 14;
        const ALTERNATE_SCROLL        = 1 << 15;
        const VI                      = 1 << 16;
        const URGENCY_HINTS           = 1 << 17;
        const DISAMBIGUATE_ESC_CODES  = 1 << 18;
        const REPORT_EVENT_TYPES      = 1 << 19;
        const REPORT_ALTERNATE_KEYS   = 1 << 20;
        const REPORT_ALL_KEYS_AS_ESC  = 1 << 21;
        const REPORT_ASSOCIATED_TEXT  = 1 << 22;
        const MOUSE_MODE              = Self::MOUSE_REPORT_CLICK.bits() | Self::MOUSE_MOTION.bits()
                                      | Self::MOUSE_DRAG.bits();
        const KITTY_KEYBOARD_PROTOCOL = Self::DISAMBIGUATE_ESC_CODES.bits()
                                      | Self::REPORT_EVENT_TYPES.bits()
                                      | Self::REPORT_ALTERNATE_KEYS.bits()
                                      | Self::REPORT_ALL_KEYS_AS_ESC.bits()
                                      | Self::REPORT_ASSOCIATED_TEXT.bits();
        const ANY                     = u32::MAX;
    }
}

/// A colour by role rather than by value: the sixteen ANSI colours, then the
/// terminal's own foreground, background and cursor, then their dim and bright
/// variants. The discriminants are alacritty 0.26's and the `ColorRequest`
/// index convention's, frozen (see the module note).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NamedColor {
    Black = 0,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Foreground = 256,
    Background,
    Cursor,
    DimBlack,
    DimRed,
    DimGreen,
    DimYellow,
    DimBlue,
    DimMagenta,
    DimCyan,
    DimWhite,
    BrightForeground,
    DimForeground,
}

impl NamedColor {
    /// The role with this `ColorRequest` index, or `None` for an index that
    /// names no role.
    pub fn from_index(index: usize) -> Option<Self> {
        use NamedColor::*;
        const LOW: [NamedColor; 16] = [
            Black,
            Red,
            Green,
            Yellow,
            Blue,
            Magenta,
            Cyan,
            White,
            BrightBlack,
            BrightRed,
            BrightGreen,
            BrightYellow,
            BrightBlue,
            BrightMagenta,
            BrightCyan,
            BrightWhite,
        ];
        const HIGH: [NamedColor; 13] = [
            Foreground,
            Background,
            Cursor,
            DimBlack,
            DimRed,
            DimGreen,
            DimYellow,
            DimBlue,
            DimMagenta,
            DimCyan,
            DimWhite,
            BrightForeground,
            DimForeground,
        ];
        match index {
            0..=15 => Some(LOW[index]),
            256..=268 => Some(HIGH[index - 256]),
            _ => None,
        }
    }
}

/// A colour by value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A colour as a cell holds it: a role, an exact value, or a palette slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Color {
    Named(NamedColor),
    Spec(Rgb),
    Indexed(u8),
}

/// An OSC 8 hyperlink. Only the tests read one: TD finds links in the text
/// itself and does not act on OSC 8, but the cores parse it and the suite
/// pins that they do.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Hyperlink {
    id: Arc<str>,
    uri: Arc<str>,
}

#[cfg(test)]
impl Hyperlink {
    pub fn new(id: &str, uri: &str) -> Self {
        Self {
            id: id.into(),
            uri: uri.into(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }
}

/// The rare things a cell can carry, kept out of line so a plain cell stays
/// small.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct CellExtra {
    zerowidth: Vec<char>,
    underline_color: Option<Color>,
}

/// One cell, by value.
///
/// A copy, never a reference into the core: rio-vt packs a cell into a
/// machine word with its style in a side table, so there is no cell in its
/// memory to lend out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
    extra: Option<Box<CellExtra>>,
}

impl Default for Cell {
    /// A blank: a space in the default colours. Never `'\0'`, whatever a core
    /// stores for an erased cell; the adapters see to that.
    fn default() -> Self {
        Self {
            c: ' ',
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            extra: None,
        }
    }
}

impl Cell {
    pub fn new(c: char, fg: Color, bg: Color, flags: Flags) -> Self {
        Self {
            c,
            fg,
            bg,
            flags,
            extra: None,
        }
    }

    /// Attach what a cell carries out of line. Nothing is allocated for a cell
    /// that carries none of it.
    pub fn with_extra(mut self, zerowidth: &[char], underline_color: Option<Color>) -> Self {
        if zerowidth.is_empty() && underline_color.is_none() {
            self.extra = None;
        } else {
            self.extra = Some(Box::new(CellExtra {
                zerowidth: zerowidth.to_vec(),
                underline_color,
            }));
        }
        self
    }

    /// Combining marks after the base character, if any.
    pub fn zerowidth(&self) -> Option<&[char]> {
        self.extra
            .as_ref()
            .map(|e| e.zerowidth.as_slice())
            .filter(|z| !z.is_empty())
    }

    /// The underline's own colour, when SGR 58 set one.
    pub fn underline_color(&self) -> Option<Color> {
        self.extra.as_ref().and_then(|e| e.underline_color)
    }
}

/// One grid row, by value, always exactly as wide as the terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row(pub(crate) Vec<Cell>);

impl std::ops::Deref for Row {
    type Target = [Cell];
    fn deref(&self) -> &[Cell] {
        &self.0
    }
}

impl std::ops::Index<Column> for Row {
    type Output = Cell;
    fn index(&self, column: Column) -> &Cell {
        &self.0[column.0]
    }
}

/// A cell and where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Indexed {
    pub point: Point,
    pub cell: Cell,
}

impl std::ops::Deref for Indexed {
    type Target = Cell;
    fn deref(&self) -> &Cell {
        &self.cell
    }
}

/// How the cursor is drawn. `Hidden` when a program has hidden it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CursorShape {
    #[default]
    Block,
    Underline,
    Beam,
    Hidden,
}

/// The cursor: where the next character will be written, in grid
/// coordinates, and how it is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cursor {
    pub point: Point,
    pub shape: CursorShape,
}

/// Everything a renderer needs for one frame, read under one lock.
pub struct RenderableContent {
    /// The visible cells, top left to bottom right, in viewport coordinates
    /// shifted by the display offset — the same `Line`s a caller adds
    /// `display_offset` to, to get a screen row.
    pub display_iter: std::vec::IntoIter<Indexed>,
    pub selection: Option<SelectionRange>,
    pub cursor: Cursor,
    pub display_offset: usize,
    pub mode: TermMode,
}

/// A pseudoterminal's size as the kernel keeps it: cells, and each cell's
/// size in device pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WindowSize {
    pub num_lines: u16,
    pub num_cols: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

/// The size a terminal is told: its grid, and the pixel size of one cell,
/// which a core needs to place pictures and to answer `CSI 16 t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TermSize {
    pub columns: usize,
    pub screen_lines: usize,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl TermSize {
    /// The smallest grid a core is ever given. rio-vt's behaviour at zero
    /// columns or lines is undefined in a release build, so the floor is
    /// applied here, once, for both cores.
    pub const MIN_COLUMNS: usize = 2;
    pub const MIN_LINES: usize = 1;

    pub fn new(columns: usize, screen_lines: usize, cell_width: u16, cell_height: u16) -> Self {
        Self {
            columns: columns.max(Self::MIN_COLUMNS),
            screen_lines: screen_lines.max(Self::MIN_LINES),
            cell_width,
            cell_height,
        }
    }

    pub fn window_size(&self) -> WindowSize {
        WindowSize {
            num_lines: self.screen_lines as u16,
            num_cols: self.columns as u16,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

/// Which clipboard an OSC 52 names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardType {
    Clipboard,
    Selection,
}

/// Something the terminal wants its owner to know or do.
#[derive(Clone)]
pub enum Event {
    /// New bytes have been parsed into the grid. Sent by the pump, never by a
    /// core.
    Wakeup,
    /// A reply for the program, to be written back to it.
    PtyWrite(String),
    /// `CSI 14 t`: the text area in pixels, formatted from the size the
    /// pseudoterminal was told.
    TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Send + Sync>),
    /// OSC 4/10/11/12 asking what colour a role is drawn in.
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Send + Sync>),
    Title(String),
    ResetTitle,
    Bell,
    ClipboardStore(ClipboardType, String),
    CursorBlinkingChange,
    MouseCursorDirty,
    /// The program has exited, with this status. Sent by the pump.
    ChildExit(ExitStatus),
    /// The terminal has ended: its program exited or its stream closed. Sent
    /// by the pump, once, last.
    Exit,
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Wakeup => write!(f, "Wakeup"),
            Event::PtyWrite(text) => write!(f, "PtyWrite({text:?})"),
            Event::TextAreaSizeRequest(_) => write!(f, "TextAreaSizeRequest"),
            Event::ColorRequest(index, _) => write!(f, "ColorRequest({index})"),
            Event::Title(title) => write!(f, "Title({title:?})"),
            Event::ResetTitle => write!(f, "ResetTitle"),
            Event::Bell => write!(f, "Bell"),
            Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({ty:?}, {text:?})"),
            Event::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::ChildExit(status) => write!(f, "ChildExit({status})"),
            Event::Exit => write!(f, "Exit"),
        }
    }
}

/// Where a terminal's events go.
///
/// **Called with the terminal's lock held**, by both cores and by the pump.
/// An implementation must only record or forward — bump a counter, send on a
/// channel — and must never take the terminal's lock itself, which would
/// deadlock the reader thread against its own listener.
pub trait Listener: Send + Sync {
    fn send_event(&self, event: Event);
}

#[cfg(test)]
mod frozen {
    //! The numbers below are the wire format between a window and a session
    //! host built from different commits. They are alacritty_terminal 0.26's,
    //! read out of its source, and a change to any of them makes a new window
    //! disagree with an old host about every screen. If one of these fails,
    //! the fix is to put the number back.
    use super::*;

    #[test]
    fn cell_flags_keep_alacritty_0_26_bits() {
        let pinned: &[(Flags, u16)] = &[
            (Flags::INVERSE, 0x0001),
            (Flags::BOLD, 0x0002),
            (Flags::ITALIC, 0x0004),
            (Flags::UNDERLINE, 0x0008),
            (Flags::WRAPLINE, 0x0010),
            (Flags::WIDE_CHAR, 0x0020),
            (Flags::WIDE_CHAR_SPACER, 0x0040),
            (Flags::DIM, 0x0080),
            (Flags::HIDDEN, 0x0100),
            (Flags::STRIKEOUT, 0x0200),
            (Flags::LEADING_WIDE_CHAR_SPACER, 0x0400),
            (Flags::DOUBLE_UNDERLINE, 0x0800),
            (Flags::UNDERCURL, 0x1000),
            (Flags::DOTTED_UNDERLINE, 0x2000),
            (Flags::DASHED_UNDERLINE, 0x4000),
        ];
        for (flag, bits) in pinned {
            assert_eq!(flag.bits(), *bits, "{flag:?} moved");
        }
        assert_eq!(Flags::ALL_UNDERLINES.bits(), 0x7808);
    }

    #[test]
    fn modes_keep_alacritty_0_26_bits() {
        let pinned: &[(TermMode, u32)] = &[
            (TermMode::SHOW_CURSOR, 1),
            (TermMode::APP_CURSOR, 1 << 1),
            (TermMode::APP_KEYPAD, 1 << 2),
            (TermMode::MOUSE_REPORT_CLICK, 1 << 3),
            (TermMode::BRACKETED_PASTE, 1 << 4),
            (TermMode::SGR_MOUSE, 1 << 5),
            (TermMode::MOUSE_MOTION, 1 << 6),
            (TermMode::LINE_WRAP, 1 << 7),
            (TermMode::LINE_FEED_NEW_LINE, 1 << 8),
            (TermMode::ORIGIN, 1 << 9),
            (TermMode::INSERT, 1 << 10),
            (TermMode::FOCUS_IN_OUT, 1 << 11),
            (TermMode::ALT_SCREEN, 1 << 12),
            (TermMode::MOUSE_DRAG, 1 << 13),
            (TermMode::UTF8_MOUSE, 1 << 14),
            (TermMode::ALTERNATE_SCROLL, 1 << 15),
            (TermMode::VI, 1 << 16),
            (TermMode::URGENCY_HINTS, 1 << 17),
            (TermMode::DISAMBIGUATE_ESC_CODES, 1 << 18),
            (TermMode::REPORT_EVENT_TYPES, 1 << 19),
            (TermMode::REPORT_ALTERNATE_KEYS, 1 << 20),
            (TermMode::REPORT_ALL_KEYS_AS_ESC, 1 << 21),
            (TermMode::REPORT_ASSOCIATED_TEXT, 1 << 22),
        ];
        for (mode, bits) in pinned {
            assert_eq!(mode.bits(), *bits, "{mode:?} moved");
        }
        assert_eq!(TermMode::MOUSE_MODE.bits(), (1 << 3) | (1 << 6) | (1 << 13));
    }

    #[test]
    fn named_colours_keep_alacritty_0_26_discriminants() {
        assert_eq!(NamedColor::Black as usize, 0);
        assert_eq!(NamedColor::BrightWhite as usize, 15);
        assert_eq!(NamedColor::Foreground as usize, 256);
        assert_eq!(NamedColor::Background as usize, 257);
        assert_eq!(NamedColor::Cursor as usize, 258);
        assert_eq!(NamedColor::DimBlack as usize, 259);
        assert_eq!(NamedColor::DimWhite as usize, 266);
        assert_eq!(NamedColor::BrightForeground as usize, 267);
        assert_eq!(NamedColor::DimForeground as usize, 268);
        for index in (0..16).chain(256..269) {
            let named = NamedColor::from_index(index).expect("a role");
            assert_eq!(named as usize, index, "from_index disagrees at {index}");
        }
        assert_eq!(NamedColor::from_index(16), None);
        assert_eq!(NamedColor::from_index(269), None);
    }
}
