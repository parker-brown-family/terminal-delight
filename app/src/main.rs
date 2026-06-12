//! terminal-delight — tabs · splits · device bezel.
//!
//! ctrl+shift+t: new tab · ctrl+pgup/pgdn: switch tab
//! ctrl+alt+r: split right · ctrl+alt+d: split down · alt+arrows: switch pane
//! Triple-button cluster (Tilix-style): [+ tab] [split right] [split down]
//! Panes close when their shell exits; empty tab closes; last tab quits.
//! Layout (tabs, panes, direction) persists and restores.
//!
//! TODO(os-chrome): theme the OS window itself — gpui supports client-side
//! decorations (WindowDecorations::Client); revisit after 0.2.

mod crt;
mod pane;
mod term;
mod theme;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use gpui::{
    App, Bounds, Context, Entity, Focusable, Hsla, KeyDownEvent, MouseButton, MouseDownEvent,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, prelude::*, px, size,
};
use gpui_platform::application;
use pane::TerminalView;
use serde::{Deserialize, Serialize};

const MAX_PANES: usize = 2; // per tab; full tiling tree lands in 0.2

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
enum SplitDir {
    Row,
    Col,
}

struct Tab {
    panes: Vec<Entity<TerminalView>>,
    dir: SplitDir,
}

#[derive(Serialize, Deserialize, Default)]
struct StateFile {
    active: usize,
    tabs: Vec<TabState>,
}

#[derive(Serialize, Deserialize)]
struct TabState {
    panes: usize,
    dir: SplitDir,
}

fn state_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/terminal-delight/state.toml")
}

