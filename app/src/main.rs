//! terminal-delight — tiling tree · tabs · device bezel · text-size scrubber.
//!
//! Splits divide ONLY the focused terminal's space (true tiling tree); every
//! other pane keeps its exact place. ctrl+shift+t / [+]: new tab ·
//! ctrl+pgup/pgdn: switch · right-click tab: rename · alt+arrows: pane focus ·
//! ctrl+scroll or the bezel scrubber: text size.
//!
//! TODO(os-chrome): client-side window decorations (WindowDecorations::Client).

mod crt;
mod pane;
mod term;
mod theme;
mod warp;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, BoxShadow, Context, Entity, EntityId, Focusable, Hsla, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, ScrollWheelEvent, TitlebarOptions,
    Window, WindowBounds, WindowOptions, canvas, div, hsla, linear_color_stop, linear_gradient,
    point, prelude::*, px, size, white,
};
use gpui_platform::application;
use pane::TerminalView;
use serde::{Deserialize, Serialize};

const MAX_PANES: usize = 8;

pub struct UiScale(pub f32);
impl gpui::Global for UiScale {}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
enum SplitDir {
    Row,
    Col,
}

/// The tiling tree: splits divide only the targeted leaf.
enum Node {
    Leaf(Entity<TerminalView>),
    Split {
        dir: SplitDir,
        a: Box<Node>,
        b: Box<Node>,
    },
}

impl Node {
    fn leaves<'a>(&'a self, out: &mut Vec<&'a Entity<TerminalView>>) {
        match self {
            Node::Leaf(e) => out.push(e),
            Node::Split { a, b, .. } => {
                a.leaves(out);
                b.leaves(out);
            }
        }
    }

    /// Replace the leaf holding `target` with a split of (old, new).
    fn split_leaf(&mut self, target: EntityId, dir: SplitDir, new: Entity<TerminalView>) -> bool {
        match self {
            Node::Leaf(e) if e.entity_id() == target => {
                let old = std::mem::replace(self, Node::Leaf(new.clone()));
                *self = Node::Split {
                    dir,
                    a: Box::new(old),
                    b: Box::new(Node::Leaf(new)),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { a, b, .. } => {
                if a.split_leaf(target, dir, new.clone()) {
                    true
                } else {
                    b.split_leaf(target, dir, new)
                }
            }
        }
    }

    /// Drop exited leaves; a split with one survivor collapses to it.
    fn reap(self, cx: &App) -> Option<Node> {
        match self {
            Node::Leaf(e) => (!e.read(cx).exited).then_some(Node::Leaf(e)),
            Node::Split { dir, a, b } => match (a.reap(cx), b.reap(cx)) {
                (Some(a), Some(b)) => Some(Node::Split {
                    dir,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(x), None) | (None, Some(x)) => Some(x),
                (None, None) => None,
            },
        }
    }

    fn to_saved(&self) -> SavedNode {
        match self {
            Node::Leaf(_) => SavedNode::Leaf,
            Node::Split { dir, a, b } => SavedNode::Split {
                dir: *dir,
                a: Box::new(a.to_saved()),
                b: Box::new(b.to_saved()),
            },
        }
    }
}

#[derive(Serialize, Deserialize)]
enum SavedNode {
    Leaf,
    Split {
        dir: SplitDir,
        a: Box<SavedNode>,
        b: Box<SavedNode>,
    },
}

struct Tab {
    root: Node,
    name: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct StateFile {
    active: usize,
    #[serde(default)]
    scale: Option<f32>,
    tabs: Vec<SavedTab>,
}

#[derive(Serialize, Deserialize)]
struct SavedTab {
    #[serde(default)]
    name: Option<String>,
    node: SavedNode,
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

/// Frame-wide jiggle: the whole device hops ±1px every so often.
struct FrameJiggle {
    started: Instant,
    rng: u64,
    px: f32,
    until: f32,
    next_at: f32,
}

impl FrameJiggle {
    fn new() -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(3);
        Self {
            started: Instant::now(),
            rng: 0x9E3779B97F4A7C15 ^ seed,
            px: 0.,
            until: 0.,
            next_at: 5.0,
        }
    }
    fn rand(&mut self) -> f32 {
        self.rng = self
            .rng
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.rng >> 33) as f32) / (u32::MAX as f32 / 2.0)
    }
    fn tick(&mut self) -> bool {
        let t = self.started.elapsed().as_secs_f32();
        if self.px != 0. && t >= self.until {
            self.px = 0.;
            return true;
        }
        if self.px == 0. && t >= self.next_at {
            self.px = if self.rand() > 1.0 { 1.0 } else { -1.0 };
            self.until = t + 0.07;
            self.next_at = t + 7.0 + self.rand() * 5.0;
            return true;
        }
        false
    }
}

struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    focus_handle: gpui::FocusHandle,
    renaming: Option<(usize, String)>,
    scrubbing: bool,
    scrub_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    jiggle: FrameJiggle,
    last_action: Instant,
}

