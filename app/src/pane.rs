//! TerminalView — one pane: a real shell with themed rendering, selection,
//! scrollback, clipboard, CRT-lite effects, and the TD_LATENCY probe.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    Pixels, ScrollWheelEvent, StyledText, TextRun, UnderlineStyle, Window, canvas, div, fill,
    font, hsla, linear_color_stop, linear_gradient, point, prelude::*, px, rgb, size,
};
use crate::crt;
use crate::term;
use crate::theme::{self, Theme};

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

pub struct TerminalView {
    focus_handle: FocusHandle,
    session: term::Session,
    pub title: String,
    pub exited: bool,
    grid: term::GridSize,
    cell_w: f32,
    cell_h: f32,
    scroll_accum: f32,
    selecting: bool,
    pending_input: Option<Instant>,
    latency_log: bool,
    /// Written by the measuring canvas during prepaint; read by sync_size.
    content_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    spawned: Instant,
    /// This pane's own CRT rhythm — desynced from every other pane.
    pub fx: crt::Fx,
    /// Barrel coefficients (from the theme's curvature dial); used to
    /// warp-correct mouse-to-cell mapping so selection lands where you look.
    warp_k: (f32, f32),
}

impl TerminalView {
    pub fn new(cx: &mut Context<Self>) -> Self {
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

        // per-pane effects clock
        cx.spawn(async move |this, cx| {
            loop {
                let active = this
                    .update(cx, |view: &mut TerminalView, cx| {
                        let th = theme::theme(cx);
                        if view.fx.tick(&th) {
                            cx.notify();
                        }
                        view.fx.active()
                    })
                    .unwrap_or(false);
                let ms = if active { 33 } else { 150 };
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(ms))
                    .await;
            }
        })
        .detach();

        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ d.as_secs())
            .unwrap_or(7);
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
            content_bounds: Arc::new(Mutex::new(None)),
            spawned: Instant::now(),
            fx: crt::Fx::new(seed),
            warp_k: (0., 0.),
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
    fn sync_size(&mut self, th: &Theme, scale: f32, window: &mut Window) {
        self.cell_h = th.cell_h * scale;
        let font = grid_font(th, FontWeight::NORMAL);
        if let Ok(w) = window.text_system().advance(
            window.text_system().resolve_font(&font),
            px(th.font_size * scale),
            'M',
        ) {
            if f32::from(w.width) > 1.0 {
                self.cell_w = f32::from(w.width);
            }
        }
        let stored = *self.content_bounds.lock().unwrap();
        let (avail_w, avail_h) = match stored {
            Some(b) => (
                f32::from(b.size.width) - PAD_X * 2.,
                f32::from(b.size.height) - PAD_Y * 2.,
            ),
            None => {
                let viewport = window.viewport_size();
                (
                    f32::from(viewport.width) - PAD_X * 2.,
                    f32::from(viewport.height) - HEADER_H - PAD_Y * 2.,
                )
            }
        };
        let cols = ((avail_w / self.cell_w).floor() as usize).max(10);
        let rows = ((avail_h / self.cell_h).floor() as usize).max(3);
        if cols != self.grid.cols || rows != self.grid.rows {
            self.grid = term::GridSize { cols, rows };
            self.session
                .resize(self.grid, self.cell_w as u16, self.cell_h as u16);
        }
    }

    fn cell_at(&self, pos: gpui::Point<Pixels>, display_offset: usize) -> (TermPoint, Side) {
        let bounds = *self.content_bounds.lock().unwrap();
        let (bx, by, bw, bh) = match bounds {
            Some(b) => (
                f32::from(b.origin.x),
                f32::from(b.origin.y),
                f32::from(b.size.width).max(1.),
                f32::from(b.size.height).max(1.),
            ),
            None => (0., HEADER_H, 1000., 1000.),
        };
        // in-rect coordinates; apply the same forward barrel the shader uses,
        // because a screen point displays content sampled from warped(point)
        let mut sx = f32::from(pos.x) - bx;
        let mut sy = f32::from(pos.y) - by;
        let (k1, k2) = self.warp_k;
        if k1.abs() > 0.0005 {
            let cu = sx / bw - 0.5;
            let cv = sy / bh - 0.5;
            let r2 = cu * cu + cv * cv;
            let f = 1.0 + k1 * r2 + k2 * r2 * r2;
            sx = (0.5 + cu * f) * bw;
            sy = (0.5 + cv * f) * bh;
        }
        let fx = (sx - PAD_X) / self.cell_w;
        let y = ((sy - PAD_Y) / self.cell_h).max(0.) as usize;
        let col = (fx.max(0.) as usize).min(self.grid.cols.saturating_sub(1));
        let row = y.min(self.grid.rows.saturating_sub(1));
        let side = if fx.fract() < 0.5 { Side::Left } else { Side::Right };
        (
            viewport_to_point(display_offset, TermPoint::new(row, Column(col))),
            side,
        )
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.exited || self.spawned.elapsed() < Duration::from_millis(150) {
            return;
        }
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        if m.control && m.shift {
            match ks.key.as_str() {
                // workspace chords: new tab
                "t" => return,
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
        if ev.modifiers.control {
            return; // workspace handles ctrl+wheel = text-size scrub
        }
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
    if m.alt {
        // alt+arrows switch panes; ctrl+alt chords split — both owned by Workspace
        if matches!(ks.key.as_str(), "left" | "right" | "up" | "down") || m.control {
            return None;
        }
        // other alt+<char>: ESC prefix for readline (alt+b, alt+f, alt+.)
        let base = ks.key_char.as_ref().map(|s| s.as_bytes().to_vec())?;
        let mut out = vec![0x1b];
        out.extend(base);
        return Some(out);
    }
    if m.control && ks.key.chars().count() == 1 {
        let c = ks.key.chars().next().unwrap().to_ascii_lowercase();
        if c.is_ascii_lowercase() {
            return Some(vec![c as u8 - b'a' + 1]);
        }
    }
    if m.control && matches!(ks.key.as_str(), "pageup" | "pagedown") {
        return None; // workspace: tab switching
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
        let th = theme::theme(cx);
        let scale = cx
            .try_global::<crate::UiScale>()
            .map(|s| s.0)
            .unwrap_or(1.0);
        self.sync_size(&th, scale, window);
        self.warp_k = (th.curvature * 0.14, th.curvature * 0.06);
        let lines = self.styled_lines(&th);
        let status = if self.exited { "exited" } else { "live" };
        let grid_label = format!("{}×{} · {} · {status}", self.grid.cols, self.grid.rows, th.name);
        let glow = th.glow;

        // solid, reflective header: gradient face + crisp top reflection line
        let mut lighter = th.surface;
        lighter.l = (lighter.l * 1.9).min(0.9);
        let mut header = div()
            .h(px(HEADER_H))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .bg(linear_gradient(
                180.,
                linear_color_stop(lighter, 0.),
                linear_color_stop(th.surface, 1.),
            ))
            .border_b_1()
            .border_color(th.accent.alpha(0.5))
            .text_color(th.accent)
            .child(format!("▸ {}", self.title))
            .child(grid_label);
        {
            let mut shadows = vec![
                // the reflection: bright inner top edge
                BoxShadow {
                    color: gpui::white().alpha(0.16),
                    offset: point(px(1.), px(1.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: true,
                },
            ];
            if glow > 0.001 {
                shadows.push(BoxShadow {
                    color: th.accent.alpha(glow * 0.5),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(16.),
                    spread_radius: px(0.),
                    inset: false,
                });
            }
            header = header.shadow(shadows.into());
        }

        let jiggle = self.fx.jiggle_px;
        div()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .size_full()
            .bg(th.bg)
            .relative()
            .flex()
            .flex_col()
            .font_family(th.font_family.clone())
            .text_size(px(th.font_size * scale))
            .text_color(th.text)
            .pt(px(jiggle.max(0.)))
            .pb(px((-jiggle).max(0.)))
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .child({
                        let store = self.content_bounds.clone();
                        let weak = cx.entity().downgrade();
                        div().absolute().inset_0().child(
                            canvas(
                                move |bounds, window, cx| {
                                    let sf = window.scale_factor();
                                    crate::warp::register([
                                        f32::from(bounds.origin.x) * sf,
                                        f32::from(bounds.origin.y) * sf,
                                        f32::from(bounds.size.width) * sf,
                                        f32::from(bounds.size.height) * sf,
                                    ]);
                                    let changed = {
                                        let mut slot = store.lock().unwrap();
                                        let changed = slot.map_or(true, |b| b != bounds);
                                        if changed {
                                            *slot = Some(bounds);
                                        }
                                        changed
                                    };
                                    if changed {
                                        let weak = weak.clone();
                                        cx.defer(move |cx| {
                                            let _ = weak.update(cx, |_, cx| cx.notify());
                                        });
                                    }
                                },
                                |_, _, _, _| {},
                            )
                            .size_full(),
                        )
                    })
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
                    ),
            )
            .when(std::env::var("TD_NOGLASS").is_err(), |el| {
                el.child(crt::glass(&th, &self.fx))
            })
    }
}

