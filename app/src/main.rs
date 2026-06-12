//! G0d — CRT-lite visual identity from GPUI primitives, driven by a
//! hot-reloaded TOML theme (edit themes while running, no recompile).
//! G0e — latency probe: TD_LATENCY=1 prints key→echo-rendered micros.

mod term;
mod theme;

use std::time::Instant;

use alacritty_terminal::{
    event::{Event as TermEvent, Notify},
    grid::Scroll,
    index::{Column, Point as TermPoint, Side},
    selection::{Selection, SelectionType},
    term::{TermMode, cell::Flags, viewport_to_point},
    vte::ansi::{Color as AnsiColor, NamedColor},
};
use futures::StreamExt;
use gpui::{
    App, Bounds, BoxShadow, ClipboardItem, Context, FocusHandle, Focusable, Font, FontWeight,
    Hsla, KeyDownEvent, Keystroke, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, ScrollWheelEvent, StyledText, TextRun, TitlebarOptions, UnderlineStyle, Window,
    WindowBounds, WindowOptions, canvas, div, fill, font, hsla, linear_color_stop,
    linear_gradient, point, prelude::*, px, rgb, size,
};
use gpui_platform::application;
use theme::Theme;

const HEADER_H: f32 = 28.0;
const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 4.0;

fn idx_color(i: u8, th: &Theme) -> Hsla {
    if (i as usize) < 16 {
        return th.ansi[i as usize];
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

fn ansi_to_hsla(color: AnsiColor, th: &Theme, default: Hsla) -> Hsla {
    match color {
        AnsiColor::Named(named) => match named {
            NamedColor::Foreground => th.text,
            NamedColor::Background => th.bg,
            NamedColor::Cursor => th.cursor,
            n => {
                let i = n as usize;
                if i < 16 { th.ansi[i] } else { default }
            }
        },
        AnsiColor::Spec(c) => rgb((c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32).into(),
        AnsiColor::Indexed(i) => idx_color(i, th),
    }
}

struct TerminalView {
    focus_handle: FocusHandle,
    session: term::Session,
    title: String,
    exited: bool,
    grid: term::GridSize,
    cell_w: f32,
    cell_h: f32,
    scroll_accum: f32,
    selecting: bool,
    pending_input: Option<Instant>,
    latency_log: bool,
}

impl TerminalView {
    fn new(cx: &mut Context<Self>) -> Self {
        let grid = term::GridSize { cols: 100, rows: 28 };
        let mut session = term::spawn(grid, 8, 20).expect("spawn shell");

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
            cell_h: 20.,
            scroll_accum: 0.,
            selecting: false,
            pending_input: None,
            latency_log: std::env::var("TD_LATENCY").is_ok(),
        }
    }

    fn handle_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) -> bool {
        match event {
            TermEvent::Wakeup => {
                if let Some(t) = self.pending_input.take() {
                    if self.latency_log {
                        eprintln!("td_latency_us={}", t.elapsed().as_micros());
                    }
                }
                cx.notify();
            }
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

    /// Measure the real cell metrics from the active theme, fit grid to window.
    fn sync_size(&mut self, th: &Theme, window: &mut Window) {
        self.cell_h = th.cell_h;
        let font = grid_font(th, FontWeight::NORMAL);
        if let Ok(w) = window.text_system().advance(
            window.text_system().resolve_font(&font),
            px(th.font_size),
            'M',
        ) {
            if f32::from(w.width) > 1.0 {
                self.cell_w = f32::from(w.width);
            }
        }
        let viewport = window.viewport_size();
        let cols =
            (((f32::from(viewport.width) - PAD_X * 2.) / self.cell_w).floor() as usize).max(20);
        let rows = (((f32::from(viewport.height) - HEADER_H - PAD_Y * 2.) / self.cell_h).floor()
            as usize)
            .max(5);
        if cols != self.grid.cols || rows != self.grid.rows {
            self.grid = term::GridSize { cols, rows };
            self.session
                .resize(self.grid, self.cell_w as u16, self.cell_h as u16);
        }
    }

    fn cell_at(&self, pos: gpui::Point<Pixels>, display_offset: usize) -> (TermPoint, Side) {
        let fx = (f32::from(pos.x) - PAD_X) / self.cell_w;
        let y = ((f32::from(pos.y) - HEADER_H - PAD_Y) / self.cell_h).max(0.) as usize;
        let col = (fx.max(0.) as usize).min(self.grid.cols.saturating_sub(1));
        let row = y.min(self.grid.rows.saturating_sub(1));
        let side = if fx.fract() < 0.5 { Side::Left } else { Side::Right };
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
            self.pending_input = Some(Instant::now());
            self.session.notifier.notify(bytes);
            cx.notify();
        }
    }

    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let dy = match ev.delta {
            gpui::ScrollDelta::Lines(l) => l.y * 3.0,
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / self.cell_h,
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
    fn styled_lines(&self, th: &Theme) -> Vec<(String, Vec<TextRun>)> {
        let term = self.session.term.lock();
        let content = term.renderable_content();
        let display_offset = content.display_offset;
        let selection = content.selection;
        let cursor = content.cursor;
        let show_cursor = content.mode.contains(TermMode::SHOW_CURSOR) && display_offset == 0;

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
            let mut fg = ansi_to_hsla(cell.fg, th, th.text);
            let mut bg: Option<Hsla> = match cell.bg {
                AnsiColor::Named(NamedColor::Background) => None,
                other => Some(ansi_to_hsla(other, th, th.bg)),
            };
            let mut flags = cell.flags;
            if selection.map_or(false, |s| s.contains(indexed.point)) {
                flags.insert(Flags::INVERSE);
            }
            if flags.contains(Flags::INVERSE) {
                let new_fg = bg.unwrap_or(th.bg);
                bg = Some(fg);
                fg = new_fg;
            }
            // themed block cursor on top of everything
            if show_cursor
                && cursor.point.line == indexed.point.line
                && cursor.point.column == indexed.point.column
            {
                bg = Some(th.cursor);
                fg = th.bg;
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
                    font: grid_font(th, weight),
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

fn grid_font(th: &Theme, weight: FontWeight) -> Font {
    let mut f = font(th.font_family.clone());
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

/// One composited scanline overlay (quads only — stays off the input path).
fn scanlines(th: &Theme) -> impl IntoElement {
    let alpha = th.scanline_opacity;
    let step = th.scanline_step;
    div().absolute().inset_0().child(
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                if alpha <= 0.001 {
                    return;
                }
                let color = hsla(0., 0., 0., alpha);
                let mut y = f32::from(bounds.origin.y);
                let bottom = f32::from(bounds.bottom());
                while y < bottom {
                    window.paint_quad(fill(
                        Bounds::new(point(bounds.origin.x, px(y)), size(bounds.size.width, px(1.))),
                        color,
                    ));
                    y += step;
                }
            },
        )
        .size_full(),
    )
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let th = theme::theme(cx);
        self.sync_size(&th, window);
        let lines = self.styled_lines(&th);
        let status = if self.exited { "exited" } else { "live" };
        let grid_label = format!("{}×{} · {} · {status}", self.grid.cols, self.grid.rows, th.name);
        let vignette = th.vignette;
        let glow = th.glow;
        let dark = |a: f32| hsla(0., 0., 0., a);

        let mut header = div()
            .h(px(HEADER_H))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .bg(th.surface)
            .border_b_1()
            .border_color(th.faint)
            .text_color(th.accent)
            .child(format!("▸ {}", self.title))
            .child(grid_label);
        if glow > 0.001 {
            header = header.shadow(
                vec![BoxShadow {
                    color: th.accent.alpha(glow * 0.5),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(16.),
                    spread_radius: px(0.),
                    inset: false,
                }]
                .into(),
            );
        }

        div()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .bg(th.bg)
            .flex()
            .flex_col()
            .font_family(th.font_family.clone())
            .text_size(px(th.font_size))
            .text_color(th.text)
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .px(px(PAD_X))
                            .py(px(PAD_Y))
                            .flex()
                            .flex_col()
                            .children(lines.into_iter().map(|(text, runs)| {
                                let line = div().h(px(self.cell_h)).whitespace_nowrap();
                                if text.is_empty() {
                                    line
                                } else {
                                    line.child(StyledText::new(text).with_runs(runs))
                                }
                            })),
                    )
                    .child(scanlines(&th))
                    .when(vignette > 0.001, |el| {
                        el.child(
                            div().absolute().top_0().left_0().right_0().h(px(70.)).bg(
                                linear_gradient(
                                    180.,
                                    linear_color_stop(dark(vignette * 0.45), 0.),
                                    linear_color_stop(dark(0.), 1.),
                                ),
                            ),
                        )
                        .child(
                            div().absolute().bottom_0().left_0().right_0().h(px(70.)).bg(
                                linear_gradient(
                                    180.,
                                    linear_color_stop(dark(0.), 0.),
                                    linear_color_stop(dark(vignette * 0.55), 1.),
                                ),
                            ),
                        )
                    }),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        theme::init(cx);
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