fn make_pane(window: &mut Window, cx: &mut Context<Workspace>) -> Entity<TerminalView> {
    let pane = cx.new(TerminalView::new);
    cx.observe(&pane, |_, _, cx| cx.notify()).detach();
    window.focus(&pane.focus_handle(cx), cx);
    pane
}

fn build_node(saved: &SavedNode, window: &mut Window, cx: &mut Context<Workspace>) -> Node {
    match saved {
        SavedNode::Leaf => Node::Leaf(make_pane(window, cx)),
        SavedNode::Split { dir, a, b } => Node::Split {
            dir: *dir,
            a: Box::new(build_node(a, window, cx)),
            b: Box::new(build_node(b, window, cx)),
        },
    }
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = load_state();
        cx.set_global(UiScale(saved.scale.unwrap_or(1.0).clamp(0.7, 1.6)));
        let mut ws = Self {
            tabs: vec![],
            active: 0,
            focus_handle: cx.focus_handle(),
            renaming: None,
            scrubbing: false,
            scrub_bounds: Arc::new(Mutex::new(None)),
            jiggle: FrameJiggle::new(),
            last_action: Instant::now() - Duration::from_secs(1),
        };
        if saved.tabs.is_empty() {
            ws.new_tab(window, cx);
        } else {
            for t in &saved.tabs {
                let root = build_node(&t.node, window, cx);
                ws.tabs.push(Tab {
                    root,
                    name: t.name.clone(),
                });
            }
            ws.active = saved.active.min(ws.tabs.len() - 1);
            ws.focus_active(window, cx);
        }
        // frame jiggle clock (cheap idle poll)
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(60))
                    .await;
                let _ = this.update(cx, |ws: &mut Workspace, cx| {
                    if ws.jiggle.tick() {
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        ws
    }

    fn pane_count(&self) -> usize {
        let mut n = 0;
        for t in &self.tabs {
            let mut v = vec![];
            t.root.leaves(&mut v);
            n += v.len();
        }
        n
    }

    fn save(&self, cx: &App) {
        let state = StateFile {
            active: self.active,
            scale: Some(cx.global::<UiScale>().0),
            tabs: self
                .tabs
                .iter()
                .map(|t| SavedTab {
                    name: t.name.clone(),
                    node: t.root.to_saved(),
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

    /// Mouse-down can dispatch more than once per physical click (capture +
    /// bubble); structural actions debounce to one per 200ms.
    fn debounced(&mut self) -> bool {
        if self.last_action.elapsed() < Duration::from_millis(200) {
            return false;
        }
        self.last_action = Instant::now();
        true
    }

    fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.debounced() {
            return;
        }
        if std::env::var("TD_KEYDEBUG").is_ok() {
            eprintln!("new_tab");
        }
        let pane = make_pane(window, cx);
        self.tabs.push(Tab {
            root: Node::Leaf(pane),
            name: None,
        });
        self.active = self.tabs.len() - 1;
        self.save(cx);
        cx.notify();
    }

    /// Split ONLY the focused terminal; everything else keeps its exact space.
    fn split(&mut self, dir: SplitDir, window: &mut Window, cx: &mut Context<Self>) {
        if !self.debounced() {
            return;
        }
        if std::env::var("TD_KEYDEBUG").is_ok() {
            eprintln!("split col={}", matches!(dir, SplitDir::Col));
        }
        if self.pane_count() >= MAX_PANES {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let mut leaves = vec![];
        tab.root.leaves(&mut leaves);
        let target = leaves
            .iter()
            .find(|p| p.focus_handle(cx).is_focused(window))
            .or_else(|| leaves.first())
            .map(|p| p.entity_id());
        let Some(target) = target else { return };
        let new_pane = make_pane(window, cx);
        self.tabs[self.active]
            .root
            .split_leaf(target, dir, new_pane);
        self.save(cx);
        cx.notify();
    }

    fn activate_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i < self.tabs.len() {
            self.active = i;
            self.focus_active(window, cx);
            self.save(cx);
            cx.notify();
        }
    }

    fn focus_active(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get(self.active) {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            if let Some(p) = leaves.first() {
                window.focus(&p.focus_handle(cx), cx);
            }
        }
    }

    fn set_scale(&mut self, value: f32, cx: &mut Context<Self>) {
        cx.set_global(UiScale(value.clamp(0.7, 1.6)));
        self.save(cx);
        cx.refresh_windows();
    }

    fn scale_from_pos(&self, x: Pixels) -> Option<f32> {
        let b = (*self.scrub_bounds.lock().unwrap())?;
        let ratio =
            ((f32::from(x) - f32::from(b.origin.x)) / f32::from(b.size.width)).clamp(0., 1.);
        Some(0.7 + ratio * 0.9)
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        // the inline rename box owns the keyboard while open
        if let Some((tab_i, mut buf)) = self.renaming.take() {
            match ks.key.as_str() {
                "enter" => {
                    if let Some(tab) = self.tabs.get_mut(tab_i) {
                        tab.name = (!buf.trim().is_empty()).then(|| buf.trim().to_string());
                    }
                    self.save(cx);
                    self.focus_active(window, cx);
                }
                "escape" => self.focus_active(window, cx),
                "backspace" => {
                    buf.pop();
                    self.renaming = Some((tab_i, buf));
                }
                _ => {
                    if let Some(ch) = ks.key_char.as_ref() {
                        if buf.chars().count() < 18 {
                            buf.push_str(ch);
                        }
                    }
                    self.renaming = Some((tab_i, buf));
                }
            }
            cx.notify();
            return;
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
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            if leaves.len() > 1 {
                let dir: i32 = match ks.key.as_str() {
                    "left" | "up" => -1,
                    "right" | "down" => 1,
                    _ => return,
                };
                let cur = leaves
                    .iter()
                    .position(|p| p.focus_handle(cx).is_focused(window))
                    .unwrap_or(0) as i32;
                let next = (cur + dir).rem_euclid(leaves.len() as i32) as usize;
                window.focus(&leaves[next].focus_handle(cx), cx);
                cx.notify();
            }
        }
    }

    /// ctrl+wheel anywhere = text-size scrub (panes skip scrolling when ctrl).
    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if !ev.modifiers.control {
            return;
        }
        let dy = match ev.delta {
            gpui::ScrollDelta::Lines(l) => l.y,
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / 20.,
        };
        let cur = cx.global::<UiScale>().0;
        self.set_scale(cur + dy * 0.05, cx);
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if self.scrubbing && ev.pressed_button == Some(MouseButton::Left) {
            if let Some(s) = self.scale_from_pos(ev.position.x) {
                self.set_scale(s, cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, _w: &mut Window, _cx: &mut Context<Self>) {
        self.scrubbing = false;
    }

    fn reap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let had = self.pane_count();
        let tabs = std::mem::take(&mut self.tabs);
        self.tabs = tabs
            .into_iter()
            .filter_map(|t| {
                t.root.reap(cx).map(|root| Tab {
                    root,
                    name: t.name,
                })
            })
            .collect();
        if self.pane_count() != had {
            if self.tabs.is_empty() {
                cx.quit();
                return;
            }
            self.active = self.active.min(self.tabs.len() - 1);
            self.focus_active(window, cx);
            self.save(cx);
        }
    }

    /// Solid, reflective bezel button — light source upper-left.
    fn bezel_btn(th: &theme::Theme, label: &str, active: bool) -> gpui::Div {
        let glint = BoxShadow {
            color: white().alpha(0.22),
            offset: point(px(1.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        };
        let seat = BoxShadow {
            color: hsla(0., 0., 0., 0.55),
            offset: point(px(2.), px(2.)),
            blur_radius: px(3.),
            spread_radius: px(0.),
            inset: false,
        };
        let b = div()
            .px_2()
            .py_0p5()
            .rounded_sm()
            .border_1()
            .text_size(px(11.))
            .cursor_pointer()
            .shadow(vec![glint, seat].into());
        if active {
            b.bg(linear_gradient(
                135.,
                linear_color_stop(th.accent.alpha(0.42), 0.),
                linear_color_stop(th.accent.alpha(0.12), 1.),
            ))
            .border_color(th.accent)
            .text_color(white().alpha(0.92))
            .child(label.to_string())
        } else {
            b.bg(linear_gradient(
                135.,
                linear_color_stop(brighten(th.surface, 1.7), 0.),
                linear_color_stop(darken(th.surface, 0.7), 1.),
            ))
            .border_color(th.accent.alpha(0.4))
            .text_color(th.text)
            .child(label.to_string())
        }
    }
}

fn darken(mut c: Hsla, f: f32) -> Hsla {
    c.l *= f;
    c
}

fn brighten(mut c: Hsla, f: f32) -> Hsla {
    c.l = (c.l * f).min(0.92);
    c
}

fn render_node(node: &Node, th: &theme::Theme, focused: Option<EntityId>) -> gpui::Div {
    match node {
        Node::Leaf(e) => {
            let is_focused = focused == Some(e.entity_id());
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .rounded_md()
                .border_1()
                .border_color(if is_focused {
                    th.accent.alpha(0.55)
                } else {
                    th.faint
                })
                .child(e.clone())
        }
        Node::Split { dir, a, b } => {
            let base = div().flex_1().min_w_0().min_h_0().flex().gap(px(3.));
            let base = match dir {
                SplitDir::Row => base.flex_row(),
                SplitDir::Col => base.flex_col(),
            };
            base.child(render_node(a, th, focused))
                .child(render_node(b, th, focused))
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reap(window, cx);
        warp::begin_frame(); // visible panes re-register their tube rects below
        let th = theme::theme(cx);
        if self.tabs.is_empty() {
            return div();
        }
        let scale = cx.global::<UiScale>().0;
        let bezel = darken(th.surface, 0.55);
        let tab = &self.tabs[self.active];
        let mut leaves = vec![];
        tab.root.leaves(&mut leaves);
        let focused_id = leaves
            .iter()
            .find(|p| p.focus_handle(cx).is_focused(window))
            .map(|p| p.entity_id());
        let focused_title = leaves
            .iter()
            .find(|p| Some(p.entity_id()) == focused_id)
            .or(leaves.first())
            .map(|p| p.read(cx).title.clone())
            .unwrap_or_default();
        let pane_count = self.pane_count();
        let tab_count = self.tabs.len();
        let jiggle = self.jiggle.px;

        // ---- tabs (right-click renames) ----
        let renaming = self.renaming.clone();
        let mut tab_strip = div().flex().flex_row().gap_1().items_center();
        for i in 0..tab_count {
            let is_active = i == self.active;
            if let Some((_, buf)) = renaming.as_ref().filter(|(ri, _)| *ri == i) {
                tab_strip = tab_strip.child(
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .border_1()
                        .border_color(th.accent)
                        .bg(darken(th.bg, 0.8))
                        .text_size(px(11.))
                        .text_color(th.text)
                        .flex()
                        .flex_row()
                        .items_center()
                        .child(buf.clone())
                        .child(div().w(px(6.)).h(px(13.)).bg(th.cursor)),
                );
                continue;
            }
            let label = self.tabs[i]
                .name
                .clone()
                .unwrap_or_else(|| format!("{}", i + 1));
            tab_strip = tab_strip.child(
                Self::bezel_btn(&th, &label, is_active)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                            ws.activate_tab(i, window, cx)
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                            let seed = ws.tabs[i].name.clone().unwrap_or_default();
                            ws.renaming = Some((i, seed));
                            window.focus(&ws.focus_handle, cx);
                            cx.notify();
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

        // ---- text-size scrubber: A ──●── A 110% ----
        let ratio = ((scale - 0.7) / 0.9).clamp(0., 1.);
        let scrub_store = self.scrub_bounds.clone();
        let scrubber = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(div().text_size(px(9.)).text_color(th.text).child("A"))
            .child(
                div()
                    .w(px(90.))
                    .h(px(12.))
                    .relative()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, ev: &MouseDownEvent, _w, cx| {
                            ws.scrubbing = true;
                            let s = ws.scale_from_pos(ev.position.x);
                            if std::env::var("TD_KEYDEBUG").is_ok() {
                                eprintln!(
                                    "scrub down at {:?} -> {:?} (bounds {:?})",
                                    ev.position.x,
                                    s,
                                    ws.scrub_bounds.lock().unwrap().map(|b| b.origin.x)
                                );
                            }
                            if let Some(s) = s {
                                ws.set_scale(s, cx);
                            }
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                *scrub_store.lock().unwrap() = Some(bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .size_full(),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(5.))
                            .h(px(3.))
                            .rounded_full()
                            .bg(darken(th.surface, 0.4))
                            .border_1()
                            .border_color(th.faint),
                    )
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(5.))
                            .h(px(3.))
                            .w(px(90. * ratio))
                            .rounded_full()
                            .bg(th.accent),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px((90. * ratio - 5.).max(0.)))
                            .top(px(1.))
                            .w(px(10.))
                            .h(px(10.))
                            .rounded_full()
                            .bg(linear_gradient(
                                135.,
                                linear_color_stop(brighten(th.accent, 1.4), 0.),
                                linear_color_stop(darken(th.accent, 0.7), 1.),
                            ))
                            .shadow(
                                vec![BoxShadow {
                                    color: white().alpha(0.4),
                                    offset: point(px(-1.), px(-1.)),
                                    blur_radius: px(1.),
                                    spread_radius: px(0.),
                                    inset: true,
                                }]
                                .into(),
                            ),
                    ),
            )
            .child(div().text_size(px(12.)).text_color(th.text).child("A"))
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(th.accent)
                    .child(format!("{}%", (scale * 100.).round() as i32)),
            );

        let cluster = div()
            .flex()
            .flex_row()
            .gap_1()
            .items_center()
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
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(scrubber)
                    .child(cluster),
            );

        let bezel_bottom = div()
            .h(px(22.))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .text_size(px(10.5))
            .text_color(th.text)
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

        let pane_area = div()
            .size_full()
            .flex()
            .p(px(3.))
            .child(render_node(&tab.root, &th, focused_id));

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
            .child(pane_area);

        div()
            .size_full()
            .bg(darken(bezel, 0.5))
            .px(px(5.))
            .pt(px(5. + jiggle.max(0.)))
            .pb(px(5. + (-jiggle).max(0.)))
            .font_family(th.font_family.clone())
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key))
            .on_scroll_wheel(cx.listener(Self::on_wheel))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .rounded(px(14.))
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(brighten(bezel, 1.6), 0.),
                        linear_color_stop(darken(bezel, 0.8), 1.),
                    ))
                    .border_2()
                    .border_color(th.accent.alpha(0.45))
                    .shadow(
                        vec![
                            // upper-left light source: glint biased to (1,1)
                            BoxShadow {
                                color: white().alpha(0.14),
                                offset: point(px(1.), px(1.)),
                                blur_radius: px(0.),
                                spread_radius: px(0.),
                                inset: true,
                            },
                            BoxShadow {
                                color: hsla(0., 0., 0., 0.5),
                                offset: point(px(-2.), px(-2.)),
                                blur_radius: px(3.),
                                spread_radius: px(0.),
                                inset: true,
                            },
                            BoxShadow {
                                color: hsla(0., 0., 0., 0.6),
                                offset: point(px(4.), px(6.)),
                                blur_radius: px(22.),
                                spread_radius: px(0.),
                                inset: false,
                            },
                            BoxShadow {
                                color: th.accent.alpha(0.10 * th.glow),
                                offset: point(px(0.), px(0.)),
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
