//! G0c — resize, scrollback, selection/clipboard, and per-cell ANSI color
//! (styled runs are the shared substrate for selection highlight and G0d).

mod term;

use alacritty_terminal::{
    event::{Event as TermEvent, Notify},
    grid::Scroll,
    index::{Column, Line, Point as TermPoint, Side},
    selection::{Selection, SelectionType},
    term::{TermMode, cell::Flags, viewport_to_point},
    vte::ansi::{Color as AnsiColor, NamedColor},
};
use futures::StreamExt;
use gpui::{
    App, Bounds, ClipboardItem, Context, FocusHandle, Focusable, Font, FontWeight, Hsla,
    KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    ScrollWheelEvent, SharedString, StyledText, TextRun, TitlebarOptions, UnderlineStyle, Window,
    WindowBounds, WindowOptions, div, font, prelude::*, px, rgb, rgba, size,
};
use gpui_platform::application;

// ---- hacker palette tokens (browser reference: src/styles/theme.css) ----
const BG: u32 = 0x050706;
const SURFACE: u32 = 0x08100d;
const TEXT: u32 = 0x86efac;
const ACCENT: u32 = 0x22c55e;
const FAINT: u32 = 0x14401f;

const FONT_FAMILY: &str = "JetBrains Mono";
const FONT_SIZE: f32 = 14.0;
const CELL_H: f32 = 20.0;
const HEADER_H: f32 = 28.0;
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 4.0;

/// ANSI 16, tinted toward the phosphor identity (theme-file-driven in MVP).
const ANSI16: [u32; 16] = [
    0x0a0f0a, 0xff4444, 0x22c55e, 0xf5c542, 0x3b82f6, 0xc45ab3, 0x67e8f9, 0x86efac,
    0x14401f, 0xff7b7b, 0x4ade80, 0xfbe08a, 0x7aa5ff, 0xe08ad4, 0xa5f3fc, 0xecfff4,
];

fn idx_color(i: u8) -> Hsla {
    if (i as usize) < 16 {
        return rgb(ANSI16[i as usize]).into();
    }
    if i >= 232 {
        let v = 8 + 10 * (i - 232) as u32;
        return rgb(v << 16 | v << 8 | v).into();
    }
    let i = i - 16;
    let lv = |n: u8| -> u32 {
        if n == 0 { 0 } else { 55 + 40 * n as u32 }
    };
    let (r, g, b) = (lv(i / 36), lv((i / 6) % 6), lv(i % 6));
    rgb(r << 16 | g << 8 | b).into()
}

