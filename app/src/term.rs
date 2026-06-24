//! G0b seam — real shell via alacritty_terminal (Option A from docs/PLAN.md §2).
//! Written clean-room against the crate's public API (docs.rs + registry source).
//!
//! alacritty_terminal's EventLoop owns the PTY reader thread, the VTE parser
//! pump, and the writer. We provide: an EventListener proxy that ships events
//! onto an async channel (consumed by the gpui entity), and keystroke bytes in
//! via the Notifier. NOTE: Event::PtyWrite (query responses like DA/DSR) is
//! emitted through the proxy and NOT auto-routed back to the PTY — the consumer
//! must bounce it via the notifier or interactive apps hang.

use std::fs::File;
use std::io;
use std::sync::Arc;

use alacritty_terminal::{
    event::{Event as TermEvent, EventListener, WindowSize},
    event_loop::{EventLoop, Msg, Notifier},
    grid::Dimensions,
    sync::FairMutex,
    term::{Config, Term},
    tty,
};
use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};

/// Grid dimensions for Term::new (the crate's own TermSize lives in its test module).
#[derive(Clone, Copy, Debug)]
pub struct GridSize {
    pub cols: usize,
    pub rows: usize,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

/// Forwards terminal events from the EventLoop thread onto an async channel.
#[derive(Clone)]
pub struct EventProxy(UnboundedSender<TermEvent>);

impl EventListener for EventProxy {
    fn send_event(&self, event: TermEvent) {
        let _ = self.0.unbounded_send(event);
    }
}

pub struct Session {
    pub term: Arc<FairMutex<Term<EventProxy>>>,
    pub notifier: Notifier,
    /// Taken once by the UI entity to drive event handling.
    pub events: Option<UnboundedReceiver<TermEvent>>,
    /// Our own handle on the PTY master — used to ask the kernel what the
    /// foreground process is (tcgetpgrp), powering mode detection.
    pub master: Option<File>,
    pub shell_pid: u32,
}

impl Session {
    /// Resize both the emulation grid and the PTY (SIGWINCH to the child).
    pub fn resize(&self, size: GridSize, cell_width: u16, cell_height: u16) {
        let window_size = WindowSize {
            num_lines: size.rows as u16,
            num_cols: size.cols as u16,
            cell_width,
            cell_height,
        };
        let _ = self.notifier.0.send(Msg::Resize(window_size));
        self.term.lock().resize(size);
    }
}

/// Spawn the user's default shell on a PTY, emulation wired, I/O thread running.
#[allow(dead_code)]
pub fn spawn(size: GridSize, cell_width: u16, cell_height: u16) -> io::Result<Session> {
    spawn_in(size, cell_width, cell_height, None)
}

/// `spawn`, but the shell starts in `cwd` (session restore). A vanished dir
/// falls back to the default start directory rather than failing the pane.
pub fn spawn_in(
    size: GridSize,
    cell_width: u16,
    cell_height: u16,
    cwd: Option<std::path::PathBuf>,
) -> io::Result<Session> {
    let (tx, rx) = unbounded();
    let proxy = EventProxy(tx);

    let window_size = WindowSize {
        num_lines: size.rows as u16,
        num_cols: size.cols as u16,
        cell_width,
        cell_height,
    };

    let mut options = tty::Options {
        working_directory: cwd.filter(|d| d.is_dir()),
        ..Default::default()
    };
    // A demo window runs THIS binary as every pane's program — a frozen screen of
    // lorem-ipsum styled like a real agent session — instead of the user's shell.
    // So a shared demo shows no real shell, cwd, scrollback, or secret, yet flows
    // through the normal PTY→grid→warp/grade render path for full fidelity. Gated
    // on TD_DEMO, which is set only for the spawned demo window (see main).
    if std::env::var_os("TD_DEMO").is_some() {
        if let Ok(exe) = std::env::current_exe() {
            options.shell = Some(tty::Shell::new(
                exe.to_string_lossy().into_owned(),
                vec!["--td-emit-demo".to_string()],
            ));
        }
    }
    let pty = tty::new(&options, window_size, 0)?;
    let master = pty.file().try_clone().ok();
    let shell_pid = pty.child().id();
    let term = Arc::new(FairMutex::new(Term::new(
        Config::default(),
        &size,
        proxy.clone(),
    )));
    let event_loop = EventLoop::new(term.clone(), proxy, pty, false, false)?;
    let notifier = Notifier(event_loop.channel());
    let _io_thread = event_loop.spawn(); // owns its thread; lives as long as the PTY

    Ok(Session {
        term,
        notifier,
        events: Some(rx),
        master,
        shell_pid,
    })
}

/// Headless, deterministic terminal-correctness matrix.
///
/// These tests drive the emulator the way the real I/O thread does
/// (`event_loop.rs:154` calls `parser.advance(&mut **terminal, bytes)`), but
/// SYNCHRONOUSLY and with NO PTY, NO child shell, and NO sleeps: we build a
/// bare [`Term`] + a `vte::ansi::Processor`, feed raw control bytes with
/// `processor.advance(&mut term, bytes)`, then assert on the resulting grid /
/// mode / emitted events. That makes them fast and CI-stable (a real
/// PTY+shell+sleep loop is the flaky pattern we deliberately avoid).
///
/// Grid/cell access mirrors the production read path in `pane.rs`
/// (`grid[Line(y)][Column(x)].{c,flags,fg}`, `term.mode().contains(...)`).
#[cfg(test)]
mod correctness {
    use std::sync::{Arc, Mutex};

