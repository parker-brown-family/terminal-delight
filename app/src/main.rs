//! terminal-delight — tiling tree · tabs · device bezel · menu-bar size scrubber.
//!
//! Splits divide ONLY the focused terminal's space (true tiling tree); every
//! other pane keeps its exact place. ctrl+shift+t / [+]: new tab ·
//! ctrl+pgup/pgdn: switch · right-click tab: rename · alt+arrows: pane focus
//! drag a tab to reorder · ctrl+click a tab: set its binder-divider colour
//! 👓 on a sub-tab header: FOCUS — mirror that pane big, rest dimmed, esc closes
//! (alt+↑/↓ jumps between your messages in a claude/codex pane) ·
//! ctrl+scroll or the bezel scrubber: menu-bar size.
//!
//! TODO(os-chrome): client-side window decorations (WindowDecorations::Client).

mod bell;
mod crt;
mod csd;
mod gamba;
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
    canvas, div, fill, hsla, linear_color_stop, linear_gradient, point, prelude::*, px, size,
    white, App, Bounds, BoxShadow, Context, Decorations, Entity, EntityId, Focusable, Hsla,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    ScrollWheelEvent, SharedString, TitlebarOptions, Window, WindowBounds, WindowDecorations,
    WindowOptions,
};
use gpui_platform::application;
use pane::{
    CloseFocusRead, ClosePane, DragPaneStart, OpenDisplayMenu, OpenFocusRead, OpenHelp,
    OpenThemeMenu, PaneRenamed, TerminalView,
};
use serde::{Deserialize, Serialize};
use theme::{PaneTheme, ThemeChoice};

const MAX_PANES: usize = 8;

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

    /// Wrap the *container* (split) whose id is `target` in a new directional
    /// split with `new` — i.e. re-frame a whole sub-region (or the root) rather
    /// than just one leaf. This is the "drag to the field edge → resplit the
    /// entire field" gesture: it fractals, because every nesting level is a
    /// container with its own id. `new_first` puts `new` on the leading side.
    fn split_node_at(&mut self, target: u64, dir: SplitDir, new: L, new_first: bool) -> bool {
        match self {
            Tree::Split { id, .. } if *id == target => {
                // momentary placeholder while we re-parent the matched subtree
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
                a.split_node_at(target, dir, new.clone(), new_first)
                    || b.split_node_at(target, dir, new, new_first)
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
                    appearance: s.appearance,
                    cwd: s.cwd,
                    resume: s.resume,
                    name: s.name,
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
                appearance: view.appearance.clone(),
                cwd: rt.cwd,
                resume: rt.resume,
                name: view.name.clone(),
            }
        })
    }
}

/// Everything a leaf carries into the state file: appearance + live work
/// (cwd and a resumable agent session, captured from the kernel at save time).
#[derive(Default)]
struct LeafState {
    appearance: PaneTheme,
    cwd: Option<String>,
    resume: Option<String>,
    name: Option<String>,
}