fn ansi_to_hsla(color: AnsiColor, default: Hsla) -> Hsla {
    match color {
        AnsiColor::Named(named) => match named {
            NamedColor::Foreground => rgb(TEXT).into(),
            NamedColor::Background => rgb(BG).into(),
            NamedColor::Cursor => rgb(ACCENT).into(),
            n => {
                let i = n as usize;
                if i < 16 { rgb(ANSI16[i]).into() } else { default }
            }
        },
        AnsiColor::Spec(c) => rgb((c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32).into(),
        AnsiColor::Indexed(i) => idx_color(i),
    }
}

struct TerminalView {
    focus_handle: FocusHandle,
    session: term::Session,
    title: String,
    exited: bool,
    grid: term::GridSize,
    cell_w: f32,
    scroll_accum: f32,
    selecting: bool,
}

impl TerminalView {
    fn new(cx: &mut Context<Self>) -> Self {
        let grid = term::GridSize { cols: 100, rows: 28 };
        let mut session =
            term::spawn(grid, 8, CELL_H as u16).expect("spawn shell");

        let mut events = session.events.take().expect("events taken once");
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                let keep_going = this
                    .update(cx, |view: &mut TerminalView, cx| {
                        view.handle_term_event(event, cx)
                    })
                    .unwrap_or(false);
                if !keep_going {
                    break;
                }
            }
        })
        .detach();

        Self {
            focus_handle: cx.focus_handle(),
            session,
            title: "shell".into(),
            exited: false,
            grid,
            cell_w: 8.4,
            scroll_accum: 0.,
            selecting: false,
        }
    }

    fn handle_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) -> bool {
        match event {
            TermEvent::Wakeup => cx.notify(),
            TermEvent::PtyWrite(text) => self.session.notifier.notify(text.into_bytes()),
            TermEvent::Title(title) => {
                self.title = title;
                cx.notify();
            }
            TermEvent::Exit | TermEvent::ChildExit(_) => {
                self.exited = true;
                cx.notify();
                return false;
            }
            _ => {}
        }
        true
    }

    /// Measure the real cell width once, then fit the grid to the window.
    fn sync_size(&mut self, window: &mut Window) {
        let font = grid_font(FontWeight::NORMAL);
        if let Ok(w) = window
            .text_system()
            .advance(window.text_system().resolve_font(&font), px(FONT_SIZE), 'M')
        {
            if f32::from(w.width) > 1.0 {
                self.cell_w = f32::from(w.width);
            }
        }
        let viewport = window.viewport_size();
        let cols = (((f32::from(viewport.width) - PAD_X * 2.) / self.cell_w).floor() as usize).max(20);
        let rows = (((f32::from(viewport.height) - HEADER_H - PAD_Y * 2.) / CELL_H).floor() as usize).max(5);
        if cols != self.grid.cols || rows != self.grid.rows {
            self.grid = term::GridSize { cols, rows };
            self.session
                .resize(self.grid, self.cell_w as u16, CELL_H as u16);
        }
    }

    fn cell_at(&self, pos: gpui::Point<Pixels>, display_offset: usize) -> (TermPoint, Side) {
        let x = ((f32::from(pos.x) - PAD_X) / self.cell_w).max(0.) as usize;
        let y = ((f32::from(pos.y) - HEADER_H - PAD_Y) / CELL_H).max(0.) as usize;
        let col = x.min(self.grid.cols.saturating_sub(1));
        let row = y.min(self.grid.rows.saturating_sub(1));
        let side = if ((f32::from(pos.x) - PAD_X) / self.cell_w).fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        (
            viewport_to_point(display_offset, TermPoint::new(row, Column(col))),
            side,
        )
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.exited {
            return;
        }
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        // clipboard chords first
        if m.control && m.shift {
            match ks.key.as_str() {
                "c" => {
                    let text = self.session.term.lock().selection_to_string();
                    if let Some(text) = text {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                    }
                    return;
                }
                "v" => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|i| i.text()) {
                        let bracketed = self
                            .session
                            .term
                            .lock()
                            .mode()
                            .contains(TermMode::BRACKETED_PASTE);
                        let bytes = if bracketed {
                            [b"\x1b[200~", text.as_bytes(), b"\x1b[201~"].concat()
                        } else {
                            text.into_bytes()
                        };
                        self.session.notifier.notify(bytes);
                    }
                    return;
                }
                _ => {}
            }
        }
        if let Some(bytes) = keystroke_bytes(ks) {
            {
                let mut term = self.session.term.lock();
                term.selection = None;
                term.scroll_display(Scroll::Bottom);
            }
            self.session.notifier.notify(bytes);
            cx.notify();
        }
    }

    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let dy = match ev.delta {
            gpui::ScrollDelta::Lines(l) => l.y * 3.0,
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / CELL_H,
        };
        self.scroll_accum += dy;
        let lines = self.scroll_accum.trunc() as i32;
        if lines != 0 {
            self.scroll_accum -= lines as f32;
            self.session.term.lock().scroll_display(Scroll::Delta(lines));
            cx.notify();
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let offset = self.session.term.lock().grid().display_offset();
        let (point, side) = self.cell_at(ev.position, offset);
        let ty = match ev.click_count {
            2 => SelectionType::Semantic,
            n if n >= 3 => SelectionType::Lines,
            _ => SelectionType::Simple,
        };
        self.session.term.lock().selection = Some(Selection::new(ty, point, side));
        self.selecting = true;
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting || !ev.pressed_button.map_or(false, |b| b == MouseButton::Left) {
            return;
        }
        let offset = self.session.term.lock().grid().display_offset();
        let (point, side) = self.cell_at(ev.position, offset);
        if let Some(sel) = self.session.term.lock().selection.as_mut() {
            sel.update(point, side);
        }
        cx.notify();
    }

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        self.selecting = false;
    }

    /// Snapshot the viewport into one styled line per row.
    fn styled_lines(&self) -> Vec<(String, Vec<TextRun>)> {
        let term = self.session.term.lock();
        let content = term.renderable_content();
        let display_offset = content.display_offset;
        let selection = content.selection;
        let cursor = content.cursor;
        let show_cursor = content.mode.contains(TermMode::SHOW_CURSOR);

        let mut lines: Vec<(String, Vec<TextRun>)> =
            (0..self.grid.rows).map(|_| (String::new(), vec![])).collect();

        for indexed in content.display_iter {
            let row = indexed.point.line.0 + display_offset as i32;
            if row < 0 || row as usize >= self.grid.rows {
                continue;
            }
            let cell = &indexed.cell;
            if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                continue;
            }
            let mut fg = ansi_to_hsla(cell.fg, rgb(TEXT).into());
            let mut bg: Option<Hsla> = match cell.bg {
                AnsiColor::Named(NamedColor::Background) => None,
                other => Some(ansi_to_hsla(other, rgb(BG).into())),
            };
            let mut flags = cell.flags;
            // selection highlight
            if selection.map_or(false, |s| s.contains(indexed.point)) {
                flags.insert(Flags::INVERSE);
            }
            // cursor block
            if show_cursor
                && cursor.point.line == indexed.point.line
                && cursor.point.column == indexed.point.column
            {
                flags.toggle(Flags::INVERSE);
            }
            if flags.contains(Flags::INVERSE) {
                let new_fg = bg.unwrap_or(rgb(BG).into());
                bg = Some(fg);
                fg = new_fg;
            }
            if flags.contains(Flags::DIM) {
                fg.a *= 0.6;
            }

            let weight = if flags.contains(Flags::BOLD) {
                FontWeight::BOLD
            } else {
                FontWeight::NORMAL
            };
            let underline = flags.contains(Flags::UNDERLINE).then(|| UnderlineStyle {
                thickness: px(1.),
                color: Some(fg),
                wavy: false,
            });

            let (text, runs) = &mut lines[row as usize];
            let ch = if cell.c == '\0' { ' ' } else { cell.c };
            let ch_len = ch.len_utf8();
            text.push(ch);

            let matches_last = runs.last().map_or(false, |r: &TextRun| {
                r.color == fg
                    && r.background_color == bg
                    && r.font.weight == weight
                    && r.underline.is_some() == underline.is_some()
            });
            if matches_last {
                runs.last_mut().unwrap().len += ch_len;
            } else {
                runs.push(TextRun {
                    len: ch_len,
                    font: grid_font(weight),
                    color: fg,
                    background_color: bg,
                    underline,
                    strikethrough: None,
                });
            }
        }
        lines
    }
}

