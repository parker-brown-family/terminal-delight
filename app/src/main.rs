//! G0b — a REAL shell in a GPUI window.
//! Grid rendered from alacritty_terminal state; keystrokes → PTY bytes.
//! Monochrome phosphor pass (per-cell color lands with G0d's CRT-lite).

mod term;

use alacritty_terminal::event::{Event as TermEvent, Notify};
use futures::StreamExt;
use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, KeyDownEvent, Keystroke, SharedString,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

// hacker palette tokens (browser reference: src/styles/theme.css)
const BG: u32 = 0x050706;
const SURFACE: u32 = 0x08100d;
const TEXT: u32 = 0x86efac;
const ACCENT: u32 = 0x22c55e;
const FAINT: u32 = 0x14401f;

const COLS: usize = 100;
const ROWS: usize = 28;
const CELL_W: f32 = 8.4; // JetBrains Mono @14px, approximate; real metrics in G0c
const CELL_H: f32 = 20.0;

struct TerminalView {
    focus_handle: FocusHandle,
    session: term::Session,
    title: String,
    exited: bool,
}

impl TerminalView {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut session = term::spawn(
            term::GridSize {
                cols: COLS,
                rows: ROWS,
            },
            CELL_W as u16,
            CELL_H as u16,
        )
        .expect("spawn shell");

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
        }
    }

    fn handle_term_event(&mut self, event: TermEvent, cx: &mut Context<Self>) -> bool {
        match event {
            TermEvent::Wakeup => cx.notify(),
            // Query responses (DA/DSR/color) must be bounced back to the PTY by us.
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

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        if self.exited {
            return;
        }
        if let Some(bytes) = keystroke_bytes(&ev.keystroke) {
            self.session.notifier.notify(bytes);
        }
    }

    /// Snapshot the grid as one String per visible line.
    fn grid_lines(&self) -> Vec<String> {
        let term = self.session.term.lock();
        let content = term.renderable_content();
        let mut lines = vec![String::with_capacity(COLS); ROWS];
        for indexed in content.display_iter {
            let row = indexed.point.line.0;
            if row >= 0 && (row as usize) < ROWS {
                lines[row as usize].push(indexed.cell.c);
            }
        }
        // cheap cursor: mark the cell (real cursor rendering lands in G0c/G0d)
        let cur = content.cursor.point;
        if let Some(line) = lines.get_mut(cur.line.0.max(0) as usize) {
            let col = cur.column.0;
            if col < line.len() {
                let byte = line
                    .char_indices()
                    .nth(col)
                    .map(|(i, c)| (i, c.len_utf8()));
                if let Some((i, len)) = byte {
                    line.replace_range(i..i + len, "█");
                }
            } else {
                while line.len() < col {
                    line.push(' ');
                }
                line.push('█');
            }
        }
        lines
    }
}

/// gpui Keystroke → PTY bytes (G0b coverage: printable + the keys vim/htop need).
fn keystroke_bytes(ks: &Keystroke) -> Option<Vec<u8>> {
    let m = &ks.modifiers;
    if m.control && ks.key.chars().count() == 1 {
        let c = ks.key.chars().next().unwrap().to_ascii_lowercase();
        if c.is_ascii_lowercase() {
            return Some(vec![c as u8 - b'a' + 1]); // ctrl-a..z incl. ctrl-c/d/z
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let lines = self.grid_lines();
        let status = if self.exited { "exited" } else { "live" };
        div()
            .track_focus(&self.focus_handle(cx))
            .on_key_down(cx.listener(Self::on_key))
            .size_full()
            .bg(rgb(BG))
            .flex()
            .flex_col()
            .font_family("JetBrains Mono")
            .text_size(px(14.))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .px_3()
                    .py_1()
                    .bg(rgb(SURFACE))
                    .border_b_1()
                    .border_color(rgb(FAINT))
                    .text_color(rgb(ACCENT))
                    .child(format!("▸ {} — real shell (G0b)", self.title))
                    .child(format!("alacritty_terminal · {status}")),
            )
            .child(
                div().flex_1().px_2().py_1().flex().flex_col().children(
                    lines
                        .into_iter()
                        .map(|l| div().h(px(CELL_H)).child(SharedString::from(l))),
                ),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(
            None,
            size(
                px(COLS as f32 * CELL_W + 24.),
                px(ROWS as f32 * CELL_H + 60.),
            ),
            cx,
        );
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
