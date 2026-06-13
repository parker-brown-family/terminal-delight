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
mod session;
mod term;
mod theme;
mod warp;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, hsla, linear_color_stop, linear_gradient, point, prelude::*, px, size, white, App,
    Bounds, BoxShadow, Context, Entity, EntityId, Focusable, Hsla, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, ScrollWheelEvent, TitlebarOptions,
    Window, WindowBounds, WindowOptions,
};
use gpui_platform::application;
use pane::{ClosePane, DragPaneStart, OpenThemeMenu, TerminalView};
use serde::{Deserialize, Serialize};
use theme::ThemeChoice;

const MAX_PANES: usize = 8;

pub struct UiScale(pub f32);
impl gpui::Global for UiScale {}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
enum SplitDir {
    Row,
    Col,
}

static SPLIT_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn next_split_id() -> u64 {
    SPLIT_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// The tiling tree: splits divide only the targeted leaf. Generic over the
/// leaf payload so the structural logic is testable without live terminals.
enum Tree<L> {
    Leaf(L),
    Split {
        id: u64,
        dir: SplitDir,
        ratio: f32,
        a: Box<Tree<L>>,
        b: Box<Tree<L>>,
    },
}

type Node = Tree<Entity<TerminalView>>;

impl<L: Clone> Tree<L> {
    fn leaves<'a>(&'a self, out: &mut Vec<&'a L>) {
        match self {
            Tree::Leaf(e) => out.push(e),
            Tree::Split { a, b, .. } => {
                a.leaves(out);
                b.leaves(out);
            }
        }
    }

    /// Replace the leaf matching `target` with a split of (old, new).
    fn split_leaf(&mut self, target: &impl Fn(&L) -> bool, dir: SplitDir, new: L) -> bool {
        match self {
            Tree::Leaf(e) if target(e) => {
                let old = std::mem::replace(self, Tree::Leaf(new.clone()));
                *self = Tree::Split {
                    id: next_split_id(),
                    dir,
                    ratio: 0.5,
                    a: Box::new(old),
                    b: Box::new(Tree::Leaf(new)),
                };
                true
            }
            Tree::Leaf(_) => false,
            Tree::Split { a, b, .. } => {
                if a.split_leaf(target, dir, new.clone()) {
                    true
                } else {
                    b.split_leaf(target, dir, new)
                }
            }
        }
    }

    /// Split the leaf matching `target` into a directional split. `new` lands on
    /// the leading side (left / top) when `new_first`, else the trailing side —
    /// this is how a dropped pane chooses L/R/T/B against the pane it lands on.
    fn split_leaf_dir(
        &mut self,
        target: &impl Fn(&L) -> bool,
        dir: SplitDir,
        new: L,
        new_first: bool,
    ) -> bool {
        match self {
            Tree::Leaf(e) if target(e) => {
                // clone `new` only as a momentary placeholder while we rebuild
                let old = std::mem::replace(self, Tree::Leaf(new.clone()));
                let (a, b) = if new_first {
                    (Tree::Leaf(new), old)
                } else {
                    (old, Tree::Leaf(new))
                };
                *self = Tree::Split {
                    id: next_split_id(),
                    dir,
                    ratio: 0.5,
                    a: Box::new(a),
                    b: Box::new(b),
                };
                true
            }
            Tree::Leaf(_) => false,
            Tree::Split { a, b, .. } => {
                a.split_leaf_dir(target, dir, new.clone(), new_first)
                    || b.split_leaf_dir(target, dir, new, new_first)
            }
        }
    }

    /// Remove the first leaf matching `target`, collapsing its parent split onto
    /// the surviving sibling. Returns the removed payload and the remaining tree
    /// (`None` only when the whole tree was a single matching leaf). This is the
    /// pull-out half of a pane drag: the dragged pane leaves its old home cleanly.
    fn remove_leaf(self, target: &impl Fn(&L) -> bool) -> (Option<L>, Option<Tree<L>>) {
        match self {
            Tree::Leaf(e) => {
                if target(&e) {
                    (Some(e), None)
                } else {
                    (None, Some(Tree::Leaf(e)))
                }
            }
            Tree::Split {
                id,
                dir,
                ratio,
                a,
                b,
            } => {
                let (taken_a, rest_a) = a.remove_leaf(target);
                if let Some(payload) = taken_a {
                    let remaining = match rest_a {
                        Some(at) => Tree::Split {
                            id,
                            dir,
                            ratio,
                            a: Box::new(at),
                            b,
                        },
                        None => *b,
                    };
                    return (Some(payload), Some(remaining));
                }
                // `a` did not hold it; a non-match always hands the subtree back
                let a = Box::new(rest_a.expect("non-match returns its subtree"));
                let (taken_b, rest_b) = b.remove_leaf(target);
                if let Some(payload) = taken_b {
                    let remaining = match rest_b {
                        Some(bt) => Tree::Split {
                            id,
                            dir,
                            ratio,
                            a,
                            b: Box::new(bt),
                        },
                        None => *a,
                    };
                    return (Some(payload), Some(remaining));
                }
                let b = Box::new(rest_b.expect("non-match returns its subtree"));
                (
                    None,
                    Some(Tree::Split {
                        id,
                        dir,
                        ratio,
                        a,
                        b,
                    }),
                )
            }
        }
    }

    /// Drop leaves where `dead` holds; a split with one survivor collapses to it.
    fn reap_where(self, dead: &impl Fn(&L) -> bool) -> Option<Tree<L>> {
        match self {
            Tree::Leaf(e) => (!dead(&e)).then_some(Tree::Leaf(e)),
            Tree::Split {
                id,
                dir,
                ratio,
                a,
                b,
            } => match (a.reap_where(dead), b.reap_where(dead)) {
                (Some(a), Some(b)) => Some(Tree::Split {
                    id,
                    dir,
                    ratio,
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(x), None) | (None, Some(x)) => Some(x),
                (None, None) => None,
            },
        }
    }

    fn dir_of(&self, target: u64) -> Option<SplitDir> {
        match self {
            Tree::Leaf(_) => None,
            Tree::Split { id, dir, a, b, .. } => {
                if *id == target {
                    Some(*dir)
                } else {
                    a.dir_of(target).or_else(|| b.dir_of(target))
                }
            }
        }
    }

    fn set_ratio(&mut self, target: u64, value: f32) -> bool {
        match self {
            Tree::Leaf(_) => false,
            Tree::Split {
                id, ratio, a, b, ..
            } => {
                if *id == target {
                    *ratio = value.clamp(0.15, 0.85);
                    true
                } else {
                    a.set_ratio(target, value) || b.set_ratio(target, value)
                }
            }
        }
    }

    fn to_saved_with(&self, leaf_state: &impl Fn(&L) -> LeafState) -> SavedNode {
        match self {
            Tree::Leaf(e) => {
                let s = leaf_state(e);
                SavedNode::Leaf {
                    theme: s.theme,
                    cwd: s.cwd,
                    resume: s.resume,
                }
            }
            Tree::Split {
                dir, ratio, a, b, ..
            } => SavedNode::Split {
                dir: *dir,
                ratio: *ratio,
                a: Box::new(a.to_saved_with(leaf_state)),
                b: Box::new(b.to_saved_with(leaf_state)),
            },
        }
    }
}

/// The live-terminal bindings of the generic tree ops.
impl Node {
    /// Drop exited leaves; a split with one survivor collapses to it.
    fn reap(self, cx: &App) -> Option<Node> {
        self.reap_where(&|e| e.read(cx).exited)
    }

    fn to_saved(&self, cx: &App) -> SavedNode {
        self.to_saved_with(&|e| {
            let view = e.read(cx);
            let rt = view.runtime();
            LeafState {
                theme: view.theme_override.clone(),
                cwd: rt.cwd,
                resume: rt.resume,
            }
        })
    }
}

/// Everything a leaf carries into the state file: appearance + live work
/// (cwd and a resumable agent session, captured from the kernel at save time).
#[derive(Default)]
struct LeafState {
    theme: Option<ThemeChoice>,
    cwd: Option<String>,
    resume: Option<String>,
}

#[derive(Serialize)]
enum SavedNode {
    Leaf {
        #[serde(skip_serializing_if = "Option::is_none")]
        theme: Option<ThemeChoice>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resume: Option<String>,
    },
    Split {
        dir: SplitDir,
        ratio: f32,
        a: Box<SavedNode>,
        b: Box<SavedNode>,
    },
}

/// Accepts both the legacy `"Leaf"` string and the current table form, so
/// pre-theme state files keep their layouts.
impl<'de> Deserialize<'de> for SavedNode {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize, Default)]
        struct LeafFields {
            #[serde(default)]
            theme: Option<ThemeChoice>,
            #[serde(default)]
            cwd: Option<String>,
            #[serde(default)]
            resume: Option<String>,
        }
        #[derive(Deserialize)]
        struct SplitFields {
            dir: SplitDir,
            #[serde(default = "default_ratio")]
            ratio: f32,
            a: Box<SavedNode>,
            b: Box<SavedNode>,
        }
        struct V;
        impl<'de> serde::de::Visitor<'de> for V {
            type Value = SavedNode;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a saved layout node")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<SavedNode, E> {
                match v {
                    "Leaf" => Ok(SavedNode::Leaf {
                        theme: None,
                        cwd: None,
                        resume: None,
                    }),
                    other => Err(E::custom(format!("unknown node: {other}"))),
                }
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<SavedNode, A::Error> {
                use serde::de::Error;
                let Some(key) = map.next_key::<String>()? else {
                    return Err(A::Error::custom("empty node"));
                };
                match key.as_str() {
                    "Leaf" => {
                        let f: LeafFields = map.next_value()?;
                        Ok(SavedNode::Leaf {
                            theme: f.theme,
                            cwd: f.cwd,
                            resume: f.resume,
                        })
                    }
                    "Split" => {
                        let f: SplitFields = map.next_value()?;
                        Ok(SavedNode::Split {
                            dir: f.dir,
                            ratio: f.ratio,
                            a: f.a,
                            b: f.b,
                        })
                    }
                    other => Err(A::Error::custom(format!("unknown node: {other}"))),
                }
            }
        }
        d.deserialize_any(V)
    }
}