// A transient (de)serialization DTO for the layout tree — built, written, and
// dropped, never held hot in a Vec — so the Leaf/Split size gap is fine; boxing
// the leaf payload would only fight serde's `skip_serializing_if`.
#[allow(clippy::large_enum_variant)]
#[derive(Serialize)]
enum SavedNode {
    Leaf {
        #[serde(default, skip_serializing_if = "PaneTheme::is_pristine")]
        appearance: PaneTheme,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resume: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
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
            appearance: Option<PaneTheme>,
            /// Legacy single full-pane override (pre per-group inherit). Read for
            /// migration only; never written — see [`PaneTheme::from_legacy`].
            #[serde(default)]
            theme: Option<ThemeChoice>,
            #[serde(default)]
            cwd: Option<String>,
            #[serde(default)]
            resume: Option<String>,
            #[serde(default)]
            name: Option<String>,
        }
        // A leaf's appearance: the new per-group form if present, else migrate a
        // legacy `theme` override, else pristine (follows outer for everything).
        fn leaf_appearance(f: &mut LeafFields) -> PaneTheme {
            f.appearance
                .take()
                .or_else(|| f.theme.take().map(PaneTheme::from_legacy))
                .unwrap_or_default()
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
                        appearance: PaneTheme::default(),
                        cwd: None,
                        resume: None,
                        name: None,
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
                        let mut f: LeafFields = map.next_value()?;
                        Ok(SavedNode::Leaf {
                            appearance: leaf_appearance(&mut f),
                            cwd: f.cwd.take(),
                            resume: f.resume.take(),
                            name: f.name.take(),
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

/// First-run invitation: a fresh window names its very first tab AND that tab's
/// sole sub-terminal with this hint, so the right-click-to-rename gesture teaches
/// itself. Every other tab / split opened afterwards gets the normal default.
const FIRST_RUN_HINT: &str = "RIGHT CLICK TO RENAME";

/// The fixed "binder divider" palette offered in a tab's colour tray — (hue,
/// saturation, lightness). Saturated-but-muted so white outer-bar text stays
/// legible on top. A stable, named set keeps tabs consistent: pink stays pink.
const TAB_SWATCHES: &[(f32, f32, f32)] = &[
    (0.00, 0.58, 0.50), // red
    (0.06, 0.62, 0.50), // orange
    (0.13, 0.62, 0.46), // amber
    (0.33, 0.50, 0.42), // green
    (0.47, 0.48, 0.42), // teal
    (0.57, 0.55, 0.48), // blue
    (0.68, 0.45, 0.52), // indigo
    (0.78, 0.42, 0.52), // violet
    (0.92, 0.55, 0.55), // pink
];

struct Tab {
    root: Node,
    name: Option<String>,
    /// The pane that last held focus in this tab — so revisiting the tab (a
    /// mother-bar click) lands on the terminal you were last in, not always the
    /// first. Refreshed each render from the live focus; never persisted.
    focused: Option<EntityId>,
    /// The "binder divider" colour for THIS tab's button — a stable property of
    /// the tab itself, NOT derived from any pane's theme. Set via ctrl+click, it
    /// never shifts when a sub-terminal overrides its look. `None` = the plain
    /// bezel button. Persisted as a hex string in the state file.
    color: Option<Hsla>,
}

impl Tab {
    fn new(root: Node, name: Option<String>) -> Self {
        Self {
            root,
            name,
            focused: None,
            color: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
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
    /// Global screen-warp (CRT barrel) amount — orthogonal to the theme, 0 = flat.
    /// Accepts a legacy bool (the old toggle: true→default dial, false→0).
    #[serde(default = "default_warp", deserialize_with = "de_warp")]
    warp: f32,
    /// Global CRT tracking-band override `[intensity, speed, size]` in 0..1, or
    /// absent = per-theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    track: Option<[f32; 3]>,
    tabs: Vec<SavedTab>,
}

fn default_warp() -> f32 {
    theme::WARP_DEFAULT
}

/// Lenient warp deserialize: a number is the dial; an old bool toggle maps
/// true→the default dial, false→flat. Keeps pre-slider state files loading.
fn de_warp<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f32, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrF32 {
        B(bool),
        F(f32),
    }
    Ok(match BoolOrF32::deserialize(d)? {
        BoolOrF32::B(true) => theme::WARP_DEFAULT,
        BoolOrF32::B(false) => 0.0,
        BoolOrF32::F(f) => f.clamp(0.0, theme::WARP_MAX),
    })
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            active: 0,
            win: None,
            scale: None,
            theme: None,
            warp: theme::WARP_DEFAULT, // fresh install: the classic dial
            track: None,
            tabs: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SavedTab {
    #[serde(default)]
    name: Option<String>,
    /// Per-tab "binder divider" colour as a hex string (e.g. `#3a8f4d`).
    /// Absent on pre-feature state files → the tab is uncoloured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
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

/// Which colour the breakout's wheel currently edits. The seed drives the whole
/// palette; `Text` and `Complement` are explicit overrides of the body text and
/// the title's complement colour respectively.
#[derive(Clone, Copy, PartialEq)]
enum WheelTarget {
    Seed,
    Text,
    Complement,
    /// The colour of the user's own input in an agent (claude/codex) session.
    Human,
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
    /// Split the pane `pane` on its `zone` side (an interior, leaf-level drop).
    Split { pane: EntityId, zone: Zone },
    /// Re-frame a whole container (the split id `container`, which may be the
    /// root) on its `zone` side — the "drag to the field edge" gesture. Fractals
    /// down the nesting because every level is a container with its own id.
    Edge { container: u64, zone: Zone },
    /// Move the dragged pane into main tab `index`.
    Tab { index: usize },
}

/// How a `Zone` maps to a split: which axis, and whether the dropped pane takes
/// the leading (left / top) side. Shared by leaf splits and container re-frames.
fn split_for(zone: Zone) -> (SplitDir, bool) {
    match zone {
        Zone::Left => (SplitDir::Row, true),
        Zone::Right => (SplitDir::Row, false),
        Zone::Top => (SplitDir::Col, true),
        Zone::Bottom => (SplitDir::Col, false),
    }
}

/// True when `pos` sits within the outer `band` (px) of `rect`'s perimeter —
/// the frame where a drop re-frames that whole container instead of splitting a
/// leaf. The band is clamped so a small container keeps a usable interior.
fn near_perimeter(rect: Bounds<Pixels>, pos: Point<Pixels>, band: f32) -> bool {
    let w = f32::from(rect.size.width).max(1.);
    let h = f32::from(rect.size.height).max(1.);
    let m = band.min(0.45 * w).min(0.45 * h);
    let l = f32::from(pos.x) - f32::from(rect.origin.x);
    let r = w - l;
    let t = f32::from(pos.y) - f32::from(rect.origin.y);
    let b = h - t;
    l.min(r).min(t).min(b) <= m
}

/// The re-frame band width (px) for the outer-edge gesture, scaled to the
/// container so the field gets a generous frame and small splits a thin one.
fn edge_band(rect: Bounds<Pixels>) -> f32 {
    let w = f32::from(rect.size.width).max(1.);
    let h = f32::from(rect.size.height).max(1.);
    (0.18 * w.min(h)).clamp(12., 34.)
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
    /// True while the cursor is currently outside the window — a release there
    /// tears the pane off into a brand-new window of its own.
    left_window: bool,
}

/// An OUTER tab being dragged along the mother bar to reorder it. Distinct from
/// `PaneDrag` (which moves a terminal between tabs); this just slides a tab
/// button left/right to a new slot in the strip.
struct TabDrag {
    /// The index of the tab grabbed when the drag began.
    from: usize,
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
    /// The ? help modal is open (keys/commands reference), themed by the outer.
    help_open: bool,
    /// Open theme breakout menu, if any.
    theme_menu: Option<MenuScope>,
    /// Window-space point to anchor the open tray at (a sub-tab icon click).
    /// None = the fixed top-right anchor used by the global/outer menu.
    menu_at: Option<Point<Pixels>>,
    /// Open monitor-OSD (display) tray, if any — same scope model as `theme_menu`.
    osd_menu: Option<MenuScope>,
    /// Window-space anchor for the open OSD tray (a pane display-icon click).
    osd_at: Option<Point<Pixels>>,
    /// The OSD slider being dragged, if any (which channel).
    slider_drag: Option<theme::GradeKey>,
    /// Live per-slider track rects for ratio math during a drag.
    slider_bounds: Arc<Mutex<std::collections::HashMap<theme::GradeKey, Bounds<Pixels>>>>,
    /// Which wheel marker (seed / text / complement) is being dragged, if any.
    /// The three markers live on the wheel; you grab one and drag it around.
    wheel_drag: Option<WheelTarget>,
    /// The marker the lightness slider edits — the one most recently grabbed.
    wheel_active: WheelTarget,
    /// True while the lightness slider (white↔black) is being dragged.
    light_drag: bool,
    /// Live lightness-slider rect, for ratio math during a drag.
    light_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// True while the global screen-warp slider is being dragged.
    warp_drag: bool,
    /// Live warp-slider rect, for ratio math during a drag.
    warp_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// Which tracking-band slider (0=intensity,1=speed,2=size) is being dragged.
    track_drag: Option<usize>,
    /// Live tracking-slider rects, for ratio math during a drag.
    track_bounds: [Arc<Mutex<Option<Bounds<Pixels>>>>; 3],
    /// Live colour-wheel rect, for polar hit-testing during a drag.
    wheel_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
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
    /// An outer tab being dragged along the strip to reorder it, if any.
    tab_drag: Option<TabDrag>,
    /// The insertion slot (0..=len) a tab-reorder release would land in.
    tab_drop: Option<usize>,
    /// Which tab's colour tray is open, if any (ctrl+click a tab opens it).
    tab_color_edit: Option<usize>,
    /// Window-space anchor for the open colour tray (the ctrl+click point).
    tab_color_at: Option<Point<Pixels>>,
    /// The pane currently mirrored in the FOCUS reading modal, if any. Weak so a
    /// closed pane (its × / shell exit) drops normally — the modal just vanishes.
    focus_read: Option<gpui::WeakEntity<TerminalView>>,
    /// A scratch window (opened while another instance is already running, or a
    /// torn-off pane): one fresh terminal, never restores or persists session
    /// state — so it can't clobber the primary window's saved layout.
    scratch: bool,
    /// Frameless drag latch: a mousedown on the mother bar arms it; the first
    /// mouse-move while armed hands off to the compositor's window-move (so a
    /// plain click on the bar doesn't get eaten). Cleared on mouse-up.
    should_move: bool,
}

fn make_pane(window: &mut Window, cx: &mut Context<Workspace>) -> Entity<TerminalView> {
    // A brand-new terminal ships as the green phosphor CRT, pinned so it does not
    // follow the warm outer cabinet (green screens in a wooden cabinet).
    let pane = make_pane_restored(session::PaneRestore::default(), window, cx);
    pane.update(cx, |view, _| view.appearance = PaneTheme::house());
    pane
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
    // the header display icon → open this pane's monitor-OSD tray at the click
    cx.subscribe(&pane, |ws, pane, ev: &OpenDisplayMenu, cx| {
        ws.osd_menu = Some(MenuScope::Pane(pane));
        ws.osd_at = Some(ev.at);
        cx.notify();
    })
    .detach();
    // the header 👓 → open the FOCUS reading modal mirroring this pane (and keep
    // typing into it: we focus the pane so keystrokes still land in the original)
    cx.subscribe_in(
        &pane,
        window,
        |ws, pane, _ev: &OpenFocusRead, window, cx| {
            ws.open_focus_read(pane.clone(), window, cx);
        },
    )
    .detach();
    // Esc inside the modal (routed up from the mirrored pane) → close it
    cx.subscribe(&pane, |ws, _pane, _ev: &CloseFocusRead, cx| {
        ws.close_focus_read(cx);
    })
    .detach();
    // F1 in any pane toggles the help modal
    cx.subscribe(&pane, |ws, _pane, _ev: &OpenHelp, cx| {
        ws.help_open = !ws.help_open;
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
            left_window: false,
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
    // a committed rename → persist so the custom name survives a restart
    cx.subscribe(&pane, |ws, _pane, _ev: &PaneRenamed, cx| {
        ws.save(cx);
    })
    .detach();
    window.focus(&pane.focus_handle(cx), cx);
    pane
}

fn build_node(saved: &SavedNode, window: &mut Window, cx: &mut Context<Workspace>) -> Node {
    match saved {
        SavedNode::Leaf {
            appearance,
            cwd,
            resume,
            name,
        } => {
            let pane = make_pane_restored(
                session::PaneRestore {
                    cwd: cwd.clone(),
                    resume: resume.clone(),
                },
                window,
                cx,
            );
            if !appearance.is_pristine() || name.is_some() {
                let appearance = appearance.clone();
                let name = name.clone();
                pane.update(cx, |view, _| {
                    view.appearance = appearance;
                    if name.is_some() {
                        view.name = name;
                    }
                });
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
    /// The primary window: restore the saved layout (or open a single fresh tab)
    /// and persist changes back to disk.
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build(false, None, window, cx)
    }

    /// A scratch window: one fresh terminal (optionally seeded with a cwd/agent
    /// session for a torn-off pane), no restore, no persistence. Opened when the
    /// hotkey fires while a primary is already running, or on a drag-out pop-out.
    fn new_scratch(
        seed: Option<session::PaneRestore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(true, seed, window, cx)
    }

    fn build(
        scratch: bool,
        seed: Option<session::PaneRestore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let saved = load_state();
        // scale + theme are read even in scratch mode so a quick window still
        // looks like the rest of the session; only the *layout* is skipped.
        // Text size now lives in the outer grade (`grade.scale`); fold a legacy
        // top-level `scale` from older state files into it on load.
        let mut outer = saved.theme.clone().unwrap_or_else(theme::house_outer);
        if let Some(s) = saved.scale {
            outer.grade.scale = s.clamp(0.7, 1.6);
        }
        theme::set_screen_warp(cx, saved.warp); // restore the global warp dial
        theme::set_tracking_dial(cx, saved.track); // restore the global tracking dial
        theme::select_outer(cx, outer);
        let mut ws = Self {
            tabs: vec![],
            active: 0,
            focus_handle: cx.focus_handle(),
            renaming: None,
            confirm_close: None,
            help_open: false,
            theme_menu: None,
            menu_at: None,
            osd_menu: None,
            osd_at: None,
            slider_drag: None,
            slider_bounds: Arc::new(Mutex::new(std::collections::HashMap::new())),
            wheel_drag: None,
            wheel_active: WheelTarget::Seed,
            light_drag: false,
            light_bounds: Arc::new(Mutex::new(None)),
            warp_drag: false,
            warp_bounds: Arc::new(Mutex::new(None)),
            track_drag: None,
            track_bounds: Default::default(),
            wheel_bounds: Arc::new(Mutex::new(None)),
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
            tab_drag: None,
            tab_drop: None,
            tab_color_edit: None,
            tab_color_at: None,
            focus_read: None,
            scratch,
            should_move: false,
        };
        if scratch {
            // one terminal, seeded if this is a torn-off pane
            let pane = match seed {
                Some(restore) => make_pane_restored(restore, window, cx),
                None => make_pane(window, cx),
            };
            ws.tabs.push(Tab::new(Node::Leaf(pane), None));
            ws.active = 0;
            ws.focus_active(window, cx);
        } else if saved.tabs.is_empty() {
            ws.new_tab(window, cx);
            // Fresh window: seed the rename hint onto the first tab + its sole
            // sub-terminal (and only those — later tabs/splits stay default).
            let mut leaves: Vec<&Entity<TerminalView>> = vec![];
            if let Some(tab) = ws.tabs.first() {
                tab.root.leaves(&mut leaves);
            }
            let first_pane = leaves.first().map(|p| (*p).clone());
            if let Some(tab) = ws.tabs.first_mut() {
                tab.name = Some(FIRST_RUN_HINT.into());
            }
            if let Some(pane) = first_pane {
                pane.update(cx, |v, _| v.name = Some(FIRST_RUN_HINT.into()));
            }
        } else {
            for t in &saved.tabs {
                let root = build_node(&t.node, window, cx);
                let mut tab = Tab::new(root, t.name.clone());
                tab.color = t.color.as_deref().and_then(theme::parse_hex);
                ws.tabs.push(tab);
            }
            ws.active = saved.active.min(ws.tabs.len() - 1);
            ws.focus_active(window, cx);
        }
        // frame jiggle clock (cheap idle poll). MUST stop when this window's
        // Workspace is dropped — otherwise every opened-then-closed window
        // (scratch, tear-off) leaves an orphan 60ms task waking forever on a
        // dead entity, and idle CPU climbs over a session.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(60))
                .await;
            if this
                .update(cx, |ws: &mut Workspace, cx| {
                    if ws.jiggle.tick() {
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        })
        .detach();
        // session checkpoint: live state (pane cwds, agent sessions, window
        // bounds) changes without structural events, so re-snapshot every 5
        // minutes — a crash loses at most that much recency, never the layout.
        // Scratch windows never persist, so they skip the checkpoint entirely.
        if !scratch {
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
        }
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
        // a scratch / torn-off window must never overwrite the primary's layout
        if self.scratch {
            return;
        }
        let state = StateFile {
            active: self.active,
            win: self.last_win,
            // Kept for backward-compat with readers of the old top-level field;
            // the source of truth is now `theme.grade.scale`.
            scale: Some(theme::outer_choice(cx).grade.scale),
            theme: Some(theme::outer_choice(cx)),
            warp: theme::screen_warp(cx),
            track: theme::tracking_dial(cx),
            tabs: self
                .tabs
                .iter()
                .map(|t| SavedTab {
                    name: t.name.clone(),
                    color: t.color.map(hsla_to_hex),
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
        self.tabs.push(Tab::new(Node::Leaf(pane), None));
        self.active = self.tabs.len() - 1;
        // make_pane focuses the pane at creation, but that doesn't stick before
        // it's mounted under the new tab — re-assert focus now so the very next
        // keystroke lands in the fresh terminal (matches activate_tab/split).
        self.focus_active(window, cx);
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
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let mut leaves = vec![];
        tab.root.leaves(&mut leaves);
        // The cap matches the CRT warp's 8-tube shader limit, which only ever
        // applies to the VISIBLE (active-tab) panes — so it's per active tab, not
        // global. (A global count silently blocked splits once enough panes were
        // open across *other* tabs.)
        if leaves.len() >= MAX_PANES {
            return;
        }
        let target_pane = leaves
            .iter()
            .find(|p| p.focus_handle(cx).is_focused(window))
            .or_else(|| leaves.first())
            .cloned();
        let Some(target_pane) = target_pane else {
            return;
        };
        let target = target_pane.entity_id();
        // inherit the split pane's live working directory — a split stays in the
        // same project (TAB = project), instead of dropping back to $HOME.
        let cwd = target_pane.read(cx).runtime().cwd;
        let new_pane = make_pane_restored(session::PaneRestore { cwd, resume: None }, window, cx);
        // Keep a handle so we can focus it AFTER it's mounted in the tree —
        // make_pane's focus-at-creation doesn't stick before the split inserts it.
        let fresh = new_pane.clone();
        self.tabs[self.active]
            .root
            .split_leaf(&|p| p.entity_id() == target, dir, new_pane);
        // Defer the focus: a bezel "split" click is still being dispatched, and
        // the root container's tracked focus handle would otherwise grab focus
        // back as the event bubbles (same race as activate_tab). Running after
        // the event settles makes the NEW pane focus stick — so it lights up as
        // the active terminal and the next keystroke lands in it.
        cx.defer_in(window, move |_ws, window, cx| {
            window.focus(&fresh.focus_handle(cx), cx);
            cx.notify();
        });
        self.save(cx);
        cx.notify();
    }

    /// Which insertion slot (0..=len) a tab-reorder release at window-x `x` would
    /// land in: count the tab buttons whose horizontal centre sits left of the
    /// cursor. Reads the same live `tab_bounds` rects the pane-drop uses.
    fn resolve_tab_slot(&self, x: Pixels) -> usize {
        let map = self.tab_bounds.lock().unwrap();
        let mut slot = 0;
        for i in 0..self.tabs.len() {
            if let Some(r) = map.get(&i) {
                let mid = f32::from(r.origin.x) + f32::from(r.size.width) / 2.0;
                if f32::from(x) > mid {
                    slot = i + 1;
                }
            }
        }
        slot
    }

    /// Slide outer tab `from` to insertion slot `to` (in the pre-removal index
    /// space, 0..=len). Keeps `self.active` pointing at the very same tab it did
    /// before, whether or not the moved tab was the active one.
    fn move_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        if from >= self.tabs.len() {
            return;
        }
        let (dest, new_active) = reorder_indices(from, to, self.tabs.len(), self.active);
        if dest == from {
            return; // no-op: dropped back into its own slot
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(dest, tab);
        self.active = new_active;
        self.save(cx);
        cx.notify();
    }

    /// Open the FOCUS reading modal on `pane`: flag it read (so Esc closes the
    /// modal), and focus it so every keystroke still lands in the real terminal
    /// while you read it blown up. Replaces any previously-read pane.
    fn open_focus_read(
        &mut self,
        pane: Entity<TerminalView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(prev) = self.focus_read.take().and_then(|w| w.upgrade()) {
            prev.update(cx, |v, _| v.set_being_read(false));
        }
        pane.update(cx, |v, _| v.set_being_read(true));
        window.focus(&pane.focus_handle(cx), cx);
        self.focus_read = Some(pane.downgrade());
        cx.notify();
    }

    /// Close the FOCUS modal and clear the read flag on its pane (if still open).
    fn close_focus_read(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.focus_read.take().and_then(|w| w.upgrade()) {
            pane.update(cx, |v, _| v.set_being_read(false));
        }
        cx.notify();
    }

    fn activate_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if i < self.tabs.len() {
            self.active = i;
            self.save(cx);
            cx.notify();
            // Defer the focus: a mother-bar click is still being dispatched, and
            // the root container's tracked focus handle would otherwise grab
            // focus back as the event bubbles. Running after the event settles
            // makes the pane focus stick so the next keystroke lands in it.
            cx.defer_in(window, |ws, window, cx| ws.focus_active(window, cx));
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
            // land on the pane last focused in this tab if it's still here,
            // otherwise the first — so a tab with one terminal just lets you type
            let ids: Vec<EntityId> = leaves.iter().map(|p| p.entity_id()).collect();
            if let Some(target) = pick_focus_target(tab.focused, &ids) {
                if let Some(p) = leaves.iter().find(|p| p.entity_id() == target) {
                    window.focus(&p.focus_handle(cx), cx);
                }
            }
        }
    }

    /// Commit an in-progress tab rename (if any) and leave edit mode — the
    /// "click off saves" behaviour. Returns true if a rename was open. An empty
    /// name clears back to the auto-numbered label, matching the Enter path.
    fn commit_rename(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((tab_i, buf)) = self.renaming.take() else {
            return false;
        };
        if let Some(tab) = self.tabs.get_mut(tab_i) {
            tab.name = (!buf.trim().is_empty()).then(|| buf.trim().to_string());
        }
        self.save(cx);
        cx.notify();
        true
    }

    /// Set the *outer* (Mother) menu-bar size — the bezel scrubber and ctrl+scroll.
    /// Panes that follow outer (the default) pick this up live; a pane that has
    /// detached its grade keeps its own size. Sizes the header, not the grid.
    fn set_scale(&mut self, value: f32, cx: &mut Context<Self>) {
        let mut choice = theme::outer_choice(cx);
        choice.grade.scale = value.clamp(0.7, 1.6);
        theme::select_outer(cx, choice);
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
        // F1 toggles help (fallback for when no terminal pane is focused; panes
        // route F1 via the OpenHelp event).
        if ks.key.as_str() == "f1" {
            self.help_open = !self.help_open;
            cx.notify();
            return;
        }
        if self.help_open && ks.key.as_str() == "escape" {
            self.help_open = false;
            cx.notify();
            return;
        }
        if self.theme_menu.is_some() && ks.key.as_str() == "escape" {
            self.theme_menu = None;
            cx.notify();
            return;
        }
        if self.osd_menu.is_some() && ks.key.as_str() == "escape" {
            self.osd_menu = None;
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
            let cur = leaves
                .iter()
                .position(|p| p.focus_handle(cx).is_focused(window))
                .unwrap_or(0);
            // In an agent (claude/codex) pane, Alt+↑/↓ navigate between YOUR
            // messages in the chat instead of moving pane focus — same as the
            // ▲/▼ header buttons. Alt+←/→ still move focus everywhere.
            if matches!(ks.key.as_str(), "up" | "down") {
                if let Some(p) = leaves.get(cur).filter(|p| p.read(cx).mode.is_agent()) {
                    let next = ks.key.as_str() == "down";
                    p.update(cx, |view, cx| view.scroll_to_human(next, cx));
                    return;
                }
            }
            if leaves.len() > 1 {
                let dir: i32 = match ks.key.as_str() {
                    "left" | "up" => -1,
                    "right" | "down" => 1,
                    _ => return,
                };
                let next = (cur as i32 + dir).rem_euclid(leaves.len() as i32) as usize;
                window.focus(&leaves[next].focus_handle(cx), cx);
                cx.notify();
            }
        }
    }

    /// ctrl+wheel anywhere = menu-bar size scrub (panes skip scrolling when ctrl).
    fn on_wheel(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if !ev.modifiers.control {
            return;
        }
        let dy = match ev.delta {
            gpui::ScrollDelta::Lines(l) => l.y,
            gpui::ScrollDelta::Pixels(p) => f32::from(p.y) / 20.,
        };
        let cur = theme::outer_choice(cx).grade.scale;
        self.set_scale(cur + dy * 0.05, cx);
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        // an outer-tab reorder in flight owns the move: track the cursor, engage
        // past a small threshold (so a plain tab click still activates), and
        // resolve which slot a release would drop the tab into.
        if self.tab_drag.is_some() {
            if ev.pressed_button != Some(MouseButton::Left) {
                self.tab_drag = None;
                self.tab_drop = None;
                cx.notify();
                return;
            }
            let pos = ev.position;
            let engaged = {
                let d = self.tab_drag.as_mut().unwrap();
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
            self.tab_drop = if engaged {
                Some(self.resolve_tab_slot(pos.x))
            } else {
                None
            };
            cx.notify();
            return;
        }
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
            // dragged past the window edge → arm the tear-off (and stop showing
            // in-window drop landings, since the release won't land on one)
            let (ow, oh) = self
                .last_win
                .map(|(_, _, w, h)| (w, h))
                .unwrap_or((0.0, 0.0));
            let outside = engaged && outside_bounds(f32::from(pos.x), f32::from(pos.y), ow, oh);
            if let Some(d) = self.drag_pane.as_mut() {
                d.left_window = outside;
            }
            self.drop_target = if engaged && !outside {
                let id = self.drag_pane.as_ref().unwrap().id;
                self.resolve_drop(pos, id)
            } else {
                None
            };
            cx.notify();
            return;
        }
        // A release that landed OUTSIDE the window never fires on_mouse_up, so a
        // continuous-adjust drag can stick "on" and re-grab the cursor on the next
        // move. The button is up here, so clear any stuck drag. (tab_drag/drag_pane
        // self-clear above, with their own drop teardown.)
        if ev.pressed_button != Some(MouseButton::Left)
            && (self.scrubbing
                || self.wheel_drag.is_some()
                || self.light_drag
                || self.warp_drag
                || self.slider_drag.is_some()
                || self.track_drag.is_some()
                || self.drag_split.is_some())
        {
            self.scrubbing = false;
            self.wheel_drag = None;
            self.light_drag = false;
            self.warp_drag = false;
            self.slider_drag = None;
            self.track_drag = None;
            self.drag_split = None;
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
        if let Some(key) = self.slider_drag {
            if ev.pressed_button == Some(MouseButton::Left) {
                if let Some(v) = self.grade_from_pos(key, ev.position.x) {
                    self.apply_grade(key, v, cx);
                }
            }
        }
        if let Some(target) = self.wheel_drag {
            if ev.pressed_button == Some(MouseButton::Left) {
                // hue + saturation follow the cursor; keep the marker's lightness
                let l = self.wheel_color(target, cx).l;
                if let Some(hex) = self.wheel_color_at(ev.position.x, ev.position.y, l) {
                    self.set_wheel_color_for(target, Some(hex), cx);
                }
            }
        }
        if self.light_drag && ev.pressed_button == Some(MouseButton::Left) {
            if let Some(l) = self.light_from_pos(ev.position.x) {
                self.set_active_lightness(l, cx);
            }
        }
        if self.warp_drag && ev.pressed_button == Some(MouseButton::Left) {
            if let Some(a) = self.warp_from_pos(ev.position.x) {
                theme::set_screen_warp(cx, a);
            }
        }
        if let Some(idx) = self.track_drag {
            if ev.pressed_button == Some(MouseButton::Left) {
                if let Some(v) = self.track_from_pos(idx, ev.position.x) {
                    self.apply_track(idx, v, cx);
                }
            }
        }
    }

    fn on_mouse_up(&mut self, ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = false;
        if self.wheel_drag.take().is_some()
            || std::mem::take(&mut self.light_drag)
            || std::mem::take(&mut self.warp_drag)
            || self.track_drag.take().is_some()
        {
            self.save(cx);
            cx.notify();
            return;
        }
        if self.slider_drag.take().is_some() {
            self.save(cx);
            cx.notify();
            return;
        }
        // a tab reorder: drop the grabbed tab into its resolved slot
        if let Some(drag) = self.tab_drag.take() {
            let slot = self.tab_drop.take();
            if drag.engaged {
                if let Some(to) = slot {
                    self.move_tab(drag.from, to, cx);
                }
            }
            cx.notify();
            return;
        }
        if let Some(drag) = self.drag_pane.take() {
            let target = self.drop_target.take();
            if drag.engaged {
                let (ow, oh) = self
                    .last_win
                    .map(|(_, _, w, h)| (w, h))
                    .unwrap_or((0.0, 0.0));
                let outside = drag.left_window
                    || outside_bounds(f32::from(ev.position.x), f32::from(ev.position.y), ow, oh);
                if let Some(target) = target {
                    self.perform_drop(drag.id, target, window, cx);
                } else if outside && self.pane_count() > 1 {
                    // released past the window edge → tear this pane off into a
                    // brand-new window. Guarded so you can't pop out your only
                    // terminal (which would just empty this window).
                    self.pop_out(drag.id, window, cx);
                }
                // a release over empty space *inside* the window stays a no-op.
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
            self.tabs.insert(from, Tab::new(root, name));
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

    /// Tear a pane off into its own new window: snapshot what it's running
    /// (cwd + resumable agent session), remove it here (which SIGHUPs the old
    /// shell), then launch a fresh seeded window. A live PTY can't be teleported
    /// across processes, so the new window re-spawns the shell in the same cwd
    /// and resumes a claude/codex session if there was one.
    fn pop_out(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        // find the pane and snapshot its runtime BEFORE we drop it
        let seed = self.tabs.iter().find_map(|t| {
            let mut v = vec![];
            t.root.leaves(&mut v);
            v.into_iter()
                .find(|e| e.entity_id() == id)
                .map(|p| p.read(cx).runtime())
        });
        let Some(rt) = seed else { return };
        // remove the leaf from its tab (same collapse-onto-sibling path as a
        // header-× close); dropping the entity releases its PTY.
        self.close_pane(id, window, cx);
        spawn_seeded_window(&rt);
    }

    /// What a release at `pos` would land on, ignoring the dragged pane itself.
    /// Main tabs win over panes, so dragging up to the strip moves between tabs.
    fn resolve_drop(&self, pos: Point<Pixels>, dragged: EntityId) -> Option<DropTarget> {
        for (&index, &rect) in self.tab_bounds.lock().unwrap().iter() {
            if rect.contains(&pos) {
                return Some(DropTarget::Tab { index });
            }
        }
        // Re-frame band: the cursor is hugging some container's perimeter. The
        // OUTERMOST (largest) qualifying container wins, so hugging the field
        // edge re-splits the whole field; hugging an inner divider re-splits
        // just that sub-region. This is what fractals the gesture down.
        let mut best: Option<(f32, u64, Bounds<Pixels>)> = None;
        for (&id, &rect) in self.split_bounds.lock().unwrap().iter() {
            if rect.contains(&pos) && near_perimeter(rect, pos, edge_band(rect)) {
                let area = f32::from(rect.size.width) * f32::from(rect.size.height);
                if best.is_none_or(|(a, ..)| area > a) {
                    best = Some((area, id, rect));
                }
            }
        }
        if let Some((_, id, rect)) = best {
            return Some(DropTarget::Edge {
                container: id,
                zone: zone_of(rect, pos),
            });
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
                self.tabs.insert(from, Tab::new(root, src_name));
            }
            return;
        };
        let source_emptied = remaining.is_none();
        if let Some(root) = remaining {
            self.tabs.insert(from, Tab::new(root, src_name));
        }

        let landed = match target {
            DropTarget::Split {
                pane: target_id,
                zone,
                ..
            } => {
                let (dir, new_first) = split_for(zone);
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
            DropTarget::Edge { container, zone } => {
                // re-frame a whole container (or the field). The dragged pane is
                // always pulled from the active tab, so the re-frame applies
                // there. If the removal collapsed the targeted container away
                // (e.g. it held the dragged pane), wrap whatever the field
                // became — that's the "resplit the entire field" fallback.
                let (dir, new_first) = split_for(zone);
                self.tabs.get_mut(from).map(|tab| {
                    if !tab
                        .root
                        .split_node_at(container, dir, pane.clone(), new_first)
                    {
                        let old = std::mem::replace(&mut tab.root, Node::Leaf(pane.clone()));
                        let (a, b) = if new_first {
                            (Node::Leaf(pane.clone()), old)
                        } else {
                            (old, Node::Leaf(pane.clone()))
                        };
                        tab.root = Node::Split {
                            id: next_split_id(),
                            dir,
                            ratio: 0.5,
                            a: Box::new(a),
                            b: Box::new(b),
                        };
                    }
                    from
                })
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
            self.tabs.push(Tab::new(Node::Leaf(pane.clone()), None));
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
            .filter_map(|t| t.root.reap(cx).map(|root| Tab::new(root, t.name)))
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

    /// The *effective* choice for a scope: a pane resolves each group to its own
    /// override or the live outer (see [`PaneTheme::effective`]); outer is itself.
    /// Shared by the theme breakout and the monitor-OSD tray — what each tray
    /// shows is what the scope currently renders with.
    fn choice_for(&self, scope: &MenuScope, cx: &App) -> ThemeChoice {
        match scope {
            MenuScope::Pane(p) => p.read(cx).appearance.effective(&theme::outer_choice(cx)),
            MenuScope::Outer => theme::outer_choice(cx),
        }
    }

    /// Apply a theme-group edit (id/seed/colour/syntax) to a scope. For a pane
    /// this pins *only* the theme group and detaches it from outer; the grade
    /// group's inherit state is untouched. `edited` is the full effective choice
    /// with the changed field — its `grade` is ignored here.
    fn set_theme_group(&mut self, scope: &MenuScope, edited: ThemeChoice, cx: &mut Context<Self>) {
        match scope {
            MenuScope::Pane(pane) => {
                let g = theme::ThemeGroup::of(&edited);
                pane.update(cx, |view, cx| {
                    view.appearance.set_theme(g);
                    cx.notify();
                });
            }
            MenuScope::Outer => theme::select_outer(cx, edited),
        }
        self.save(cx);
        cx.notify();
    }

    /// Flip a pane's theme-group "follow outer" switch (no-op for the outer
    /// scope, which has nothing to follow). Non-destructive: the pane's retained
    /// override survives, so re-detaching restores it.
    fn toggle_theme_inherit(&mut self, scope: &MenuScope, cx: &mut Context<Self>) {
        if let MenuScope::Pane(pane) = scope {
            let outer = theme::outer_choice(cx);
            pane.update(cx, |view, cx| {
                view.appearance.toggle_theme(&outer);
                cx.notify();
            });
            self.save(cx);
            cx.notify();
        }
    }

    /// Flip a pane's grade-group "follow outer" switch (see
    /// [`Self::toggle_theme_inherit`]).
    fn toggle_grade_inherit(&mut self, scope: &MenuScope, cx: &mut Context<Self>) {
        if let MenuScope::Pane(pane) = scope {
            let outer = theme::outer_choice(cx);
            pane.update(cx, |view, cx| {
                view.appearance.toggle_grade(&outer);
                cx.notify();
            });
            self.save(cx);
            cx.notify();
        }
    }

    /// The choice the open theme breakout is editing (pane override or outer).
    fn menu_choice(&self, cx: &App) -> ThemeChoice {
        match &self.theme_menu {
            Some(scope) => self.choice_for(scope, cx),
            None => theme::outer_choice(cx),
        }
    }

    /// Apply a theme-group edit to the open theme breakout's scope.
    fn set_menu_choice(&mut self, choice: ThemeChoice, cx: &mut Context<Self>) {
        if let Some(scope) = self.theme_menu.clone() {
            self.set_theme_group(&scope, choice, cx);
        }
    }

    /// Slider track ratio (0..1) for the active OSD slider at window-x `x`.
    fn grade_from_pos(&self, key: theme::GradeKey, x: Pixels) -> Option<f32> {
        let b = *self.slider_bounds.lock().unwrap().get(&key)?;
        let w = f32::from(b.size.width);
        if w <= 0.0 {
            return None;
        }
        Some(((f32::from(x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0))
    }

    /// Set one channel of the active OSD scope's grade to `v` (live, persisted).
    /// For a pane this pins *only* the grade group (seeding from the currently
    /// shown grade) and detaches it from outer; the theme group is untouched.
    fn apply_grade(&mut self, key: theme::GradeKey, ratio: f32, cx: &mut Context<Self>) {
        let Some(scope) = self.osd_menu.clone() else {
            return;
        };
        // `ratio` is the 0..1 track position; map it into the channel's stored
        // units (colour channels are 0..1; text size is 0.7..1.6).
        let (min, max, _) = key.range();
        let mut grade = self.choice_for(&scope, cx).grade;
        grade.set(key, min + ratio * (max - min));
        self.write_grade(&scope, grade, cx);
    }

    /// Reset the active OSD scope's grade to the neutral identity — no monitor
    /// grading at all (this clears the shipped house grade too, see
    /// [`theme::Grade::neutral`]). A pane stays detached; "follow outer" re-inherits.
    fn reset_grade(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.osd_menu.clone() else {
            return;
        };
        self.write_grade(&scope, theme::Grade::neutral(), cx);
    }

    /// Commit a grade to a scope: pin it on a pane (grade group only), or set it
    /// on the outer choice.
    fn write_grade(&mut self, scope: &MenuScope, grade: theme::Grade, cx: &mut Context<Self>) {
        match scope {
            MenuScope::Pane(pane) => {
                pane.update(cx, |view, cx| {
                    view.appearance.set_grade(grade);
                    cx.notify();
                });
            }
            MenuScope::Outer => {
                let mut choice = theme::outer_choice(cx);
                choice.grade = grade;
                theme::select_outer(cx, choice);
            }
        }
        self.save(cx);
        cx.notify();
    }

    /// One OSD slider: label + draggable track + a centre-relative readout
    /// (`0` = neutral). Mirrors the text-size scrubber's bounds-capture + drag.
    fn slider_row(
        &self,
        key: theme::GradeKey,
        label: &str,
        value: f32,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        const TRACK: f32 = 150.;
        let store = self.slider_bounds.clone();
        // `value` is in the channel's stored units; normalise to a 0..1 track
        // fraction so colour channels (0..1) and text size (0.7..1.6) share the
        // same slider geometry.
        let (min, max, neutral) = key.range();
        let v = value.clamp(min, max);
        let frac = ((v - min) / (max - min)).clamp(0., 1.);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(74.))
                    .text_size(px(10.))
                    .text_color(th.text.alpha(0.8))
                    .child(label.to_string()),
            )
            .child(
                div()
                    .w(px(TRACK))
                    .h(px(14.))
                    .relative()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.slider_drag = Some(key);
                            if let Some(v) = ws.grade_from_pos(key, ev.position.x) {
                                ws.apply_grade(key, v, cx);
                            }
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                store.lock().unwrap().insert(key, bounds);
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
                            .top(px(6.))
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
                            .top(px(6.))
                            .h(px(3.))
                            .w(px(TRACK * frac))
                            .rounded_full()
                            .bg(th.accent),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px((TRACK * frac - 5.).max(0.)))
                            .top(px(2.))
                            .w(px(10.))
                            .h(px(10.))
                            .rounded_full()
                            .bg(linear_gradient(
                                135.,
                                linear_color_stop(brighten(th.accent, 1.4), 0.),
                                linear_color_stop(darken(th.accent, 0.7), 1.),
                            )),
                    ),
            )
            .child(
                div()
                    .w(px(28.))
                    .text_size(px(9.))
                    .text_color(th.accent)
                    // Text size reads as an absolute "110%"; colour channels read
                    // as a signed offset from neutral ("-12", "+0").
                    .child(if matches!(key, theme::GradeKey::Scale) {
                        format!("{}%", (v * 100.).round() as i32)
                    } else {
                        format!("{:+}", ((v - neutral) * 100.).round() as i32)
                    }),
            )
    }

    /// Write a colour to one wheel target (seed / text / complement) in the open
    /// breakout's scope. `None` clears that target (back to theme/dynamic-derived).
    fn set_wheel_color_for(
        &mut self,
        target: WheelTarget,
        hex: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let mut choice = self.menu_choice(cx);
        match target {
            WheelTarget::Seed => choice.seed = hex,
            WheelTarget::Text => choice.text = hex,
            WheelTarget::Complement => choice.complement = hex,
            WheelTarget::Human => choice.human = hex,
        }
        self.set_menu_choice(choice, cx);
    }

    /// The three marker colours on the wheel for the open scope: each is its
    /// explicit override if set, else the value the theme/dynamic currently
    /// derives — so a marker always sits where its colour really is.
    fn wheel_markers(&self, cx: &App) -> [(WheelTarget, &'static str, Hsla); 4] {
        let choice = self.menu_choice(cx);
        let resolved = theme::resolve(cx, &choice);
        let pick = |o: &Option<String>, derived: Hsla| {
            o.as_deref().and_then(theme::parse_hex).unwrap_or(derived)
        };
        [
            (WheelTarget::Seed, "◉", pick(&choice.seed, resolved.accent)),
            (WheelTarget::Text, "T", pick(&choice.text, resolved.text)),
            (
                WheelTarget::Complement,
                "C",
                pick(&choice.complement, resolved.complement),
            ),
            (
                WheelTarget::Human,
                "👤",
                pick(&choice.human, resolved.human),
            ),
        ]
    }

    /// Which of the three markers is nearest the click — the one you grabbed.
    fn wheel_grab(&self, x: Pixels, y: Pixels, cx: &App) -> Option<WheelTarget> {
        let b = (*self.wheel_bounds.lock().unwrap())?;
        let rad = f32::from(b.size.width).min(f32::from(b.size.height)) / 2.0;
        if rad <= 0.0 {
            return None;
        }
        let cx0 = f32::from(b.origin.x) + f32::from(b.size.width) / 2.0;
        let cy0 = f32::from(b.origin.y) + f32::from(b.size.height) / 2.0;
        let (px_, py_) = (f32::from(x), f32::from(y));
        let d2 = |c: Hsla| {
            let ang = c.h.rem_euclid(1.0) * std::f32::consts::TAU;
            let sat = c.s.clamp(0.0, 1.0);
            let mx = cx0 + ang.cos() * sat * rad;
            let my = cy0 + ang.sin() * sat * rad;
            (mx - px_).powi(2) + (my - py_).powi(2)
        };
        let nearest = self
            .wheel_markers(cx)
            .into_iter()
            .map(|(t, _, c)| (t, d2(c)))
            .min_by(|a, b| a.1.total_cmp(&b.1))?;
        // When markers stack (a greyscale palette piles them all at the centre)
        // the nearest is ambiguous and the same pip always wins — you can never
        // drag the others out. So if the user-selected (active, front-most) pip
        // sits within a marker's width of the press, grab that one. The PICK row
        // promotes a pip to active; elsewhere the literal nearest still wins.
        const TIE: f32 = 16.0; // px — a marker is 18px wide
        let active_d2 = d2(self.wheel_color(self.wheel_active, cx));
        if active_d2 <= nearest.1 + TIE * TIE {
            return Some(self.wheel_active);
        }
        Some(nearest.0)
    }

    /// Map a wheel point to a hex at lightness `l`: angle → hue, radius →
    /// saturation. Preserving the dragged marker's own `l` (instead of a fixed
    /// mid) is what lets the lightness slider reach white/grey/black.
    fn wheel_color_at(&self, x: Pixels, y: Pixels, l: f32) -> Option<String> {
        let b = (*self.wheel_bounds.lock().unwrap())?;
        let cx = f32::from(b.origin.x) + f32::from(b.size.width) / 2.0;
        let cy = f32::from(b.origin.y) + f32::from(b.size.height) / 2.0;
        let rad = f32::from(b.size.width).min(f32::from(b.size.height)) / 2.0;
        if rad <= 0.0 {
            return None;
        }
        let (dx, dy) = (f32::from(x) - cx, f32::from(y) - cy);
        let dist = (dx * dx + dy * dy).sqrt().min(rad);
        let ang = dy.atan2(dx) / std::f32::consts::TAU;
        let hue = ang - ang.floor();
        let sat = (dist / rad).min(1.0);
        Some(hsla_to_hex(hsla(hue, sat, l.clamp(0.0, 1.0), 1.0)))
    }

    /// The current effective colour of one wheel marker (override or derived).
    fn wheel_color(&self, target: WheelTarget, cx: &App) -> Hsla {
        self.wheel_markers(cx)
            .into_iter()
            .find(|(t, _, _)| *t == target)
            .map(|(_, _, c)| c)
            .unwrap_or_default()
    }

    /// Lightness `0..1` for the lightness slider at window-x `x`.
    fn light_from_pos(&self, x: Pixels) -> Option<f32> {
        let b = (*self.light_bounds.lock().unwrap())?;
        let w = f32::from(b.size.width);
        if w <= 0.0 {
            return None;
        }
        Some(((f32::from(x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0))
    }

    /// Warp amount `0..=WARP_MAX` for the screen-warp slider at window-x `x`.
    fn warp_from_pos(&self, x: Pixels) -> Option<f32> {
        let b = (*self.warp_bounds.lock().unwrap())?;
        let w = f32::from(b.size.width);
        if w <= 0.0 {
            return None;
        }
        let frac = ((f32::from(x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0);
        Some(frac * theme::WARP_MAX)
    }

    /// Ratio `0..1` for tracking slider `idx` at window-x `x`.
    fn track_from_pos(&self, idx: usize, x: Pixels) -> Option<f32> {
        let b = (*self.track_bounds[idx].lock().unwrap())?;
        let w = f32::from(b.size.width);
        if w <= 0.0 {
            return None;
        }
        Some(((f32::from(x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0))
    }

    /// Set tracking dial component `idx` (during a drag) and repaint.
    fn apply_track(&mut self, idx: usize, v: f32, cx: &mut Context<Self>) {
        let mut d = theme::tracking_dial(cx).unwrap_or([0.2, 0.5, 0.3]);
        d[idx] = v.clamp(0.0, 1.0);
        theme::set_tracking_dial(cx, Some(d));
    }

    /// One tracking-band slider (idx 0=intensity, 1=speed, 2=size). The global
    /// tracking dial overrides every theme's roll bar; first drag seeds from the
    /// current look so it starts where you are.
    fn track_slider(
        &self,
        idx: usize,
        label: &str,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        const TRACK: f32 = 108.;
        let store = self.track_bounds[idx].clone();
        let seed = theme::tracking_dials_of(cx, th);
        let frac = seed[idx].clamp(0.0, 1.0);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(58.))
                    .text_size(px(10.))
                    .text_color(th.text.alpha(0.8))
                    .child(label.to_string()),
            )
            .child(
                div()
                    .w(px(TRACK))
                    .h(px(14.))
                    .relative()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.track_drag = Some(idx);
                            if let Some(v) = ws.track_from_pos(idx, ev.position.x) {
                                let mut d = seed;
                                d[idx] = v;
                                theme::set_tracking_dial(cx, Some(d));
                            }
                        }),
                    )
                    .child(
                        canvas(
                            move |b, _, _| {
                                *store.lock().unwrap() = Some(b);
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
                            .top(px(6.))
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
                            .top(px(6.))
                            .h(px(3.))
                            .w(px(TRACK * frac))
                            .rounded_full()
                            .bg(th.accent),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px((TRACK * frac - 5.0).max(0.0)))
                            .top(px(2.))
                            .w(px(10.))
                            .h(px(10.))
                            .rounded_full()
                            .border_2()
                            .border_color(white())
                            .bg(th.accent),
                    ),
            )
    }

    /// The global screen-warp dial — a 0→WARP_MAX slider (0 = dead flat). Drag to
    /// find your fishbowl; a readout shows the live amount. Orthogonal to theme.
    fn warp_slider(&self, th: &theme::Theme, cx: &mut Context<Self>) -> gpui::Div {
        const TRACK: f32 = 132.;
        let store = self.warp_bounds.clone();
        let amount = theme::screen_warp(cx);
        let frac = (amount / theme::WARP_MAX).clamp(0.0, 1.0);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .w(px(64.))
                    .text_size(px(10.))
                    .text_color(th.text.alpha(0.8))
                    .child("screen warp"),
            )
            .child(
                div()
                    .w(px(TRACK))
                    .h(px(14.))
                    .relative()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, ev: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.warp_drag = true;
                            if let Some(a) = ws.warp_from_pos(ev.position.x) {
                                theme::set_screen_warp(cx, a);
                            }
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                *store.lock().unwrap() = Some(bounds);
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
                            .top(px(6.))
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
                            .top(px(6.))
                            .h(px(3.))
                            .w(px(TRACK * frac))
                            .rounded_full()
                            .bg(th.accent),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px((TRACK * frac - 5.0).max(0.0)))
                            .top(px(2.))
                            .w(px(10.))
                            .h(px(10.))
                            .rounded_full()
                            .border_2()
                            .border_color(white())
                            .bg(th.accent),
                    ),
            )
            .child(
                div()
                    .w(px(28.))
                    .text_size(px(9.))
                    .text_color(th.accent)
                    .child(format!("{amount:.2}")),
            )
    }

    /// Set the lightness of the active marker (keeping its hue + saturation).
    fn set_active_lightness(&mut self, l: f32, cx: &mut Context<Self>) {
        let t = self.wheel_active;
        let c = self.wheel_color(t, cx);
        self.set_wheel_color_for(t, Some(hsla_to_hex(hsla(c.h, c.s, l, 1.0))), cx);
    }

    /// The colour wheel: a canvas-painted HSV disk (hue = angle, saturation =
    /// radius) carrying FOUR draggable markers — ◉ seed, T text, C complement,
    /// 👤 human (your-input). You grab whichever marker is nearest the press and
    /// drag it around to set that colour. Drives the same scope as the breakout.
    fn color_wheel(
        &self,
        markers: [(WheelTarget, &'static str, Hsla); 4],
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        const D: f32 = 132.0;
        let r = D / 2.0;
        let store = self.wheel_bounds.clone();
        let mut wheel = div()
            .w(px(D))
            .h(px(D))
            .relative()
            .rounded_full()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    if let Some(t) = ws.wheel_grab(ev.position.x, ev.position.y, cx) {
                        ws.wheel_drag = Some(t);
                        ws.wheel_active = t; // the lightness slider follows it
                        let l = ws.wheel_color(t, cx).l;
                        if let Some(hex) = ws.wheel_color_at(ev.position.x, ev.position.y, l) {
                            ws.set_wheel_color_for(t, Some(hex), cx);
                        }
                    }
                }),
            )
            .child(
                canvas(
                    move |bounds, _, _| {
                        *store.lock().unwrap() = Some(bounds);
                    },
                    move |bounds: Bounds<Pixels>, _, window, _| {
                        let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
                        let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
                        let rad =
                            f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 2.0;
                        let cell = 3.5_f32;
                        let mut yy = cy - rad;
                        while yy <= cy + rad {
                            let mut xx = cx - rad;
                            while xx <= cx + rad {
                                let dx = xx + cell / 2.0 - cx;
                                let dy = yy + cell / 2.0 - cy;
                                let dist = (dx * dx + dy * dy).sqrt();
                                if dist <= rad {
                                    let ang = dy.atan2(dx) / std::f32::consts::TAU;
                                    let hue = ang - ang.floor();
                                    let sat = (dist / rad).min(1.0);
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(px(xx), px(yy)),
                                            size(px(cell + 0.6), px(cell + 0.6)),
                                        ),
                                        hsla(hue, sat, 0.55, 1.0),
                                    ));
                                }
                                xx += cell;
                            }
                            yy += cell;
                        }
                    },
                )
                .size_full(),
            );
        // Paint the active (selected) marker LAST so it sits on top of any pile —
        // when a greyscale palette stacks every pip at the centre, the one the
        // PICK row promoted is the visible, grabbable one.
        let active = self.wheel_active;
        let mut ordered: Vec<_> = markers.into_iter().collect();
        ordered.sort_by_key(|(t, _, _)| *t == active);
        for (t, glyph, c) in ordered {
            let ang = c.h.rem_euclid(1.0) * std::f32::consts::TAU;
            let sat = c.s.clamp(0.0, 1.0);
            let (dx, dy) = (r + ang.cos() * sat * r, r + ang.sin() * sat * r);
            // dark glyph on a light marker, light glyph on a dark one
            let glyph_col = if c.l > 0.55 {
                hsla(0., 0., 0.08, 0.95)
            } else {
                white()
            };
            // the active pip wears an amber ring so the front-most is obvious
            let ring = if t == active {
                hsla(0.09, 0.9, 0.6, 1.0)
            } else {
                white()
            };
            wheel = wheel.child(
                div()
                    .absolute()
                    .left(px(dx - 9.0))
                    .top(px(dy - 9.0))
                    .w(px(18.))
                    .h(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_2()
                    .border_color(ring)
                    .bg(c)
                    .text_size(px(10.))
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_color(glyph_col)
                    .shadow(vec![BoxShadow {
                        color: hsla(0., 0., 0., 0.7),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(2.),
                        spread_radius: px(1.),
                        inset: false,
                    }])
                    .child(glyph),
            );
        }
        wheel
    }

    /// Lightness slider for the active wheel marker — a dark→light ramp of its
    /// hue. Drag it to reach white / grey / black (pair with a low-saturation
    /// marker near the wheel's centre for true neutrals). Industry-standard HSL:
    /// hue + saturation on the disk, lightness here.
    fn lightness_bar(&self, cx: &mut Context<Self>) -> gpui::Div {
        const W: f32 = 132.0;
        let store = self.light_bounds.clone();
        let c = self.wheel_color(self.wheel_active, cx);
        let l = c.l.clamp(0.0, 1.0);
        div()
            .w(px(W))
            .h(px(14.))
            .relative()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.light_drag = true;
                    if let Some(l) = ws.light_from_pos(ev.position.x) {
                        ws.set_active_lightness(l, cx);
                    }
                }),
            )
            .child(
                canvas(
                    move |bounds, _, _| {
                        *store.lock().unwrap() = Some(bounds);
                    },
                    |_, _, _, _| {},
                )
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded_full()
                    .border_1()
                    .border_color(hsla(0., 0., 0., 0.4))
                    .bg(linear_gradient(
                        90.,
                        linear_color_stop(hsla(c.h, c.s, 0.06, 1.), 0.),
                        linear_color_stop(hsla(c.h, c.s, 0.97, 1.), 1.),
                    )),
            )
            .child(
                div()
                    .absolute()
                    .left(px((W * l - 6.0).clamp(0.0, W - 12.0)))
                    .top(px(1.))
                    .w(px(12.))
                    .h(px(12.))
                    .rounded_full()
                    .border_2()
                    .border_color(white())
                    .bg(c),
            )
    }

    /// Solid, reflective bezel button — light source upper-left.
    /// A consistent header icon button (≈2× glyphs). The caller adds the glyph
    /// child (an emoji via `.child("…")` or `pane::eq_icon`) and `on_mouse_down`.
    fn hicon(th: &theme::Theme, active: bool) -> gpui::Div {
        div()
            .px_1()
            .rounded_sm()
            .border_1()
            .border_color(th.accent.alpha(0.5))
            .bg(if active {
                th.accent.alpha(0.2)
            } else {
                th.accent.alpha(0.0)
            })
            .cursor_pointer()
    }

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

/// Hover popup for a theme button. Shows the full theme name (the in-button
/// caption is truncated — e.g. `tactical` for `tactical-overdrive`). For the
/// hot-reloaded `custom` slot it also shows the resolved file path on THIS
/// machine and a clickable "Open in editor" line.
struct ThemeTooltip {
    name: SharedString,
    /// `Some` only for the custom slot — the file to reveal/open.
    path: Option<PathBuf>,
    bg: Hsla,
    text: Hsla,
    accent: Hsla,
    faint: Hsla,
}

impl Render for ThemeTooltip {
    fn render(&mut self, _w: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .px(px(9.))
            .py(px(6.))
            .rounded_md()
            .border_1()
            .border_color(self.accent.alpha(0.5))
            .bg(self.bg)
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.45),
                offset: point(px(0.), px(2.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
                inset: false,
            }])
            .text_color(self.text)
            .child(div().text_size(px(12.)).child(self.name.clone()));
        if let Some(path) = self.path.clone() {
            card = card
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(self.faint)
                        .child(path.display().to_string()),
                )
                .child(
                    div()
                        .id("open-theme-file")
                        .mt(px(2.))
                        .text_size(px(11.))
                        .text_color(self.accent)
                        .cursor_pointer()
                        .child("▸ Open in editor  ⧉")
                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            theme::open_in_default_app(&path);
                        }),
                );
        }
        card
    }
}

/// Theme-icon button for the breakout menu: glyph over a tiny caption naming the
/// slot, so two themes that share a glyph stay tellable apart.
fn theme_icon_btn(th: &theme::Theme, icon: &str, label: &str, active: bool) -> gpui::Div {
    let inner = div()
        .flex()
        .flex_col()
        .items_center()
        .gap_0()
        .child(div().text_size(px(14.)).child(icon.to_string()))
        .child(
            div()
                .text_size(px(8.))
                .text_color(th.text.alpha(if active { 0.85 } else { 0.6 }))
                .child(label.to_string()),
        );
    let b = div()
        .w(px(46.))
        .h(px(40.))
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

fn darken(mut c: Hsla, f: f32) -> Hsla {
    c.l *= f;
    c
}

fn brighten(mut c: Hsla, f: f32) -> Hsla {
    c.l = (c.l * f).min(0.92);
    c
}

/// `Hsla` → `#rrggbb` (drops alpha) for storing a wheel-picked seed colour.
fn hsla_to_hex(c: Hsla) -> String {
    let (h, s, l) = (
        c.h.rem_euclid(1.0),
        c.s.clamp(0.0, 1.0),
        c.l.clamp(0.0, 1.0),
    );
    let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = chroma * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match (hp as i32).min(5) {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = l - chroma / 2.0;
    let to = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u32;
    format!("#{:02x}{:02x}{:02x}", to(r1), to(g1), to(b1))
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
            // is a dragged pane hugging THIS container's edge (a whole-region
            // re-frame)? which side will it split toward?
            let edge_zone = match drop {
                Some(DropTarget::Edge { container, zone }) if *container == id => Some(*zone),
                _ => None,
            };
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
            let base = base.child(measure).child(first).child(handle).child(second);
            // re-frame preview: a heavier accent slab spanning HALF this whole
            // container (the field, for the root) — reads bolder than a leaf
            // split's soft slab so the gesture is unmistakable.
            base.when_some(edge_zone, |d, zone| {
                let slab = div()
                    .absolute()
                    .bg(th.accent.alpha(0.28))
                    .border_2()
                    .border_color(th.accent.alpha(0.85));
                let slab = match zone {
                    Zone::Left => slab.left_0().top_0().bottom_0().w(gpui::relative(0.5)),
                    Zone::Right => slab.right_0().top_0().bottom_0().w(gpui::relative(0.5)),
                    Zone::Top => slab.top_0().left_0().right_0().h(gpui::relative(0.5)),
                    Zone::Bottom => slab.bottom_0().left_0().right_0().h(gpui::relative(0.5)),
                };
                d.child(slab)
            })
        }
    }
}

// Capture/demo hook: when TD_FOCUS_DEMO is set, the workspace auto-opens the
// FOCUS reading modal on the first pane at first render, so headless screenshot
// tooling can frame the frosted-glass backdrop without a synthetic mouse click
// on the 👓 glyph. Inert (never armed) unless the env var is present.
static FOCUS_DEMO_ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// When the FOCUS modal opened — drives a ~220ms ease-in of the dim + frosted
// blur so the backdrop melts behind the panel instead of snapping. Set lazily
// in render while the modal is open, cleared when it closes.
static FOCUS_OPEN_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reap(window, cx);
        warp::begin_frame(); // visible panes re-register their tube rects below
                             // An open overlay (theme breakout / confirm dialog) flattens the glass:
                             // the warp is a pixel post-process, so a panel over a tube would bow out
                             // of reach of its own flat hit box. Suppress so the menu reads true.
        warp::set_suppressed(
            self.theme_menu.is_some()
                || self.osd_menu.is_some()
                || self.confirm_close.is_some()
                || self.help_open
                || self.tab_color_edit.is_some()
                || self.focus_read.as_ref().and_then(|w| w.upgrade()).is_some(),
        );
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
            return csd::decorate(div(), window);
        }
        // demo/capture hook (TD_FOCUS_DEMO): auto-open FOCUS on the first pane once,
        // so the frosted backdrop can be screenshotted without a mouse. Arms only
        // ON SUCCESS (retries each render until a leaf exists), and sets the flag
        // directly rather than calling window.focus mid-render.
        if std::env::var("TD_FOCUS_DEMO").is_ok()
            && !FOCUS_DEMO_ARMED.load(std::sync::atomic::Ordering::Relaxed)
        {
            let first = self.tabs.get(self.active).and_then(|tab| {
                let mut leaves = vec![];
                tab.root.leaves(&mut leaves);
                leaves.first().cloned()
            });
            if let Some(pane) = first {
                FOCUS_DEMO_ARMED.store(true, std::sync::atomic::Ordering::Relaxed);
                eprintln!("terminal-delight: TD_FOCUS_DEMO — auto-opening FOCUS modal");
                pane.update(cx, |v, _| v.set_being_read(true));
                self.focus_read = Some(pane.downgrade());
                cx.notify();
            }
        }
        // remember which pane currently holds focus in the active tab, so a later
        // mother-bar click returns to that exact terminal (the "most recent" one)
        let active = self.active;
        let focused_id = self.tabs.get(active).and_then(|tab| {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            leaves
                .iter()
                .find(|p| p.focus_handle(cx).is_focused(window))
                .map(|p| p.entity_id())
        });
        if let Some(id) = focused_id {
            if let Some(tab) = self.tabs.get_mut(active) {
                tab.focused = Some(id);
            }
        }
        let scale = theme::outer_choice(cx).grade.scale;
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
        // while a tab is being dragged, an accent bar marks the slot it'd land in
        let dragging_tab = self.tab_drag.as_ref().is_some_and(|d| d.engaged);
        let drop_slot = self.tab_drop;
        let drop_marker = || div().w(px(3.)).h(px(18.)).rounded_full().bg(th.accent);
        for i in 0..tab_count {
            if dragging_tab && drop_slot == Some(i) {
                tab_strip = tab_strip.child(drop_marker());
            }
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
                        // clicking the edit box itself keeps editing (don't let
                        // it bubble to the root's commit-on-click-off handler)
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
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
            let tab_grp = SharedString::from(format!("tab-grp-{i}"));
            let pencil_col = th.text.alpha(0.8);
            // hover-revealed ✎ affordance: invites the rename without a word
            let pencil = div()
                .id(SharedString::from(format!("tab-pencil-{i}")))
                .text_size(px(10.))
                .text_color(hsla(0., 0., 0., 0.)) // hidden until the tab is hovered
                .group_hover(tab_grp.clone(), move |s| s.text_color(pencil_col))
                .cursor_pointer()
                .child("✎")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let seed = ws.tabs[i].name.clone().unwrap_or_default();
                        ws.renaming = Some((i, seed));
                        window.focus(&ws.focus_handle, cx);
                        cx.notify();
                    }),
                );
            // tint the tab to its binder-divider colour (a property of the tab,
            // never inherited from a pane) — text stays the outer-bar text colour
            let mut btn = Self::bezel_btn(&th, &label, is_active);
            if let Some(c) = self.tabs[i].color {
                btn = btn
                    .bg(linear_gradient(
                        135.,
                        linear_color_stop(brighten(c, 1.35), 0.),
                        linear_color_stop(darken(c, 0.6), 1.),
                    ))
                    .border_color(if is_active { th.accent } else { c });
            }
            tab_strip = tab_strip.child(
                btn.group(tab_grp)
                    .relative()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, ev: &MouseDownEvent, window, cx| {
                            // don't let the click bubble to the root's focus
                            // handle, which would steal focus from the pane
                            cx.stop_propagation();
                            if ev.modifiers.control {
                                // ctrl+click → open this tab's colour tray (just
                                // this tab; never touches a pane's theme)
                                ws.tab_color_edit = Some(i);
                                ws.tab_color_at = Some(ev.position);
                                ws.tab_drag = None;
                                cx.notify();
                            } else if ev.click_count >= 2 {
                                // double-click to rename (the file-manager gesture)
                                let seed = ws.tabs[i].name.clone().unwrap_or_default();
                                ws.renaming = Some((i, seed));
                                window.focus(&ws.focus_handle, cx);
                                cx.notify();
                            } else {
                                // select now; arm a reorder drag that engages only
                                // if the cursor travels far enough (else it stays a
                                // plain click)
                                ws.activate_tab(i, window, cx);
                                ws.tab_drag = Some(TabDrag {
                                    from: i,
                                    start: ev.position,
                                    at: ev.position,
                                    engaged: false,
                                });
                                ws.tab_drop = None;
                            }
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
                    .child(pencil)
                    .child(close_x),
            );
        }
        if dragging_tab && drop_slot == Some(tab_count) {
            tab_strip = tab_strip.child(drop_marker());
        }
        tab_strip = tab_strip.child(Self::bezel_btn(&th, "+", false).on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, _: &MouseDownEvent, window, cx| ws.new_tab(window, cx)),
        ));

        // ---- menu-bar size scrubber: ▭ ──●── ▭ 110% ----
        // Drives the per-pane HEADER height (+ its glyphs/icons/title), not the
        // terminal text. The two flanking ▭ glyphs (small → large) read as
        // "short bar → tall bar".
        let ratio = ((scale - 0.7) / 0.9).clamp(0., 1.);
        let scrub_store = self.scrub_bounds.clone();
        let scrubber = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(div().text_size(px(9.)).text_color(th.text).child("▭"))
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
            .child(div().text_size(px(15.)).text_color(th.text).child("▭"))
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
            .child(Self::bezel_btn(&th, "◧ split", false).on_mouse_down(
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

        // Frameless window controls — only when the OS gave us none (client-side
        // decorations). A small minimize / maximize-toggle / close cluster that
        // lives at the right end of the mother bar; each stops propagation so it
        // never arms the drag-to-move latch.
        let is_client = matches!(window.window_decorations(), Decorations::Client { .. });
        let win_btn = |glyph: &'static str, danger: bool| {
            let hover = if danger {
                hsla(0., 0.7, 0.55, 1.)
            } else {
                th.accent
            };
            div()
                .id(SharedString::from(format!("winctl-{glyph}")))
                .w(px(20.))
                .h(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .text_size(px(12.))
                .text_color(th.text.alpha(0.7))
                .cursor_pointer()
                .hover(move |s| s.bg(hover.alpha(0.9)).text_color(white()))
                .child(glyph)
        };
        let win_controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .when(is_client, |row| {
                row.child(win_btn("—", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.minimize_window();
                    }),
                ))
                .child(win_btn("☐", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.zoom_window();
                    }),
                ))
                .child(win_btn("✕", true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        window.remove_window();
                    }),
                ))
            });

        let bezel_top = div()
            .h(px(43.))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .gap_3()
            // the mother bar is the move handle: arm on press, hand off to the
            // compositor on the first drag (a plain click stays a click).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, _cx| {
                    ws.should_move = true;
                }),
            )
            .on_mouse_move(cx.listener(|ws, _: &MouseMoveEvent, window, _cx| {
                if ws.should_move {
                    ws.should_move = false;
                    window.start_window_move();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseUpEvent, _w, _cx| {
                    ws.should_move = false;
                }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        // The mother TITLE — painted in the complement colour (the
                        // wheel's `C` target; defaults to the accent's complement /
                        // the active dynamic's complement).
                        div()
                            .text_size(px(14.))
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.complement)
                            .child("▸ TERMINAL-DELIGHT"),
                    )
                    .child(
                        // Decoration only — stays a dim foreground tint.
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
                        // outer theme: a consistent 🎨 (trigger for the breakout)
                        Self::hicon(&th, self.theme_menu.is_some())
                            .text_size(px(pane::HICON))
                            .line_height(px(pane::HICON))
                            .child("🎨")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.theme_menu = Some(MenuScope::Outer);
                                    ws.menu_at = None;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        // outer display: a consistent EQ-waveform (monitor-OSD).
                        // The outer (Mother) bezel chrome stays a fixed size — the
                        // scrubber sizes the per-terminal menu bars, not this bar.
                        Self::hicon(&th, self.osd_menu.is_some())
                            .flex()
                            .items_center()
                            .child(pane::eq_icon(th.accent, 1.0))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.osd_menu = Some(MenuScope::Outer);
                                    ws.osd_at = None;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        // help: keys + commands reference, themed by the outer
                        Self::hicon(&th, self.help_open)
                            .text_size(px(pane::HICON))
                            .line_height(px(pane::HICON))
                            .child("❔")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.help_open = true;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(scrubber)
                    .child(cluster)
                    .child(win_controls),
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
            .child(div().child(format!("🎨 · {}", focused_title)))
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
            // Theme-group "follow outer" state — only panes can inherit.
            let following = match &scope {
                MenuScope::Pane(p) => p.read(cx).appearance.inherit_theme,
                MenuScope::Outer => false,
            };
            // THEME picker (retained — hacker/custom/field/tactical are the base
            // theming). The dynamics glyph column is an orthogonal dimension
            // layered on top of whichever theme is active here.
            let mut theme_row = div().flex().flex_row().flex_wrap().gap_2();
            for (id, icon, lbl) in theme::all_themes(cx) {
                let active = cur.id == id;
                let cur_c = cur.clone();
                let tip_name: SharedString = id.clone().into();
                let tip_path = (id == "custom").then(theme::theme_path);
                let (tip_bg, tip_text, tip_accent, tip_faint) =
                    (darken(th.surface, 0.85), th.text, th.accent, th.faint);
                let mk_tip = move |_w: &mut Window, cx: &mut App| -> gpui::AnyView {
                    cx.new(|_| ThemeTooltip {
                        name: tip_name.clone(),
                        path: tip_path.clone(),
                        bg: tip_bg,
                        text: tip_text,
                        accent: tip_accent,
                        faint: tip_faint,
                    })
                    .into()
                };
                let btn = theme_icon_btn(&th, &icon, &lbl, active)
                    .id(SharedString::from(format!("theme-btn-{id}")))
                    .tooltip_show_delay(Duration::from_millis(1500))
                    .map(|b| {
                        if id == "custom" {
                            b.hoverable_tooltip(mk_tip)
                        } else {
                            b.tooltip(mk_tip)
                        }
                    });
                theme_row = theme_row.child(btn.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        // Switch the base theme only; seed/colour/syntax/grade and
                        // the dynamic dimension all carry over unchanged.
                        ws.set_menu_choice(
                            ThemeChoice {
                                id: id.clone(),
                                ..cur_c.clone()
                            },
                            cx,
                        );
                    }),
                ));
            }
            // The three colours live as draggable markers ON the wheel — ◉ seed,
            // T text, C complement. Grab one and drag it around to set it.
            let wheel = self.color_wheel(self.wheel_markers(cx), cx);
            let lbar = self.lightness_bar(cx);
            // PICK row — promote a pip to the front. With a greyscale palette every
            // marker stacks at the wheel's centre, so the same one always grabs;
            // clicking a chip here makes that pip active (front-most + amber ring)
            // and a centre-drag then pulls *it* out. Also what the lightness bar
            // follows. Each chip wears its marker's colour for at-a-glance ID.
            let active_t = self.wheel_active;
            let mut pick_row = div()
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_2()
                .text_size(px(9.));
            for (target, glyph, col) in self.wheel_markers(cx) {
                let on = target == active_t;
                let gcol = if col.l > 0.55 {
                    hsla(0., 0., 0.08, 0.95)
                } else {
                    white()
                };
                let idn = match target {
                    WheelTarget::Seed => "pick-seed",
                    WheelTarget::Text => "pick-text",
                    WheelTarget::Complement => "pick-comp",
                    WheelTarget::Human => "pick-human",
                };
                pick_row = pick_row.child(
                    div()
                        .id(idn)
                        .w(px(18.))
                        .h(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_2()
                        .border_color(if on {
                            hsla(0.09, 0.9, 0.6, 1.0)
                        } else {
                            th.text.alpha(0.25)
                        })
                        .bg(col)
                        .text_color(gcol)
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .cursor_pointer()
                        .child(glyph)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.wheel_active = target;
                                cx.notify();
                            }),
                        ),
                );
            }
            // Small chips to clear an override back to theme/dynamic-derived.
            let mut seed_row = div()
                .flex()
                .flex_row()
                .items_center()
                .justify_center()
                .gap_2()
                .text_size(px(9.));
            for (target, idn, lbl) in [
                (WheelTarget::Seed, "wr-seed", "↺◉"),
                (WheelTarget::Text, "wr-text", "↺T"),
                (WheelTarget::Complement, "wr-comp", "↺C"),
                (WheelTarget::Human, "wr-human", "↺👤"),
            ] {
                seed_row = seed_row.child(
                    div()
                        .id(idn)
                        .text_color(th.text.alpha(0.5))
                        .cursor_pointer()
                        .child(lbl)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.set_wheel_color_for(target, None, cx);
                            }),
                        ),
                );
            }
            let mut color_row = div().flex().flex_row().gap_2();
            for mode in theme::ColorMode::ALL {
                let active = cur.color == mode;
                let cur_c = cur.clone();
                color_row = color_row.child(
                    color_mode_btn(&th, mode.icon(), mode.caption(), active).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.set_menu_choice(
                                ThemeChoice {
                                    color: mode,
                                    ..cur_c.clone()
                                },
                                cx,
                            );
                        }),
                    ),
                );
            }
            // SYNTAX axis: an off/on overlay orthogonal to the source row above.
            // On = recolour default-fg text by token class (the old `code` look),
            // letting program ANSI still pass through the chosen source mode.
            let mut syntax_row = div().flex().flex_row().gap_2();
            for (on, icon, caption) in [(false, "○", "off"), (true, "◆", "code")] {
                let active = cur.syntax == on;
                let cur_c = cur.clone();
                syntax_row =
                    syntax_row.child(color_mode_btn(&th, icon, caption, active).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.set_menu_choice(
                                ThemeChoice {
                                    syntax: on,
                                    ..cur_c.clone()
                                },
                                cx,
                            );
                        }),
                    ));
            }
            let label = |s: &str| {
                div()
                    .text_size(px(9.))
                    .text_color(th.text.alpha(0.55))
                    .child(s.to_string())
            };
            // ---- dynamics glyph column: the theme tray is a vertical box of
            // dynamics (glyph only — no caption, no hover). The seed wheel below
            // stays the colour knob; the dynamic decides how that seed becomes the
            // palette. Overflow wraps into more columns to the right.
            let mut dyn_entries: Vec<(theme::Dynamic, bool)> = theme::Dynamic::NAMED
                .iter()
                .cloned()
                .map(|d| {
                    let active = cur.dynamic.same_kind(&d);
                    (d, active)
                })
                .collect();
            dyn_entries.push((
                theme::Dynamic::Custom(Box::default()),
                matches!(cur.dynamic, theme::Dynamic::Custom(_)),
            ));
            const PER_COL: usize = 6; // wrap past this many into a new column
            let mut dyn_cols = div().flex().flex_row().gap_2();
            for chunk in dyn_entries.chunks(PER_COL) {
                let mut col = div().flex().flex_col().gap_2();
                for (d, active) in chunk {
                    let active = *active;
                    let glyph = d.glyph().to_string();
                    let box_id = SharedString::from(format!("dyn-{}", d.label()));
                    let d_click = d.clone();
                    let cur_c = cur.clone();
                    col = col.child(
                        div()
                            .id(box_id)
                            .w(px(40.))
                            .h(px(40.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .border_1()
                            .border_color(if active {
                                th.accent
                            } else {
                                th.faint.alpha(0.5)
                            })
                            .bg(if active {
                                th.accent.alpha(0.16)
                            } else {
                                darken(th.surface, 0.3)
                            })
                            .text_size(px(18.))
                            .text_color(if active { th.text } else { th.text.alpha(0.7) })
                            .cursor_pointer()
                            .child(glyph)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    // Picking a colour set clears the wheel
                                    // overrides so its signature palette shows;
                                    // the user then tweaks from there.
                                    ws.set_menu_choice(
                                        ThemeChoice {
                                            dynamic: d_click.clone(),
                                            seed: None,
                                            text: None,
                                            complement: None,
                                            ..cur_c.clone()
                                        },
                                        cx,
                                    );
                                }),
                            ),
                    );
                }
                dyn_cols = dyn_cols.child(col);
            }
            // The right-hand controls: seed wheel + text axes + the follow-outer
            // toggle, stacked. A tiny scope hint replaces the old text title.
            let mut controls = div()
                .flex()
                .flex_col()
                .gap_2()
                .flex_1()
                .child(
                    div()
                        .text_size(px(8.5))
                        .text_color(th.text.alpha(0.45))
                        .child(if is_pane { "THIS PANE" } else { "OUTER" }),
                )
                .child(label("THEME"))
                .child(theme_row)
                .child(label(
                    "WHEEL — pick a pip, drag it out · ◉ seed T text C comp",
                ))
                .child(div().flex().justify_center().py_1().child(wheel))
                .child(div().flex().justify_center().child(lbar))
                .child(div().flex().justify_center().pt_1().child(pick_row))
                .child(seed_row)
                .child(label("PROGRAM COLOUR"))
                .child(color_row)
                .child(label("CODE"))
                .child(syntax_row);
            if is_pane {
                // Per-group toggle: on = this pane's theme follows the outer scope
                // live; off = it keeps its own retained theme. Non-destructive.
                let lbl = if following {
                    "◉ follow outer"
                } else {
                    "◯ follow outer"
                };
                controls = controls.child(Self::bezel_btn(&th, lbl, following).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        if let Some(scope) = ws.theme_menu.clone() {
                            ws.toggle_theme_inherit(&scope, cx);
                        }
                    }),
                ));
            }
            // A sub-tab icon click anchors the tray at the click (right edge at
            // the cursor, opening down-left like the global menu); clamp it fully
            // on-screen. The global/outer menu (menu_at == None) keeps its fixed
            // top-right anchor under the titlebar control.
            const PANEL_W: f32 = 344.; // wider: glyph column + controls side by side
            const PANEL_H_EST: f32 = 458.; // generous, incl. colour wheel + pick row + follow-outer
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
                .flex_row()
                .gap_3()
                .text_size(px(10.))
                .text_color(th.text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                // Left: the vertical dynamics glyph column(s). Right: seed wheel +
                // text axes + follow-outer (built above as `controls`).
                .child(dyn_cols)
                .child(controls);
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

        // ---- monitor OSD (display) tray: brightness/contrast/… sliders, per scope ----
        let osd_overlay = self.osd_menu.clone().map(|scope| {
            let is_pane = matches!(scope, MenuScope::Pane(_));
            let cur = self.choice_for(&scope, cx);
            let grade = cur.grade;
            // Grade-group "follow outer" state — independent of the theme tray.
            let following = match &scope {
                MenuScope::Pane(p) => p.read(cx).appearance.inherit_grade,
                MenuScope::Outer => false,
            };
            let label = |s: &str| {
                div()
                    .text_size(px(9.))
                    .text_color(th.text.alpha(0.55))
                    .child(s.to_string())
            };
            let mut rows = div().flex().flex_col().gap_1();
            for (key, name) in theme::Grade::CHANNELS {
                rows = rows.child(self.slider_row(key, name, grade.get(key), &th, cx));
            }
            const PANEL_W: f32 = 300.;
            const PANEL_H_EST: f32 = 306.; // 7 slider rows + reset + follow-outer
            let mut panel = div().absolute().w(px(PANEL_W));
            panel = match self.osd_at {
                Some(at) => {
                    let vp = window.viewport_size();
                    let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
                    let right = (vw - f32::from(at.x)).clamp(8., (vw - PANEL_W - 8.).max(8.));
                    let top = (f32::from(at.y) + 6.).clamp(8., (vh - PANEL_H_EST - 8.).max(8.));
                    panel.right(px(right)).top(px(top))
                }
                None => panel.top(px(36.)).right(px(110.)),
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
                    "DISPLAY — THIS PANE"
                } else {
                    "DISPLAY — OUTER"
                }))
                .child(rows)
                .child(
                    Self::bezel_btn(&th, "reset", grade.is_neutral()).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.reset_grade(cx);
                        }),
                    ),
                )
                .child(self.warp_slider(&th, cx))
                .child(self.track_slider(0, "roll", &th, cx))
                .child(self.track_slider(1, "roll spd", &th, cx))
                .child(self.track_slider(2, "roll size", &th, cx))
                .child(
                    div()
                        .id("track-reset")
                        .text_size(px(9.))
                        .text_color(th.text.alpha(0.5))
                        .cursor_pointer()
                        .child("↺ roll → per-theme")
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                theme::set_tracking_dial(cx, None);
                                ws.save(cx);
                            }),
                        ),
                );
            if is_pane {
                // Grade-group toggle, independent of the theme tray's: on = this
                // pane's monitor grade tracks the outer sliders live; off = it
                // keeps its own. Non-destructive (PaneTheme::toggle_grade).
                let lbl = if following {
                    "◉ follow outer"
                } else {
                    "◯ follow outer"
                };
                panel = panel.child(Self::bezel_btn(&th, lbl, following).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        if let Some(scope) = ws.osd_menu.clone() {
                            ws.toggle_grade_inherit(&scope, cx);
                        }
                    }),
                ));
            }
            // full-screen scrim: click anywhere outside closes
            div()
                .absolute()
                .inset_0()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.osd_menu = None;
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

        // ---- ? help modal: keys + commands, themed by the outer, over a dim scrim ----
        let help_overlay = self.help_open.then(|| {
            let (kc, dc, hc) = (th.accent, th.text.alpha(0.85), th.complement);
            let row = move |k: &str, d: &str| {
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_center()
                    .child(
                        div()
                            .min_w(px(150.))
                            .flex_none()
                            .text_color(kc)
                            .text_size(px(11.5))
                            .child(k.to_string()),
                    )
                    .child(
                        div()
                            .text_color(dc)
                            .text_size(px(11.5))
                            .child(d.to_string()),
                    )
            };
            let section = move |title: &str, rows: Vec<gpui::Div>| {
                let mut s = div().flex().flex_col().gap_1().child(
                    div()
                        .text_color(hc)
                        .text_size(px(10.5))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(title.to_string()),
                );
                for r in rows {
                    s = s.child(r);
                }
                s
            };
            let col_a = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap_4()
                .child(section(
                    "TABS & PANES",
                    vec![
                        row("Ctrl+Shift+T", "New tab"),
                        row("Ctrl+PgUp / PgDn", "Switch tabs"),
                        row("Ctrl+Alt+R / D", "Split ↔ / ↕"),
                        row("Alt + arrows", "Move focus between panes"),
                        row("drag a sub-tab", "Move / split · drag out = new window"),
                    ],
                ))
                .child(section(
                    "EDITING & CLIPBOARD",
                    vec![
                        row("right-click", "Copy · Paste · Open link · Clear"),
                        row("Ctrl+Shift+C / V", "Copy / Paste"),
                        row("double / triple-click", "Select word / line"),
                        row("Shift+Enter", "Newline (multiline in claude/codex)"),
                    ],
                ))
                .child(section(
                    "LINKS",
                    vec![row("Shift- or Ctrl-click", "Open a URL or file path")],
                ));
            let col_b = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap_4()
                .child(section(
                    "SCROLLBACK",
                    vec![
                        row("scroll wheel", "Scroll history"),
                        row("Ctrl+Shift+K", "Clear scrollback (not Ctrl+L)"),
                    ],
                ))
                .child(section(
                    "LOOK & FEEL",
                    vec![
                        row("theme icon (top-right)", "Themes & colour wheel"),
                        row("⛭", "Monitor grade (brightness, contrast…)"),
                        row("A──A · Ctrl+wheel", "Text size"),
                    ],
                ))
                .child(section(
                    "AGENTS · claude / codex",
                    vec![
                        row("Alt + ↑ / ↓", "Jump to your previous / next message"),
                        row("▲ ▼ (pane header)", "Same — navigate your own messages"),
                        row("your input colour", "Your turns stand out (set via the 👤 wheel pip)"),
                        row("bell on finish", "Pane shows ● done + a per-agent sound"),
                    ],
                ))
                .child(section(
                    "WINDOW",
                    vec![row("Ctrl+Alt+T", "New window (quick scratch)")],
                ));
            let close_x = div()
                .px_2()
                .py_0p5()
                .rounded_sm()
                .cursor_pointer()
                .text_color(th.text)
                .text_size(px(14.))
                .child("✕")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.help_open = false;
                        cx.notify();
                    }),
                );
            let panel = div()
                .w(gpui::relative(0.9))
                .max_w(px(940.))
                .max_h(gpui::relative(0.88))
                .overflow_hidden()
                .p_5()
                .rounded_lg()
                .border_1()
                .border_color(th.accent.alpha(0.6))
                .bg(darken(th.surface, 0.45))
                .text_color(th.text)
                .font_family(th.font_family.clone())
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.6),
                    offset: point(px(0.), px(8.)),
                    blur_radius: px(30.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .flex()
                .flex_col()
                .gap_4()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                .text_color(th.complement)
                                .child("▸ TERMINAL-DELIGHT · HELP"),
                        )
                        .child(close_x),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .w_full()
                        .gap_8()
                        .child(col_a)
                        .child(col_b),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(th.text.alpha(0.5))
                        .child("Esc or click outside to close · themes are live-editable TOML while it runs"),
                );
            div()
                .absolute()
                .inset_0()
                .bg(th.bg.alpha(0.74))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.help_open = false;
                        cx.notify();
                    }),
                )
                .child(panel)
        });

        // ---- per-tab colour tray (ctrl+click a tab) — a small swatch popover ----
        let tab_color_overlay = self.tab_color_edit.and_then(|i| {
            let tab = self.tabs.get(i)?;
            let current = tab.color;
            let label = tab.name.clone().unwrap_or_else(|| format!("tab {}", i + 1));
            let at = self.tab_color_at.unwrap_or_default();
            let mut swatches = div().flex().flex_row().flex_wrap().gap_1().max_w(px(184.));
            for &(h, s, l) in TAB_SWATCHES {
                let c = hsla(h, s, l, 1.);
                let selected = current.is_some_and(|cc| {
                    (cc.h - h).abs() < 0.001 && (cc.s - s).abs() < 0.001 && (cc.l - l).abs() < 0.001
                });
                swatches = swatches.child(
                    div()
                        .id(SharedString::from(format!(
                            "tab-swatch-{i}-{}",
                            (h * 1000.) as i32
                        )))
                        .w(px(22.))
                        .h(px(22.))
                        .rounded_full()
                        .bg(c)
                        .cursor_pointer()
                        .when(selected, |d| d.border_2().border_color(white()))
                        .when(!selected, |d| {
                            d.border_1().border_color(hsla(0., 0., 0., 0.5))
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                if let Some(t) = ws.tabs.get_mut(i) {
                                    t.color = Some(c);
                                }
                                ws.tab_color_edit = None;
                                ws.save(cx);
                                cx.notify();
                            }),
                        ),
                );
            }
            let clear = div()
                .id(SharedString::from(format!("tab-swatch-clear-{i}")))
                .px_2()
                .py_0p5()
                .rounded_sm()
                .border_1()
                .border_color(th.accent.alpha(0.5))
                .text_size(px(10.))
                .text_color(th.text)
                .cursor_pointer()
                .child("clear")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        if let Some(t) = ws.tabs.get_mut(i) {
                            t.color = None;
                        }
                        ws.tab_color_edit = None;
                        ws.save(cx);
                        cx.notify();
                    }),
                );
            let panel = div()
                .absolute()
                .left(px(f32::from(at.x)))
                .top(px(f32::from(at.y) + 8.))
                .p_2()
                .rounded_md()
                .border_1()
                .border_color(th.accent.alpha(0.6))
                .bg(darken(th.surface, 0.6))
                .shadow(vec![BoxShadow {
                    color: hsla(0., 0., 0., 0.55),
                    offset: point(px(3.), px(5.)),
                    blur_radius: px(16.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .flex()
                .flex_col()
                .gap_2()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(th.text.alpha(0.8))
                        .child(format!("\u{201c}{label}\u{201d} tab colour")),
                )
                .child(swatches)
                .child(div().flex().flex_row().justify_end().child(clear));
            // full-window scrim: a click anywhere else dismisses the tray
            Some(
                div()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            ws.tab_color_edit = None;
                            cx.notify();
                        }),
                    )
                    .child(panel),
            )
        });

        // ---- FOCUS reading modal: an 80%-of-window mirror of one pane ----
        // Everything else dims back. The body is the SAME styled rows the live
        // pane builds, just scaled to fill the modal — so it's a true live
        // mirror (no second terminal) and keystrokes still reach the original.
        // FOCUS ease-in: a smoothstep ramp (0→1 over ~220ms) the dim + blur ride
        // on, so the frosted backdrop melts in. Kept alive by requesting frames
        // until it settles; reset when the modal closes.
        let focus_on = self.focus_read.as_ref().and_then(|w| w.upgrade()).is_some();
        let focus_ramp = {
            let mut guard = FOCUS_OPEN_AT.lock().unwrap();
            if focus_on {
                let t0 = *guard.get_or_insert_with(std::time::Instant::now);
                let e = t0.elapsed().as_secs_f32();
                let f = (e / 0.22).clamp(0.0, 1.0);
                f * f * (3.0 - 2.0 * f) // smoothstep
            } else {
                *guard = None;
                0.0
            }
        };
        if focus_on && focus_ramp < 1.0 {
            window.request_animation_frame();
        }
        let focus_overlay = self
            .focus_read
            .as_ref()
            .and_then(|w| w.upgrade())
            .map(|pane| {
                let snap = pane.update(cx, |v, cx| v.mirror_snapshot(cx));
                let (ww, wh) = self
                    .last_win
                    .map(|(_, _, w, h)| (w, h))
                    .unwrap_or((1200., 800.));
                let panel_w = (ww * 0.8).max(320.);
                let panel_h = (wh * 0.8).max(240.);
                let pad = 16.0_f32;
                let hdr_h = 30.0_f32;
                let avail_w = (panel_w - pad * 2.).max(1.);
                let avail_h = (panel_h - hdr_h - pad * 2.).max(1.);
                let content_w = (snap.cols as f32 * snap.cell_w).max(1.);
                let content_h = (snap.rows as f32 * snap.cell_h).max(1.);
                // scale the whole grid to fit the modal (tighter axis wins so the
                // entire screen stays visible); never shrink past ~0.7×.
                let ms = (avail_w / content_w)
                    .min(avail_h / content_h)
                    .clamp(0.7, 6.0);
                let cell_h = snap.cell_h * ms;
                let body = div()
                    .flex()
                    .flex_col()
                    .text_size(px(snap.base_size * ms))
                    .text_color(snap.text)
                    .font_family(snap.font_family.clone())
                    .children(snap.lines.into_iter().map(move |(text, runs)| {
                        let line = div().h(px(cell_h)).whitespace_nowrap();
                        if text.is_empty() {
                            line
                        } else {
                            line.child(gpui::StyledText::new(text).with_runs(runs))
                        }
                    }));
                let header = div()
                    .h(px(hdr_h))
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .text_size(px(12.))
                    .text_color(snap.accent)
                    .child(format!("👓  FOCUS · {}", snap.title))
                    .child(div().text_color(snap.text.alpha(0.6)).child("esc to close"));
                let panel = div()
                    .w(px(panel_w))
                    .h(px(panel_h))
                    .flex()
                    .flex_col()
                    .rounded(px(12.))
                    .overflow_hidden()
                    .bg(snap.bg)
                    .border_2()
                    .border_color(snap.accent.alpha(0.7))
                    .shadow(vec![
                        BoxShadow {
                            color: hsla(0., 0., 0., 0.7),
                            offset: point(px(0.), px(10.)),
                            blur_radius: px(40.),
                            spread_radius: px(2.),
                            inset: false,
                        },
                        BoxShadow {
                            color: snap.accent.alpha(0.18),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(48.),
                            spread_radius: px(2.),
                            inset: false,
                        },
                    ])
                    .child(header)
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .p(px(pad))
                            .child(body),
                    )
                    // Measure the panel's exact on-screen box (physical px) and arm
                    // the FOCUS backdrop blur: the CRT post-pass frosts everything
                    // outside this rect while the panel itself stays razor-sharp.
                    // Using the real prepaint bounds (not an analytic centre) keeps
                    // the sharp/blur edge pixel-aligned through the CSD shadow margin.
                    .child(
                        div().absolute().inset_0().child(
                            gpui::canvas(
                                move |bounds, window, _cx| {
                                    let sf = window.scale_factor();
                                    crate::warp::set_focus_blur(
                                        [
                                            f32::from(bounds.origin.x) * sf,
                                            f32::from(bounds.origin.y) * sf,
                                            f32::from(bounds.size.width) * sf,
                                            f32::from(bounds.size.height) * sf,
                                        ],
                                        28.0 * sf * focus_ramp, // blur radius (eases in)
                                        16.0 * sf,              // feather across the panel edge
                                        focus_ramp,             // frosted-glass tint (eases in)
                                        12.0 * sf, // corner radius — matches rounded(12)
                                    );
                                },
                                |_, _, _, _| {},
                            )
                            .size_full(),
                        ),
                    )
                    // clicks inside the panel must not fall through to the scrim
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                    );
                // Dim + LOCK scrim over the whole window. `.occlude()` makes it
                // swallow every mouse event (clicks AND scroll) so nothing behind
                // the modal can be focused, scrolled, or typed into — you stay in
                // the FOCUS pane. The 0.60 dim rides UNDER the frosted backdrop the
                // CRT pass paints (the shader blurs these dimmed pixels). A click on
                // the dimmed area outside the panel closes it; esc closes too.
                div()
                    .absolute()
                    .inset_0()
                    .occlude()
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(hsla(0., 0., 0., 0.6 * focus_ramp))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| ws.close_focus_read(cx)),
                    )
                    // The scrim occludes the pane behind it, so route the wheel back
                    // to the focused terminal's scrollback and re-paint the mirror.
                    // Scrolling anywhere over the modal (panel or dimmed surround)
                    // drives the read pane — you never lose the wheel while reading.
                    .on_scroll_wheel(cx.listener(|ws, ev: &ScrollWheelEvent, _w, cx| {
                        if let Some(pane) = ws.focus_read.as_ref().and_then(|w| w.upgrade()) {
                            pane.update(cx, |v, cx| v.scroll_by_wheel(ev, cx));
                            cx.notify();
                        }
                    }))
                    .child(panel)
            });

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
            // Doubled black bezel gap between the mother bar and the screen.
            .mt(px(7.))
            .child(pane_area);

        let root = div()
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
            // Click anywhere outside an open tab-rename box (a terminal, the
            // bezel, empty space) saves the rename — the edit no longer eats
            // keystrokes meant for the pane you just clicked into. The tab /
            // pencil / rename-box handlers stop_propagation, so they don't trip
            // this; a click ON the edit box keeps editing.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    ws.commit_rename(cx);
                }),
            )
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
                    .children(osd_overlay)
                    .children(confirm_overlay)
                    .children(help_overlay)
                    .children(tab_color_overlay)
                    .children(drag_chip)
                    // the FOCUS reading modal rides above everything else
                    .children(focus_overlay),
            );
        // Frameless: wrap the cabinet in client-side decorations (shadow margin,
        // rounded clip, live resize edges) so it runs with no system titlebar.
        csd::decorate(root, window)
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
    fn tab_focus_prefers_the_remembered_pane_then_falls_back() {
        let leaves = [10u32, 20, 30];
        // remembered pane still open → focus it
        assert_eq!(pick_focus_target(Some(20), &leaves), Some(20));
        // remembered pane was closed → fall back to the first
        assert_eq!(pick_focus_target(Some(99), &leaves), Some(10));
        // nothing remembered (fresh tab) → first pane, so a 1-pane tab just types
        assert_eq!(pick_focus_target(None, &leaves), Some(10));
        // empty tab → nothing to focus
        assert_eq!(pick_focus_target::<u32>(Some(1), &[]), None);
        assert_eq!(pick_focus_target::<u32>(None, &[]), None);
    }

    #[test]
    fn outside_bounds_fires_only_past_the_window_edge() {
        // inside (and on the edge) is not a tear-off
        assert!(!outside_bounds(0.0, 0.0, 800.0, 600.0));
        assert!(!outside_bounds(400.0, 300.0, 800.0, 600.0));
        assert!(!outside_bounds(800.0, 600.0, 800.0, 600.0));
        // any axis past the edge is
        assert!(outside_bounds(-1.0, 300.0, 800.0, 600.0));
        assert!(outside_bounds(400.0, -1.0, 800.0, 600.0));
        assert!(outside_bounds(801.0, 300.0, 800.0, 600.0));
        assert!(outside_bounds(400.0, 601.0, 800.0, 600.0));
    }

    #[test]
    fn reorder_matches_a_real_remove_then_insert_and_follows_active() {
        // For every small strip, every grab, every drop slot, and every active
        // selection: the (dest, new_active) math must agree with actually doing
        // the remove+insert on a labelled vec — i.e. `active` keeps pointing at
        // the SAME tab id after the move.
        for len in 1..=6usize {
            for from in 0..len {
                for to in 0..=len {
                    for active in 0..len {
                        let (dest, new_active) = reorder_indices(from, to, len, active);
                        // simulate on ids 0..len
                        let mut v: Vec<usize> = (0..len).collect();
                        let was_active_id = v[active];
                        let t = v.remove(from);
                        v.insert(dest, t);
                        assert!(dest < len, "dest in range");
                        assert_eq!(
                            v[new_active], was_active_id,
                            "active still points at its tab (len={len} from={from} to={to} active={active})"
                        );
                    }
                }
            }
        }
        // a couple of explicit landmarks
        assert_eq!(reorder_indices(0, 3, 4, 0), (2, 2)); // drag tab0 right two slots
        assert_eq!(reorder_indices(3, 0, 4, 3), (0, 0)); // drag last tab to front
        assert_eq!(reorder_indices(1, 1, 4, 2).0, 1); // drop in own slot → no-op dest
    }

    #[test]
    fn comm_truncates_to_the_kernel_15_char_limit() {
        // "terminal-delight" is 16 chars; /proc/<pid>/comm shows only 15
        assert_eq!(truncated_comm("terminal-delight"), "terminal-deligh");
        assert_eq!(truncated_comm("short"), "short");
    }

    #[test]
    fn scratch_decision_covers_force_seed_and_peer() {
        // lone launch, nothing running → primary restore, no seed
        let (scratch, seed) = scratch_decision(false, false, None, None);
        assert!(!scratch);
        assert!(seed.is_none());

        // a sibling is already running → scratch, still no seed
        let (scratch, seed) = scratch_decision(false, true, None, None);
        assert!(scratch);
        assert!(seed.is_none());

        // forced scratch with no peer (TD_SCRATCH=1)
        assert!(scratch_decision(true, false, None, None).0);

        // a torn-off pane seeds cwd/resume and is always scratch
        let (scratch, seed) = scratch_decision(
            false,
            false,
            Some("/tmp/work".into()),
            Some("claude --resume x".into()),
        );
        assert!(scratch);
        let seed = seed.expect("seeded");
        assert_eq!(seed.cwd.as_deref(), Some("/tmp/work"));
        assert_eq!(seed.resume.as_deref(), Some("claude --resume x"));
    }

    #[test]
    fn hsla_to_hex_matches_known_colours_and_round_trips() {
        // primaries + a grey land on their exact hex
        assert_eq!(hsla_to_hex(hsla(0.0, 1.0, 0.5, 1.0)), "#ff0000");
        assert_eq!(hsla_to_hex(hsla(1.0 / 3.0, 1.0, 0.5, 1.0)), "#00ff00");
        assert_eq!(hsla_to_hex(hsla(2.0 / 3.0, 1.0, 0.5, 1.0)), "#0000ff");
        assert_eq!(hsla_to_hex(hsla(0.0, 0.0, 0.5, 1.0)), "#808080");
        // hex -> hsla -> hex is stable (the wheel stores what it reads back)
        for hexs in ["#2f6fdd", "#31d7ff", "#00ff9c", "#ff8a3d", "#872d73"] {
            let c = theme::parse_hex(hexs).unwrap();
            let back = theme::parse_hex(&hsla_to_hex(c)).unwrap();
            assert!((c.h - back.h).abs() < 0.01 || (c.s < 0.02));
            assert!((c.s - back.s).abs() < 0.02);
            assert!((c.l - back.l).abs() < 0.02);
        }
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
            appearance: if *l == 2 {
                PaneTheme::from_legacy(ThemeChoice {
                    id: "hacker".into(),
                    seed: None,
                    ..Default::default()
                })
            } else {
                PaneTheme::default()
            },
            ..Default::default()
        });
        let SavedNode::Split { a, b, .. } = &saved else {
            panic!("split expected");
        };
        let SavedNode::Leaf { appearance, .. } = &**a else {
            panic!("leaf expected");
        };
        assert!(appearance.is_pristine(), "leaf 1 should follow outer");
        let SavedNode::Leaf { appearance, .. } = &**b else {
            panic!("override lost");
        };
        assert_eq!(appearance.theme.as_ref().unwrap().id, "hacker");
        assert!(
            !appearance.inherit_theme,
            "an override pins the theme group"
        );
    }

    #[test]
    fn to_saved_carries_per_leaf_custom_name() {
        // a right-click rename lands in LeafState.name and must survive a
        // serialize→deserialize round-trip so the name persists across reboot.
        let t: Tree<u32> = Tree::Leaf(7);
        let saved = t.to_saved_with(&|_| LeafState {
            name: Some("build".into()),
            ..Default::default()
        });
        assert!(
            matches!(&saved, SavedNode::Leaf { name: Some(n), .. } if n == "build"),
            "custom name lost on save"
        );
        let toml = toml::to_string(&SavedTab {
            name: None,
            color: None,
            node: saved,
        })
        .expect("serialize");
        let back: SavedTab = toml::from_str(&toml).expect("deserialize");
        assert!(
            matches!(back.node, SavedNode::Leaf { name: Some(n), .. } if n == "build"),
            "custom name lost on reload"
        );
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
        let SavedNode::Leaf { appearance, .. } = &state.tabs[0].node else {
            panic!("string leaf should parse to a Leaf");
        };
        assert!(appearance.is_pristine(), "a bare string leaf follows outer");
    }

    #[test]
    fn legacy_per_pane_theme_override_migrates_to_full_override() {
        // Pre-per-group state files wrote a single `theme` table under a leaf,
        // meaning "this pane overrides everything and follows outer for nothing".
        let legacy = r#"
active = 0
[[tabs]]
[tabs.node.Leaf.theme]
id = "hacker"
"#;
        let state: StateFile = toml::from_str(legacy).expect("legacy leaf parses");
        let SavedNode::Leaf { appearance, .. } = &state.tabs[0].node else {
            panic!("leaf expected");
        };
        assert_eq!(appearance.theme.as_ref().unwrap().id, "hacker");
        assert!(
            !appearance.inherit_theme && !appearance.inherit_grade,
            "a legacy override follows outer for neither group"
        );
    }

    #[test]
    fn screen_warp_is_a_dial_that_round_trips_and_migrates_legacy_bool() {
        // absent `warp` → the default dial
        let legacy: StateFile = toml::from_str("active = 0\n[[tabs]]\nnode = \"Leaf\"\n").unwrap();
        assert!(
            (legacy.warp - theme::WARP_DEFAULT).abs() < 1e-6,
            "absent → default"
        );
        // a float dial survives the wire
        let s = StateFile {
            warp: 1.2,
            ..Default::default()
        };
        let back: StateFile = toml::from_str(&toml::to_string(&s).unwrap()).unwrap();
        assert!((back.warp - 1.2).abs() < 1e-6, "dial round-trips");
        // legacy bool toggle migrates: true → default dial, false → flat
        let on: StateFile =
            toml::from_str("active = 0\nwarp = true\n[[tabs]]\nnode = \"Leaf\"\n").unwrap();
        assert!(
            (on.warp - theme::WARP_DEFAULT).abs() < 1e-6,
            "true → default dial"
        );
        let off: StateFile =
            toml::from_str("active = 0\nwarp = false\n[[tabs]]\nnode = \"Leaf\"\n").unwrap();
        assert_eq!(off.warp, 0.0, "false → flat");
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
            warp: theme::WARP_DEFAULT,
            track: None,
            tabs: vec![SavedTab {
                name: None,
                color: None,
                node: SavedNode::Leaf {
                    appearance: PaneTheme::from_legacy(ThemeChoice {
                        id: "hacker".into(),
                        seed: None,
                        ..Default::default()
                    }),
                    cwd: None,
                    resume: None,
                    name: None,
                },
            }],
        };
        let body = toml::to_string(&state).expect("serializes");
        let back: StateFile = toml::from_str(&body).expect("round-trips");
        assert_eq!(back.theme.as_ref().unwrap().id, "tactical-overdrive");
        assert_eq!(back.win, Some((12.0, 34.0, 1280.0, 720.0)));
        let SavedNode::Leaf { appearance, .. } = &back.tabs[0].node else {
            panic!("leaf override lost");
        };
        assert_eq!(appearance.theme.as_ref().unwrap().id, "hacker");
        assert!(!appearance.inherit_theme);
    }

    #[test]
    fn leaf_work_state_round_trips() {
        let state = StateFile {
            active: 0,
            win: None,
            scale: None,
            theme: None,
            warp: theme::WARP_DEFAULT,
            track: None,
            tabs: vec![SavedTab {
                name: Some("agents".into()),
                color: None,
                node: SavedNode::Leaf {
                    appearance: PaneTheme::default(),
                    cwd: Some("/home/user/proj".into()),
                    resume: Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d".into()),
                    name: None,
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
    fn split_node_at_reframes_a_whole_container() {
        // build a row of two: 1 | 2
        let mut t: Tree<u32> = Tree::Leaf(1);
        t.split_leaf(&|l| *l == 1, SplitDir::Row, 2);
        let root_id = match &t {
            Tree::Split { id, .. } => *id,
            _ => panic!("expected a split root"),
        };
        // re-frame the ENTIRE container as a column, dropped pane trailing
        // (a Bottom drop on the field) → Col( Row(1,2), 3 )
        assert!(t.split_node_at(root_id, SplitDir::Col, 3, false));
        let Tree::Split {
            a,
            b,
            dir: SplitDir::Col,
            ..
        } = &t
        else {
            panic!("the whole field should now be a column");
        };
        assert!(matches!(**a, Tree::Split { .. }), "old row stays intact");
        assert!(matches!(**b, Tree::Leaf(3)), "dropped pane trails");
        assert_eq!(leaf_ids(&t), vec![1, 2, 3]);
        // a miss on an unknown container id changes nothing
        assert!(!t.split_node_at(999_999, SplitDir::Row, 4, true));
        assert_eq!(leaf_ids(&t), vec![1, 2, 3]);
    }

    #[test]
    fn near_perimeter_is_the_outer_frame_only() {
        let rect = Bounds {
            origin: point(px(0.), px(0.)),
            size: size(px(400.), px(300.)),
        };
        let band = edge_band(rect); // 0.18 * 300 = 54 → clamped to 34
        assert!((band - 34.).abs() < 0.01);
        // dead center is interior → a leaf split, not a re-frame
        assert!(!near_perimeter(rect, point(px(200.), px(150.)), band));
        // hugging each edge → re-frame
        assert!(near_perimeter(rect, point(px(4.), px(150.)), band));
        assert!(near_perimeter(rect, point(px(398.), px(150.)), band));
        assert!(near_perimeter(rect, point(px(200.), px(2.)), band));
        assert!(near_perimeter(rect, point(px(200.), px(298.)), band));
    }

    #[test]
    fn split_for_maps_edges_to_axes() {
        assert!(matches!(split_for(Zone::Left), (SplitDir::Row, true)));
        assert!(matches!(split_for(Zone::Right), (SplitDir::Row, false)));
        assert!(matches!(split_for(Zone::Top), (SplitDir::Col, true)));
        assert!(matches!(split_for(Zone::Bottom), (SplitDir::Col, false)));
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

    // load_state() reads $HOME so isn't callable in tests, but its body is
    // `read.ok().and_then(parse.ok()).unwrap_or_default()` — pin that parse
    // contract so a corrupt or old state.toml degrades to a clean boot instead
    // of bricking startup or silently producing a zero-tab workspace.
    #[test]
    fn malformed_or_partial_state_falls_back_safely() {
        // garbage never parses → load_state's .ok() yields the default
        assert!(toml::from_str::<StateFile>("not valid [ toml").is_err());
        let recovered: StateFile = toml::from_str::<StateFile>("not valid [ toml")
            .ok()
            .unwrap_or_default();
        assert!(recovered.tabs.is_empty(), "garbage degrades to default");
        // `tabs` has no serde default: a file with no [[tabs]] must be REJECTED
        // (→ fall back to a fresh one-tab workspace), not load as zero tabs.
        assert!(
            toml::from_str::<StateFile>("active = 0").is_err(),
            "a tabs-less file is rejected, not silently empty"
        );
        // the real pre-window-persistence shape: only active + one leaf tab,
        // win/scale/theme all absent → loads with those optionals None.
        let legacy = "active = 0\n[[tabs]]\nnode = \"Leaf\"\n";
        let s: StateFile = toml::from_str(legacy).expect("minimal legacy state loads");
        assert_eq!(s.tabs.len(), 1);
        assert!(s.win.is_none() && s.scale.is_none() && s.theme.is_none());
    }

    #[test]
    fn nested_layout_round_trips_and_legacy_split_defaults_ratio() {
        // a 3-deep tree: Row( Col(leaf,leaf) @0.7 , leaf ) @0.3
        let leaf = || SavedNode::Leaf {
            appearance: PaneTheme::default(),
            cwd: None,
            resume: None,
            name: None,
        };
        let node = SavedNode::Split {
            dir: SplitDir::Row,
            ratio: 0.3,
            a: Box::new(SavedNode::Split {
                dir: SplitDir::Col,
                ratio: 0.7,
                a: Box::new(leaf()),
                b: Box::new(leaf()),
            }),
            b: Box::new(leaf()),
        };
        let state = StateFile {
            active: 0,
            win: None,
            scale: None,
            theme: None,
            warp: theme::WARP_DEFAULT,
            track: None,
            tabs: vec![SavedTab {
                name: None,
                color: None,
                node,
            }],
        };
        let body = toml::to_string(&state).expect("serializes");
        let back: StateFile = toml::from_str(&body).expect("round-trips");
        let SavedNode::Split { dir, ratio, a, .. } = &back.tabs[0].node else {
            panic!("outer must stay a split");
        };
        assert!(matches!(dir, SplitDir::Row));
        assert!((ratio - 0.3).abs() < 1e-6, "outer ratio survives nesting");
        let SavedNode::Split { dir, ratio, .. } = a.as_ref() else {
            panic!("inner must stay a split");
        };
        assert!(matches!(dir, SplitDir::Col));
        assert!((ratio - 0.7).abs() < 1e-6, "inner ratio survives nesting");

        // a saved Split missing its ratio (older format) fills the neutral 0.5
        let no_ratio =
            "active = 0\n[[tabs]]\n[tabs.node.Split]\ndir = \"Col\"\na = \"Leaf\"\nb = \"Leaf\"\n";
        let s: StateFile = toml::from_str(no_ratio).expect("ratio-less split loads");
        let SavedNode::Split { ratio, .. } = &s.tabs[0].node else {
            panic!("expected a split");
        };
        assert!(
            (ratio - default_ratio()).abs() < 1e-6,
            "missing ratio -> 0.5"
        );
    }
}

/// Which pane a tab should focus when you switch to it: the one you were last in
/// (if it's still open), else the first. Pure so the precedence is testable.
fn pick_focus_target<T: PartialEq + Copy>(remembered: Option<T>, leaves: &[T]) -> Option<T> {
    remembered
        .filter(|id| leaves.contains(id))
        .or_else(|| leaves.first().copied())
}

/// True when the cursor sits outside the `w`×`h` window content box. During an
/// X11 header drag the implicit pointer grab keeps delivering positions past the
/// edge, so this is how we notice a pane being dragged out for a tear-off.
fn outside_bounds(x: f32, y: f32, w: f32, h: f32) -> bool {
    x < 0.0 || y < 0.0 || x > w || y > h
}

/// Pure index math for an outer-tab reorder. Moving the tab at `from` into the
/// insertion slot `to` (pre-removal space, 0..=len) lands it at `dest`; the
/// `active` selection is remapped so it keeps pointing at the very same tab.
/// Returns `(dest, new_active)`. `dest == from` means a no-op (dropped back in
/// its own slot).
fn reorder_indices(from: usize, to: usize, len: usize, active: usize) -> (usize, usize) {
    let to = to.min(len);
    // removal then insertion shifts the destination one left when moving right
    let dest = if to > from { to - 1 } else { to };
    let new_active = if active == from {
        dest
    } else {
        let mut a = active;
        if a > from {
            a -= 1; // the removal pulled it left
        }
        if a >= dest {
            a += 1; // the insertion pushed it right
        }
        a
    };
    (dest, new_active)
}

/// `/proc/<pid>/comm` truncates the process name to 15 visible chars; mirror that
/// so the running-instance check compares like with like.
fn truncated_comm(name: &str) -> String {
    name.chars().take(15).collect()
}

/// Resolve scratch-mode + an optional seed from the inputs. Factored out (pure)
/// so the env/proc plumbing in `main` stays testable.
fn scratch_decision(
    force: bool,
    peer_running: bool,
    cwd: Option<String>,
    resume: Option<String>,
) -> (bool, Option<session::PaneRestore>) {
    let seeded = cwd.is_some() || resume.is_some();
    let seed = if seeded {
        Some(session::PaneRestore { cwd, resume })
    } else {
        None
    };
    (force || seeded || peer_running, seed)
}

/// Is another terminal-delight process already alive? Cheap, permissionless
/// `/proc` comm scan — no lockfile to leak. Drives the conditional boot: a second
/// launch (e.g. the Ctrl+Alt+T hotkey) opens a quick scratch window instead of
/// re-restoring the whole saved session.
fn another_instance_running() -> bool {
    let me = std::process::id();
    let want = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
        .map(|n| truncated_comm(&n));
    let Some(want) = want else { return false };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return false;
    };
    for e in entries.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if pid == me {
            continue;
        }
        if let Ok(comm) = std::fs::read_to_string(e.path().join("comm")) {
            if comm.trim() == want {
                return true;
            }
        }
    }
    false
}

/// Launch a fresh, detached terminal-delight seeded with a torn-off pane's cwd
/// and agent session. The child sees a peer (us) running, so it boots as a
/// scratch window automatically; the seed env tells it what to reopen.
fn spawn_seeded_window(rt: &session::PaneRuntime) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    cmd.env("TD_SCRATCH", "1");
    if let Some(cwd) = &rt.cwd {
        cmd.env("TD_SEED_CWD", cwd);
    }
    if let Some(resume) = &rt.resume {
        cmd.env("TD_SEED_RESUME", resume);
    }
    // detach into its own session so it outlives us and ignores our signals
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let _ = cmd.spawn();
}

fn main() {
    // Decide boot mode before the window opens: forced scratch (TD_SCRATCH),
    // a seeded tear-off (TD_SEED_*), or "a sibling is already running" all open
    // a small single-terminal window; a lone launch restores the full session.
    let force = std::env::var_os("TD_SCRATCH").is_some();
    let seed_cwd = std::env::var("TD_SEED_CWD").ok().filter(|s| !s.is_empty());
    let seed_resume = std::env::var("TD_SEED_RESUME")
        .ok()
        .filter(|s| !s.is_empty());
    let (scratch, seed) =
        scratch_decision(force, another_instance_running(), seed_cwd, seed_resume);

    application().run(move |cx: &mut App| {
        theme::init(cx);
        bell::ensure_seeded(); // populate the sounds dir from bundled defaults if empty
        let bounds = if scratch {
            // a quick window: ~45% of the display wide, ~40% tall, centred
            let size_px = cx
                .primary_display()
                .map(|d| d.bounds().size)
                .map(|s| {
                    size(
                        px((f32::from(s.width) * 0.45).max(480.0)),
                        px((f32::from(s.height) * 0.40).max(480.0)),
                    )
                })
                .unwrap_or_else(|| size(px(860.), px(640.)));
            Bounds::centered(None, size_px, cx)
        } else {
            // reboot into the exact window the user closed (or crashed) from
            match load_state().win {
                Some((x, y, w, h)) => Bounds {
                    origin: point(px(x), px(y)),
                    size: size(px(w.max(480.)), px(h.max(320.))),
                },
                None => Bounds::centered(None, size(px(1280.), px(720.)), cx),
            }
        };
        let seed = seed.clone();
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
                // Frameless: no OS titlebar — the cabinet IS the chrome. We draw
                // our own move handle (the mother bar), resize edges + window
                // controls (see `csd` + the bezel_top control cluster).
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            },
            move |window, cx| {
                // First-run self-diagnostics for untested boxes (AMD/Intel,
                // Wayland, fractional scaling): record installed fonts so the grid
                // can fall back deliberately, and surface the GPU/driver gpui chose.
                pane::init_font_registry(cx.text_system().all_font_names());
                if let Some(msg) = pane::font_diagnostic() {
                    eprintln!("terminal-delight: {msg}");
                }
                if let Some(g) = window.gpu_specs() {
                    eprintln!(
                        "terminal-delight: GPU {} (driver {} {}){}",
                        g.device_name,
                        g.driver_name,
                        g.driver_info,
                        if g.is_software_emulated {
                            " [SOFTWARE renderer — expect low FPS]"
                        } else {
                            ""
                        },
                    );
                }
                if scratch {
                    cx.new(|cx| Workspace::new_scratch(seed.clone(), window, cx))
                } else {
                    cx.new(|cx| Workspace::new(window, cx))
                }
            },
        )
        .expect("open window");
        cx.activate(true);
    });
}