fn load_state() -> StateFile {
    fs::read_to_string(state_path())
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    fx: crt::Fx,
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut ws = Self {
            tabs: vec![],
            active: 0,
            fx: crt::Fx::new(),
        };
        let saved = load_state();
        if saved.tabs.is_empty() {
            ws.new_tab(window, cx);
        } else {
            for t in &saved.tabs {
                let dir = t.dir;
                ws.tabs.push(Tab { panes: vec![], dir });
                ws.active = ws.tabs.len() - 1;
                for _ in 0..t.panes.clamp(1, MAX_PANES) {
                    ws.spawn_pane(window, cx);
                }
            }
            ws.active = saved.active.min(ws.tabs.len().saturating_sub(1));
            ws.focus_active(window, cx);
        }
        // effects clock: frame-rate only while something animates
        cx.spawn(async move |this, cx| {
            loop {
                let active = this
                    .update(cx, |ws: &mut Workspace, cx| {
                        let th = theme::theme(cx);
                        if ws.fx.tick(&th) {
                            cx.notify();
                        }
                        ws.fx.active()
                    })
                    .unwrap_or(false);
                let ms = if active { 33 } else { 120 };
                cx.background_executor()
                    .timer(Duration::from_millis(ms))
                    .await;
            }
        })
        .detach();
        ws
    }

    fn save(&self) {
        let state = StateFile {
            active: self.active,
            tabs: self
                .tabs
                .iter()
                .map(|t| TabState {
                    panes: t.panes.len().max(1),
                    dir: t.dir,
                })
                .collect(),
        };
        if let Ok(body) = toml::to_string(&state) {
            if let Some(dir) = state_path().parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(state_path(), body);
        }
    }

    fn spawn_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active = self.active;
        let Some(tab) = self.tabs.get_mut(active) else {
            return;
        };
        if tab.panes.len() >= MAX_PANES {
            return;
        }
        if std::env::var("TD_KEYDEBUG").is_ok() {
            eprintln!("spawn_pane: tab {} panes {}", active, self.tabs[active].panes.len());
        }
        let pane = cx.new(TerminalView::new);
        cx.observe(&pane, |_, _, cx| cx.notify()).detach();
        window.focus(&pane.focus_handle(cx), cx);
        self.tabs[active].panes.push(pane);
        self.save();
        cx.notify();
    }

    fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.tabs.push(Tab {
            panes: vec![],
            dir: SplitDir::Row,
        });
        self.active = self.tabs.len() - 1;
        self.spawn_pane(window, cx);
    }

    fn split(&mut self, dir: SplitDir, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.dir = dir;
        }
        self.spawn_pane(window, cx);
    }

    fn activate_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i < self.tabs.len() {
            self.active = i;
            self.focus_active(window, cx);
            self.save();
            cx.notify();
        }
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active) {
            if let Some(p) = tab.panes.first() {
                window.focus(&p.focus_handle(cx), cx);
            }
        }
    }

    fn focused_pane(&self, window: &Window, cx: &App) -> usize {
        self.tabs
            .get(self.active)
            .map(|t| {
                t.panes
                    .iter()
                    .position(|p| p.focus_handle(cx).is_focused(window))
                    .unwrap_or(0)
            })
            .unwrap_or(0)
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        if std::env::var("TD_KEYDEBUG").is_ok() {
            eprintln!("ws on_key: key={:?} ctrl={} alt={} shift={}", ks.key, m.control, m.alt, m.shift);
        }
        if m.control && m.shift && ks.key.as_str() == "t" {
            self.new_tab(window, cx);
            return;
        }
        if m.control && !m.alt && self.tabs.len() > 1 {
            match ks.key.as_str() {
                "pageup" => {
                    let n = self.tabs.len();
                    self.activate_tab((self.active + n - 1) % n, window, cx);
                    return;
                }
                "pagedown" => {
                    let n = self.tabs.len();
                    self.activate_tab((self.active + 1) % n, window, cx);
                    return;
                }
                _ => {}
            }
        }
        if m.control && m.alt {
            match ks.key.as_str() {
                "r" => self.split(SplitDir::Row, window, cx),
                "d" => self.split(SplitDir::Col, window, cx),
                _ => {}
            }
            return;
        }
        if m.alt && !m.control {
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            if tab.panes.len() > 1 {
                let dir: i32 = match ks.key.as_str() {
                    "left" | "up" => -1,
                    "right" | "down" => 1,
                    _ => return,
                };
                let cur = self.focused_pane(window, cx) as i32;
                let next = (cur + dir).rem_euclid(tab.panes.len() as i32) as usize;
                window.focus(&tab.panes[next].focus_handle(cx), cx);
                cx.notify();
            }
        }
    }

    /// Drop exited panes; drop empty tabs; quit when none remain.
    fn reap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut dirty = false;
        for tab in &mut self.tabs {
            let before = tab.panes.len();
            tab.panes.retain(|p| !p.read(cx).exited);
            dirty |= tab.panes.len() != before;
        }
        let before_tabs = self.tabs.len();
        self.tabs.retain(|t| !t.panes.is_empty());
        dirty |= self.tabs.len() != before_tabs;
        if dirty {
            if self.tabs.is_empty() {
                cx.quit();
                return;
            }
            self.active = self.active.min(self.tabs.len() - 1);
            self.focus_active(window, cx);
            self.save();
        }
    }

    /// Small bezel button (tab strip + triple cluster share this look).
    fn bezel_btn(th: &theme::Theme, label: &str, active: bool) -> gpui::Div {
        let b = div()
            .px_2()
            .py_0p5()
            .rounded_sm()
            .border_1()
            .text_size(px(11.))
            .cursor_pointer();
        if active {
            b.bg(th.accent.alpha(0.16))
                .border_color(th.accent.alpha(0.6))
                .text_color(th.accent)
                .child(label.to_string())
        } else {
            b.border_color(th.faint)
                .text_color(th.text.alpha(0.65))
                .child(label.to_string())
        }
    }
}