fn grid_font(weight: FontWeight) -> Font {
    let mut f = font(FONT_FAMILY);
    f.weight = weight;
    f
}

/// gpui Keystroke → PTY bytes.
fn keystroke_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
    let m = &ks.modifiers;
    if m.control && ks.key.chars().count() == 1 {
        let c = ks.key.chars().next().unwrap().to_ascii_lowercase();
        if c.is_ascii_lowercase() {
            return Some(vec![c as u8 - b'a' + 1]);
        }
    }
    let seq: &[u8] = match ks.key.as_str() {
        "enter" => b"\r",
        "backspace" => &[0x7f],
        "tab" => b"\t",
        "escape" => &[0x1b],
        "up" => b"\x1b[A",
        "down" => b"\x1b[B",
        "right" => b"\x1b[C",
        "left" => b"\x1b[D",
        "home" => b"\x1b[H",
        "end" => b"\x1b[F",
        "pageup" => b"\x1b[5~",
        "pagedown" => b"\x1b[6~",
        "delete" => b"\x1b[3~",
        "space" => b" ",
        _ => return ks.key_char.as_ref().map(|s| s.as_bytes().to_vec()),
    };
    Some(seq.to_vec())
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_size(window);
        let lines = self.styled_lines();
        let status = if self.exited { "exited" } else { "live" };
        let grid_label = format!("{}×{} · {status}", self.grid.cols, self.grid.rows);

        div()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .bg(rgb(BG))
            .flex()
            .flex_col()
            .font_family(FONT_FAMILY)
            .text_size(px(FONT_SIZE))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .h(px(HEADER_H))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .bg(rgb(SURFACE))
                    .border_b_1()
                    .border_color(rgb(FAINT))
                    .text_color(rgb(ACCENT))
                    .child(format!("▸ {}", self.title))
                    .child(grid_label),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .px(px(PAD_X))
                    .py(px(PAD_Y))
                    .flex()
                    .flex_col()
                    .children(lines.into_iter().map(|(text, runs)| {
                        let line = div().h(px(CELL_H)).whitespace_nowrap();
                        if text.is_empty() {
                            line
                        } else {
                            line.child(StyledText::new(text).with_runs(runs))
                        }
                    })),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1100.), px(640.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("terminal-delight".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(TerminalView::new);
                window.focus(&view.focus_handle(cx), cx);
                view
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}