fn default_ratio() -> f32 {
    0.5
}

struct Tab {
    root: Node,
    name: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct StateFile {
    active: usize,
    /// Window origin + size (x, y, w, h) for exact-place reboot.
    #[serde(default)]
    win: Option<(f32, f32, f32, f32)>,
    #[serde(default)]
    scale: Option<f32>,
    /// Outer (workspace) theme choice; panes may carry their own override.
    #[serde(default)]
    theme: Option<ThemeChoice>,
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

/// Which scope the open theme breakout is editing.
#[derive(Clone)]
enum MenuScope {
    Outer,
    Pane(Entity<TerminalView>),
}

/// Which side of the pane under the cursor a dragged sub-tab will split.
#[derive(Clone, Copy, PartialEq)]
enum Zone {
    Left,
    Right,
    Top,
    Bottom,
}

/// Where a dragged sub-tab will land when released.
#[derive(Clone)]
enum DropTarget {
    /// Split the pane `pane` on its `zone` side.
    Split { pane: EntityId, zone: Zone },
    /// Move the dragged pane into main tab `index`.
    Tab { index: usize },
}

/// A sub-tab being dragged by its header.
struct PaneDrag {
    /// The pane entity being moved.
    id: EntityId,
    /// Where the grab started (window space) — engages past a small threshold.
    start: Point<Pixels>,
    /// Latest cursor position, for the floating drag chip.
    at: Point<Pixels>,
    /// True once the cursor moved far enough to be a drag, not a stray click.
    engaged: bool,
}

struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    focus_handle: gpui::FocusHandle,
    renaming: Option<(usize, String)>,
    /// Tab index awaiting a "close all its panes?" confirmation, if any.
    confirm_close: Option<usize>,
    /// Open theme breakout menu, if any.
    theme_menu: Option<MenuScope>,
    /// Window-space point to anchor the open tray at (a sub-tab icon click).
    /// None = the fixed top-right anchor used by the global/outer menu.
    menu_at: Option<Point<Pixels>>,
    scrubbing: bool,
    scrub_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    jiggle: FrameJiggle,
    last_action: Instant,
    /// split-id being dragged, if any
    drag_split: Option<u64>,
    split_bounds: Arc<Mutex<std::collections::HashMap<u64, Bounds<Pixels>>>>,
    /// Last seen window bounds (x, y, w, h), refreshed each render for save().
    last_win: Option<(f32, f32, f32, f32)>,
    /// A sub-tab being dragged by its header, if any.
    drag_pane: Option<PaneDrag>,
    /// The resolved drop landing under the cursor while dragging (for overlay).
    drop_target: Option<DropTarget>,
    /// Live per-pane content rects (entity → box) for drop hit-testing.
    pane_bounds: Arc<Mutex<std::collections::HashMap<EntityId, Bounds<Pixels>>>>,
    /// Live per-tab button rects (index → box) for "drop onto a main tab".
    tab_bounds: Arc<Mutex<std::collections::HashMap<usize, Bounds<Pixels>>>>,
}

fn make_pane(window: &mut Window, cx: &mut Context<Workspace>) -> Entity<TerminalView> {
    make_pane_restored(session::PaneRestore::default(), window, cx)
}

fn make_pane_restored(
    restore: session::PaneRestore,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<TerminalView> {
    let pane = cx.new(|cx| TerminalView::new_restored(restore, cx));
    cx.observe(&pane, |_, _, cx| cx.notify()).detach();
    cx.subscribe(&pane, |ws, pane, ev: &OpenThemeMenu, cx| {
        ws.theme_menu = Some(MenuScope::Pane(pane));
        ws.menu_at = Some(ev.at);
        cx.notify();
    })
    .detach();
    // grab the header → begin a sub-tab drag (the workspace drives it from here)
    cx.subscribe(&pane, |ws, pane, ev: &DragPaneStart, cx| {
        let start = ev.at;
        ws.drag_pane = Some(PaneDrag {
            id: pane.entity_id(),
            start,
            at: start,
            engaged: false,
        });
        ws.drop_target = None;
        cx.notify();
    })
    .detach();
    // the header × → close just this pane (window-aware: refocuses what's left)
    cx.subscribe_in(&pane, window, |ws, pane, _ev: &ClosePane, window, cx| {
        ws.close_pane(pane.entity_id(), window, cx);
    })
    .detach();
    window.focus(&pane.focus_handle(cx), cx);
    pane
}

fn build_node(saved: &SavedNode, window: &mut Window, cx: &mut Context<Workspace>) -> Node {
    match saved {
        SavedNode::Leaf { theme, cwd, resume } => {
            let pane = make_pane_restored(
                session::PaneRestore {
                    cwd: cwd.clone(),
                    resume: resume.clone(),
                },
                window,
                cx,
            );
            if theme.is_some() {
                let choice = theme.clone();
                pane.update(cx, |view, _| view.theme_override = choice);
            }
            Node::Leaf(pane)
        }
        SavedNode::Split { dir, ratio, a, b } => Node::Split {
            id: next_split_id(),
            dir: *dir,
            ratio: (*ratio).clamp(0.15, 0.85),
            a: Box::new(build_node(a, window, cx)),
            b: Box::new(build_node(b, window, cx)),
        },
    }
}

impl Workspace {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let saved = load_state();
        cx.set_global(UiScale(saved.scale.unwrap_or(1.0).clamp(0.7, 1.6)));
        if let Some(choice) = saved.theme.clone() {
            theme::select_outer(cx, choice);
        }
        let mut ws = Self {
            tabs: vec![],
            active: 0,
            focus_handle: cx.focus_handle(),
            renaming: None,
            confirm_close: None,
            theme_menu: None,
            menu_at: None,
            scrubbing: false,
            scrub_bounds: Arc::new(Mutex::new(None)),
            jiggle: FrameJiggle::new(),
            last_action: Instant::now() - Duration::from_secs(1),
            drag_split: None,
            split_bounds: Arc::new(Mutex::new(std::collections::HashMap::new())),
            last_win: None,
            drag_pane: None,
            drop_target: None,
            pane_bounds: Arc::new(Mutex::new(std::collections::HashMap::new())),
            tab_bounds: Arc::new(Mutex::new(std::collections::HashMap::new())),
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
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            let _ = this.update(cx, |ws: &mut Workspace, cx| {
                if ws.jiggle.tick() {
                    cx.notify();
                }
            });
        })
        .detach();
        // session checkpoint: live state (pane cwds, agent sessions, window
        // bounds) changes without structural events, so re-snapshot every 5
        // minutes — a crash loses at most that much recency, never the layout.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_secs(300))
                .await;
            if this
                .update(cx, |ws: &mut Workspace, cx| ws.save(cx))
                .is_err()
            {
                break;
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
            win: self.last_win,
            scale: Some(cx.global::<UiScale>().0),
            theme: Some(theme::outer_choice(cx)),
            tabs: self
                .tabs
                .iter()
                .map(|t| SavedTab {
                    name: t.name.clone(),
                    node: t.root.to_saved(cx),
                })
                .collect(),
        };
        if let Ok(body) = toml::to_string(&state) {
            let _ = session::write_atomic(&state_path(), &body);
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
            .split_leaf(&|p| p.entity_id() == target, dir, new_pane);
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

    /// How many panes a tab holds — drives the "close more than one?" gate.
    fn tab_pane_count(&self, i: usize) -> usize {
        self.tabs
            .get(i)
            .map(|t| {
                let mut v = vec![];
                t.root.leaves(&mut v);
                v.len()
            })
            .unwrap_or(0)
    }

    /// The tab's X was clicked. A single-pane tab closes immediately; a tab
    /// holding more than one pane asks first (themed confirm overlay).
    fn request_close_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i >= self.tabs.len() {
            return;
        }
        if self.tab_pane_count(i) > 1 {
            self.confirm_close = Some(i);
            cx.notify();
        } else {
            self.close_tab(i, window, cx);
        }
    }

    /// Remove tab `i`; dropping its subtree drops each pane entity, which closes
    /// the PTY (the shell gets SIGHUP). Quits the app if it was the last tab —
    /// same end-state as the last shell exiting (see `reap`).
    fn close_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i >= self.tabs.len() {
            return;
        }
        self.confirm_close = None;
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            cx.quit();
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.focus_active(window, cx);
        self.save(cx);
        cx.notify();
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
        if self.theme_menu.is_some() && ks.key.as_str() == "escape" {
            self.theme_menu = None;
            cx.notify();
            return;
        }
        if self.confirm_close.is_some() && ks.key.as_str() == "escape" {
            self.confirm_close = None;
            cx.notify();
            return;
        }
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
        // a sub-tab drag in flight owns the move: track the cursor, engage past
        // a small threshold (so a plain header click still focuses), and resolve
        // the drop landing under the cursor for the overlay.
        if self.drag_pane.is_some() {
            if ev.pressed_button != Some(MouseButton::Left) {
                self.drag_pane = None;
                self.drop_target = None;
                cx.notify();
                return;
            }
            let pos = ev.position;
            let engaged = {
                let d = self.drag_pane.as_mut().unwrap();
                d.at = pos;
                if !d.engaged {
                    let dx = f32::from(pos.x) - f32::from(d.start.x);
                    let dy = f32::from(pos.y) - f32::from(d.start.y);
                    if (dx * dx + dy * dy).sqrt() > 6.0 {
                        d.engaged = true;
                    }
                }
                d.engaged
            };
            self.drop_target = if engaged {
                let id = self.drag_pane.as_ref().unwrap().id;
                self.resolve_drop(pos, id)
            } else {
                None
            };
            cx.notify();
            return;
        }
        if ev.pressed_button == Some(MouseButton::Left) {
            if let Some(split_id) = self.drag_split {
                let bounds = self.split_bounds.lock().unwrap().get(&split_id).copied();
                if std::env::var("TD_KEYDEBUG").is_ok() {
                    eprintln!(
                        "dragging {split_id}: bounds={:?} pos={:?}",
                        bounds.map(|b| b.origin.x),
                        ev.position.x
                    );
                }
                if let (Some(b), Some(tab)) = (bounds, self.tabs.get_mut(self.active)) {
                    // ratio along the split's own axis; dir recovered from shape
                    let rx = ((f32::from(ev.position.x) - f32::from(b.origin.x))
                        / f32::from(b.size.width).max(1.))
                    .clamp(0., 1.);
                    let ry = ((f32::from(ev.position.y) - f32::from(b.origin.y))
                        / f32::from(b.size.height).max(1.))
                    .clamp(0., 1.);
                    let dir = tab.root.dir_of(split_id);
                    let ratio = match dir {
                        Some(SplitDir::Row) => rx,
                        Some(SplitDir::Col) => ry,
                        None => return,
                    };
                    tab.root.set_ratio(split_id, ratio);
                    cx.notify();
                }
                return;
            }
        }
        if self.scrubbing && ev.pressed_button == Some(MouseButton::Left) {
            if let Some(s) = self.scale_from_pos(ev.position.x) {
                self.set_scale(s, cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = false;
        if let Some(drag) = self.drag_pane.take() {
            let target = self.drop_target.take();
            if drag.engaged {
                if let Some(target) = target {
                    self.perform_drop(drag.id, target, window, cx);
                }
                // a release over empty space is a no-op for now; out-to-new-
                // window pop-out is the remaining piece (needs gpui multi-window).
            }
            cx.notify();
            return;
        }
        if self.drag_split.take().is_some() {
            self.save(cx);
        }
    }

    /// Close just one pane (its header ×). Removes the leaf, collapses its parent
    /// split onto the sibling, drops the now-empty tab, and quits if it was the
    /// last pane anywhere — the same end-state as the shell exiting on its own.
    fn close_pane(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(from) = self.tabs.iter().position(|t| {
            let mut v = vec![];
            t.root.leaves(&mut v);
            v.iter().any(|e| e.entity_id() == id)
        }) else {
            return;
        };
        let pred = |e: &Entity<TerminalView>| e.entity_id() == id;
        let tab = self.tabs.remove(from);
        let name = tab.name;
        // dropping the taken Entity releases its PTY (SIGHUP) — that's the close
        let (_taken, remaining) = tab.root.remove_leaf(&pred);
        if let Some(root) = remaining {
            self.tabs.insert(from, Tab { root, name });
        }
        if self.tabs.is_empty() {
            cx.quit();
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.focus_active(window, cx);
        self.save(cx);
        cx.notify();
    }

    /// What a release at `pos` would land on, ignoring the dragged pane itself.
    /// Main tabs win over panes, so dragging up to the strip moves between tabs.
    fn resolve_drop(&self, pos: Point<Pixels>, dragged: EntityId) -> Option<DropTarget> {
        for (&index, &rect) in self.tab_bounds.lock().unwrap().iter() {
            if rect.contains(&pos) {
                return Some(DropTarget::Tab { index });
            }
        }
        for (&id, &rect) in self.pane_bounds.lock().unwrap().iter() {
            if id != dragged && rect.contains(&pos) {
                return Some(DropTarget::Split {
                    pane: id,
                    zone: zone_of(rect, pos),
                });
            }
        }
        None
    }

    /// Land a dragged sub-tab: pull it out of its source tab (collapsing what it
    /// leaves behind), then either split the pane it was dropped on (L/R/T/B) or
    /// add it to the main tab it was dropped on. The pane is never lost — an
    /// unmatched target gives it a fresh tab of its own.
    fn perform_drop(
        &mut self,
        dragged: EntityId,
        target: DropTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // dropping a pane onto itself changes nothing
        if let DropTarget::Split { pane, .. } = &target {
            if *pane == dragged {
                return;
            }
        }
        let Some(from) = self.tabs.iter().position(|t| {
            let mut v = vec![];
            t.root.leaves(&mut v);
            v.iter().any(|e| e.entity_id() == dragged)
        }) else {
            return;
        };

        let pred = |e: &Entity<TerminalView>| e.entity_id() == dragged;
        let src = self.tabs.remove(from);
        let src_name = src.name;
        let (taken, remaining) = src.root.remove_leaf(&pred);
        let Some(pane) = taken else {
            if let Some(root) = remaining {
                self.tabs.insert(
                    from,
                    Tab {
                        root,
                        name: src_name,
                    },
                );
            }
            return;
        };
        let source_emptied = remaining.is_none();
        if let Some(root) = remaining {
            self.tabs.insert(
                from,
                Tab {
                    root,
                    name: src_name,
                },
            );
        }

        let landed = match target {
            DropTarget::Split {
                pane: target_id,
                zone,
                ..
            } => {
                let (dir, new_first) = match zone {
                    Zone::Left => (SplitDir::Row, true),
                    Zone::Right => (SplitDir::Row, false),
                    Zone::Top => (SplitDir::Col, true),
                    Zone::Bottom => (SplitDir::Col, false),
                };
                let tgt = |e: &Entity<TerminalView>| e.entity_id() == target_id;
                let mut hit = None;
                for (i, tab) in self.tabs.iter_mut().enumerate() {
                    if tab.root.split_leaf_dir(&tgt, dir, pane.clone(), new_first) {
                        hit = Some(i);
                        break;
                    }
                }
                hit
            }
            DropTarget::Tab { index, .. } => {
                // a source-tab removal shifts later indices down by one
                let t = if source_emptied && index > from {
                    index - 1
                } else {
                    index
                };
                self.tabs.get_mut(t).map(|tab| {
                    let old = std::mem::replace(&mut tab.root, Node::Leaf(pane.clone()));
                    tab.root = Node::Split {
                        id: next_split_id(),
                        dir: SplitDir::Row,
                        ratio: 0.5,
                        a: Box::new(old),
                        b: Box::new(Node::Leaf(pane.clone())),
                    };
                    t
                })
            }
        };
        let landed = landed.unwrap_or_else(|| {
            self.tabs.push(Tab {
                root: Node::Leaf(pane.clone()),
                name: None,
            });
            self.tabs.len() - 1
        });

        self.active = landed.min(self.tabs.len().saturating_sub(1));
        window.focus(&pane.focus_handle(cx), cx);
        self.save(cx);
        cx.notify();
    }

    fn reap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // drop a menu whose pane is gone
        if let Some(MenuScope::Pane(p)) = &self.theme_menu {
            if p.read(cx).exited {
                self.theme_menu = None;
            }
        }
        let had = self.pane_count();
        let tabs = std::mem::take(&mut self.tabs);
        self.tabs = tabs
            .into_iter()
            .filter_map(|t| t.root.reap(cx).map(|root| Tab { root, name: t.name }))
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

    /// The choice the open menu is editing (pane override or outer).
    fn menu_choice(&self, cx: &App) -> ThemeChoice {
        match &self.theme_menu {
            Some(MenuScope::Pane(p)) => p
                .read(cx)
                .theme_override
                .clone()
                .unwrap_or_else(|| theme::outer_choice(cx)),
            _ => theme::outer_choice(cx),
        }
    }

    /// Apply a choice to the open menu's scope. None clears a pane override
    /// (back to "follow outer").
    fn set_menu_choice(&mut self, choice: Option<ThemeChoice>, cx: &mut Context<Self>) {
        match self.theme_menu.clone() {
            Some(MenuScope::Pane(pane)) => {
                pane.update(cx, |view, cx| {
                    view.theme_override = choice;
                    cx.notify();
                });
            }
            Some(MenuScope::Outer) => {
                if let Some(choice) = choice {
                    theme::select_outer(cx, choice);
                }
            }
            None => return,
        }
        self.save(cx);
        cx.notify();
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
            .shadow(vec![glint, seat]);
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

/// Seed-colour presets for the breakout menu (IMT picker set).
const SEED_SWATCHES: &[&str] = &[
    "#2f6fdd", "#31d7ff", "#00ff9c", "#ff8a3d", "#8fa85f", "#872d73", "#828282",
];

/// Theme-icon button for the breakout menu.
fn theme_icon_btn(th: &theme::Theme, icon: &str, active: bool) -> gpui::Div {
    let b = div()
        .w(px(46.))
        .h(px(34.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .text_size(px(14.))
        .cursor_pointer();
    if active {
        b.bg(linear_gradient(
            135.,
            linear_color_stop(th.accent.alpha(0.45), 0.),
            linear_color_stop(th.accent.alpha(0.15), 1.),
        ))
        .border_color(th.accent)
        .text_color(white().alpha(0.95))
        .child(icon.to_string())
    } else {
        b.bg(darken(th.surface, 0.8))
            .border_color(th.accent.alpha(0.35))
            .text_color(th.text)
            .child(icon.to_string())
    }
}

/// Text-colour-mode button for the breakout menu: glyph over a tiny caption.
fn color_mode_btn(th: &theme::Theme, icon: &str, caption: &str, active: bool) -> gpui::Div {
    let inner = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_0()
        .child(div().text_size(px(15.)).child(icon.to_string()))
        .child(div().text_size(px(8.)).child(caption.to_string()));
    let b = div()
        .w(px(58.))
        .h(px(38.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .cursor_pointer();
    if active {
        b.bg(linear_gradient(
            135.,
            linear_color_stop(th.accent.alpha(0.45), 0.),
            linear_color_stop(th.accent.alpha(0.15), 1.),
        ))
        .border_color(th.accent)
        .text_color(white().alpha(0.95))
        .child(inner)
    } else {
        b.bg(darken(th.surface, 0.8))
            .border_color(th.accent.alpha(0.35))
            .text_color(th.text)
            .child(inner)
    }
}

/// Seed-colour swatch. `color: None` renders the rainbow "theme default" dot.
fn seed_swatch(color: Option<Hsla>, active: bool) -> gpui::Div {
    let b = div()
        .w(px(20.))
        .h(px(20.))
        .rounded_full()
        .cursor_pointer()
        .border_2();
    let b = match color {
        Some(c) => b.bg(c),
        None => b.bg(linear_gradient(
            135.,
            linear_color_stop(hsla(0., 0.9, 0.6, 1.), 0.),
            linear_color_stop(hsla(0.75, 0.9, 0.6, 1.), 1.),
        )),
    };
    if active {
        b.border_color(white().alpha(0.92))
    } else {
        b.border_color(hsla(0., 0., 0., 0.45))
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

/// Which side of `rect` the cursor `pos` is nearest — the side a dropped pane
/// will split toward. Picks the closest of the four edges (top/bottom win ties).
fn zone_of(rect: Bounds<Pixels>, pos: Point<Pixels>) -> Zone {
    let w = f32::from(rect.size.width).max(1.);
    let h = f32::from(rect.size.height).max(1.);
    let rx = (f32::from(pos.x) - f32::from(rect.origin.x)) / w;
    let ry = (f32::from(pos.y) - f32::from(rect.origin.y)) / h;
    let (dl, dr, dt, db) = (rx, 1.0 - rx, ry, 1.0 - ry);
    let m = dl.min(dr).min(dt).min(db);
    if m == dt {
        Zone::Top
    } else if m == db {
        Zone::Bottom
    } else if m == dl {
        Zone::Left
    } else {
        Zone::Right
    }
}

// the tiling tree is recursive; threading layout + drag state straight through
// reads clearer than bundling it into a context struct just to satisfy the lint
#[allow(clippy::too_many_arguments)]
fn render_node(
    node: &Node,
    th: &theme::Theme,
    focused: Option<EntityId>,
    dragging: Option<u64>,
    registry: &Arc<Mutex<std::collections::HashMap<u64, Bounds<Pixels>>>>,
    pane_reg: &Arc<Mutex<std::collections::HashMap<EntityId, Bounds<Pixels>>>>,
    drop: Option<&DropTarget>,
    cx: &mut Context<Workspace>,
) -> gpui::Div {
    match node {
        Node::Leaf(e) => {
            let id = e.entity_id();
            let is_focused = focused == Some(id);
            // highlight in the PANE's own theme (override / mode tint), not the
            // outer chrome's; shadows (not border width) so the grid never reflows
            let acc = e.read(cx).resolved_theme(cx).accent;
            // is a dragged sub-tab hovering THIS pane right now? which side?
            let drop_zone = match drop {
                Some(DropTarget::Split { pane, zone, .. }) if *pane == id => Some(*zone),
                _ => None,
            };
            let store = pane_reg.clone();
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .rounded_md()
                .border_1()
                .border_color(if is_focused { acc } else { th.faint })
                .when(is_focused, |d| {
                    d.shadow(vec![
                        // crisp 1px outer ring: reads as a double border
                        BoxShadow {
                            color: acc.alpha(0.9),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(1.),
                            inset: false,
                        },
                        // soft halo around the live tube
                        BoxShadow {
                            color: acc.alpha(0.55),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(16.),
                            spread_radius: px(2.),
                            inset: false,
                        },
                    ])
                })
                // measure this pane's box (entity → rect) for drop hit-testing
                .child(
                    div().absolute().inset_0().child(
                        canvas(
                            move |bounds, _, _| {
                                store.lock().unwrap().insert(id, bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .size_full(),
                    ),
                )
                .child(e.clone())
                // translucent slab on the side the drop will split toward
                .when_some(drop_zone, |d, zone| {
                    let slab = div().absolute().bg(th.accent.alpha(0.30));
                    let slab = match zone {
                        Zone::Left => slab.left_0().top_0().bottom_0().w(gpui::relative(0.5)),
                        Zone::Right => slab.right_0().top_0().bottom_0().w(gpui::relative(0.5)),
                        Zone::Top => slab.top_0().left_0().right_0().h(gpui::relative(0.5)),
                        Zone::Bottom => slab.bottom_0().left_0().right_0().h(gpui::relative(0.5)),
                    };
                    d.child(slab)
                })
        }
        Node::Split {
            id,
            dir,
            ratio,
            a,
            b,
        } => {
            let id = *id;
            let dir = *dir;
            let is_dragging = dragging == Some(id);
            let store = registry.clone();
            // measure this split's container so drags map to a ratio
            let measure = div().absolute().inset_0().child(
                canvas(
                    move |bounds, _, _| {
                        store.lock().unwrap().insert(id, bounds);
                    },
                    |_, _, _, _| {},
                )
                .size_full(),
            );
            // the grab handle: flat black bar; accent only while dragging
            let mut handle = div().flex_none().bg(if is_dragging {
                th.accent.alpha(0.8)
            } else {
                gpui::black()
            });
            handle = match dir {
                SplitDir::Row => handle.w(px(7.)).h_full().cursor_col_resize(),
                SplitDir::Col => handle.h(px(7.)).w_full().cursor_row_resize(),
            };

            let first = div().min_w_0().min_h_0().flex().child(render_node(
                a, th, focused, dragging, registry, pane_reg, drop, cx,
            ));
            let first = match dir {
                SplitDir::Row => first.h_full().w(gpui::relative(*ratio)),
                SplitDir::Col => first.w_full().h(gpui::relative(*ratio)),
            };
            let second = div().flex_1().min_w_0().min_h_0().flex().child(render_node(
                b, th, focused, dragging, registry, pane_reg, drop, cx,
            ));

            let ratio_now = *ratio;
            let store2 = registry.clone();
            let base = div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .relative()
                .flex()
                // proven pattern: container-level mousedown + handle-zone math
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                        if std::env::var("TD_KEYDEBUG").is_ok() {
                            eprintln!(
                                "split {id} container mousedown at {:?}, bounds {:?}",
                                ev.position,
                                ws.split_bounds
                                    .lock()
                                    .unwrap()
                                    .get(&id)
                                    .map(|b| (b.origin.x, b.size.width))
                            );
                        }
                        if ws.drag_split.is_some() {
                            return; // an inner split already claimed this press
                        }
                        let Some(b) = store2.lock().unwrap().get(&id).copied() else {
                            return;
                        };
                        let (along, extent) = match dir {
                            SplitDir::Row => (
                                f32::from(ev.position.x) - f32::from(b.origin.x),
                                f32::from(b.size.width),
                            ),
                            SplitDir::Col => (
                                f32::from(ev.position.y) - f32::from(b.origin.y),
                                f32::from(b.size.height),
                            ),
                        };
                        let strip = ratio_now * extent;
                        if along >= strip - 6. && along <= strip + 13. {
                            if std::env::var("TD_KEYDEBUG").is_ok() {
                                eprintln!("split handle grabbed: {id}");
                            }
                            ws.drag_split = Some(id);
                            cx.notify();
                        }
                    }),
                );
            let base = match dir {
                SplitDir::Row => base.flex_row(),
                SplitDir::Col => base.flex_col(),
            };
            base.child(measure).child(first).child(handle).child(second)
        }
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reap(window, cx);
        warp::begin_frame(); // visible panes re-register their tube rects below
                             // An open overlay (theme breakout / confirm dialog) flattens the glass:
                             // the warp is a pixel post-process, so a panel over a tube would bow out
                             // of reach of its own flat hit box. Suppress so the menu reads true.
        warp::set_suppressed(self.theme_menu.is_some() || self.confirm_close.is_some());
        // drop-hit-test rects are rebuilt every frame by the canvases below, so
        // a closed pane / removed tab never leaves a stale target behind.
        self.pane_bounds.lock().unwrap().clear();
        self.tab_bounds.lock().unwrap().clear();
        let wb = window.bounds();
        self.last_win = Some((
            f32::from(wb.origin.x),
            f32::from(wb.origin.y),
            f32::from(wb.size.width),
            f32::from(wb.size.height),
        ));
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
            // the per-tab close affordance — an X in the tab's own frame
            let close_x = div()
                .px_1()
                .text_size(px(12.))
                .text_color(if is_active { th.text } else { th.faint })
                .cursor_pointer()
                .child("×")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        ws.request_close_tab(i, window, cx);
                    }),
                );
            tab_strip = tab_strip.child(
                Self::bezel_btn(&th, &label, is_active)
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
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
                    )
                    .child({
                        // measure this tab button's box for "drop onto a tab"
                        let store = self.tab_bounds.clone();
                        div().absolute().inset_0().child(
                            canvas(
                                move |bounds, _, _| {
                                    store.lock().unwrap().insert(i, bounds);
                                },
                                |_, _, _, _| {},
                            )
                            .size_full(),
                        )
                    })
                    .when(
                        matches!(&self.drop_target, Some(DropTarget::Tab { index, .. }) if *index == i),
                        |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .rounded_sm()
                                    .border_1()
                                    .border_color(th.accent)
                                    .bg(th.accent.alpha(0.25)),
                            )
                        },
                    )
                    .child(close_x),
            );
        }
        tab_strip = tab_strip.child(Self::bezel_btn(&th, "+", false).on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, _: &MouseDownEvent, window, cx| ws.new_tab(window, cx)),
        ));

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
                            .shadow(vec![BoxShadow {
                                color: white().alpha(0.4),
                                offset: point(px(-1.), px(-1.)),
                                blur_radius: px(1.),
                                spread_radius: px(0.),
                                inset: true,
                            }]),
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
            .child(Self::bezel_btn(&th, "◫ split", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                    ws.split(SplitDir::Row, window, cx)
                }),
            ))
            .child(Self::bezel_btn(&th, "⬓ split", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                    ws.split(SplitDir::Col, window, cx)
                }),
            ));

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
                    .child(
                        // outer theme: the icon is the trigger for the breakout
                        Self::bezel_btn(&th, &th.icon, self.theme_menu.is_some()).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.theme_menu = Some(MenuScope::Outer);
                                ws.menu_at = None; // global menu uses the fixed anchor
                                cx.notify();
                            }),
                        ),
                    )
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
            .child(div().child(format!("{} · {}", th.icon, focused_title)))
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

        // ---- theme breakout: icon grid + seed swatches, per scope ----
        let menu_overlay = self.theme_menu.clone().map(|scope| {
            let is_pane = matches!(scope, MenuScope::Pane(_));
            let cur = self.menu_choice(cx);
            let color = cur.color;
            let has_override = match &scope {
                MenuScope::Pane(p) => p.read(cx).theme_override.is_some(),
                MenuScope::Outer => true,
            };
            let mut theme_row = div().flex().flex_row().gap_2();
            for (id, icon) in theme::all_themes(cx) {
                let active = cur.id == id;
                let seed = cur.seed.clone();
                theme_row = theme_row.child(theme_icon_btn(&th, &icon, active).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.set_menu_choice(
                            Some(ThemeChoice {
                                id: id.clone(),
                                seed: seed.clone(),
                                color,
                            }),
                            cx,
                        );
                    }),
                ));
            }
            let mut seed_row = div().flex().flex_row().items_center().gap_2();
            {
                let id = cur.id.clone();
                seed_row = seed_row.child(seed_swatch(None, cur.seed.is_none()).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.set_menu_choice(
                            Some(ThemeChoice {
                                id: id.clone(),
                                seed: None,
                                color,
                            }),
                            cx,
                        );
                    }),
                ));
            }
            for &hex in SEED_SWATCHES {
                let active = cur.seed.as_deref() == Some(hex);
                let swatch = theme::parse_hex(hex);
                let id = cur.id.clone();
                seed_row = seed_row.child(seed_swatch(swatch, active).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.set_menu_choice(
                            Some(ThemeChoice {
                                id: id.clone(),
                                seed: Some(hex.to_string()),
                                color,
                            }),
                            cx,
                        );
                    }),
                ));
            }
            let mut color_row = div().flex().flex_row().gap_2();
            for mode in theme::ColorMode::ALL {
                let active = cur.color == mode;
                let id = cur.id.clone();
                let seed = cur.seed.clone();
                color_row = color_row.child(
                    color_mode_btn(&th, mode.icon(), mode.caption(), active).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.set_menu_choice(
                                Some(ThemeChoice {
                                    id: id.clone(),
                                    seed: seed.clone(),
                                    color: mode,
                                }),
                                cx,
                            );
                        }),
                    ),
                );
            }
            let label = |s: &str| {
                div()
                    .text_size(px(9.))
                    .text_color(th.text.alpha(0.55))
                    .child(s.to_string())
            };
            // A sub-tab icon click anchors the tray at the click (right edge at
            // the cursor, opening down-left like the global menu); clamp it fully
            // on-screen. The global/outer menu (menu_at == None) keeps its fixed
            // top-right anchor under the titlebar control.
            const PANEL_W: f32 = 286.;
            const PANEL_H_EST: f32 = 240.; // generous, incl. the follow-outer row
            let mut panel = div().absolute().w(px(PANEL_W));
            panel = match self.menu_at {
                Some(at) => {
                    let vp = window.viewport_size();
                    let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
                    let right = (vw - f32::from(at.x)).clamp(8., (vw - PANEL_W - 8.).max(8.));
                    let top = (f32::from(at.y) + 6.).clamp(8., (vh - PANEL_H_EST - 8.).max(8.));
                    panel.right(px(right)).top(px(top))
                }
                None => panel.top(px(36.)).right(px(150.)),
            };
            panel = panel
                .p_3()
                .rounded_md()
                .border_1()
                .border_color(th.accent.alpha(0.55))
                .bg(darken(th.surface, 0.6))
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.6),
                    offset: point(px(4.), px(6.)),
                    blur_radius: px(18.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .flex()
                .flex_col()
                .gap_2()
                .text_size(px(10.))
                .text_color(th.text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(label(if is_pane {
                    "THEME — THIS PANE"
                } else {
                    "THEME — OUTER"
                }))
                .child(theme_row)
                .child(label("SEED COLOUR"))
                .child(seed_row)
                .child(label("TEXT COLOUR"))
                .child(color_row);
            if is_pane {
                panel = panel.child(
                    Self::bezel_btn(&th, "follow outer", !has_override).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.set_menu_choice(None, cx);
                        }),
                    ),
                );
            }
            // full-screen scrim: click anywhere outside closes
            div()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.theme_menu = None;
                        cx.notify();
                    }),
                )
                .child(panel)
        });

        // ---- confirm overlay: closing a tab that holds more than one pane ----
        let confirm_overlay = self.confirm_close.and_then(|i| {
            let name = self
                .tabs
                .get(i)?
                .name
                .clone()
                .unwrap_or_else(|| format!("tab {}", i + 1));
            let n = self.tab_pane_count(i);
            let danger = hsla(0., 0.72, 0.60, 1.);
            let confirm_btn = div()
                .px_2()
                .py_0p5()
                .rounded_sm()
                .border_1()
                .border_color(danger)
                .bg(danger.alpha(0.18))
                .text_color(white().alpha(0.95))
                .text_size(px(11.))
                .cursor_pointer()
                .child(format!("CLOSE {n} PANES"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        ws.close_tab(i, window, cx);
                    }),
                );
            let cancel_btn = Self::bezel_btn(&th, "CANCEL", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.confirm_close = None;
                    cx.notify();
                }),
            );
            let panel = div()
                .w(px(340.))
                .p_4()
                .rounded_md()
                .border_1()
                .border_color(danger.alpha(0.7))
                .bg(darken(th.surface, 0.6))
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.6),
                    offset: point(px(4.), px(6.)),
                    blur_radius: px(18.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .flex()
                .flex_col()
                .gap_3()
                .text_color(th.text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .text_color(danger)
                        .child("CLOSE TAB?"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(th.text.alpha(0.8))
                        .child(format!(
                            "\"{name}\" holds {n} panes — closing it ends all {n} shells."
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_end()
                        .gap_2()
                        .child(cancel_btn)
                        .child(confirm_btn),
                );
            // centered scrim; click outside cancels
            Some(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            ws.confirm_close = None;
                            cx.notify();
                        }),
                    )
                    .child(panel),
            )
        });

        // a small chip trails the cursor while a sub-tab is being dragged
        let drag_chip = self.drag_pane.as_ref().filter(|d| d.engaged).map(|d| {
            div()
                .absolute()
                .left(px(f32::from(d.at.x) + 12.))
                .top(px(f32::from(d.at.y) + 12.))
                .px_2()
                .py_0p5()
                .rounded_sm()
                .bg(th.accent.alpha(0.9))
                .text_color(th.bg)
                .text_size(px(10.5))
                .child("⇲ moving sub-tab — drop on a pane or tab")
        });

        let dragging = self.drag_split;
        let registry = self.split_bounds.clone();
        let pane_reg = self.pane_bounds.clone();
        let drop = self.drop_target.clone();
        let pane_area = div().size_full().flex().p(px(3.)).child(render_node(
            &tab.root,
            &th,
            focused_id,
            dragging,
            &registry,
            &pane_reg,
            drop.as_ref(),
            cx,
        ));

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
                    .relative()
                    .rounded(px(14.))
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(brighten(bezel, 1.6), 0.),
                        linear_color_stop(darken(bezel, 0.8), 1.),
                    ))
                    .border_2()
                    .border_color(th.accent.alpha(0.45))
                    .shadow(vec![
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
                    ])
                    .child(bezel_top)
                    .child(screen)
                    .child(bezel_bottom)
                    .children(menu_overlay)
                    .children(confirm_overlay)
                    .children(drag_chip),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf_ids(t: &Tree<u32>) -> Vec<u32> {
        let mut v = vec![];
        t.leaves(&mut v);
        v.into_iter().copied().collect()
    }

    #[test]
    fn split_divides_only_the_target_leaf() {
        let mut t: Tree<u32> = Tree::Leaf(1);
        assert!(t.split_leaf(&|l| *l == 1, SplitDir::Row, 2));
        assert!(t.split_leaf(&|l| *l == 2, SplitDir::Col, 3));
        assert_eq!(leaf_ids(&t), vec![1, 2, 3]);
        // leaf 1 keeps its exact place: the root split's `a` arm is untouched
        let Tree::Split {
            a,
            dir: SplitDir::Row,
            ..
        } = &t
        else {
            panic!("root must stay a Row split");
        };
        assert!(matches!(**a, Tree::Leaf(1)));
        // a miss leaves the tree untouched
        assert!(!t.split_leaf(&|l| *l == 99, SplitDir::Row, 4));
        assert_eq!(leaf_ids(&t), vec![1, 2, 3]);
    }

    #[test]
    fn reap_collapses_splits_down_to_the_survivors() {
        let mut t: Tree<u32> = Tree::Leaf(1);
        t.split_leaf(&|l| *l == 1, SplitDir::Row, 2);
        t.split_leaf(&|l| *l == 2, SplitDir::Col, 3);
        let t = t.reap_where(&|l| *l == 2).expect("two survivors");
        assert_eq!(leaf_ids(&t), vec![1, 3]);
        let t = t.reap_where(&|l| *l == 3).expect("one survivor");
        assert!(matches!(t, Tree::Leaf(1)));
        assert!(t.reap_where(&|_| true).is_none());
    }

    #[test]
    fn ratio_clamps_and_targets_the_right_split() {
        let mut t: Tree<u32> = Tree::Leaf(1);
        t.split_leaf(&|l| *l == 1, SplitDir::Row, 2);
        let Tree::Split { id, .. } = &t else {
            panic!("split expected");
        };
        let id = *id;
        assert_eq!(t.dir_of(id), Some(SplitDir::Row));
        assert!(t.set_ratio(id, 0.99));
        let Tree::Split { ratio, .. } = &t else {
            panic!()
        };
        assert!((ratio - 0.85).abs() < f32::EPSILON, "clamped to 0.85");
        assert!(!t.set_ratio(id + 999, 0.5));
        assert_eq!(t.dir_of(id + 999), None);
    }

    #[test]
    fn to_saved_carries_per_leaf_theme_overrides() {
        let mut t: Tree<u32> = Tree::Leaf(1);
        t.split_leaf(&|l| *l == 1, SplitDir::Col, 2);
        let saved = t.to_saved_with(&|l| LeafState {
            theme: (*l == 2).then(|| ThemeChoice {
                id: "hacker".into(),
                seed: None,
                ..Default::default()
            }),
            ..Default::default()
        });
        let SavedNode::Split { a, b, .. } = &saved else {
            panic!("split expected");
        };
        assert!(matches!(**a, SavedNode::Leaf { theme: None, .. }));
        let SavedNode::Leaf { theme: Some(t), .. } = &**b else {
            panic!("override lost");
        };
        assert_eq!(t.id, "hacker");
    }

    #[test]
    fn legacy_state_file_with_string_leaf_still_loads() {
        let legacy = r#"
active = 0
[[tabs]]
node = "Leaf"
[[tabs]]
[tabs.node.Split]
dir = "Row"
ratio = 0.5
a = "Leaf"
b = "Leaf"
"#;
        let state: StateFile = toml::from_str(legacy).expect("legacy state parses");
        assert_eq!(state.tabs.len(), 2);
        assert!(matches!(
            state.tabs[0].node,
            SavedNode::Leaf { theme: None, .. }
        ));
    }

    #[test]
    fn state_with_pane_theme_round_trips() {
        let state = StateFile {
            active: 0,
            win: Some((12.0, 34.0, 1280.0, 720.0)),
            scale: Some(1.0),
            theme: Some(ThemeChoice {
                id: "tactical-overdrive".into(),
                seed: Some("#31d7ff".into()),
                ..Default::default()
            }),
            tabs: vec![SavedTab {
                name: None,
                node: SavedNode::Leaf {
                    theme: Some(ThemeChoice {
                        id: "hacker".into(),
                        seed: None,
                        ..Default::default()
                    }),
                    cwd: None,
                    resume: None,
                },
            }],
        };
        let body = toml::to_string(&state).expect("serializes");
        let back: StateFile = toml::from_str(&body).expect("round-trips");
        assert_eq!(back.theme.as_ref().unwrap().id, "tactical-overdrive");
        assert_eq!(back.win, Some((12.0, 34.0, 1280.0, 720.0)));
        let SavedNode::Leaf { theme: Some(t), .. } = &back.tabs[0].node else {
            panic!("leaf override lost");
        };
        assert_eq!(t.id, "hacker");
    }

    #[test]
    fn leaf_work_state_round_trips() {
        let state = StateFile {
            active: 0,
            win: None,
            scale: None,
            theme: None,
            tabs: vec![SavedTab {
                name: Some("agents".into()),
                node: SavedNode::Leaf {
                    theme: None,
                    cwd: Some("/home/user/proj".into()),
                    resume: Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d".into()),
                },
            }],
        };
        let body = toml::to_string(&state).expect("serializes");
        let back: StateFile = toml::from_str(&body).expect("round-trips");
        let SavedNode::Leaf { cwd, resume, .. } = &back.tabs[0].node else {
            panic!("leaf lost");
        };
        assert_eq!(cwd.as_deref(), Some("/home/user/proj"));
        assert_eq!(
            resume.as_deref(),
            Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d")
        );
    }

    #[test]
    fn split_leaf_dir_places_the_dropped_pane_on_the_chosen_side() {
        let mut t: Tree<u32> = Tree::Leaf(1);
        // drop 2 onto the LEADING side of leaf 1 (a Left / Top drop)
        assert!(t.split_leaf_dir(&|l| *l == 1, SplitDir::Row, 2, true));
        let Tree::Split {
            a,
            b,
            dir: SplitDir::Row,
            ..
        } = &t
        else {
            panic!("row split expected");
        };
        assert!(matches!(**a, Tree::Leaf(2)), "dropped pane must lead");
        assert!(matches!(**b, Tree::Leaf(1)));
        // a trailing drop on a deeper leaf keeps left-to-right reading order
        assert!(t.split_leaf_dir(&|l| *l == 1, SplitDir::Col, 3, false));
        assert_eq!(leaf_ids(&t), vec![2, 1, 3]);
        // a miss changes nothing
        assert!(!t.split_leaf_dir(&|l| *l == 99, SplitDir::Row, 4, true));
        assert_eq!(leaf_ids(&t), vec![2, 1, 3]);
    }

    #[test]
    fn remove_leaf_collapses_the_parent_onto_the_sibling() {
        let mut t: Tree<u32> = Tree::Leaf(1);
        t.split_leaf(&|l| *l == 1, SplitDir::Row, 2);
        t.split_leaf(&|l| *l == 2, SplitDir::Col, 3); // 1 | (2 / 3)
        let (taken, rest) = t.remove_leaf(&|l| *l == 2);
        assert_eq!(taken, Some(2));
        let rest = rest.expect("two survivors remain");
        assert_eq!(leaf_ids(&rest), vec![1, 3]);
        // removing the sole leaf empties the tree (source tab would close)
        let (taken, rest) = Tree::<u32>::Leaf(7).remove_leaf(&|l| *l == 7);
        assert_eq!(taken, Some(7));
        assert!(rest.is_none());
        // a miss hands the tree back untouched
        let (taken, rest) = Tree::<u32>::Leaf(9).remove_leaf(&|l| *l == 0);
        assert_eq!(taken, None);
        assert_eq!(leaf_ids(&rest.unwrap()), vec![9]);
    }
}

fn main() {
    application().run(|cx: &mut App| {
        theme::init(cx);
        // reboot into the exact window the user closed (or crashed) from
        let bounds = match load_state().win {
            Some((x, y, w, h)) => Bounds {
                origin: point(px(x), px(y)),
                size: size(px(w.max(480.)), px(h.max(320.))),
            },
            None => Bounds::centered(None, size(px(1280.), px(720.)), cx),
        };
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("terminal-delight".into()),
                    ..Default::default()
                }),
                // WM_CLASS / Wayland app_id — must match terminal-delight.desktop
                // (packaging/) for the dock to pick up our icon.
                app_id: Some("terminal-delight".into()),
                ..Default::default()
            },
            |window, cx| cx.new(|cx| Workspace::new(window, cx)),
        )
        .expect("open window");
        cx.activate(true);
    });
}