fn darken(mut c: Hsla, f: f32) -> Hsla {
    c.l *= f;
    c
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reap(window, cx);
        let th = theme::theme(cx);
        if self.tabs.is_empty() {
            return div();
        }
        let focused = self.focused_pane(window, cx);
        let jiggle = self.fx.jiggle_px;
        let bezel = darken(th.surface, 0.55);
        let tab = &self.tabs[self.active];
        let dir = tab.dir;
        let focused_title = tab
            .panes
            .get(focused)
            .map(|p| p.read(cx).title.clone())
            .unwrap_or_default();
        let pane_count: usize = self.tabs.iter().map(|t| t.panes.len()).sum();
        let tab_count = self.tabs.len();

        // ---- bezel top: brand · tabs · triple cluster ----
        let mut tab_strip = div().flex().flex_row().gap_1().items_center();
        for i in 0..tab_count {
            let is_active = i == self.active;
            tab_strip = tab_strip.child(
                Self::bezel_btn(&th, &format!("{}", i + 1), is_active).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        ws.activate_tab(i, window, cx)
                    }),
                ),
            );
        }
        tab_strip = tab_strip.child(
            Self::bezel_btn(&th, "+", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| ws.new_tab(window, cx)),
            ),
        );

        let triple = div()
            .flex()
            .flex_row()
            .gap_1()
            .items_center()
            .child(
                Self::bezel_btn(&th, "⊞ tab", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| ws.new_tab(window, cx)),
                ),
            )
            .child(
                Self::bezel_btn(&th, "◫ split", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        ws.split(SplitDir::Row, window, cx)
                    }),
                ),
            )
            .child(
                Self::bezel_btn(&th, "⬓ split", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        ws.split(SplitDir::Col, window, cx)
                    }),
                ),
            );

        let bezel_top = div()
            .h(px(34.))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.accent)
                            .child("▸ TERMINAL-DELIGHT"),
                    )
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(th.text.alpha(0.4))
                            .child("// SUB-TERMINAL"),
                    )
                    .child(tab_strip),
            )
            .child(triple);

        // ---- bezel bottom: the codex-style metadata readout ----
        let bezel_bottom = div()
            .h(px(22.))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .text_size(px(10.5))
            .text_color(th.text.alpha(0.55))
            .child(div().child(format!("{} · {}", th.name, focused_title)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .child(format!(
                        "{} tab{} · {} pane{}",
                        tab_count,
                        if tab_count == 1 { "" } else { "s" },
                        pane_count,
                        if pane_count == 1 { "" } else { "s" }
                    ))
                    .child(div().text_color(th.accent).child("● READY")),
            );

        // ---- the screen, inset into the device ----
        let mut pane_area = div()
            .size_full()
            .flex()
            .gap(px(3.))
            .pt(px(3. + jiggle.max(0.)))
            .pb(px(3. + (-jiggle).max(0.)))
            .px(px(3.));
        pane_area = match dir {
            SplitDir::Row => pane_area.flex_row(),
            SplitDir::Col => pane_area.flex_col(),
        };
        let pane_area = pane_area.children(tab.panes.iter().enumerate().map(|(i, p)| {
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .rounded_md()
                .border_1()
                .border_color(if i == focused {
                    th.accent.alpha(0.55)
                } else {
                    th.faint
                })
                .child(p.clone())
        }));

        let screen = div()
            .flex_1()
            .min_h_0()
            .relative()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(th.bg)
            .border_1()
            .border_color(darken(th.surface, 0.3))
            .mx_2()
            .child(pane_area)
            .when(std::env::var("TD_NOGLASS").is_err(), |el| {
                el.child(crt::glass(&th, &self.fx))
            });

        // ---- device shell ----
        div()
            .size_full()
            .bg(darken(bezel, 0.5))
            .p(px(5.))
            .font_family(th.font_family.clone())
            .on_key_down(cx.listener(Self::on_key))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .rounded(px(14.))
                    .bg(bezel)
                    .border_2()
                    .border_color(th.accent.alpha(0.22))
                    .shadow(
                        vec![
                            gpui::BoxShadow {
                                color: gpui::hsla(0., 0., 0., 0.6),
                                offset: gpui::point(px(0.), px(6.)),
                                blur_radius: px(22.),
                                spread_radius: px(0.),
                                inset: false,
                            },
                            gpui::BoxShadow {
                                color: th.accent.alpha(0.10 * th.glow),
                                offset: gpui::point(px(0.), px(0.)),
                                blur_radius: px(30.),
                                spread_radius: px(2.),
                                inset: false,
                            },
                        ]
                        .into(),
                    )
                    .child(bezel_top)
                    .child(screen)
                    .child(bezel_bottom),
            )
    }
}

fn main() {
    application().run(|cx: &mut App| {
        theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1280.), px(720.)), cx);
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
