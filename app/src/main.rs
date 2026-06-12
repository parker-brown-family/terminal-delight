//! G0a spike — GPUI window + fixed-grid monospace text + keyboard input echo.
//! Proves the substrate on this box (X11 + NVIDIA + wgpu renderer).
//! NOT a shell (that's G0b) — keys echo into the grid to prove the input→render loop.

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, KeyDownEvent, SharedString, TitlebarOptions,
    Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};
use gpui_platform::application;

// hacker palette tokens (from the browser reference, src/styles/theme.css)
const BG: u32 = 0x050706;
const SURFACE: u32 = 0x08100d;
const TEXT: u32 = 0x86efac;
const ACCENT: u32 = 0x22c55e;
const FAINT: u32 = 0x14401f;

struct GridSpike {
    focus_handle: FocusHandle,
    lines: Vec<String>,
}

impl GridSpike {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            lines: vec![
                "terminal-delight :: G0a substrate spike".into(),
                "gpui (wgpu renderer) / X11 / RTX 3080 — if you can read this, the gate is half-open".into(),
                "type to echo · enter = new line · backspace works".into(),
                "".into(),
                "$ ".into(),
            ],
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let last = self.lines.last_mut().expect("grid never empty");
        match ks.key.as_str() {
            "enter" => self.lines.push("$ ".into()),
            "backspace" => {
                if last.len() > 2 {
                    last.pop();
                }
            }
            "space" => last.push(' '),
            _ => {
                if let Some(ch) = ks.key_char.as_ref() {
                    last.push_str(ch);
                } else if ks.key.chars().count() == 1
                    && !ks.modifiers.control
                    && !ks.modifiers.platform
                {
                    last.push_str(&ks.key);
                }
            }
        }
        cx.notify();
    }
}

impl Focusable for GridSpike {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for GridSpike {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                // pane header, echoing the chrome from the browser reference
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
                    .child("▸ g0a — grid + input spike")
                    .child("gpui/wgpu"),
            )
            .child(
                div().flex_1().p_3().flex().flex_col().children(
                    self.lines
                        .iter()
                        .map(|l| div().h(px(20.)).child(SharedString::from(l.clone()))),
                ),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.), px(640.)), cx);
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
                let grid = cx.new(GridSpike::new);
                window.focus(&grid.focus_handle(cx), cx);
                grid
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}