    use alacritty_terminal::event::{Event as TermEvent, EventListener};
    use alacritty_terminal::grid::{Dimensions, Scroll};
    use alacritty_terminal::index::{Column, Line};
    use alacritty_terminal::term::cell::Flags;
    use alacritty_terminal::term::{ClipboardType, Config, Term, TermMode};
    use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};

    use super::GridSize;

    /// Records every event the emulator emits, so OSC-driven side effects
    /// (clipboard stores from OSC 52, title changes, etc.) can be asserted on.
    /// Cheap, synchronous, and `Send + Sync` — no channel, no thread.
    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<TermEvent>>>);

    impl Recorder {
        fn events(&self) -> Vec<TermEvent> {
            self.0.lock().unwrap().clone()
        }
    }

    impl EventListener for Recorder {
        fn send_event(&self, event: TermEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    /// A fresh emulator + parser + event recorder at the given size.
    fn harness(cols: usize, rows: usize) -> (Term<Recorder>, Processor, Recorder) {
        let size = GridSize { cols, rows };
        let rec = Recorder::default();
        let term = Term::new(Config::default(), &size, rec.clone());
        (term, Processor::new(), rec)
    }

    /// 80x24 default — the common case.
    fn term80() -> (Term<Recorder>, Processor, Recorder) {
        harness(80, 24)
    }

    /// Feed bytes into the emulator exactly as the real reader thread does.
    fn feed(term: &mut Term<Recorder>, p: &mut Processor, bytes: &[u8]) {
        p.advance(term, bytes);
    }

    /// The `char` at a live-screen cell (row/col, 0-based from the top).
    fn ch(term: &Term<Recorder>, row: i32, col: usize) -> char {
        term.grid()[Line(row)][Column(col)].c
    }

    /// The flags at a live-screen cell.
    fn flags(term: &Term<Recorder>, row: i32, col: usize) -> Flags {
        term.grid()[Line(row)][Column(col)].flags
    }

    /// Read the live top row as a string, skipping wide-char spacers (the same
    /// thing `pane.rs::live_rows` does for the HUD).
    fn row_text(term: &Term<Recorder>, row: i32) -> String {
        let grid = term.grid();
        let cols = grid.columns();
        let mut s = String::new();
        for c in 0..cols {
            let cell = &grid[Line(row)][Column(c)];
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            s.push(if cell.c == '\0' { ' ' } else { cell.c });
        }
        s.trim_end().to_string()
    }

    // ----------------------------------------------------------------------
    // Sanity: the harness itself behaves.
    // ----------------------------------------------------------------------

    #[test]
    fn fresh_term_has_expected_dimensions() {
        let (term, ..) = term80();
        assert_eq!(term.grid().columns(), 80);
        assert_eq!(term.grid().screen_lines(), 24);
    }

    #[test]
    fn plain_ascii_lands_on_the_grid() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"hello");
        assert_eq!(row_text(&term, 0), "hello");
        assert_eq!(ch(&term, 0, 0), 'h');
        assert_eq!(ch(&term, 0, 4), 'o');
    }

    #[test]
    fn newline_and_carriage_return_move_the_cursor() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"ab\r\ncd");
        assert_eq!(row_text(&term, 0), "ab");
        assert_eq!(row_text(&term, 1), "cd");
    }

    #[test]
    fn fresh_term_starts_with_no_history() {
        let (term, ..) = term80();
        // total_lines == screen_lines until the screen overflows into history.
        assert_eq!(term.grid().total_lines(), term.grid().screen_lines());
        assert_eq!(term.grid().display_offset(), 0);
    }

    // ----------------------------------------------------------------------
    // (a) Wide-char width — CJK occupies WIDE_CHAR + a WIDE_CHAR_SPACER.
    // ----------------------------------------------------------------------

    #[test]
    fn cjk_sets_wide_char_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "你".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert_eq!(ch(&term, 0, 0), '你');
    }

    #[test]
    fn cjk_occupies_a_spacer_cell() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "你".as_bytes());
        // The cell immediately right of a wide char is a spacer (no glyph).
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn cjk_advances_cursor_by_two_columns() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "你A".as_bytes());
        // 你 at col0(+spacer col1), 'A' lands at col2.
        assert_eq!(ch(&term, 0, 2), 'A');
        assert!(!flags(&term, 0, 2).contains(Flags::WIDE_CHAR));
    }

    #[test]
    fn multiple_cjk_pack_two_columns_each() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "日本語".as_bytes());
        assert_eq!(ch(&term, 0, 0), '日');
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 2), '本');
        assert!(flags(&term, 0, 3).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 4), '語');
        assert_eq!(row_text(&term, 0), "日本語");
    }

    #[test]
    fn ascii_is_not_wide() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"A");
        assert!(!flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(!flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
    }

    #[test]
    fn wide_char_at_last_column_wraps() {
        // A 2-wide glyph cannot start in the final column of an odd-width grid;
        // alacritty marks the trailing cell LEADING_WIDE_CHAR_SPACER and wraps
        // the glyph to the next line. Use a 3-col grid so col2 is the last.
        let (mut term, mut p, _) = harness(3, 4);
        feed(&mut term, &mut p, b"AB"); // fill col0,col1; cursor at col2 (last)
        feed(&mut term, &mut p, "你".as_bytes());
        // The wide char could not fit at col2, so it wrapped to row1 col0.
        assert!(
            flags(&term, 2 - 2, 2).contains(Flags::LEADING_WIDE_CHAR_SPACER)
                || ch(&term, 1, 0) == '你'
        );
        assert_eq!(ch(&term, 1, 0), '你');
    }

    // ----------------------------------------------------------------------
    // (b) Emoji / ZWJ width.
    // ----------------------------------------------------------------------

    #[test]
    fn emoji_is_wide() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "😀".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        assert_eq!(ch(&term, 0, 0), '😀');
    }

    #[test]
    fn text_after_emoji_lands_two_columns_over() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "😀X".as_bytes());
        assert_eq!(ch(&term, 0, 2), 'X');
    }

    #[test]
    fn zwj_family_emoji_collapses_into_the_base_cell() {
        // 👨‍👩‍👧 is base + ZWJ + emoji + ZWJ + emoji. vte appends the
        // zero-width joiners/combining members onto the base cell as `extra`
        // rather than consuming extra columns: the base cell stays WIDE_CHAR
        // and the visible advance is still 2 columns, with the trailing text
        // landing right after the spacer.
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "👨‍👩‍👧!".as_bytes());
        assert!(flags(&term, 0, 0).contains(Flags::WIDE_CHAR));
        assert!(flags(&term, 0, 1).contains(Flags::WIDE_CHAR_SPACER));
        // Trailing '!' is the first non-spacer after the cluster.
        let txt = row_text(&term, 0);
        assert!(txt.ends_with('!'), "row was {txt:?}");
    }

    #[test]
    fn combining_accent_does_not_consume_a_column() {
        // 'e' + U+0301 (combining acute) is one grid cell, not two.
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, "e\u{0301}X".as_bytes());
        // 'X' must be at col1: the accent attached to the 'e' cell.
        assert_eq!(ch(&term, 0, 1), 'X');
    }

    // ----------------------------------------------------------------------
    // (c) Alt-screen — DECSET/DECRST 1049.
    // ----------------------------------------------------------------------

    #[test]
    fn alt_screen_off_by_default() {
        let (term, ..) = term80();
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn decset_1049_enters_alt_screen() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1049h");
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn decrst_1049_leaves_alt_screen() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1049h");
        assert!(term.mode().contains(TermMode::ALT_SCREEN));
        feed(&mut term, &mut p, b"\x1b[?1049l");
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn legacy_1047_alt_screen_is_unknown_in_this_version() {
        // DOCUMENTED GAP: alacritty_terminal 0.26 / vte 0.15 only map DEC 1049
        // (SwapScreenAndSetRestoreCursor) to alt-screen — the older 1047 (and
        // 1048 cursor save/restore) are unrecognised private modes and are
        // ignored, so 1047 does NOT toggle ALT_SCREEN here. Apps that emit 1049
        // (vim, less, tmux, fzf, $PAGER) are covered by the tests above; this
        // assertion pins the no-op so a future crate bump that adds 1047 support
        // turns this red and prompts a real toggle assertion.
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1047h");
        assert!(!term.mode().contains(TermMode::ALT_SCREEN));
    }

    #[test]
    fn alt_screen_content_is_isolated_from_primary() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"primary");
        feed(&mut term, &mut p, b"\x1b[?1049h");
        // Alt screen starts blank (no primary content bleeds through).
        assert_eq!(row_text(&term, 0), "");
        // 1049 SAVES the cursor, so it is still at the column after "primary".
        // Real full-screen apps home the cursor before drawing; do the same so
        // the written text is deterministic regardless of the saved column.
        feed(&mut term, &mut p, b"\x1b[H");
        feed(&mut term, &mut p, b"alt");
        assert_eq!(row_text(&term, 0), "alt");
        feed(&mut term, &mut p, b"\x1b[?1049l");
        // Primary content restored on exit.
        assert_eq!(row_text(&term, 0), "primary");
    }

    // ----------------------------------------------------------------------
    // (d) Mouse modes — each DECSET sets the matching TermMode flag.
    // ----------------------------------------------------------------------

    #[test]
    fn mouse_click_mode_1000() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1000h");
        assert!(term.mode().contains(TermMode::MOUSE_REPORT_CLICK));
        feed(&mut term, &mut p, b"\x1b[?1000l");
        assert!(!term.mode().contains(TermMode::MOUSE_REPORT_CLICK));
    }

    #[test]
    fn mouse_drag_mode_1002() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1002h");
        assert!(term.mode().contains(TermMode::MOUSE_DRAG));
    }

    #[test]
    fn mouse_motion_mode_1003() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1003h");
        assert!(term.mode().contains(TermMode::MOUSE_MOTION));
    }

    #[test]
    fn sgr_mouse_mode_1006() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1006h");
        assert!(term.mode().contains(TermMode::SGR_MOUSE));
        feed(&mut term, &mut p, b"\x1b[?1006l");
        assert!(!term.mode().contains(TermMode::SGR_MOUSE));
    }

    #[test]
    fn utf8_mouse_mode_1005() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1005h");
        assert!(term.mode().contains(TermMode::UTF8_MOUSE));
    }

    #[test]
    fn focus_event_mode_1004() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1004h");
        assert!(term.mode().contains(TermMode::FOCUS_IN_OUT));
        feed(&mut term, &mut p, b"\x1b[?1004l");
        assert!(!term.mode().contains(TermMode::FOCUS_IN_OUT));
    }

    #[test]
    fn alternate_scroll_mode_1007() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1007h");
        assert!(term.mode().contains(TermMode::ALTERNATE_SCROLL));
    }

    // ----------------------------------------------------------------------
    // (e) Bracketed paste — DECSET 2004.
    // ----------------------------------------------------------------------

    #[test]
    fn bracketed_paste_off_by_default() {
        let (term, ..) = term80();
        assert!(!term.mode().contains(TermMode::BRACKETED_PASTE));
    }

    #[test]
    fn bracketed_paste_toggles() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?2004h");
        assert!(term.mode().contains(TermMode::BRACKETED_PASTE));
        feed(&mut term, &mut p, b"\x1b[?2004l");
        assert!(!term.mode().contains(TermMode::BRACKETED_PASTE));
    }

    // ----------------------------------------------------------------------
    // Other DEC private modes we read elsewhere in the app.
    // ----------------------------------------------------------------------

    #[test]
    fn app_cursor_keys_mode_1() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[?1h");
        assert!(term.mode().contains(TermMode::APP_CURSOR));
        feed(&mut term, &mut p, b"\x1b[?1l");
        assert!(!term.mode().contains(TermMode::APP_CURSOR));
    }

    #[test]
    fn show_cursor_mode_25() {
        let (mut term, mut p, _) = term80();
        // SHOW_CURSOR is set by default; ?25l hides it.
        assert!(term.mode().contains(TermMode::SHOW_CURSOR));
        feed(&mut term, &mut p, b"\x1b[?25l");
        assert!(!term.mode().contains(TermMode::SHOW_CURSOR));
        feed(&mut term, &mut p, b"\x1b[?25h");
        assert!(term.mode().contains(TermMode::SHOW_CURSOR));
    }

    #[test]
    fn line_wrap_mode_7() {
        let (mut term, mut p, _) = term80();
        // LINE_WRAP (autowrap, DECAWM) is on by default.
        assert!(term.mode().contains(TermMode::LINE_WRAP));
        feed(&mut term, &mut p, b"\x1b[?7l");
        assert!(!term.mode().contains(TermMode::LINE_WRAP));
    }

    // ----------------------------------------------------------------------
    // (f) Scrollback — overflow grows history; scroll_display moves the offset.
    // ----------------------------------------------------------------------

    #[test]
    fn overflow_grows_history() {
        let (mut term, mut p, _) = harness(20, 4); // 4 visible rows
        let before = term.grid().total_lines();
        assert_eq!(before, 4);
        // Print 10 lines: 6 must spill into scrollback history.
        for i in 0..10 {
            feed(&mut term, &mut p, format!("L{i}\r\n").as_bytes());
        }
        let after = term.grid().total_lines();
        assert!(after > before, "history did not grow: {before} -> {after}");
        // history = total - screen.
        assert_eq!(after - term.grid().screen_lines(), after - 4);
        assert!(
            after - 4 >= 6,
            "expected >=6 history rows, got {}",
            after - 4
        );
    }

    #[test]
    fn display_offset_zero_at_bottom() {
        let (mut term, mut p, _) = harness(20, 4);
        for i in 0..10 {
            feed(&mut term, &mut p, format!("L{i}\r\n").as_bytes());
        }
        // Not scrolled back yet — viewport pinned to the live bottom.
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn scroll_display_moves_offset_into_history() {
        let (mut term, mut p, _) = harness(20, 4);
        for i in 0..10 {
            feed(&mut term, &mut p, format!("L{i}\r\n").as_bytes());
        }
        term.scroll_display(Scroll::Delta(3));
        assert_eq!(term.grid().display_offset(), 3);
        term.scroll_display(Scroll::Top);
        assert!(term.grid().display_offset() > 0);
        term.scroll_display(Scroll::Bottom);
        assert_eq!(term.grid().display_offset(), 0);
    }

    #[test]
    fn scrolled_back_content_is_a_prior_line() {
        let (mut term, mut p, _) = harness(20, 6);
        for i in 0..20 {
            feed(&mut term, &mut p, format!("line{i}\r\n").as_bytes());
        }
        term.scroll_display(Scroll::Top);
        // The very top of history is the oldest retained line.
        let top = row_text(&term, 0);
        assert!(top.starts_with("line"), "top of scrollback was {top:?}");
    }

    // ----------------------------------------------------------------------
    // (g) ANSI 16-colour cell fg.
    // ----------------------------------------------------------------------

    fn fg(term: &Term<Recorder>, row: i32, col: usize) -> Color {
        term.grid()[Line(row)][Column(col)].fg
    }

    #[test]
    fn sgr_31_sets_named_red_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[31mR");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Red));
    }

    #[test]
    fn sgr_32_sets_named_green_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[32mG");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Green));
    }

    #[test]
    fn sgr_34_sets_named_blue_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[34mB");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Blue));
    }

    #[test]
    fn sgr_91_sets_bright_red_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[91mr");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::BrightRed));
    }

    #[test]
    fn sgr_0_resets_fg_to_default() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[31mR\x1b[0mD");
        assert_eq!(fg(&term, 0, 0), Color::Named(NamedColor::Red));
        assert_eq!(fg(&term, 0, 1), Color::Named(NamedColor::Foreground));
    }

    #[test]
    fn sgr_256_indexed_fg() {
        let (mut term, mut p, _) = term80();
        // 38;5;208 = indexed orange.
        feed(&mut term, &mut p, b"\x1b[38;5;208mX");
        assert_eq!(fg(&term, 0, 0), Color::Indexed(208));
    }

    #[test]
    fn sgr_truecolor_fg() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[38;2;10;20;30mX");
        match fg(&term, 0, 0) {
            Color::Spec(rgb) => {
                assert_eq!((rgb.r, rgb.g, rgb.b), (10, 20, 30));
            }
            other => panic!("expected truecolor spec, got {other:?}"),
        }
    }

    #[test]
    fn sgr_bold_sets_bold_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[1mX");
        assert!(flags(&term, 0, 0).contains(Flags::BOLD));
    }

    #[test]
    fn sgr_underline_sets_underline_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[4mX");
        assert!(flags(&term, 0, 0).contains(Flags::UNDERLINE));
    }

    #[test]
    fn sgr_inverse_sets_inverse_flag() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"\x1b[7mX");
        assert!(flags(&term, 0, 0).contains(Flags::INVERSE));
    }

    // ----------------------------------------------------------------------
    // (h) OSC 8 hyperlinks — uri (and id) attach to the painted cells.
    // ----------------------------------------------------------------------

    #[test]
    fn osc8_hyperlink_attaches_uri_to_cells() {
        let (mut term, mut p, _) = term80();
        // OSC 8 ; ; URI ST  text  OSC 8 ; ; ST   (BEL-terminated form)
        feed(
            &mut term,
            &mut p,
            b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07",
        );
        let cell = &term.grid()[Line(0)][Column(0)];
        let hl = cell.hyperlink().expect("cell 0 should carry a hyperlink");
        assert_eq!(hl.uri(), "https://example.com");
        // 'l' of "link" is the painted glyph.
        assert_eq!(ch(&term, 0, 0), 'l');
    }

    #[test]
    fn osc8_hyperlink_with_id_param() {
        let (mut term, mut p, _) = term80();
        feed(
            &mut term,
            &mut p,
            b"\x1b]8;id=42;https://rust-lang.org\x07rust\x1b]8;;\x07",
        );
        let hl = term.grid()[Line(0)][Column(0)]
            .hyperlink()
            .expect("hyperlink present");
        assert_eq!(hl.uri(), "https://rust-lang.org");
        // alacritty's Hyperlink::id() returns &str; the explicit id is preserved.
        assert_eq!(hl.id(), "42");
    }

    #[test]
    fn osc8_close_stops_attaching_links() {
        let (mut term, mut p, _) = term80();
        feed(
            &mut term,
            &mut p,
            b"\x1b]8;;https://a.test\x07A\x1b]8;;\x07B",
        );
        assert!(term.grid()[Line(0)][Column(0)].hyperlink().is_some());
        // 'B' is printed after the link was closed — no hyperlink.
        assert!(term.grid()[Line(0)][Column(1)].hyperlink().is_none());
    }

    #[test]
    fn cells_without_osc8_have_no_hyperlink() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"plain");
        assert!(term.grid()[Line(0)][Column(0)].hyperlink().is_none());
    }

    // ----------------------------------------------------------------------
    // (i) OSC 52 — clipboard store. Default Config osc52 == OnlyCopy, so a
    //     copy ('c') sequence emits Event::ClipboardStore with the decoded text.
    // ----------------------------------------------------------------------

    #[test]
    fn osc52_copy_emits_clipboard_store() {
        let (mut term, mut p, rec) = term80();
        // base64("hi") == "aGk=".
        feed(&mut term, &mut p, b"\x1b]52;c;aGk=\x07");
        let stored: Vec<_> = rec
            .events()
            .into_iter()
            .filter_map(|e| match e {
                TermEvent::ClipboardStore(ClipboardType::Clipboard, s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(stored, vec!["hi".to_string()]);
    }

    #[test]
    fn osc52_decodes_longer_payload() {
        let (mut term, mut p, rec) = term80();
        // base64("terminal-delight") == "dGVybWluYWwtZGVsaWdodA==".
        feed(&mut term, &mut p, b"\x1b]52;c;dGVybWluYWwtZGVsaWdodA==\x07");
        let stored: Vec<_> = rec
            .events()
            .into_iter()
            .filter_map(|e| match e {
                TermEvent::ClipboardStore(_, s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(stored, vec!["terminal-delight".to_string()]);
    }

    #[test]
    fn osc52_invalid_base64_emits_nothing() {
        let (mut term, mut p, rec) = term80();
        feed(&mut term, &mut p, b"\x1b]52;c;not-valid-base64!!!\x07");
        let any_store = rec
            .events()
            .into_iter()
            .any(|e| matches!(e, TermEvent::ClipboardStore(..)));
        assert!(!any_store, "invalid base64 must not store to clipboard");
    }

    // ----------------------------------------------------------------------
    // Misc OSC: title / colour — exercise the event surface used by the app.
    // ----------------------------------------------------------------------

    #[test]
    fn osc0_sets_window_title() {
        let (mut term, mut p, rec) = term80();
        feed(&mut term, &mut p, b"\x1b]0;my-title\x07");
        let titled = rec
            .events()
            .into_iter()
            .any(|e| matches!(e, TermEvent::Title(t) if t == "my-title"));
        assert!(titled, "OSC 0 should emit a Title event");
    }

    // ----------------------------------------------------------------------
    // Cursor movement / erase — the geometry the warp hit-test relies on.
    // ----------------------------------------------------------------------

    #[test]
    fn cup_positions_the_cursor() {
        let (mut term, mut p, _) = term80();
        // CUP row3 col5 (1-based) then write — lands at grid (2,4).
        feed(&mut term, &mut p, b"\x1b[3;5fZ");
        assert_eq!(ch(&term, 2, 4), 'Z');
    }

    #[test]
    fn ed_clears_the_screen() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"dirty\r\nmore");
        feed(&mut term, &mut p, b"\x1b[2J");
        assert_eq!(row_text(&term, 0), "");
        assert_eq!(row_text(&term, 1), "");
    }

    #[test]
    fn el_clears_to_end_of_line() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"keepXXXX");
        // Move cursor back to col4 and erase to EOL.
        feed(&mut term, &mut p, b"\x1b[1;5H\x1b[K");
        assert_eq!(row_text(&term, 0), "keep");
    }

    #[test]
    fn backspace_moves_left_without_erasing() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"ab\x08c");
        // 'c' overwrites 'b'.
        assert_eq!(row_text(&term, 0), "ac");
    }

    #[test]
    fn tab_advances_to_next_stop() {
        let (mut term, mut p, _) = term80();
        feed(&mut term, &mut p, b"a\tb");
        assert_eq!(ch(&term, 0, 0), 'a');
        // Default tab stops every 8 columns → 'b' at col8.
        assert_eq!(ch(&term, 0, 8), 'b');
    }

    #[test]
    fn autowrap_pushes_overflow_to_next_row() {
        let (mut term, mut p, _) = harness(4, 4);
        feed(&mut term, &mut p, b"abcdef");
        assert_eq!(row_text(&term, 0), "abcd");
        assert_eq!(row_text(&term, 1), "ef");
    }
}
