//! MVP 0.1 — two-pane real terminal.
//! ctrl+alt+r / ctrl+alt+d: split · alt+←/→: switch focus · pane closes when
//! its shell exits (last one quits the app) · pane count restores on launch.

mod crt;
mod pane;
mod term;
mod theme;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{
    App, Bounds, Context, Entity, Focusable, KeyDownEvent, TitlebarOptions, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;
use pane::TerminalView;

const MAX_PANES: usize = 2; // MVP 0.1 scope; tree tiling lands in 0.2+

fn state_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/terminal-delight/state.toml")
}

fn load_pane_count() -> usize {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.trim_start().starts_with("panes"))
                .and_then(|l| l.split('=').nth(1))
                .and_then(|v| v.trim().parse::<usize>().ok())
        })
        .unwrap_or(1)
        .clamp(1, MAX_PANES)
}

fn save_pane_count(n: usize) {
    let path = state_path();
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let _ = fs::write(path, format!("panes = {}\n", n.max(1)));
}

struct Workspace {
    panes: Vec<Entity<TerminalView>>,
    fx: crt::Fx,
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut ws = Self {
            panes: vec![],
            fx: crt::Fx::new(),
        };
        for _ in 0..load_pane_count() {
            ws.add_pane(window, cx);
        }
        // effects clock: ~30fps while a sweep/jiggle runs, lazy when idle
        let fx_debug = std::env::var("TD_FXDEBUG").is_ok();
        cx.spawn(async move |this, cx| {
            let mut n: u64 = 0;
            loop {
                n += 1;
                let result = this.update(cx, |ws: &mut Workspace, cx| {
                    let th = theme::theme(cx);
                    let changed = ws.fx.tick(&th);
                    if changed {
                        cx.notify();
                    }
                    (ws.fx.active(), changed, ws.fx.band, ws.fx.flicker_mul)
                });
                if fx_debug && n % 30 == 0 {
                    eprintln!("fx tick#{n}: {result:?}");
                }
                let active = result.map(|r| r.0).unwrap_or(false);
                let ms = if active { 33 } else { 120 };
                cx.background_executor()
                    .timer(Duration::from_millis(ms))
                    .await;
            }
        })
        .detach();
        ws
    }

    fn add_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() >= MAX_PANES {
            return;
        }
        let pane = cx.new(TerminalView::new);
        // pane notifies on every term event; re-render lets reap() see exits
        cx.observe(&pane, |_, _, cx| cx.notify()).detach();
        window.focus(&pane.focus_handle(cx), cx);
        self.panes.push(pane);
        save_pane_count(self.panes.len());
        cx.notify();
    }

    fn focused_index(&self, window: &Window, cx: &App) -> usize {
        self.panes
            .iter()
            .position(|p| p.focus_handle(cx).is_focused(window))
            .unwrap_or(0)
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        if m.control && m.alt {
            if matches!(ks.key.as_str(), "r" | "d") {
                self.add_pane(window, cx);
            }
            return;
        }
        if m.alt && !m.control && self.panes.len() > 1 {
            let dir: i32 = match ks.key.as_str() {
                "left" => -1,
                "right" => 1,
                _ => return,
            };
            let cur = self.focused_index(window, cx) as i32;
            let next = (cur + dir).rem_euclid(self.panes.len() as i32) as usize;
            window.focus(&self.panes[next].focus_handle(cx), cx);
            cx.notify();
        }
    }

    /// Drop panes whose shells exited; quit when the last one goes.
    fn reap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let before = self.panes.len();
        self.panes.retain(|p| !p.read(cx).exited);
        if self.panes.len() != before {
            save_pane_count(self.panes.len().max(1));
            if self.panes.is_empty() {
                cx.quit();
            } else {
                window.focus(&self.panes[0].focus_handle(cx), cx);
            }
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reap(window, cx);
        let th = theme::theme(cx);
        let focused = self.focused_index(window, cx);
        let jiggle = self.fx.jiggle_px;
        div()
            .size_full()
            .bg(th.bg)
            .relative()
            .on_key_down(cx.listener(Self::on_key))
            .child(
                // pane row; jiggle = the CRT vertical-hold hop
                div()
                    .size_full()
                    .flex()
                    .flex_row()
                    .gap(px(3.))
                    .pt(px(3. + jiggle.max(0.)))
                    .pb(px(3. + (-jiggle).max(0.)))
                    .px(px(3.))
                    .children(self.panes.iter().enumerate().map(|(i, p)| {
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .rounded_md()
                            .border_1()
                            .border_color(if i == focused {
                                th.accent.alpha(0.55)
                            } else {
                                th.faint
                            })
                            .child(p.clone())
                    })),
            )
            .when(std::env::var("TD_NOGLASS").is_err(), |el| {
                el.child(crt::glass(&th, &self.fx))
            })
    }
}

fn main() {
    application().run(|cx: &mut App| {
        theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(700.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("terminal-delight".into()),
                    ..Default::default()
                }),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Workspace::new(window, cx)),
        )
        .expect("open window");
        cx.activate(true);
    });
}
