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
mod demo;
mod gamba;
mod hud;
mod lang;
mod mcp;
mod mcp_tail;
mod mcp_transport;
mod pane;
mod plugins;
mod recover;
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
    white, App, Bounds, BoxShadow, ClipboardItem, Context, Decorations, Entity, EntityId,
    Focusable, Hsla, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point, ScrollWheelEvent, SharedString, TitlebarOptions, Window, WindowBounds,
    WindowDecorations, WindowOptions,
};
use gpui_platform::application;
use pane::{
    CloseFocusRead, ClosePane, DragPaneStart, OpenAgentPanel, OpenDisplayMenu, OpenFind,
    OpenFocusRead, OpenHelp, OpenThemeMenu, PaneRenamed, RequestCloseTab, TerminalView,
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
    /// The "binder divider" FILL colour for THIS tab's button — a stable property
    /// of the tab itself, NOT derived from any pane's theme. Set via the tab
    /// config pane, it never shifts when a sub-terminal overrides its look.
    /// `None` = inherit the group's colour (if grouped) else the plain bezel.
    /// Persisted as a hex string in the state file.
    color: Option<Hsla>,
    /// TEXT-colour override for this tab's label. `None` = inherit the group's
    /// text colour (if any) else the outer bar's text colour. Persisted as hex.
    text_color: Option<Hsla>,
    /// The group this tab belongs to (a [`TabGroup::id`]), if any. Members of a
    /// group render under a shared colour band on the mother bar. Persisted.
    group: Option<u32>,
}

impl Tab {
    fn new(root: Node, name: Option<String>) -> Self {
        Self {
            root,
            name,
            focused: None,
            color: None,
            text_color: None,
            group: None,
        }
    }
}

/// `true` if `c` counts as part of a "word" for ctrl-arrow navigation.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// An inline single-line text editor with a caret and a selection range —
/// backs the tab- and group-rename boxes so they honour normal text navigation:
/// arrows (char), ctrl+arrows (word), shift+… (extend selection), home/end,
/// ctrl+a (select all), backspace/delete. Indices are into `chars` (so a
/// multi-byte glyph is one step). `anchor == cursor` means no selection.
#[derive(Clone, Default)]
struct EditBuffer {
    chars: Vec<char>,
    cursor: usize,
    anchor: usize,
}

impl EditBuffer {
    /// Seed from an existing name with the whole thing selected (the file-manager
    /// rename gesture: the first keystroke replaces it, arrows still navigate).
    fn seeded(s: &str) -> Self {
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len();
        Self {
            chars,
            cursor: len,
            anchor: 0,
        }
    }

    fn text(&self) -> String {
        self.chars.iter().collect()
    }

    fn has_sel(&self) -> bool {
        self.cursor != self.anchor
    }

    fn sel_range(&self) -> (usize, usize) {
        (self.cursor.min(self.anchor), self.cursor.max(self.anchor))
    }

    /// Drop the selected run (if any); leaves the caret collapsed at its start.
    fn delete_sel(&mut self) -> bool {
        if !self.has_sel() {
            return false;
        }
        let (a, b) = self.sel_range();
        self.chars.drain(a..b);
        self.cursor = a;
        self.anchor = a;
        true
    }

    fn prev_word(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && !is_word_char(self.chars[i - 1]) {
            i -= 1;
        }
        while i > 0 && is_word_char(self.chars[i - 1]) {
            i -= 1;
        }
        i
    }

    fn next_word(&self) -> usize {
        let n = self.chars.len();
        let mut i = self.cursor;
        while i < n && !is_word_char(self.chars[i]) {
            i += 1;
        }
        while i < n && is_word_char(self.chars[i]) {
            i += 1;
        }
        i
    }

    /// Apply one keystroke. Enter/Escape are handled by the caller before this is
    /// reached. `max` caps the inserted length.
    fn apply(&mut self, key: &str, m: &gpui::Modifiers, key_char: Option<&str>, max: usize) {
        let extend = m.shift;
        let n = self.chars.len();
        match key {
            "left" => {
                let to = if m.control {
                    self.prev_word()
                } else if self.has_sel() && !extend {
                    self.sel_range().0
                } else {
                    self.cursor.saturating_sub(1)
                };
                self.cursor = to;
                if !extend {
                    self.anchor = to;
                }
            }
            "right" => {
                let to = if m.control {
                    self.next_word()
                } else if self.has_sel() && !extend {
                    self.sel_range().1
                } else {
                    (self.cursor + 1).min(n)
                };
                self.cursor = to;
                if !extend {
                    self.anchor = to;
                }
            }
            "home" => {
                self.cursor = 0;
                if !extend {
                    self.anchor = 0;
                }
            }
            "end" => {
                self.cursor = n;
                if !extend {
                    self.anchor = n;
                }
            }
            "a" if m.control => {
                self.anchor = 0;
                self.cursor = n;
            }
            "backspace" => {
                if !self.delete_sel() && self.cursor > 0 {
                    let to = if m.control {
                        self.prev_word()
                    } else {
                        self.cursor - 1
                    };
                    self.chars.drain(to..self.cursor);
                    self.cursor = to;
                    self.anchor = to;
                }
            }
            "delete" => {
                if !self.delete_sel() && self.cursor < n {
                    let to = if m.control {
                        self.next_word()
                    } else {
                        self.cursor + 1
                    };
                    self.chars.drain(self.cursor..to);
                    self.anchor = self.cursor;
                }
            }
            _ => {
                // a printable character: ctrl/alt chords never type
                if m.control || m.alt {
                    return;
                }
                if let Some(ch) = key_char.filter(|c| !c.is_empty()) {
                    self.delete_sel();
                    let incoming: Vec<char> = ch.chars().collect();
                    let room = max.saturating_sub(self.chars.len());
                    for c in incoming.into_iter().take(room) {
                        self.chars.insert(self.cursor, c);
                        self.cursor += 1;
                    }
                    self.anchor = self.cursor;
                }
            }
        }
    }
}

/// Render an [`EditBuffer`] as text with a selection highlight and a caret, in a
/// flex row. Shared by the tab- and group-rename boxes.
fn render_edit_buffer(
    eb: &EditBuffer,
    s: f32,
    text_col: Hsla,
    caret_col: Hsla,
    sel_col: Hsla,
) -> gpui::Div {
    let span =
        |run: &[char], col: Hsla| div().text_color(col).child(run.iter().collect::<String>());
    let caret = || div().w(px(2. * s)).h(px(13. * s)).bg(caret_col);
    let highlight = |run: &[char]| {
        div()
            .bg(sel_col)
            .text_color(text_col)
            .child(run.iter().collect::<String>())
    };
    let mut row = div().flex().flex_row().items_center();
    let (a, b) = eb.sel_range();
    let len = eb.chars.len();
    if a == b {
        row = row
            .child(span(&eb.chars[0..a], text_col))
            .child(caret())
            .child(span(&eb.chars[a..len], text_col));
    } else {
        row = row.child(span(&eb.chars[0..a], text_col));
        if eb.cursor == a {
            row = row.child(caret());
        }
        row = row.child(highlight(&eb.chars[a..b]));
        if eb.cursor == b {
            row = row.child(caret());
        }
        row = row.child(span(&eb.chars[b..len], text_col));
    }
    row
}

/// A browser-style tab group: a coloured band on the mother bar wrapping a run of
/// adjacent member tabs. The group's colour + text colour *lead* its members (a
/// member tab can still override with its own). Persisted in the state file.
#[derive(Clone)]
struct TabGroup {
    id: u32,
    name: Option<String>,
    /// The band/fill colour — always set (a group is never colourless).
    color: Hsla,
    /// Optional text-colour lead for member labels; `None` = the bar's text.
    text_color: Option<Hsla>,
    /// Folded into a single counted pill when true (unless it holds the active
    /// tab, which force-expands so you never lose your place).
    collapsed: bool,
}

/// Which colour a tab-config wheel pip edits: the button FILL or the label TEXT.
#[derive(Clone, Copy, PartialEq)]
enum TabPip {
    Fill,
    Text,
}

/// Whether the tab-config wheel writes THIS tab's override or its GROUP's colour.
#[derive(Clone, Copy, PartialEq)]
enum TabScope {
    ThisTab,
    Group,
}

/// The agent-wall filter (the chip strip above the MCP pane list): show every
/// pane, just one tab group, or only the ungrouped panes. Transient view state.
#[derive(Clone, Copy, PartialEq)]
enum McpFilter {
    All,
    Group(u32),
    Ungrouped,
}

/// One agent's slice of the savings rollup (from the leanctx-savings plugin).
#[derive(Clone, Default)]
struct SavingsAgent {
    id: String,
    kind: String,
    calls: u64,
    saved_est: u64,
    last_seen: String,
}

/// One hot file's compression slice.
#[derive(Clone, Default)]
struct SavingsFile {
    path: String,
    saved: u64,
    pct: f32,
}

/// Parsed result of the `</> savings` action: lean-ctx's precomputed token-savings
/// rollup. The global trio is exact; per-agent `saved_est` is an estimate (≈) until
/// lean-ctx stamps the agent id into its savings ledger.
#[derive(Clone, Default)]
struct SavingsView {
    tokens_saved: u64,
    gain_pct: f32,
    usd: f64,
    score: u32,
    level: String,
    agent_count: usize,
    agents: Vec<SavingsAgent>,
    top_files: Vec<SavingsFile>,
    note: String,
}

impl SavingsView {
    /// Parse the plugin's compact JSON payload. Returns `None` if it isn't the
    /// shape we expect (the caller shows the raw text as a status line instead).
    fn from_json(s: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(s.trim()).ok()?;
        let u = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
        let f = |k: &str| v.get(k).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
        let s_ = |k: &str| {
            v.get(k)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let agents = v
            .get("agents")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| SavingsAgent {
                        id: x
                            .get("id")
                            .and_then(|y| y.as_str())
                            .unwrap_or("")
                            .to_string(),
                        kind: x
                            .get("type")
                            .and_then(|y| y.as_str())
                            .unwrap_or("")
                            .to_string(),
                        calls: x.get("calls").and_then(|y| y.as_u64()).unwrap_or(0),
                        saved_est: x.get("saved_est").and_then(|y| y.as_u64()).unwrap_or(0),
                        last_seen: x
                            .get("last_seen")
                            .and_then(|y| y.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let top_files = v
            .get("top_files")
            .and_then(serde_json::Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| SavingsFile {
                        path: x
                            .get("path")
                            .and_then(|y| y.as_str())
                            .unwrap_or("")
                            .to_string(),
                        saved: x.get("saved").and_then(|y| y.as_u64()).unwrap_or(0),
                        pct: x.get("pct").and_then(|y| y.as_f64()).unwrap_or(0.0) as f32,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(SavingsView {
            tokens_saved: u("tokens_saved"),
            gain_pct: f("gain_pct") as f32,
            usd: f("usd"),
            score: u("score") as u32,
            level: s_("level"),
            agent_count: u("agent_count") as usize,
            agents,
            top_files,
            note: s_("note"),
        })
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
    /// Tab groups (browser-style colour bands). Absent on pre-feature files.
    #[serde(default)]
    groups: Vec<SavedGroup>,
    /// Read-only MCP control-surface policy (the mother-bar robot panel). Absent
    /// on pre-feature files → the locked-down [`mcp::McpConfig`] default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mcp: Option<mcp::McpConfig>,
    /// Global FOCUS-reader preference: inherit the read pane's CRT look (barrel
    /// curvature + glare) instead of the flat default. Absent on pre-feature
    /// files → `false` (the flat reader everyone had before).
    #[serde(default)]
    focus_inherit: bool,
    /// Chrome language for the UI (the language pack). Absent on old files →
    /// English; keycaps and symbols are never translated.
    #[serde(default)]
    lang: lang::Lang,
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
            groups: Vec::new(),
            mcp: None,
            focus_inherit: false,
            lang: lang::Lang::default(),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SavedTab {
    #[serde(default)]
    name: Option<String>,
    /// Per-tab "binder divider" FILL colour as a hex string (e.g. `#3a8f4d`).
    /// Absent on pre-feature state files → the tab inherits group/bezel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    /// Per-tab label TEXT-colour override as a hex string; absent = inherit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text_color: Option<String>,
    /// The group id this tab belongs to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    group: Option<u32>,
    node: SavedNode,
}

/// A persisted tab group. Colours are hex strings; `id` ties tabs to groups.
#[derive(Serialize, Deserialize)]
struct SavedGroup {
    id: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text_color: Option<String>,
    #[serde(default)]
    collapsed: bool,
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

/// Count the terminal leaves (panes) under a saved node — the "richness" metric
/// the regression guard compares before letting a save overwrite the session.
fn count_saved_leaves(n: &SavedNode) -> usize {
    match n {
        SavedNode::Leaf { .. } => 1,
        SavedNode::Split { a, b, .. } => count_saved_leaves(a) + count_saved_leaves(b),
    }
}

/// Decide whether `new` (panes,tabs) is a CATASTROPHIC shrink versus what is
/// already on disk — losing more than half the panes or tabs of a non-trivial
/// session. A deliberate close (`allow_shrink`) is always fine; this only fires
/// for an *unintended* collapse (a checkpoint or dead-agent reap writing a tree
/// that mysteriously lost most of its panes). Pure, so it is unit-testable.
fn is_catastrophic_shrink(
    old_leaves: usize,
    old_tabs: usize,
    new_leaves: usize,
    new_tabs: usize,
    allow_shrink: bool,
) -> bool {
    if allow_shrink {
        return false;
    }
    (old_leaves >= 4 && new_leaves * 2 < old_leaves) || (old_tabs >= 3 && new_tabs * 2 < old_tabs)
}

/// Keep the newest ~10 timestamped snapshots of `state.toml` under a `backups/`
/// sibling dir, copying the CURRENT good file in before it is overwritten. Makes
/// any bad save recoverable with a single `cp` — defense in depth behind the
/// richness guard.
fn rotate_state_backup(path: &std::path::Path) {
    let Some(dir) = path.parent() else { return };
    if !path.exists() {
        return;
    }
    let backups = dir.join("backups");
    if fs::create_dir_all(&backups).is_err() {
        return;
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let _ = fs::copy(path, backups.join(format!("state-{stamp:020}.toml")));
    // prune to the newest 10 (names sort chronologically by the zero-padded stamp)
    if let Ok(entries) = fs::read_dir(&backups) {
        let mut files: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("state-") && s.ends_with(".toml"))
            })
            .collect();
        files.sort();
        const KEEP: usize = 10;
        if files.len() > KEEP {
            for f in &files[..files.len() - KEEP] {
                let _ = fs::remove_file(f);
            }
        }
    }
}

/// The single primary-state write chokepoint. Applies the richness-regression
/// guard (refusing an unintended catastrophic shrink, preserving the on-disk
/// session and snapshotting it as `state.toml.last-good`) and rotates a backup
/// before every accepted write. `new_leaves`/`new_tabs` describe the tree being
/// written; `allow_shrink` is true only for an explicit user close.
fn persist_primary_state(body: &str, new_leaves: usize, new_tabs: usize, allow_shrink: bool) {
    let path = state_path();
    let disk = load_state();
    let old_leaves: usize = disk.tabs.iter().map(|t| count_saved_leaves(&t.node)).sum();
    let old_tabs = disk.tabs.len();
    if is_catastrophic_shrink(old_leaves, old_tabs, new_leaves, new_tabs, allow_shrink) {
        // Keep the rich on-disk session; stash a copy as last-good for recovery.
        let _ = fs::copy(&path, path.with_file_name("state.toml.last-good"));
        eprintln!(
            "terminal-delight: REFUSED a session shrink ({old_leaves}->{new_leaves} panes, \
             {old_tabs}->{new_tabs} tabs) — kept on-disk session + wrote state.toml.last-good"
        );
        return;
    }
    rotate_state_backup(&path);
    let _ = session::write_atomic(&path, body);
}

/// The layout for a demo window: the throwaway state file named by
/// `TD_DEMO_STATE` (written by [`Workspace::share_demo`]), NOT the real session.
fn load_demo_state() -> StateFile {
    std::env::var_os("TD_DEMO_STATE")
        .and_then(|p| fs::read_to_string(p).ok())
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

/// A whole tab group being dragged by its handle to reorder it (all its members
/// move together, staying contiguous) among the strip's tabs and other groups.
struct GroupDrag {
    /// The group id grabbed when the drag began.
    gid: u32,
    /// Where the grab started (window space) — engages past a small threshold.
    start: Point<Pixels>,
    /// Latest cursor position.
    at: Point<Pixels>,
    /// True once the cursor moved far enough to be a drag, not a stray click.
    engaged: bool,
}

/// What the find panel is searching, and where it centres.
enum FindScope {
    /// Ctrl+F: just this pane; the panel centres over the pane's on-screen box.
    InPane(EntityId),
    /// Ctrl+Shift+F: every pane across every tab; the panel centres on the window.
    Global,
}

/// One row in the find panel: a matched line in a pane, carrying everything to
/// render it (mode label, title, snippet + match highlight) and to jump to it
/// (the pane handle + its tab, the grid line, and the matched column span).
struct FindHit {
    pane: gpui::WeakEntity<TerminalView>,
    pane_id: EntityId,
    tab: usize,
    mode: String,
    is_agent: bool,
    title: String,
    line: i32,
    snippet: String,
    /// Char indices into `snippet` that matched (also column indices).
    positions: Vec<usize>,
    score: i64,
    /// Global scope only: how many lines matched in this pane (the row shows one).
    same_pane_count: usize,
}

/// Live state of the open find panel: the query box, the computed hits, and the
/// keyboard selection. `None` when the panel is closed.
struct FindState {
    scope: FindScope,
    query: EditBuffer,
    results: Vec<FindHit>,
    selected: usize,
}

/// The language dropdown (click the 🌐 pill in the help header). A fuzzy-search box
/// over the 9 languages — owns the keyboard while up, like the find panel: typing
/// filters, ↑/↓ move the selection, ↵ applies, esc closes. Replaces the old
/// click-to-cycle pill (painful past a few languages).
struct LangPicker {
    query: EditBuffer,
    selected: usize,
}

/// The FOCUS reader's wrapped layout, captured each render so the mouse handlers
/// can map a screen click back to a source cell (for selection + copy) through the
/// exact frame the user is looking at. All metrics are logical px, matching the
/// captured `focus_body_bounds`.
struct FocusMap {
    /// One entry per on-screen visual row, in paint order: `(src_row, src_col0,
    /// glyph_cols)`. `src_col0` is where this wrapped row begins in its source row.
    rows: Vec<(usize, usize, usize)>,
    /// Source grid-row texts, for assembling the copied selection (one logical line
    /// per source row, regardless of how many visual rows it wrapped into).
    src_lines: Vec<String>,
    line_h: f32,
    glyph_w: f32,
    /// Left inset of the content inside the clip box.
    pad: f32,
    /// Top of the first visual row inside the clip box (`pad + v_offset −
    /// scroll_y`), so a click's y resolves to a visual row.
    top: f32,
    k1: f32,
    k2: f32,
    /// Whether the reader is curved this frame (so the hit-test applies the warp).
    inherit: bool,
}

struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    focus_handle: gpui::FocusHandle,
    /// The find panel (Ctrl+F / Ctrl+Shift+F), if open. Owns the keyboard while
    /// up, like the rename editors — typing edits the query, ↑/↓ move the
    /// selection, ↵ jumps to the hit, esc closes.
    find: Option<FindState>,
    /// The language dropdown (🌐 in the help header), if open. Owns the keyboard
    /// while up, like `find`.
    lang_picker: Option<LangPicker>,
    renaming: Option<(usize, EditBuffer)>,
    /// Tab index awaiting a "close all its panes?" confirmation, if any.
    confirm_close: Option<usize>,
    /// The ? help modal is open (keys/commands reference), themed by the outer.
    help_open: bool,
    /// Help modal view: false = keyboard shortcuts, true = the full feature list.
    help_features: bool,
    /// Active chrome language (the language pack); persisted in state.toml.
    lang: lang::Lang,
    /// Open theme breakout menu, if any.
    theme_menu: Option<MenuScope>,
    /// Window-space point to anchor the open tray at (a sub-tab icon click).
    /// None = the fixed top-right anchor used by the global/outer menu.
    menu_at: Option<Point<Pixels>>,
    /// Open monitor-OSD (display) tray, if any — same scope model as `theme_menu`.
    osd_menu: Option<MenuScope>,
    /// Window-space anchor for the open OSD tray (a pane display-icon click).
    osd_at: Option<Point<Pixels>>,
    /// Read-only MCP control-surface policy (persisted). The 🤖 mother-bar
    /// button edits this; the live snapshot it would expose is derived per-frame.
    mcp: mcp::McpConfig,
    /// The 🤖 MCP control panel is open. Outer-only (global), so a plain bool.
    mcp_menu: bool,
    /// The 👻 dead-agent recover manifest overlay is open.
    dead_menu: bool,
    /// Manifest filter: show every dead agent, or just one project. Transient.
    dead_filter: Option<String>,
    /// The 🧩 plugins panel overlay is open (MCP plugin host — see [`plugins`]).
    plugins_menu: bool,
    /// Last plugin action result line (e.g. "wrote …/<id>.cdx"), shown as a
    /// transient toast in the graveyard / plugins panel.
    harvest_status: Option<String>,
    /// The </> LeanCTX token-savings overlay is open (leanctx-savings plugin).
    savings_menu: bool,
    /// Parsed savings rollup (the number + per-agent/file breakdown), or `None`
    /// before the first fetch.
    savings_view: Option<SavingsView>,
    /// Why the savings fetch couldn't run (plugin missing / error), shown in the
    /// overlay instead of the card.
    savings_status: Option<String>,
    /// 🎨 toggle in the MCP panel: tint each pane row with that pane's own
    /// resolved screen background + text colour. Defaults off (session-scoped).
    mcp_theme_preview: bool,
    /// Agent-wall view filter (the chip strip) — transient, not persisted.
    mcp_filter: McpFilter,
    /// Agent-wall state filter. Combines with [`Self::mcp_filter`] so a group
    /// chip plus "blocked" means blocked agents in that group only.
    mcp_state_filter: Option<hud::AgentState>,
    /// Agent-wall program/mode filter, generated from the live pane modes.
    /// Combines with group and state filters.
    mcp_program_filter: Option<String>,
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
    /// A whole group being dragged by its handle to reorder it, if any.
    group_drag: Option<GroupDrag>,
    /// The insertion slot (0..=len) a tab-reorder release would land in.
    tab_drop: Option<usize>,
    /// `true` when the resolved tab-drop slot is a fresh row below the last row
    /// (the cursor was dragged past the bottom of the strip) — drives the wide
    /// "drop onto a new row" bar instead of the thin between-tabs caret.
    tab_drop_newrow: bool,
    /// Browser-style tab groups (colour bands). Members reference a group by id.
    groups: Vec<TabGroup>,
    /// Monotonic id source for new groups (never reused, so stale refs stay safe).
    next_group_id: u32,
    /// Which tab's config pane is open, if any (right-click / ctrl+click a tab).
    tab_menu: Option<usize>,
    /// Window-space anchor for the open tab config pane.
    tab_menu_at: Option<Point<Pixels>>,
    /// Which group's own config menu is open (right-click its handle/pill), if
    /// any. While open, `tab_menu` points at a representative member so the
    /// shared colour wheel edits the GROUP (scope = Group) and the per-tab menu
    /// is suppressed — group properties are managed from the group, never from a
    /// member tab.
    group_menu: Option<u32>,
    /// Window-space anchor for the open group config menu.
    group_menu_at: Option<Point<Pixels>>,
    /// Whether the tab pane's wheel edits this tab's override or its group colour.
    tab_scope: TabScope,
    /// Which pip (Fill / Text) the tab pane's wheel + lightness slider drive.
    tab_pip: TabPip,
    /// The tab-pane wheel pip being dragged, if any.
    tab_wheel_drag: Option<TabPip>,
    /// Live tab-pane wheel rect, for polar hit-testing during a drag.
    tab_wheel_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// True while the tab-pane lightness slider is being dragged.
    tab_light_drag: bool,
    /// Live tab-pane lightness-slider rect, for ratio math during a drag.
    tab_light_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// Inline group-name editor: (group id, buffer) while renaming a group.
    group_rename: Option<(u32, EditBuffer)>,
    /// The pane currently mirrored in the FOCUS reading modal, if any. Weak so a
    /// closed pane (its × / shell exit) drops normally — the modal just vanishes.
    focus_read: Option<gpui::WeakEntity<TerminalView>>,
    /// User text-size multiplier for the FOCUS mirror, on top of the auto-fit
    /// scale. 1.0 = fit-to-modal; the header slider drives it `FZ_MIN..=FZ_MAX`.
    /// Non-destructive — it scales only the mirror, never the real terminal's
    /// grid. Reset to 1.0 whenever the modal (re)opens on a pane.
    focus_zoom: f32,
    /// True while the FOCUS text-size slider thumb is being dragged.
    focus_zoom_drag: bool,
    /// On-screen box of the FOCUS slider track (captured each frame), so a drag
    /// anywhere in the window maps the cursor-x back to a 0..1 track fraction.
    focus_zoom_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// Vertical pan offset (px) of the zoomed mirror inside the FOCUS panel. When
    /// the text is scaled up past the panel it overflows the bottom; the wheel
    /// pans this 0..=`focus_overflow` so you can reach the last row. Reset on open.
    focus_scroll_y: f32,
    /// How far (px) the scaled mirror overflows the panel's inner height this
    /// frame (0 = it fits). Refreshed each render; the scrim's wheel handler reads
    /// it to decide pan-the-modal vs. scroll-the-terminal.
    focus_overflow: f32,
    /// The mirror's scaled cell height (px), captured each render so a line-delta
    /// wheel event pans the modal by whole rows.
    focus_line_h: f32,
    /// On-screen box of the FOCUS reading area (the clip box below the header),
    /// captured each frame. This is the SAME rect registered as the warp tube, so a
    /// click normalises into it and applies the identical barrel map the shader
    /// gathers with — the curve-aware hit-test the live pane already uses.
    focus_body_bounds: Arc<Mutex<Option<Bounds<Pixels>>>>,
    /// The reader's wrapped layout this frame, so a click maps back to a real
    /// source cell for selection + copy. Refreshed every render.
    focus_map: Arc<Mutex<Option<FocusMap>>>,
    /// Click-drag text selection in the reader, as source `(row, col)` cells
    /// `(anchor, head)`. Stored in SOURCE coordinates so it survives a zoom/wrap
    /// reflow. `None` = nothing selected.
    focus_sel: Option<((usize, usize), (usize, usize))>,
    /// True while a left-drag is actively extending the reader selection.
    focus_sel_drag: bool,
    /// Global, persisted: when on, the FOCUS reader inherits the read pane's CRT
    /// look (barrel curvature + screen glare) instead of the flat default. One
    /// toggle in the modal header; applies to every reader open from then on.
    focus_inherit_theme: bool,
    /// A scratch window (opened while another instance is already running, or a
    /// torn-off pane): one fresh terminal, never restores or persists session
    /// state — so it can't clobber the primary window's saved layout.
    scratch: bool,
    /// Frameless drag latch: a mousedown on the mother bar arms it; the first
    /// mouse-move while armed hands off to the compositor's window-move (so a
    /// plain click on the bar doesn't get eaten). Cleared on mouse-up.
    should_move: bool,
    /// One-shot "this save is an EXPLICIT user shrink" flag (set by `close_tab`
    /// / `close_pane` just before `save`). It is the sole thing that lets the
    /// richness-regression guard accept a save that drops >50% of panes/tabs —
    /// so a deliberate close persists, but a checkpoint or dead-agent reap can
    /// never silently shrink the saved session. Consumed (reset) on read.
    permit_shrink: std::cell::Cell<bool>,
    /// Set when an auto-`reap` removed panes (a shell/agent exited). While true,
    /// the 30s checkpoint does NOT persist — so a startup where several resumed
    /// agents died can't bake its shrink into `state.toml` before you act (e.g.
    /// resurrect them via the 🪦 dead-agents tool). Cleared by any real user
    /// save (which supersedes the staleness with intent).
    degraded: std::cell::Cell<bool>,
}

fn make_pane(window: &mut Window, cx: &mut Context<Workspace>) -> Entity<TerminalView> {
    // A brand-new terminal with no restore context. The pinned house DESIGN
    // appearance is applied by make_pane_restored, so this is a thin alias.
    make_pane_restored(session::PaneRestore::default(), window, cx)
}

fn make_pane_restored(
    restore: session::PaneRestore,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Entity<TerminalView> {
    let pane = cx.new(|cx| TerminalView::new_restored(restore, cx));
    // Every freshly-built pane ships as the pinned house DESIGN screen (Wood ·
    // hacker · agentic · theme) — it does NOT follow the warm outer cabinet.
    // Centralising the default HERE (not just
    // in make_pane) is what kills the orange-overglow CLASS: split, tear-off and
    // any future creation site inherit the right look for free. The restore path
    // (build_node) is the sole exception and re-applies the pane's SAVED
    // appearance right after — which may legitimately be pristine/follow-outer.
    pane.update(cx, |view, _| view.appearance = PaneTheme::house());
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
    // Ctrl+Shift+A in any pane opens the agent-watch (MCP) panel — same surface
    // the header robot icon toggles on.
    cx.subscribe(&pane, |ws, _pane, _ev: &OpenAgentPanel, cx| {
        ws.mcp_menu = true;
        cx.notify();
    })
    .detach();
    // Ctrl+F / Ctrl+Shift+F in a pane → open the find panel (this pane, or global)
    cx.subscribe_in(&pane, window, |ws, pane, ev: &OpenFind, window, cx| {
        ws.open_find(pane.entity_id(), ev.global, window, cx);
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
    // Ctrl+W in a pane → close the whole active tab, always via the confirm dialog
    cx.subscribe(&pane, |ws, _pane, _ev: &RequestCloseTab, cx| {
        ws.confirm_close_active_tab(cx);
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
            // Restore the pane's EXACT saved appearance, overriding the green
            // house default that make_pane_restored pins. A saved pristine
            // appearance means "follow the outer cabinet" and must win here —
            // hence this applies unconditionally, not just when non-pristine.
            let appearance = appearance.clone();
            let name = name.clone();
            pane.update(cx, |view, _| {
                view.appearance = appearance;
                if name.is_some() {
                    view.name = name;
                }
            });
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
        Self::build(false, false, None, window, cx)
    }

    /// A demo window: restores the cloned layout from `TD_DEMO_STATE` but never
    /// persists (treated as scratch for saving). Every pane runs the frozen
    /// lorem-ipsum emitter — see [`Self::share_demo`] and `term::spawn_in`.
    fn new_demo(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::build(false, true, None, window, cx)
    }

    /// A scratch window: one fresh terminal (optionally seeded with a cwd/agent
    /// session for a torn-off pane), no restore, no persistence. Opened when the
    /// hotkey fires while a primary is already running, or on a drag-out pop-out.
    fn new_scratch(
        seed: Option<session::PaneRestore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::build(true, false, seed, window, cx)
    }

    fn build(
        scratch: bool,
        demo: bool,
        seed: Option<session::PaneRestore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // A demo window restores its cloned layout from the throwaway demo state,
        // never the real session — and never writes back (scratch for saving).
        let saved = if demo {
            load_demo_state()
        } else {
            load_state()
        };
        // scale + theme are read even in scratch mode so a quick window still
        // looks like the rest of the session; only the *layout* is skipped.
        // Text size now lives in the outer grade (`grade.scale`); fold a legacy
        // top-level `scale` from older state files into it on load.
        let mut outer = saved.theme.clone().unwrap_or_else(theme::house_outer);
        if let Some(s) = saved.scale {
            outer.grade.scale = s.clamp(0.7, 1.6);
        }
        // Warp + tracking now ride the grade group (per-pane override + inherit);
        // fold a legacy top-level `warp`/`track` from older state files into the
        // outer grade so a saved fishbowl/roll survives the migration.
        outer.grade.warp = saved.warp.clamp(0.0, theme::WARP_MAX);
        outer.grade.tracking = saved.track;
        theme::select_outer(cx, outer);
        let mut ws = Self {
            tabs: vec![],
            active: 0,
            focus_handle: cx.focus_handle(),
            find: None,
            lang_picker: None,
            renaming: None,
            confirm_close: None,
            help_open: false,
            help_features: false,
            theme_menu: None,
            menu_at: None,
            osd_menu: None,
            osd_at: None,
            mcp: saved.mcp.clone().unwrap_or_default(),
            mcp_menu: false,
            dead_menu: false,
            dead_filter: None,
            plugins_menu: false,
            harvest_status: None,
            savings_menu: false,
            savings_view: None,
            savings_status: None,
            mcp_theme_preview: false,
            mcp_filter: McpFilter::All,
            mcp_state_filter: None,
            mcp_program_filter: None,
            slider_drag: None,
            slider_bounds: Arc::new(Mutex::new(std::collections::HashMap::new())),
            wheel_drag: None,
            wheel_active: WheelTarget::Seed,
            light_drag: false,
            light_bounds: Arc::new(Mutex::new(None)),
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
            group_drag: None,
            tab_drop: None,
            tab_drop_newrow: false,
            groups: Vec::new(),
            next_group_id: 1,
            tab_menu: None,
            tab_menu_at: None,
            group_menu: None,
            group_menu_at: None,
            tab_scope: TabScope::ThisTab,
            tab_pip: TabPip::Fill,
            tab_wheel_drag: None,
            tab_wheel_bounds: Arc::new(Mutex::new(None)),
            tab_light_drag: false,
            tab_light_bounds: Arc::new(Mutex::new(None)),
            group_rename: None,
            focus_read: None,
            focus_zoom: 1.0,
            focus_zoom_drag: false,
            focus_zoom_bounds: Arc::new(Mutex::new(None)),
            focus_scroll_y: 0.0,
            focus_overflow: 0.0,
            focus_line_h: 0.0,
            focus_body_bounds: Arc::new(Mutex::new(None)),
            focus_map: Arc::new(Mutex::new(None)),
            focus_sel: None,
            focus_sel_drag: false,
            focus_inherit_theme: saved.focus_inherit,
            lang: saved.lang,
            // a demo window restores a layout (so `scratch` is false to take the
            // restore branch below) yet must never overwrite the real state
            scratch: scratch || demo,
            should_move: false,
            permit_shrink: std::cell::Cell::new(false),
            degraded: std::cell::Cell::new(false),
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
            ws.groups = saved
                .groups
                .iter()
                .filter_map(|g| {
                    Some(TabGroup {
                        id: g.id,
                        name: g.name.clone(),
                        color: theme::parse_hex(&g.color)?,
                        text_color: g.text_color.as_deref().and_then(theme::parse_hex),
                        collapsed: g.collapsed,
                    })
                })
                .collect();
            ws.next_group_id = ws.groups.iter().map(|g| g.id + 1).max().unwrap_or(1);
            let live: std::collections::HashSet<u32> = ws.groups.iter().map(|g| g.id).collect();
            for t in &saved.tabs {
                let root = build_node(&t.node, window, cx);
                let mut tab = Tab::new(root, t.name.clone());
                tab.color = t.color.as_deref().and_then(theme::parse_hex);
                tab.text_color = t.text_color.as_deref().and_then(theme::parse_hex);
                // drop a dangling group ref (a group that failed to parse / vanished)
                tab.group = t.group.filter(|g| live.contains(g));
                ws.tabs.push(tab);
            }
            ws.prune_groups();
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
        // bounds) changes without structural events, so re-snapshot every 30s —
        // a crash loses at most that much recency, never the layout. (Clean quit
        // via ✕ saves immediately, so this only covers crashes / WM kills.)
        // Scratch windows never persist, so they skip the checkpoint entirely.
        if !scratch {
            cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_secs(30))
                    .await;
                if this
                    .update(cx, |ws: &mut Workspace, cx| {
                        // D: never let the timer bake in a reap-shrunk tree —
                        // wait for the user to act (a real save clears `degraded`).
                        if !ws.degraded.get() {
                            ws.save(cx);
                        }
                    })
                    .is_err()
                {
                    break;
                }
            })
            .detach();
        }
        // Read-only MCP control surface: when an orchestrator launches us with
        // TD_MCP set (stdio piped), speak JSON-RPC on stdin/stdout. `build` runs
        // once for this process's first window, and `start` is a process-wide
        // singleton (an atomic guard), so the server attaches exactly once —
        // whichever window mode the orchestrator chose to launch.
        if std::env::var_os("TD_MCP").is_some() {
            // A SECOND, explicit opt-in promotes the read-only watch surface to a
            // remote-control one (an agent may change pane appearance via
            // set_pane_config). The robot panel persists this in state; the env
            // var forces it on for a headless / orchestrated launch.
            if std::env::var_os("TD_MCP_WRITE").is_some() {
                ws.mcp.writable = true;
            }
            mcp_transport::start(cx);
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

    /// Serialise the live workspace into the persistable [`StateFile`] — the tab/
    /// split tree, per-pane appearance, groups, theme, and MCP policy. Shared by
    /// [`Self::save`] (writes the real state) and [`Self::share_demo`] (clones the
    /// layout into a throwaway demo state), so the two can never drift.
    fn build_state(&self, cx: &App) -> StateFile {
        StateFile {
            active: self.active,
            win: self.last_win,
            // Kept for backward-compat with readers of the old top-level field;
            // the source of truth is now `theme.grade.scale`.
            scale: Some(theme::outer_choice(cx).grade.scale),
            theme: Some(theme::outer_choice(cx)),
            // Back-compat mirror: warp + tracking live in the outer grade now; keep
            // writing the legacy top-level fields so older readers still work.
            warp: theme::outer_choice(cx).grade.warp,
            track: theme::outer_choice(cx).grade.tracking,
            tabs: self
                .tabs
                .iter()
                .map(|t| SavedTab {
                    name: t.name.clone(),
                    color: t.color.map(hsla_to_hex),
                    text_color: t.text_color.map(hsla_to_hex),
                    group: t.group,
                    node: t.root.to_saved(cx),
                })
                .collect(),
            groups: self
                .groups
                .iter()
                .map(|g| SavedGroup {
                    id: g.id,
                    name: g.name.clone(),
                    color: hsla_to_hex(g.color),
                    text_color: g.text_color.map(hsla_to_hex),
                    collapsed: g.collapsed,
                })
                .collect(),
            mcp: Some(self.mcp.clone()),
            focus_inherit: self.focus_inherit_theme,
            lang: self.lang,
        }
    }

    fn save(&self, cx: &App) {
        // a scratch / torn-off window must never overwrite the primary's layout
        if self.scratch {
            return;
        }
        // A real (user-driven) save supersedes any reap staleness: the user has
        // engaged with the current tree, so it becomes the new truth.
        self.degraded.set(false);
        // EXPLICIT user shrink? (set by close_tab / close_pane). Consume it.
        let allow_shrink = self.permit_shrink.replace(false);
        if let Ok(body) = toml::to_string(&self.build_state(cx)) {
            persist_primary_state(&body, self.pane_count(), self.tabs.len(), allow_shrink);
        }
    }

    /// "Share a demo of this layout": clone the CURRENT live layout + appearance
    /// into a throwaway state file and launch a detached window from it with
    /// TD_DEMO set, so every pane runs the frozen lorem-ipsum emitter instead of
    /// a real shell. A faithful, safe-to-screen-share twin of this wall. The
    /// child loads the demo state (never the real one) and never persists.
    fn share_demo(&self, cx: &App) {
        let state = self.build_state(cx);
        let Ok(body) = toml::to_string(&state) else {
            return;
        };
        // a unique throwaway path (pid + nanos) so concurrent demos don't collide
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path =
            std::env::temp_dir().join(format!("td-demo-{}-{stamp}.toml", std::process::id()));
        if session::write_atomic(&path, &body).is_err() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut cmd = std::process::Command::new(exe);
        cmd.env("TD_DEMO", "1")
            .env("TD_DEMO_STATE", &path)
            // belt-and-suspenders: also a peerless launch must boot non-restoring
            .env_remove("TD_SCRATCH")
            .env_remove("TD_SEED_CWD")
            .env_remove("TD_SEED_RESUME");
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let _ = cmd.spawn();
    }

    /// Build the read-only snapshot the MCP control surface would expose: every
    /// pane across every tab, with its kernel-derived identity (mode, pid, cwd)
    /// and — for an agent — its resumable session (the durable key a watch rule
    /// binds to, and the pointer to its on-disk tool-call transcript). The
    /// `exposed` flag applies the current policy. Strictly read-only: this never
    /// writes a byte to any PTY.
    fn mcp_snapshot(&self, cx: &App) -> Vec<mcp::PaneInfo> {
        let mut out = vec![];
        let outer = theme::outer_choice(cx);
        for (ti, tab) in self.tabs.iter().enumerate() {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for leaf in leaves {
                let p = leaf.read(cx);
                let rt = p.runtime();
                let is_agent = p.mode.is_agent();
                let title = p
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| (!p.title.is_empty()).then(|| p.title.clone()))
                    .unwrap_or_else(|| p.mode.label().to_string());
                out.push(mcp::PaneInfo {
                    tab: ti,
                    title,
                    mode: p.mode.label().to_string(),
                    is_agent,
                    pid: p.shell_pid(),
                    cwd: rt.cwd,
                    session: rt.resume,
                    exposed: mcp::should_expose(&self.mcp, is_agent),
                    // the look the pane actually renders with (own override else
                    // inherited outer), in the config API's 0..100 percents
                    grade: Self::grade_report(&p.appearance.effective(&outer).grade),
                });
            }
        }
        out
    }

    /// The window-level outer grade, for `get_pane_config`'s `outer` target.
    fn mcp_outer_grade(&self, cx: &App) -> mcp::GradeReport {
        Self::grade_report(&theme::outer_choice(cx).grade)
    }

    /// Search every EXPOSED pane's recent scrollback for `needle` (exact,
    /// case-insensitive) — the main-thread half of the MCP `grep` tool. Mirrors
    /// `mcp_snapshot`'s walk + policy filter, so the operator's expose toggle
    /// governs on-screen-content disclosure exactly as it gates `list_panes`.
    fn mcp_search(&self, needle: &str, cap: usize, cx: &App) -> Vec<mcp::PaneMatches> {
        let mut out = vec![];
        for (ti, tab) in self.tabs.iter().enumerate() {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for leaf in leaves {
                let p = leaf.read(cx);
                if !mcp::should_expose(&self.mcp, p.mode.is_agent()) {
                    continue;
                }
                let hits = p.grep_grid(needle, cap);
                if hits.is_empty() {
                    continue;
                }
                let title = p
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| (!p.title.is_empty()).then(|| p.title.clone()))
                    .unwrap_or_else(|| p.mode.label().to_string());
                out.push(mcp::PaneMatches {
                    pid: p.shell_pid(),
                    tab: ti,
                    title,
                    mode: p.mode.label().to_string(),
                    matches: hits
                        .into_iter()
                        .map(|h| mcp::GrepMatch {
                            line: h.line,
                            col: h.positions.first().copied().unwrap_or(0),
                            text: h.text,
                        })
                        .collect(),
                });
            }
        }
        out
    }

    /// Bridge a stored [`theme::Grade`] into the config API's uniform `0..100`
    /// percents — the single seam between the appearance model and `mcp`.
    fn grade_report(g: &theme::Grade) -> mcp::GradeReport {
        use theme::GradeKey as K;
        mcp::GradeReport {
            brightness: K::Brightness.to_percent(g.brightness),
            contrast: K::Contrast.to_percent(g.contrast),
            colour: K::Colour.to_percent(g.colour),
            text: K::Text.to_percent(g.text),
            background: K::Background.to_percent(g.background),
            gamma: K::Gamma.to_percent(g.gamma),
            menu_bar: K::Scale.to_percent(g.scale),
            text_size: K::TextSize.to_percent(g.text_size),
            warp: K::Warp.to_percent(g.warp),
            crawl: g.crawl,
            crawl_angle: K::CrawlAngle.to_percent(g.crawl_angle),
            crawl_depth: K::CrawlDepth.to_percent(g.crawl_depth),
        }
    }

    /// Apply a partial [`mcp::ConfigPatch`] (0..100 percents) onto a stored grade
    /// in place. PATCH semantics: an absent field is left unchanged; each present
    /// channel goes through [`theme::Grade::set`], which clamps into its range.
    /// The API is "dumb" — this stores the absolute number given; it never
    /// interprets a relative ask. The agent does that math from a prior read.
    fn apply_config_patch(g: &mut theme::Grade, patch: &mcp::ConfigPatch) {
        use theme::GradeKey as K;
        if let Some(p) = patch.brightness {
            g.set(K::Brightness, K::Brightness.from_percent(p));
        }
        if let Some(p) = patch.contrast {
            g.set(K::Contrast, K::Contrast.from_percent(p));
        }
        if let Some(p) = patch.colour {
            g.set(K::Colour, K::Colour.from_percent(p));
        }
        if let Some(p) = patch.text {
            g.set(K::Text, K::Text.from_percent(p));
        }
        if let Some(p) = patch.background {
            g.set(K::Background, K::Background.from_percent(p));
        }
        if let Some(p) = patch.gamma {
            g.set(K::Gamma, K::Gamma.from_percent(p));
        }
        if let Some(p) = patch.menu_bar {
            g.set(K::Scale, K::Scale.from_percent(p));
        }
        if let Some(p) = patch.text_size {
            g.set(K::TextSize, K::TextSize.from_percent(p));
        }
        if let Some(p) = patch.warp {
            g.set(K::Warp, K::Warp.from_percent(p));
        }
        if let Some(p) = patch.crawl_angle {
            g.set(K::CrawlAngle, K::CrawlAngle.from_percent(p));
        }
        if let Some(p) = patch.crawl_depth {
            g.set(K::CrawlDepth, K::CrawlDepth.from_percent(p));
        }
        if let Some(c) = patch.crawl {
            g.crawl = c;
        }
    }

    /// Apply a parsed `set_pane_config` batch on the gpui main thread (called by
    /// the transport's ticker). Each update pins the targeted pane's grade group
    /// (an OSD-equivalent edit, so it persists and repaints identically) or sets
    /// the outer grade; the per-target outcome reports the resulting effective
    /// grade or why it was refused. One bad pid never sinks the rest, and the
    /// state is saved once iff anything changed.
    fn apply_mcp_config(
        &mut self,
        updates: &[mcp::ConfigUpdate],
        cx: &mut Context<Self>,
    ) -> Vec<mcp::ApplyOutcome> {
        let mut out = Vec::with_capacity(updates.len());
        let mut changed = false;
        for (target, patch) in updates {
            match target {
                mcp::Target::Outer => {
                    let mut choice = theme::outer_choice(cx);
                    Self::apply_config_patch(&mut choice.grade, patch);
                    let report = Self::grade_report(&choice.grade);
                    theme::select_outer(cx, choice);
                    changed = true;
                    out.push((target.clone(), Ok(report)));
                }
                mcp::Target::Pane(pid) => {
                    // locate an EXPOSED leaf with this shell pid (collect the
                    // handle first so the &self.tabs borrow ends before we mutate)
                    let mut hit = None;
                    'find: for tab in &self.tabs {
                        let mut leaves = vec![];
                        tab.root.leaves(&mut leaves);
                        for leaf in leaves {
                            let p = leaf.read(cx);
                            if p.shell_pid() == *pid {
                                let exposed = mcp::should_expose(&self.mcp, p.mode.is_agent());
                                hit = Some((leaf.clone(), exposed));
                                break 'find;
                            }
                        }
                    }
                    match hit {
                        Some((leaf, true)) => {
                            let outer = theme::outer_choice(cx);
                            let report = leaf.update(cx, |view, cx| {
                                let mut g = view.appearance.effective(&outer).grade;
                                Self::apply_config_patch(&mut g, patch);
                                view.appearance.set_grade(g);
                                cx.notify();
                                Self::grade_report(&g)
                            });
                            changed = true;
                            out.push((target.clone(), Ok(report)));
                        }
                        Some((_, false)) => out.push((
                            target.clone(),
                            Err(format!(
                                "pane {pid} is not exposed under the current policy"
                            )),
                        )),
                        None => out.push((target.clone(), Err(format!("no pane with pid {pid}")))),
                    }
                }
            }
        }
        if changed {
            self.save(cx);
            cx.notify();
        }
        out
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
        self.save(cx);
        cx.notify();
        // Defer the focus: new_tab fires from a mother-bar mouse-down listener, so
        // the root container's tracked focus handle would grab focus back as the
        // event bubbles (same race as activate_tab/split). A synchronous
        // focus_active here never sticks — the new terminal opens unfocused. Running
        // after the event settles makes the fresh pane light up as the active
        // terminal so the very next keystroke lands in it.
        cx.defer_in(window, |ws, window, cx| ws.focus_active(window, cx));
    }

    /// Bring a dead agent back: open a fresh tab whose shell resumes its saved
    /// conversation (`claude --resume` / `codex resume`) in its original cwd —
    /// the same restore path a reboot uses. Never writes to a live PTY.
    fn resurrect_agent(
        &mut self,
        cwd: Option<String>,
        resume: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let restore = session::PaneRestore {
            cwd,
            resume: Some(resume),
        };
        let pane = make_pane_restored(restore, window, cx);
        self.tabs.push(Tab::new(Node::Leaf(pane), None));
        self.active = self.tabs.len() - 1;
        self.dead_menu = false;
        self.save(cx);
        cx.notify();
        cx.defer_in(window, |ws, window, cx| ws.focus_active(window, cx));
    }

    /// Harvest one agent session (live pane or 🪦 tombstone) into a portable
    /// `.cdx` via the context-delight plugin, over MCP. Sets [`Self::harvest_status`]
    /// with the result (or the reason it couldn't run) for the panel to show.
    ///
    /// The whole thing is bounded by the plugin client's per-request deadline, so
    /// a wedged plugin can never freeze the UI — it just reports a timeout.
    fn harvest_agent(&mut self, session_id: String, cx: &mut Context<Self>) {
        let home = session::home_dir();
        let plugins = plugins::discover(&home);
        let Some(cdx) = plugins.iter().find(|m| m.name == "context-delight") else {
            self.harvest_status = Some("context-delight plugin not found (install cdx-mcp)".into());
            cx.notify();
            return;
        };
        let dir = plugins::harvest_dir(&home);
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.harvest_status = Some(format!("cannot create {}: {e}", dir.display()));
            cx.notify();
            return;
        }
        let out = dir.join(format!("{session_id}.cdx"));
        self.harvest_status = match plugins::harvest(cdx, &session_id, &out) {
            Ok(msg) => Some(msg),
            Err(e) => Some(format!("harvest failed: {e}")),
        };
        cx.notify();
    }

    /// Fetch lean-ctx's token-savings rollup via the leanctx-savings plugin and
    /// open the </> overlay with it. `agent_id` focuses one agent (the per-agent
    /// button); `None` is the whole-fleet total. Synchronous but bounded by the
    /// plugin client's deadline — lean-ctx's `gain` returns in ~180ms — and the
    /// numbers are precomputed, so this never tokenizes anything itself.
    fn fetch_savings(&mut self, agent_id: Option<String>, cx: &mut Context<Self>) {
        let home = session::home_dir();
        let plugins = plugins::discover(&home);
        let Some(lc) = plugins.iter().find(|m| m.name == "leanctx-savings") else {
            self.savings_status =
                Some("leanctx-savings plugin not found (install leanctx-mcp)".into());
            self.savings_view = None;
            self.savings_menu = true;
            cx.notify();
            return;
        };
        match plugins::savings(lc, agent_id.as_deref()) {
            Ok(text) => match SavingsView::from_json(&text) {
                Some(v) => {
                    self.savings_view = Some(v);
                    self.savings_status = None;
                }
                None => {
                    self.savings_view = None;
                    self.savings_status = Some(text);
                }
            },
            Err(e) => {
                self.savings_view = None;
                self.savings_status = Some(format!("savings failed: {e}"));
            }
        }
        self.savings_menu = true;
        cx.notify();
    }

    /// The </> LeanCTX token-savings overlay: lean-ctx's precomputed savings as a
    /// big headline number plus the per-agent and top-file breakdown. A flat
    /// (warp-suppressed) float card — registered in `set_suppressed`/`close_popups`
    /// so it floats above the CRT glass instead of bowing with it.
    fn render_savings_overlay(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        if !self.savings_menu {
            return None;
        }
        // brand: theme-INDEPENDENT lean-ctx identity — white "Lean", green "CTX",
        // and a </> chip (white brackets, green slash) on a dark ground, so the
        // logo + colour read identically under every TD theme.
        let green = hsla(0.43, 0.68, 0.55, 1.);
        let white = hsla(0., 0., 0.96, 1.);
        let brand = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .px_1p5()
                    .py_0p5()
                    .rounded_sm()
                    .bg(hsla(0., 0., 0.06, 1.))
                    .border_1()
                    .border_color(green.alpha(0.5))
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_size(px(13.))
                    .child(div().text_color(white).child("\u{003c}"))
                    .child(div().text_color(green).child("/"))
                    .child(div().text_color(white).child("\u{003e}")),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_size(px(15.))
                    .text_color(white)
                    .child("Lean"),
            )
            .child(
                div()
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .text_size(px(15.))
                    .text_color(green)
                    .child("CTX"),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(white.alpha(0.5))
                    .child("token savings"),
            );

        let mut body = div().flex().flex_col().gap_2();

        if let Some(v) = &self.savings_view {
            // ---- headline trio: saved tokens · compression · USD ----
            let stat = |big: String, sub: &str, col: gpui::Hsla| {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_size(px(20.))
                            .text_color(col)
                            .child(big),
                    )
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(th.text.alpha(0.5))
                            .child(sub.to_string()),
                    )
            };
            body = body.child(
                div()
                    .flex()
                    .flex_row()
                    .items_end()
                    .gap_6()
                    .child(stat(hud::fmt_tokens(v.tokens_saved), "tokens saved", green))
                    .child(stat(
                        format!("{:.0}%", v.gain_pct),
                        "compression",
                        th.accent,
                    ))
                    .child(stat(format!("${:.0}", v.usd), "USD saved", th.complement))
                    .child(div().flex_1().min_w(px(0.)))
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_full()
                            .border_1()
                            .border_color(green.alpha(0.5))
                            .text_size(px(10.))
                            .text_color(green)
                            .child(format!("\u{2605} {}  \u{00b7}  {}", v.score, v.level)),
                    ),
            );
            // ---- "across N agents" + per-agent estimate list ----
            body = body.child(
                div()
                    .text_size(px(10.))
                    .text_color(th.text.alpha(0.75))
                    .child(format!(
                        "across {} agents (per-agent \u{2248} estimated):",
                        v.agent_count
                    )),
            );
            let mut alist = div().flex().flex_col().gap_1();
            for a in v.agents.iter().take(6) {
                let seen: String = a.last_seen.chars().take(16).collect();
                let seen = seen.replace('T', " ");
                alist = alist.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .border_1()
                        .border_color(th.text.alpha(0.10))
                        .text_size(px(9.5))
                        .child(
                            div()
                                .w(px(150.))
                                .flex_none()
                                .overflow_hidden()
                                .text_color(th.text)
                                .child(a.id.clone()),
                        )
                        .child(
                            div()
                                .w(px(110.))
                                .flex_none()
                                .text_color(th.text.alpha(0.5))
                                .child(a.kind.clone()),
                        )
                        .child(
                            div()
                                .w(px(64.))
                                .flex_none()
                                .text_color(th.text.alpha(0.55))
                                .child(format!("{} calls", a.calls)),
                        )
                        .child(div().flex_1().min_w(px(0.)))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::BOLD)
                                .text_color(green)
                                .child(format!("\u{2248} {}", hud::fmt_tokens(a.saved_est))),
                        )
                        .child(
                            div()
                                .w(px(110.))
                                .flex_none()
                                .text_size(px(8.))
                                .text_color(th.text.alpha(0.4))
                                .child(seen),
                        ),
                );
            }
            body = body.child(
                alist
                    .id("savings-agents")
                    .min_h(px(0.))
                    .max_h(px(180.))
                    .overflow_y_scroll(),
            );
            // ---- top files by compression ----
            if !v.top_files.is_empty() {
                let mut flist = div().flex().flex_col().gap_0p5().text_size(px(9.));
                for f in v.top_files.iter().take(5) {
                    let short: String = f
                        .path
                        .rsplit('/')
                        .next()
                        .unwrap_or(&f.path)
                        .chars()
                        .take(40)
                        .collect();
                    flist = flist.child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .overflow_hidden()
                                    .text_color(th.text.alpha(0.7))
                                    .child(short),
                            )
                            .child(
                                div()
                                    .text_color(green.alpha(0.85))
                                    .child(hud::fmt_tokens(f.saved)),
                            )
                            .child(
                                div()
                                    .w(px(46.))
                                    .flex_none()
                                    .text_color(th.text.alpha(0.45))
                                    .child(format!("{:.0}%", f.pct)),
                            ),
                    );
                }
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .text_size(px(9.5))
                                .text_color(th.text.alpha(0.55))
                                .child("top files by compression:"),
                        )
                        .child(flist),
                );
            }
            // ---- honesty caveat ----
            body = body.child(
                div()
                    .text_size(px(8.))
                    .text_color(th.text.alpha(0.45))
                    .child(v.note.clone()),
            );
        } else if let Some(err) = &self.savings_status {
            body = body.child(
                div()
                    .text_size(px(10.5))
                    .text_color(hsla(0., 0.7, 0.62, 1.))
                    .child(err.clone()),
            );
        }

        let panel = div()
            .w(px(420.))
            .max_h(px(500.))
            .overflow_hidden()
            .p_3()
            .rounded_md()
            .border_2()
            .border_color(th.accent.alpha(0.85))
            .bg(darken(th.surface, 0.62))
            .shadow(float_shadows(th.accent))
            .flex()
            .flex_col()
            .gap_3()
            .text_size(px(10.))
            .text_color(th.text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
            )
            .child(brand)
            .child(body)
            .child(
                div()
                    .text_size(px(8.5))
                    .text_color(th.accent.alpha(0.8))
                    .child("esc or click outside to close"),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(th.bg.alpha(0.70))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.savings_menu = false;
                        cx.notify();
                    }),
                )
                .child(panel),
        )
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

    /// Where a tab-reorder release at cursor `pos` lands, now that tabs can wrap
    /// onto several rows (a grid). Picks the ROW the cursor sits in, then the
    /// insertion slot within it by x-midpoint. Returns `(slot, new_row)` — the
    /// flag is set when the cursor was dragged below every row, i.e. "start a
    /// fresh row" (the wide drop bar). Reads the same live `tab_bounds` rects the
    /// pane-drop uses.
    fn resolve_tab_slot(&self, pos: Point<Pixels>) -> (usize, bool) {
        let map = self.tab_bounds.lock().unwrap();
        let (x, y) = (f32::from(pos.x), f32::from(pos.y));
        let n = self.tabs.len();
        // (index, top, bottom, mid-x) for every measured tab button
        let mut items: Vec<(usize, f32, f32, f32)> = Vec::new();
        for i in 0..n {
            if let Some(r) = map.get(&i) {
                let top = f32::from(r.origin.y);
                let bot = top + f32::from(r.size.height);
                let midx = f32::from(r.origin.x) + f32::from(r.size.width) / 2.0;
                items.push((i, top, bot, midx));
            }
        }
        if items.is_empty() {
            return (0, false);
        }
        let min_top = items.iter().fold(f32::INFINITY, |a, t| a.min(t.1));
        let max_bot = items.iter().fold(f32::NEG_INFINITY, |a, t| a.max(t.2));
        // dragged below every row → a brand-new row at the very end
        if y > max_bot + 2.0 {
            return (n, true);
        }
        // the row the cursor sits in; above-everything clamps to the first row,
        // a gap between rows snaps to the nearest row by vertical centre.
        let tol = 2.0;
        let mut row: Vec<&(usize, f32, f32, f32)> = if y < min_top {
            items
                .iter()
                .filter(|t| (t.1 - min_top).abs() < tol)
                .collect()
        } else {
            items
                .iter()
                .filter(|t| y >= t.1 - tol && y <= t.2 + tol)
                .collect()
        };
        if row.is_empty() {
            let nearest_top = items
                .iter()
                .min_by(|a, b| {
                    let da = ((a.1 + a.2) / 2.0 - y).abs();
                    let db = ((b.1 + b.2) / 2.0 - y).abs();
                    da.total_cmp(&db)
                })
                .map(|t| t.1)
                .unwrap_or(min_top);
            row = items
                .iter()
                .filter(|t| (t.1 - nearest_top).abs() < tol)
                .collect();
        }
        // within the chosen row, the x midpoints decide the insertion index
        let mut slot = row.iter().map(|t| t.0).min().unwrap_or(0);
        for t in &row {
            if x > t.3 {
                slot = t.0 + 1;
            }
        }
        (slot, false)
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

    /// Keyboard tab reorder (ctrl+shift+pgup / pgdn): slide the active tab one
    /// slot in `dir` (−1 left / +1 right) — but never across a group boundary. A
    /// grouped tab can't be shoved out of its group, nor an ungrouped tab pulled
    /// into one; only same-group (or both-ungrouped) neighbours swap. Mirrors the
    /// drag-reorder group clamp so both gestures behave alike.
    fn nudge_active_tab(&mut self, dir: i32, cx: &mut Context<Self>) {
        let cur = self.active;
        let n = self.tabs.len();
        let nb = match dir {
            d if d < 0 && cur > 0 => cur - 1,
            d if d > 0 && cur + 1 < n => cur + 1,
            _ => return,
        };
        // boundary clamp: the swap is allowed only when both tabs share the same
        // group membership (both `None`, or both the same group id).
        if self.tabs[cur].group != self.tabs[nb].group {
            return;
        }
        self.tabs.swap(cur, nb);
        self.active = nb;
        self.save(cx);
        cx.notify();
    }

    /// Slide a whole group to insertion slot `to` (pre-removal index space,
    /// 0..=len): every member moves together and ends up contiguous (this also
    /// heals a group that drag-reorder had split), preserving member order, with
    /// `active` still pointing at the very same tab it did before.
    fn move_group(&mut self, gid: u32, to: usize, cx: &mut Context<Self>) {
        let n = self.tabs.len();
        let to = to.min(n);
        let is_member: Vec<bool> = self.tabs.iter().map(|t| t.group == Some(gid)).collect();
        if !is_member.iter().any(|&b| b) {
            return;
        }
        // splice point translated into the member-free ("rest") index space
        let members_before_to = (0..to).filter(|&i| is_member[i]).count();
        let dest = to - members_before_to;
        let active_was = self.active;
        // pull tabs out, tagged with their original index, into rest + block
        let mut rest: Vec<(usize, Tab)> = Vec::with_capacity(n);
        let mut block: Vec<(usize, Tab)> = Vec::new();
        for (i, t) in self.tabs.drain(..).enumerate() {
            if is_member[i] {
                block.push((i, t));
            } else {
                rest.push((i, t));
            }
        }
        let dest = dest.min(rest.len());
        let mut rebuilt: Vec<(usize, Tab)> = Vec::with_capacity(n);
        rebuilt.extend(rest.drain(..dest));
        rebuilt.extend(block);
        rebuilt.append(&mut rest);
        // keep `active` on the same tab by matching its original index
        self.active = rebuilt
            .iter()
            .position(|(oi, _)| *oi == active_was)
            .unwrap_or_else(|| self.active.min(n.saturating_sub(1)));
        self.tabs = rebuilt.into_iter().map(|(_, t)| t).collect();
        self.save(cx);
        cx.notify();
    }

    // ---- tab groups + per-tab/group colour ------------------------------------

    /// The group tab `i` belongs to, if any (resolved by id).
    fn group_of(&self, i: usize) -> Option<&TabGroup> {
        let id = self.tabs.get(i)?.group?;
        self.groups.iter().find(|g| g.id == id)
    }

    /// Position of group `id` in `self.groups`, if it exists.
    fn group_index(&self, id: u32) -> Option<usize> {
        self.groups.iter().position(|g| g.id == id)
    }

    /// Mutable handle to tab `i`'s group (split borrow: read the id first).
    fn group_mut_of(&mut self, i: usize) -> Option<&mut TabGroup> {
        let id = self.tabs.get(i)?.group?;
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// The *resolved* (fill, text) colours for tab `i`'s button: the tab's own
    /// override wins, else the group leads, else `None` (plain bezel / bar text).
    fn resolved_tab_colors(&self, i: usize) -> (Option<Hsla>, Option<Hsla>) {
        let Some(tab) = self.tabs.get(i) else {
            return (None, None);
        };
        let g = self.group_of(i);
        let fill = tab.color.or_else(|| g.map(|gr| gr.color));
        let text = tab.text_color.or_else(|| g.and_then(|gr| gr.text_color));
        (fill, text)
    }

    /// The two colours the tab pane's wheel shows for tab `i` in the active
    /// scope. Unset values fall back to the bar's natural surface/text so a pip
    /// always sits somewhere grabbable.
    fn tab_scope_colors(&self, i: usize, cx: &App) -> (Hsla, Hsla) {
        let th = theme::theme(cx);
        match self.tab_scope {
            TabScope::Group => match self.group_of(i) {
                Some(g) => (g.color, g.text_color.unwrap_or(th.text)),
                None => (th.surface, th.text),
            },
            TabScope::ThisTab => {
                let (rf, rt) = self.resolved_tab_colors(i);
                (rf.unwrap_or(th.surface), rt.unwrap_or(th.text))
            }
        }
    }

    /// The active-scope colour of one pip (for lightness / drag math).
    fn tab_pip_color(&self, i: usize, pip: TabPip, cx: &App) -> Hsla {
        let (f, t) = self.tab_scope_colors(i, cx);
        match pip {
            TabPip::Fill => f,
            TabPip::Text => t,
        }
    }

    /// The two wheel markers (pip, glyph, colour) for tab `i` in the active scope.
    fn tab_pip_colors(&self, i: usize, cx: &App) -> [(TabPip, &'static str, Hsla); 2] {
        let (f, t) = self.tab_scope_colors(i, cx);
        [(TabPip::Fill, "▣", f), (TabPip::Text, "T", t)]
    }

    /// Write a pip colour into the active scope (this tab's override, or its
    /// group). `None` clears a tab override / a group's text lead; a group's fill
    /// is never cleared (a group always has a colour).
    fn tab_set_pip(&mut self, pip: TabPip, hex: Option<String>, cx: &mut Context<Self>) {
        let Some(i) = self.tab_menu else { return };
        let col = hex.as_deref().and_then(theme::parse_hex);
        match (self.tab_scope, pip) {
            (TabScope::ThisTab, TabPip::Fill) => {
                if let Some(t) = self.tabs.get_mut(i) {
                    t.color = col;
                }
            }
            (TabScope::ThisTab, TabPip::Text) => {
                if let Some(t) = self.tabs.get_mut(i) {
                    t.text_color = col;
                }
            }
            (TabScope::Group, TabPip::Fill) => {
                if let (Some(c), Some(g)) = (col, self.group_mut_of(i)) {
                    g.color = c;
                }
            }
            (TabScope::Group, TabPip::Text) => {
                if let Some(g) = self.group_mut_of(i) {
                    g.text_color = col;
                }
            }
        }
        self.save(cx);
        cx.notify();
    }

    /// Drop empty groups (no member tabs reference them). Run after any change
    /// that can orphan a group (close tab, remove-from-group, load).
    fn prune_groups(&mut self) {
        let live: std::collections::HashSet<u32> =
            self.tabs.iter().filter_map(|t| t.group).collect();
        self.groups.retain(|g| live.contains(&g.id));
    }

    /// Start a fresh single-member group from tab `i`, seeded with its current
    /// fill (or a default teal). Switches the pane's wheel to the Group scope.
    fn new_group_from(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(i) else { return };
        let color = tab.color.unwrap_or_else(|| hsla(0.47, 0.5, 0.5, 1.0));
        let id = self.next_group_id;
        self.next_group_id += 1;
        self.groups.push(TabGroup {
            id,
            name: None,
            color,
            text_color: None,
            collapsed: false,
        });
        if let Some(t) = self.tabs.get_mut(i) {
            t.group = Some(id);
        }
        self.tab_scope = TabScope::Group;
        self.save(cx);
        cx.notify();
    }

    /// Assign tab `i` to group `gid` and slide it adjacent to that group's run so
    /// the colour band stays continuous. Updates `tab_menu` to the tab's new
    /// index (the move reorders the strip).
    fn add_tab_to_group(&mut self, i: usize, gid: u32, cx: &mut Context<Self>) {
        if i >= self.tabs.len() || self.tabs[i].group == Some(gid) {
            return;
        }
        self.tabs[i].group = Some(gid);
        // the slot just past the group's current run (excluding i itself)
        let last = self
            .tabs
            .iter()
            .enumerate()
            .filter(|(j, t)| *j != i && t.group == Some(gid))
            .map(|(j, _)| j)
            .max();
        if let Some(last) = last {
            let slot = last + 1;
            let (dest, _) = reorder_indices(i, slot, self.tabs.len(), self.active);
            self.move_tab(i, slot, cx);
            self.tab_menu = Some(dest);
        } else {
            self.save(cx);
        }
        cx.notify();
    }

    /// Remove tab `i` from its group and drop the group if it's now empty.
    fn remove_from_group(&mut self, i: usize, cx: &mut Context<Self>) {
        if let Some(t) = self.tabs.get_mut(i) {
            t.group = None;
        }
        self.prune_groups();
        self.tab_scope = TabScope::ThisTab;
        self.save(cx);
        cx.notify();
    }

    /// Fold / unfold a group.
    fn toggle_group_collapsed(&mut self, gid: u32, cx: &mut Context<Self>) {
        if let Some(g) = self.groups.iter_mut().find(|g| g.id == gid) {
            g.collapsed = !g.collapsed;
            self.save(cx);
            cx.notify();
        }
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
        self.focus_read = Some(pane.downgrade());
        // Each FOCUS opens at fit-to-modal; the header slider takes it from there.
        self.focus_zoom = 1.0;
        // A fresh open starts at the top of the mirror (no pan carried over) with
        // nothing selected.
        self.focus_scroll_y = 0.0;
        self.focus_sel = None;
        self.focus_sel_drag = false;
        // Defer the focus: this runs from the 👓 header button's mouse-down
        // listener, so a synchronous `window.focus` gets grabbed straight back by
        // the root container's tracked focus handle (the same race new_tab/split
        // dodge — see the focus-back-race note). Running after the event settles
        // makes the CLICKED pane reliably take focus, so when you 👓 a pane other
        // than the active one, keystrokes follow it into the reader.
        cx.defer_in(window, move |_ws, window, cx| {
            window.focus(&pane.focus_handle(cx), cx);
        });
        cx.notify();
    }

    /// Close the FOCUS modal and clear the read flag on its pane (if still open).
    fn close_focus_read(&mut self, cx: &mut Context<Self>) {
        if let Some(pane) = self.focus_read.take().and_then(|w| w.upgrade()) {
            pane.update(cx, |v, _| v.set_being_read(false));
        }
        self.focus_sel = None;
        self.focus_sel_drag = false;
        *self.focus_map.lock().unwrap() = None;
        cx.notify();
    }

    /// Open the find panel: `global` searches every pane (centred on the window),
    /// else just `pane_id` (centred over that pane). The workspace root takes the
    /// keyboard so typing edits the query box — panes route keys to their PTY, so
    /// the find box lives here. Deferred focus dodges the focus-back race the
    /// rename editors hit (the pane's own focus would otherwise grab it back).
    fn open_find(
        &mut self,
        pane_id: EntityId,
        global: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.find = Some(FindState {
            scope: if global {
                FindScope::Global
            } else {
                FindScope::InPane(pane_id)
            },
            query: EditBuffer::seeded(""),
            results: Vec::new(),
            selected: 0,
        });
        cx.defer_in(window, |ws, window, cx| {
            window.focus(&ws.focus_handle, cx);
        });
        cx.notify();
    }

    /// Fuzzy-search the panes in `scope` for `query`, building the find rows. In
    /// `InPane` every matching line is its own row (newest first); in `Global`
    /// each matching pane contributes one row (its best line + a match count),
    /// ranked by score. Empty query → no results.
    fn compute_find(&self, query: &str, scope: &FindScope, cx: &App) -> Vec<FindHit> {
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            return Vec::new();
        }
        // Most-recent lines scanned per pane — bounds the per-keystroke cost when
        // a global search sweeps many deep-scrollback panes at once.
        const CAP: usize = 2500;
        let mut hits: Vec<FindHit> = Vec::new();
        for (ti, tab) in self.tabs.iter().enumerate() {
            let mut leaves = vec![];
            tab.root.leaves(&mut leaves);
            for leaf in leaves {
                let id = leaf.entity_id();
                if let FindScope::InPane(target) = scope {
                    if id != *target {
                        continue;
                    }
                }
                let p = leaf.read(cx);
                let mode = p.mode.label().to_string();
                let is_agent = p.mode.is_agent();
                let title = p
                    .name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| (!p.title.is_empty()).then(|| p.title.clone()))
                    .unwrap_or_else(|| mode.clone());
                let gh = p.search_grid(&needle, CAP);
                if gh.is_empty() {
                    continue;
                }
                match scope {
                    FindScope::InPane(_) => {
                        for h in gh.into_iter().rev() {
                            hits.push(FindHit {
                                pane: leaf.downgrade(),
                                pane_id: id,
                                tab: ti,
                                mode: mode.clone(),
                                is_agent,
                                title: title.clone(),
                                line: h.line,
                                snippet: h.text,
                                positions: h.positions,
                                score: h.score,
                                same_pane_count: 0,
                            });
                        }
                    }
                    FindScope::Global => {
                        let count = gh.len();
                        let best = gh.into_iter().max_by_key(|h| h.score).unwrap();
                        hits.push(FindHit {
                            pane: leaf.downgrade(),
                            pane_id: id,
                            tab: ti,
                            mode,
                            is_agent,
                            title,
                            line: best.line,
                            snippet: best.text,
                            positions: best.positions,
                            score: best.score,
                            same_pane_count: count,
                        });
                    }
                }
            }
        }
        if matches!(scope, FindScope::Global) {
            hits.sort_by_key(|h| std::cmp::Reverse(h.score));
        }
        hits
    }

    /// ↵ in the find panel: jump to the selected hit — scroll its pane to the
    /// matched line (highlighting the span), focus that exact leaf in its tab, and
    /// close the panel. An empty result set just closes + refocuses the pane.
    fn jump_to_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(find) = self.find.take() else {
            return;
        };
        let Some(hit) = find.results.get(find.selected) else {
            self.focus_active(window, cx);
            cx.notify();
            return;
        };
        let (tab, line, pane_id) = (hit.tab, hit.line, hit.pane_id);
        let sel = match (hit.positions.first(), hit.positions.last()) {
            (Some(&a), Some(&b)) => Some((a, b)),
            _ => None,
        };
        if let Some(p) = hit.pane.upgrade() {
            p.update(cx, |v, cx| v.scroll_to_line(line, sel, cx));
        }
        if let Some(t) = self.tabs.get_mut(tab) {
            t.focused = Some(pane_id);
        }
        self.activate_tab(tab, window, cx);
        cx.notify();
    }

    /// Render a matched line as a highlighted snippet: the run is windowed around
    /// the first match (so a long line shows the hit in context, with `…` ellipses
    /// where trimmed) and the matched chars are painted in `hit_col`/bold while the
    /// rest stays `base_col`. Consecutive matched/unmatched chars are grouped into
    /// single spans so the row stays cheap even on a wide line.
    fn find_snippet(
        snippet: &str,
        positions: &[usize],
        hit_col: gpui::Hsla,
        base_col: gpui::Hsla,
    ) -> gpui::Div {
        let chars: Vec<char> = snippet.chars().collect();
        let n = chars.len();
        let first = positions.first().copied().unwrap_or(0);
        let win_start = first.saturating_sub(12);
        let win_end = (win_start + 96).min(n);
        let pos: std::collections::HashSet<usize> = positions.iter().copied().collect();
        let mut row = div()
            .flex()
            .flex_row()
            .items_center()
            .whitespace_nowrap()
            .text_size(px(9.));
        if win_start > 0 {
            row = row.child(div().text_color(base_col.alpha(0.5)).child("\u{2026}"));
        }
        let mut i = win_start;
        while i < win_end {
            let matched = pos.contains(&i);
            let mut j = i;
            let mut s = String::new();
            while j < win_end && pos.contains(&j) == matched {
                s.push(chars[j]);
                j += 1;
            }
            row = if matched {
                row.child(
                    div()
                        .text_color(hit_col)
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(s),
                )
            } else {
                row.child(div().text_color(base_col).child(s))
            };
            i = j;
        }
        if win_end < n {
            row = row.child(div().text_color(base_col.alpha(0.5)).child("\u{2026}"));
        }
        row
    }

    /// The find panel overlay: a fuzzy-search box + a results list styled like the
    /// 🤖 MCP pane list (mode label · title · matched-line snippet). It centres over
    /// the searched pane (Ctrl+F) or the whole window (Ctrl+Shift+F), positioned
    /// absolutely from the measured pane/window box. Only a window of rows around
    /// the selection renders, so the keyboard selection always stays in view.
    fn render_find(&self, th: &theme::Theme, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let find = self.find.as_ref()?;
        let (ww, wh) = self
            .last_win
            .map(|(_, _, w, h)| (w, h))
            .unwrap_or((1200., 800.));
        let global = matches!(find.scope, FindScope::Global);
        const ROW_H: f32 = 36.;
        const VISIBLE: usize = 11;
        let panel_w = if global { 600. } else { 480. };
        let n = find.results.len();
        let shown = n.clamp(1, VISIBLE);
        let panel_h = 72. + shown as f32 * ROW_H + 26.;
        // centre point in window-relative logical px
        let (cxp, cyp) = if let FindScope::InPane(id) = &find.scope {
            self.pane_bounds
                .lock()
                .unwrap()
                .get(id)
                .map(|b| {
                    (
                        f32::from(b.origin.x) + f32::from(b.size.width) * 0.5,
                        f32::from(b.origin.y) + f32::from(b.size.height) * 0.5,
                    )
                })
                .unwrap_or((ww * 0.5, wh * 0.5))
        } else {
            (ww * 0.5, wh * 0.5)
        };
        let left = (cxp - panel_w * 0.5).clamp(8., (ww - panel_w - 8.).max(8.));
        let top = (cyp - panel_h * 0.5).clamp(8., (wh - panel_h - 8.).max(8.));

        // windowed slice so the selection stays on-screen during keyboard nav
        let start = if n <= VISIBLE {
            0
        } else {
            find.selected.saturating_sub(VISIBLE / 2).min(n - VISIBLE)
        };
        let end = (start + VISIBLE).min(n);

        let q = find.query.text();
        let scope_lbl = if global {
            "FIND \u{2014} ALL PANES"
        } else {
            "FIND IN PANE"
        };
        let input = {
            let eb = render_edit_buffer(&find.query, 1.0, th.text, th.accent, th.accent.alpha(0.3));
            if q.is_empty() {
                eb.child(
                    div()
                        .text_color(th.text.alpha(0.35))
                        .child("type to search\u{2026}"),
                )
            } else {
                eb
            }
        };
        let header = div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .pt_2()
            .pb_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(10.))
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.accent)
                            .child(format!("\u{1f50d}  {scope_lbl}")),
                    )
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(th.text.alpha(0.5))
                            .child(if q.trim().is_empty() {
                                String::new()
                            } else {
                                format!("{n} match{}", if n == 1 { "" } else { "es" })
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(th.bg.alpha(0.55))
                    .border_1()
                    .border_color(th.accent.alpha(0.4))
                    .text_size(px(13.))
                    .child(input),
            );

        let mut list = div().flex().flex_col().gap_0p5().px_1();
        for i in start..end {
            let hit = &find.results[i];
            let selected = i == find.selected;
            let mode_col = if hit.is_agent {
                th.accent
            } else {
                th.text.alpha(0.5)
            };
            let title_line = div().flex().flex_row().items_center().gap_2().child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(px(10.))
                    .text_color(th.text)
                    .child(hit.title.clone()),
            );
            let title_line = if global && hit.same_pane_count > 1 {
                title_line.child(
                    div()
                        .flex_none()
                        .text_size(px(8.))
                        .text_color(th.accent.alpha(0.75))
                        .child(format!("{}\u{00d7}", hit.same_pane_count)),
                )
            } else {
                title_line
            };
            let row = div()
                .id(gpui::SharedString::from(format!("find-row-{i}")))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_2()
                .h(px(ROW_H))
                .rounded_sm()
                .cursor_pointer()
                .bg(if selected {
                    th.accent.alpha(0.16)
                } else {
                    hsla(0., 0., 0., 0.)
                })
                .border_1()
                .border_color(if selected {
                    th.accent.alpha(0.55)
                } else {
                    hsla(0., 0., 0., 0.)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if let Some(f) = ws.find.as_mut() {
                            f.selected = i;
                        }
                        ws.jump_to_find(window, cx);
                    }),
                )
                .child(
                    div()
                        .w(px(54.))
                        .flex_none()
                        .text_size(px(9.))
                        .font_weight(gpui::FontWeight::EXTRA_BOLD)
                        .text_color(mode_col)
                        .child(hit.mode.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .flex()
                        .flex_col()
                        .child(title_line)
                        .child(Self::find_snippet(
                            &hit.snippet,
                            &hit.positions,
                            th.accent,
                            th.text.alpha(0.8),
                        )),
                );
            list = list.child(row);
        }
        if n == 0 {
            list = list.child(
                div()
                    .px_2()
                    .py_2()
                    .text_size(px(10.))
                    .text_color(th.text.alpha(0.4))
                    .child(if q.trim().is_empty() {
                        "Start typing to fuzzy-search the terminal\u{2026}".to_string()
                    } else {
                        format!("No matches for \u{201c}{}\u{201d}", q.trim())
                    }),
            );
        }

        let panel = div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(panel_w))
            .flex()
            .flex_col()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(darken(th.surface, 0.35))
            .border_2()
            .border_color(th.accent.alpha(0.85))
            .shadow(float_shadows(th.accent))
            .child(header)
            .child(div().pb_1().flex().flex_col().child(list))
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .pt_0p5()
                    .text_size(px(8.5))
                    .text_color(th.text.alpha(0.45))
                    .child("\u{2191}\u{2193} select \u{00b7} \u{21b5} jump \u{00b7} esc close"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(hsla(0., 0., 0., 0.28))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        ws.find = None;
                        ws.focus_active(window, cx);
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }

    /// The languages matching the picker query, best-match first. Empty query →
    /// all languages in canonical order. Matches the autonym, the English name, or
    /// the code (so "de", "german", "deutsch" all find German).
    fn filtered_langs(query: &str) -> Vec<lang::Lang> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return lang::Lang::ALL.to_vec();
        }
        let mut scored: Vec<(i64, usize, lang::Lang)> = lang::Lang::ALL
            .iter()
            .enumerate()
            .filter_map(|(i, &l)| {
                let key = format!("{} {} {}", l.native(), l.english(), l.code()).to_lowercase();
                pane::fuzzy_match(&key, &q).map(|(s, _)| (s, i, l))
            })
            .collect();
        // higher score first; ties keep canonical order
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        scored.into_iter().map(|(_, _, l)| l).collect()
    }

    /// The language dropdown (🌐 in the help header): a fuzzy-search box over the 9
    /// languages, looking like the find panel. Its own scrim closes it. The native
    /// names need the script fallback or they'd tofu, since this is a separate
    /// overlay from the help panel that already sets it.
    /// 🧩 the plugins panel: every discovered MCP plugin and the actions it
    /// offers. Read-only (discovery only) — running an action happens from the
    /// surface it targets (e.g. the 🪦 graveyard's ⬇ .cdx button). The
    /// authoritative tool list still comes from each plugin's live `tools/list`;
    /// this lists what the manifests advertise.
    fn render_plugins_overlay(
        &self,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        if !self.plugins_menu {
            return None;
        }
        let home = session::home_dir();
        let found = plugins::discover(&home);
        let n = found.len();

        let mut list = div().flex().flex_col().gap_2();
        if found.is_empty() {
            list = list.child(
                div()
                    .text_size(px(9.5))
                    .text_color(th.text.alpha(0.6))
                    .child(
                        "no plugins found \u{2014} install cdx-mcp (cargo install --path crates/cdx-mcp) or drop a plugin.json under ~/.config/terminal-delight/plugins/",
                    ),
            );
        }
        for (pi, m) in found.iter().enumerate() {
            let mut actions_row = div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_1()
                .text_size(px(8.5));
            for a in &m.actions {
                actions_row = actions_row.child(
                    div()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(th.complement.alpha(0.5))
                        .text_color(th.complement)
                        .child(format!("{}  \u{00b7}  {}", a.label, a.surfaces.join("/"))),
                );
            }
            list = list.child(
                div()
                    .id(SharedString::from(format!("plg-{pi}")))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .border_1()
                    .border_color(th.text.alpha(0.12))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .text_color(th.text)
                                    .child(m.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(8.5))
                                    .text_color(th.text.alpha(0.45))
                                    .child(format!("v{}", m.version)),
                            )
                            .child(div().flex_1().min_w(px(0.)))
                            .child(
                                div()
                                    .text_size(px(8.))
                                    .text_color(th.accent.alpha(0.7))
                                    .child(m.command.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(th.text.alpha(0.7))
                            .child(m.description.clone()),
                    )
                    .child(actions_row),
            );
        }

        let panel = div()
            .w(gpui::relative(0.6))
            .max_h(gpui::relative(0.8))
            .overflow_hidden()
            .p_3()
            .rounded_md()
            .border_2()
            .border_color(th.accent.alpha(0.85))
            .bg(darken(th.surface, 0.6))
            .shadow(float_shadows(th.accent))
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(10.))
            .text_color(th.text)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_size(px(13.))
                    .child(div().child("\u{1f9e9}"))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.complement)
                            .child("PLUGINS"),
                    )
                    .child(div().flex_1().min_w(px(0.)))
                    .child(
                        div()
                            .text_color(th.text.alpha(0.7))
                            .child(format!("{n} installed")),
                    ),
            )
            .child(
                div()
                    .text_size(px(8.5))
                    .text_color(th.accent.alpha(0.85))
                    .child(
                        "plugins are MCP servers terminal-delight launches on demand \u{2014} run their actions from the agent wall or the 🪦 graveyard",
                    ),
            )
            .child(list);

        Some(
            div()
                .absolute()
                .inset_0()
                .bg(th.bg.alpha(0.70))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.plugins_menu = false;
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }

    fn render_lang_picker(&self, th: &theme::Theme, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let lp = self.lang_picker.as_ref()?;
        let (ww, wh) = self
            .last_win
            .map(|(_, _, w, h)| (w, h))
            .unwrap_or((1200., 800.));
        let langs = Self::filtered_langs(&lp.query.text());
        let q = lp.query.text();
        const ROW_H: f32 = 34.;
        let panel_w = 340.;
        let shown = langs.len().max(1);
        let panel_h = 78. + shown as f32 * ROW_H + 24.;
        let left = (ww * 0.5 - panel_w * 0.5).clamp(8., (ww - panel_w - 8.).max(8.));
        let top = (wh * 0.28 - panel_h * 0.5).clamp(8., (wh - panel_h - 8.).max(8.));
        let sel = lp.selected.min(shown - 1);

        let input = {
            let eb = render_edit_buffer(&lp.query, 1.0, th.text, th.accent, th.accent.alpha(0.3));
            if q.is_empty() {
                eb.child(
                    div()
                        .text_color(th.text.alpha(0.35))
                        .child("type a language\u{2026}"),
                )
            } else {
                eb
            }
        };
        let header = div()
            .flex()
            .flex_col()
            .gap_1()
            .px_2()
            .pt_2()
            .pb_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(10.))
                            .font_weight(gpui::FontWeight::EXTRA_BOLD)
                            .text_color(th.accent)
                            .child("\u{1f310}  LANGUAGE"),
                    )
                    .child(
                        div()
                            .text_size(px(9.))
                            .text_color(th.text.alpha(0.5))
                            .child(format!("{}/{}", langs.len(), lang::Lang::ALL.len())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(th.bg.alpha(0.55))
                    .border_1()
                    .border_color(th.accent.alpha(0.4))
                    .text_size(px(13.))
                    .child(input),
            );

        let mut list = div().flex().flex_col().gap_0p5().px_1();
        for (i, &l) in langs.iter().enumerate() {
            let selected = i == sel;
            let is_current = l == self.lang;
            let row = div()
                .id(("lang-row", i))
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .h(px(ROW_H))
                .px_2()
                .rounded_sm()
                .cursor_pointer()
                .when(selected, |d| d.bg(th.accent.alpha(0.18)))
                .child(
                    div()
                        .w(px(16.))
                        .flex_none()
                        .text_color(th.accent)
                        .child(if is_current { "\u{2713}" } else { "" }),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(14.))
                        .text_color(th.text)
                        .child(l.native().to_string()),
                )
                .child(
                    div()
                        .text_size(px(9.5))
                        .text_color(th.text.alpha(0.45))
                        .child(l.english().to_string()),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.lang = l;
                        lang::set_current(l);
                        ws.lang_picker = None;
                        ws.save(cx);
                        cx.notify();
                    }),
                );
            list = list.child(row);
        }
        if langs.is_empty() {
            list = list.child(
                div()
                    .px_2()
                    .py_2()
                    .text_size(px(10.))
                    .text_color(th.text.alpha(0.4))
                    .child(format!("No language matches \u{201c}{}\u{201d}", q.trim())),
            );
        }

        let panel = div()
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(panel_w))
            .flex()
            .flex_col()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(darken(th.surface, 0.35))
            .border_1()
            .border_color(th.accent.alpha(0.6))
            .font_family(th.font_family.clone())
            .map(|mut d| {
                // CJK/Devanagari fallback so 中文 / 日本語 / 한국어 / हिन्दी render as real
                // glyphs in the list (separate overlay from the help panel).
                if let Some(fb) = pane::script_fallbacks() {
                    d.text_style().font_fallbacks = Some(fb);
                }
                d
            })
            .shadow(vec![BoxShadow {
                color: hsla(0., 0., 0., 0.7),
                offset: point(px(0.), px(10.)),
                blur_radius: px(36.),
                spread_radius: px(2.),
                inset: false,
            }])
            .child(header)
            .child(div().pb_1().flex().flex_col().child(list))
            .child(
                div()
                    .px_2()
                    .pb_1()
                    .pt_0p5()
                    .text_size(px(8.5))
                    .text_color(th.text.alpha(0.45))
                    .child("\u{2191}\u{2193} select \u{00b7} \u{21b5} choose \u{00b7} esc close"),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
            );

        Some(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(hsla(0., 0., 0., 0.28))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.lang_picker = None;
                        cx.notify();
                    }),
                )
                .child(panel),
        )
    }

    /// FOCUS text-size slider range — a multiplier on the auto-fit scale.
    /// 1.0 (fit) sits inside the range so the thumb has travel both ways. The
    /// low end reaches well below fit so a dense read can shrink to a compact,
    /// glanceable column (the reader wraps, so smaller never means narrower-only).
    const FZ_MIN: f32 = 0.35;
    const FZ_MAX: f32 = 3.0;

    /// Map a window-x to a 0..1 fraction along the FOCUS slider track (`None`
    /// until the track has been measured once).
    fn focus_zoom_from_pos(&self, x: Pixels) -> Option<f32> {
        let b = (*self.focus_zoom_bounds.lock().unwrap())?;
        let w = f32::from(b.size.width);
        if w <= 0.0 {
            return None;
        }
        Some(((f32::from(x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0))
    }

    /// Map a window position to the source `(row, col)` cell under it in the FOCUS
    /// reader, or `None` if the reader isn't laid out yet or the click misses it.
    /// Mirrors the live pane's curve-aware hit-test: normalise into the reading
    /// rect, apply the SAME barrel map the shader gathers with (only when the
    /// reader is curved), then undo the pad + scroll to land on a wrapped row, and
    /// map that back to a source cell via the frame's wrap map.
    fn focus_cell_at(&self, pos: Point<Pixels>) -> Option<(usize, usize)> {
        let bounds = (*self.focus_body_bounds.lock().unwrap())?;
        let guard = self.focus_map.lock().unwrap();
        let map = guard.as_ref()?;
        if map.rows.is_empty() {
            return None;
        }
        let (bx, by) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
        let bw = f32::from(bounds.size.width).max(1.0);
        let bh = f32::from(bounds.size.height).max(1.0);
        let (u, v) = ((f32::from(pos.x) - bx) / bw, (f32::from(pos.y) - by) / bh);
        let (lu, lv) = if map.inherit {
            pane::warp_screen_to_content(u, v, map.k1, map.k2)
        } else {
            (u, v)
        };
        let content_y = lv * bh - map.top;
        let vrow = ((content_y / map.line_h).max(0.0) as usize).min(map.rows.len() - 1);
        let (src_row, src_col0, cols) = map.rows[vrow];
        let content_x = (lu * bw - map.pad).max(0.0);
        let col_in = (content_x / map.glyph_w.max(0.1)).floor() as usize;
        Some((src_row, src_col0 + col_in.min(cols)))
    }

    /// Assemble the reader's current selection into copyable text — one logical
    /// line per SOURCE row (the visual wrap is cosmetic, so a wrapped paragraph
    /// copies back as the single line it really is). Trailing blanks are trimmed.
    fn focus_selection_text(&self) -> Option<String> {
        let (a, b) = self.focus_sel?;
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let guard = self.focus_map.lock().unwrap();
        let map = guard.as_ref()?;
        let mut out = String::new();
        for row in start.0..=end.0 {
            let line = map.src_lines.get(row).map(|s| s.as_str()).unwrap_or("");
            let chars: Vec<char> = line.trim_end_matches(' ').chars().collect();
            let from = if row == start.0 { start.1 } else { 0 }.min(chars.len());
            let to = if row == end.0 { end.1 + 1 } else { chars.len() }.min(chars.len());
            if from < to {
                out.extend(&chars[from..to]);
            }
            if row != end.0 {
                out.push('\n');
            }
        }
        (!out.is_empty()).then_some(out)
    }

    /// Copy the reader selection to the clipboard (and X11 PRIMARY for middle-click
    /// paste), matching how the live pane copies. No-op when nothing is selected.
    fn copy_focus_selection(&self, cx: &mut Context<Self>) {
        if let Some(text) = self.focus_selection_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            cx.write_to_primary(ClipboardItem::new_string(text));
        }
    }

    /// Set the FOCUS text-size multiplier from a 0..1 track fraction. Live only;
    /// the zoom is per-open and intentionally never persisted.
    fn set_focus_zoom(&mut self, frac: f32, cx: &mut Context<Self>) {
        let z = Self::FZ_MIN + frac.clamp(0.0, 1.0) * (Self::FZ_MAX - Self::FZ_MIN);
        if (z - self.focus_zoom).abs() > f32::EPSILON {
            self.focus_zoom = z;
            cx.notify();
        }
    }

    /// The FOCUS modal's header text-size slider: a small "A …──●── A" track
    /// that scales the mirror's text `FZ_MIN..=FZ_MAX`× the auto-fit size. The
    /// track box is captured each frame into `focus_zoom_bounds`, so a drag that
    /// leaves the track still maps the cursor-x back to a fraction (same trick as
    /// the OSD grade sliders). Live-only — the zoom never persists.
    fn focus_zoom_slider(
        &self,
        accent: gpui::Hsla,
        text: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        const TRACK: f32 = 110.;
        let store = self.focus_zoom_bounds.clone();
        let frac = ((self.focus_zoom - Self::FZ_MIN) / (Self::FZ_MAX - Self::FZ_MIN)).clamp(0., 1.);
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            // small "A" — the shrink end
            .child(
                div()
                    .text_size(px(9.))
                    .text_color(text.alpha(0.7))
                    .child("A"),
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
                            // don't let the panel/scrim see this (no close, no move)
                            cx.stop_propagation();
                            ws.focus_zoom_drag = true;
                            if let Some(f) = ws.focus_zoom_from_pos(ev.position.x) {
                                ws.set_focus_zoom(f, cx);
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
                    // groove
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(px(6.))
                            .h(px(3.))
                            .rounded_full()
                            .bg(accent.alpha(0.18)),
                    )
                    // fill up to the thumb
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top(px(6.))
                            .h(px(3.))
                            .w(px(TRACK * frac))
                            .rounded_full()
                            .bg(accent),
                    )
                    // thumb
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
                                linear_color_stop(brighten(accent, 1.4), 0.),
                                linear_color_stop(darken(accent, 0.7), 1.),
                            )),
                    ),
            )
            // large "A" — the grow end
            .child(
                div()
                    .text_size(px(15.))
                    .text_color(text.alpha(0.7))
                    .child("A"),
            )
    }

    /// The FOCUS header's "Inherit theme" toggle: a small pill that flips the
    /// global [`Self::focus_inherit_theme`] preference (and persists it) so every
    /// reader from now on either inherits the read pane's CRT look (curve + glare)
    /// or stays flat. A filled dot ◉ = on, hollow ○ = off.
    fn focus_inherit_toggle(
        &self,
        accent: gpui::Hsla,
        text: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let on = self.focus_inherit_theme;
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_2()
            .py_0p5()
            .rounded_md()
            .cursor_pointer()
            .border_1()
            .border_color(if on {
                accent.alpha(0.8)
            } else {
                text.alpha(0.22)
            })
            .bg(if on {
                accent.alpha(0.15)
            } else {
                hsla(0., 0., 0., 0.)
            })
            .text_size(px(11.))
            .text_color(if on { accent } else { text.alpha(0.7) })
            .child(if on { "◉" } else { "○" })
            .child("Inherit theme")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    // don't let the panel/scrim see this (no close)
                    cx.stop_propagation();
                    ws.focus_inherit_theme = !ws.focus_inherit_theme;
                    ws.save(cx);
                    cx.notify();
                }),
            )
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

    /// Does any pane in tab `i` have an unacknowledged "agent finished" bell? It
    /// drives the per-tab 🔔 badge, so a run that finishes in a background tab is
    /// visible on the mother bar without opening every pane. Mirrors the pane's
    /// own bell — the in-terminal ack click clears both at once.
    fn tab_has_bell(&self, i: usize, cx: &App) -> bool {
        let Some(tab) = self.tabs.get(i) else {
            return false;
        };
        let mut leaves = vec![];
        tab.root.leaves(&mut leaves);
        leaves.iter().any(|p| p.read(cx).has_bell())
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

    /// Ctrl+W: always raise the confirmation dialog for the active tab — even a
    /// single-pane tab. Closing a tab ends live shells, so it's never silent.
    fn confirm_close_active_tab(&mut self, cx: &mut Context<Self>) {
        if self.active < self.tabs.len() {
            self.confirm_close = Some(self.active);
            cx.notify();
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
        self.tab_menu = None;
        self.tabs.remove(i);
        if self.tabs.is_empty() {
            cx.quit();
            return;
        }
        self.prune_groups();
        self.active = self.active.min(self.tabs.len() - 1);
        self.focus_active(window, cx);
        // Closing a whole tab is an EXPLICIT shrink — allow it past the guard.
        self.permit_shrink.set(true);
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
        let Some((tab_i, eb)) = self.renaming.take() else {
            return false;
        };
        let name = eb.text();
        if let Some(tab) = self.tabs.get_mut(tab_i) {
            tab.name = (!name.trim().is_empty()).then(|| name.trim().to_string());
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

    /// Is any floating surface (modal or menu) open above the workspace?
    fn any_popup_open(&self) -> bool {
        self.help_open
            || self.mcp_menu
            || self.dead_menu
            || self.plugins_menu
            || self.savings_menu
            || self.confirm_close.is_some()
            || self.find.is_some()
            || self.lang_picker.is_some()
            || self.theme_menu.is_some()
            || self.osd_menu.is_some()
            || self.tab_menu.is_some()
            || self.group_menu.is_some()
            || self.focus_read.is_some()
    }

    /// Close every open popup (modal or menu). Esc and click-outside both funnel
    /// through here so dismissal is identical everywhere across the app. Returns
    /// whether anything was open. NEVER touches a terminal pane.
    fn close_popups(&mut self) -> bool {
        let open = self.any_popup_open();
        if open {
            self.help_open = false;
            self.mcp_menu = false;
            self.dead_menu = false;
            self.plugins_menu = false;
            self.savings_menu = false;
            self.confirm_close = None;
            self.find = None;
            self.lang_picker = None;
            self.theme_menu = None;
            self.osd_menu = None;
            self.tab_menu = None;
            self.group_menu = None;
            self.focus_read = None;
        }
        open
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
        // Esc closes whatever popup (modal or menu) is open — one consistent path
        // for the whole app. A capture-phase handler (see render) catches it even
        // while a terminal holds focus; this is the same dismissal for the
        // no-pane-focused case. Esc NEVER closes a terminal.
        if ks.key.as_str() == "escape" && self.close_popups() {
            cx.notify();
            return;
        }
        // The language dropdown owns the keyboard while open: esc closes, ↵ applies
        // the selected language, ↑/↓ (tab) move, everything else edits the fuzzy query.
        if let Some(mut lp) = self.lang_picker.take() {
            match ks.key.as_str() {
                "escape" => {
                    cx.notify();
                    return;
                }
                "enter" => {
                    let langs = Self::filtered_langs(&lp.query.text());
                    if let Some(&l) = langs.get(lp.selected.min(langs.len().saturating_sub(1))) {
                        self.lang = l;
                        lang::set_current(l);
                        self.save(cx);
                    }
                    cx.notify();
                    return;
                }
                "down" | "tab" => {
                    let n = Self::filtered_langs(&lp.query.text()).len();
                    if n > 0 {
                        lp.selected = (lp.selected + 1).min(n - 1);
                    }
                    self.lang_picker = Some(lp);
                    cx.notify();
                    return;
                }
                "up" => {
                    lp.selected = lp.selected.saturating_sub(1);
                    self.lang_picker = Some(lp);
                    cx.notify();
                    return;
                }
                _ => {
                    let before = lp.query.text();
                    lp.query
                        .apply(ks.key.as_str(), m, ks.key_char.as_deref(), 24);
                    if lp.query.text() != before {
                        lp.selected = 0;
                    }
                    self.lang_picker = Some(lp);
                    cx.notify();
                    return;
                }
            }
        }
        // The find panel owns the keyboard while open: esc closes, ↵ jumps to the
        // selected hit, ↑/↓ move the selection, everything else edits the query and
        // re-runs the fuzzy search.
        if let Some(mut find) = self.find.take() {
            match ks.key.as_str() {
                "escape" => {
                    self.focus_active(window, cx);
                    cx.notify();
                    return;
                }
                "enter" => {
                    self.find = Some(find);
                    self.jump_to_find(window, cx);
                    return;
                }
                "down" | "tab" => {
                    if !find.results.is_empty() {
                        find.selected = (find.selected + 1).min(find.results.len() - 1);
                    }
                    self.find = Some(find);
                    cx.notify();
                    return;
                }
                "up" => {
                    find.selected = find.selected.saturating_sub(1);
                    self.find = Some(find);
                    cx.notify();
                    return;
                }
                _ => {
                    let before = find.query.text();
                    find.query
                        .apply(ks.key.as_str(), m, ks.key_char.as_deref(), 80);
                    if find.query.text() != before {
                        find.results = self.compute_find(&find.query.text(), &find.scope, cx);
                        find.selected = 0;
                    }
                    self.find = Some(find);
                    cx.notify();
                    return;
                }
            }
        }
        // the inline group-name editor owns the keyboard while open
        if let Some((gid, mut eb)) = self.group_rename.take() {
            match ks.key.as_str() {
                "enter" | "escape" => {
                    if ks.key.as_str() == "enter" {
                        let name = eb.text();
                        if let Some(g) = self.groups.iter_mut().find(|g| g.id == gid) {
                            g.name = (!name.trim().is_empty()).then(|| name.trim().to_string());
                        }
                        self.save(cx);
                    }
                    self.focus_active(window, cx);
                }
                _ => {
                    eb.apply(ks.key.as_str(), m, ks.key_char.as_deref(), 18);
                    self.group_rename = Some((gid, eb));
                }
            }
            cx.notify();
            return;
        }
        if self.tab_menu.is_some() && ks.key.as_str() == "escape" {
            self.tab_menu = None;
            cx.notify();
            return;
        }
        // The close-tab confirmation owns the keyboard while up: Esc cancels,
        // Enter confirms (so the serious dialog is fully keyboard-drivable).
        if let Some(i) = self.confirm_close {
            match ks.key.as_str() {
                "escape" => {
                    self.confirm_close = None;
                    cx.notify();
                }
                "enter" => self.close_tab(i, window, cx),
                _ => {}
            }
            return;
        }
        // the inline rename box owns the keyboard while open
        if let Some((tab_i, mut eb)) = self.renaming.take() {
            match ks.key.as_str() {
                "enter" => {
                    let name = eb.text();
                    if let Some(tab) = self.tabs.get_mut(tab_i) {
                        tab.name = (!name.trim().is_empty()).then(|| name.trim().to_string());
                    }
                    self.save(cx);
                    self.focus_active(window, cx);
                }
                "escape" => self.focus_active(window, cx),
                _ => {
                    eb.apply(ks.key.as_str(), m, ks.key_char.as_deref(), 18);
                    self.renaming = Some((tab_i, eb));
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
                // ctrl+shift+pgup → MOVE the active tab left (clamped to its
                // group); plain ctrl+pgup → switch to the previous tab.
                "pageup" => {
                    if m.shift {
                        self.nudge_active_tab(-1, cx);
                    } else {
                        let n = self.tabs.len();
                        self.activate_tab((self.active + n - 1) % n, window, cx);
                    }
                    return;
                }
                // ctrl+shift+pgdn → MOVE right (clamped); plain ctrl+pgdn → next.
                "pagedown" => {
                    if m.shift {
                        self.nudge_active_tab(1, cx);
                    } else {
                        let n = self.tabs.len();
                        self.activate_tab((self.active + 1) % n, window, cx);
                    }
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
                self.tab_drop_newrow = false;
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
            if engaged {
                let (slot, new_row) = self.resolve_tab_slot(pos);
                self.tab_drop = Some(slot);
                self.tab_drop_newrow = new_row;
            } else {
                self.tab_drop = None;
                self.tab_drop_newrow = false;
            }
            cx.notify();
            return;
        }
        // a group drag in flight: track the cursor, engage past the threshold (so a
        // plain handle click still folds the group), and resolve the drop slot —
        // the whole group lands at the slot a release would drop it into.
        if self.group_drag.is_some() {
            if ev.pressed_button != Some(MouseButton::Left) {
                self.group_drag = None;
                self.tab_drop = None;
                self.tab_drop_newrow = false;
                cx.notify();
                return;
            }
            let pos = ev.position;
            let engaged = {
                let d = self.group_drag.as_mut().unwrap();
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
            if engaged {
                let (slot, new_row) = self.resolve_tab_slot(pos);
                self.tab_drop = Some(slot);
                self.tab_drop_newrow = new_row;
            } else {
                self.tab_drop = None;
                self.tab_drop_newrow = false;
            }
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
                || self.slider_drag.is_some()
                || self.track_drag.is_some()
                || self.tab_wheel_drag.is_some()
                || self.tab_light_drag
                || self.focus_zoom_drag
                || self.focus_sel_drag
                || self.drag_split.is_some())
        {
            self.scrubbing = false;
            self.wheel_drag = None;
            self.light_drag = false;
            self.slider_drag = None;
            self.track_drag = None;
            self.tab_wheel_drag = None;
            self.tab_light_drag = false;
            self.focus_zoom_drag = false;
            self.focus_sel_drag = false;
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
        if self.focus_zoom_drag && ev.pressed_button == Some(MouseButton::Left) {
            if let Some(frac) = self.focus_zoom_from_pos(ev.position.x) {
                self.set_focus_zoom(frac, cx);
            }
        }
        // Extend the FOCUS reader selection as the cursor drags — the head moves to
        // the source cell under it, the anchor stays put.
        if self.focus_sel_drag && ev.pressed_button == Some(MouseButton::Left) {
            if let (Some(cell), Some((anchor, head))) =
                (self.focus_cell_at(ev.position), self.focus_sel)
            {
                if head != cell {
                    self.focus_sel = Some((anchor, cell));
                    cx.notify();
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
        if let Some(idx) = self.track_drag {
            if ev.pressed_button == Some(MouseButton::Left) {
                if let Some(v) = self.track_from_pos(idx, ev.position.x) {
                    self.apply_track(idx, v, cx);
                }
            }
        }
        // tab-config wheel pip drag: hue + saturation follow the cursor, keep `l`
        if let (Some(pip), Some(i)) = (self.tab_wheel_drag, self.tab_menu) {
            if ev.pressed_button == Some(MouseButton::Left) {
                let b = *self.tab_wheel_bounds.lock().unwrap();
                if let Some(b) = b {
                    let l = self.tab_pip_color(i, pip, cx).l;
                    if let Some(hex) = disk_color_at(b, ev.position.x, ev.position.y, l) {
                        self.tab_set_pip(pip, Some(hex), cx);
                    }
                }
            }
        }
        // tab-config lightness drag for the active pip
        if self.tab_light_drag && ev.pressed_button == Some(MouseButton::Left) {
            if let Some(i) = self.tab_menu {
                let b = *self.tab_light_bounds.lock().unwrap();
                if let Some(b) = b {
                    let frac = ((f32::from(ev.position.x) - f32::from(b.origin.x))
                        / f32::from(b.size.width).max(1.))
                    .clamp(0., 1.);
                    let c = self.tab_pip_color(i, self.tab_pip, cx);
                    self.tab_set_pip(
                        self.tab_pip,
                        Some(hsla_to_hex(hsla(c.h, c.s, frac, 1.))),
                        cx,
                    );
                }
            }
        }
    }

    fn on_mouse_up(&mut self, ev: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = false;
        if self.wheel_drag.take().is_some()
            || std::mem::take(&mut self.light_drag)
            || self.track_drag.take().is_some()
            || self.tab_wheel_drag.take().is_some()
            || std::mem::take(&mut self.tab_light_drag)
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
        // FOCUS text-size: live-only, never persisted, so just drop the latch.
        if std::mem::take(&mut self.focus_zoom_drag) {
            cx.notify();
            return;
        }
        // FOCUS selection: a real drag copies the selected text; a plain click
        // (anchor == head) just clears the selection without copying.
        if std::mem::take(&mut self.focus_sel_drag) {
            match self.focus_sel {
                Some((a, b)) if a != b => self.copy_focus_selection(cx),
                _ => self.focus_sel = None,
            }
            cx.notify();
            return;
        }
        // a group drag: drop the whole group into its resolved slot, or — if it
        // never engaged — treat the handle press as a click that folds the group.
        if let Some(drag) = self.group_drag.take() {
            let slot = self.tab_drop.take();
            if drag.engaged {
                if let Some(to) = slot {
                    self.move_group(drag.gid, to, cx);
                }
            } else {
                self.toggle_group_collapsed(drag.gid, cx);
            }
            cx.notify();
            return;
        }
        // a tab reorder: drop the grabbed tab into its resolved slot
        if let Some(drag) = self.tab_drag.take() {
            let slot = self.tab_drop.take();
            self.tab_drop_newrow = false;
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
        // Closing a pane is an EXPLICIT shrink — allow it past the guard.
        self.permit_shrink.set(true);
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
            .filter_map(|t| {
                // rebuild around the live subtree but CARRY the tab's identity —
                // name, colour overrides, and group membership all survive a reap
                t.root.reap(cx).map(|root| Tab {
                    root,
                    name: t.name,
                    focused: t.focused,
                    color: t.color,
                    text_color: t.text_color,
                    group: t.group,
                })
            })
            .collect();
        if self.pane_count() != had {
            if self.tabs.is_empty() {
                cx.quit();
                return;
            }
            // a reaped-away tab may have emptied a group
            self.prune_groups();
            self.active = self.active.min(self.tabs.len() - 1);
            self.focus_active(window, cx);
            // D: an auto-reap (a shell/agent exited) shrinks the live tree, but
            // we do NOT persist it here. Mark the session degraded so the 30s
            // checkpoint holds off — the rich on-disk session (with the now-dead
            // panes) survives until the user acts (resurrect via 🪦, or any real
            // structural change, which saves and clears `degraded`). A genuine
            // mass die-off is also caught by the richness guard in `save`.
            self.degraded.set(true);
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

    /// Flip the active OSD scope's Star-Wars text-crawl mode (per-pane via the
    /// grade group, exactly like a slider). On ⇒ the grid renders in the crawl
    /// font and the renderer perspective-warps the tube.
    fn toggle_crawl(&mut self, cx: &mut Context<Self>) {
        let Some(scope) = self.osd_menu.clone() else {
            return;
        };
        let mut grade = self.choice_for(&scope, cx).grade;
        grade.crawl = !grade.crawl;
        self.write_grade(&scope, grade, cx);
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
                    // Sizes (menu bar, terminal text) read as absolute "110%";
                    // crawl angle in degrees, crawl depth as a ratio; colour
                    // channels read as a signed offset ("-12", "+0").
                    .child(match key {
                        theme::GradeKey::Scale | theme::GradeKey::TextSize => {
                            format!("{}%", (v * 100.).round() as i32)
                        }
                        theme::GradeKey::CrawlAngle => format!("{}\u{00b0}", v.round() as i32),
                        theme::GradeKey::CrawlDepth => format!("{v:.1}\u{00d7}"),
                        _ => format!("{:+}", ((v - neutral) * 100.).round() as i32),
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
        disk_color_at(b, x, y, l)
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

    /// Ratio `0..1` for tracking slider `idx` at window-x `x`.
    fn track_from_pos(&self, idx: usize, x: Pixels) -> Option<f32> {
        let b = (*self.track_bounds[idx].lock().unwrap())?;
        let w = f32::from(b.size.width);
        if w <= 0.0 {
            return None;
        }
        Some(((f32::from(x) - f32::from(b.origin.x)) / w).clamp(0.0, 1.0))
    }

    /// The active OSD scope's resolved tracking dials — the seed a roll slider
    /// starts from when this scope hasn't pinned its own tracking yet.
    fn scope_track_seed(&self, scope: &MenuScope, cx: &App) -> [f32; 3] {
        let th = match scope {
            MenuScope::Pane(p) => p.read(cx).resolved_theme(cx),
            MenuScope::Outer => (*theme::resolve(cx, &theme::outer_choice(cx))).clone(),
        };
        theme::tracking_dials_of(&th)
    }

    /// Set tracking dial component `idx` on the active OSD scope (pane or outer),
    /// pinning the scope's grade tracking. Seeds the other two dials from the
    /// scope's current look so a drag starts where you are. Per-pane via grade.
    fn apply_track(&mut self, idx: usize, v: f32, cx: &mut Context<Self>) {
        let Some(scope) = self.osd_menu.clone() else {
            return;
        };
        let seed = self.scope_track_seed(&scope, cx);
        let mut grade = self.choice_for(&scope, cx).grade;
        let mut d = grade.tracking.unwrap_or(seed);
        d[idx] = v.clamp(0.0, 1.0);
        grade.tracking = Some(d);
        self.write_grade(&scope, grade, cx);
    }

    /// One tracking-band slider (idx 0=intensity, 1=speed, 2=size). Writes the
    /// active OSD scope's grade tracking (per-pane override + inherit, like the
    /// other DISPLAY channels); the fill starts from the scope's current look.
    fn track_slider(
        &self,
        idx: usize,
        label: &str,
        th: &theme::Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        const TRACK: f32 = 108.;
        let store = self.track_bounds[idx].clone();
        let frac = theme::tracking_dials_of(th)[idx].clamp(0.0, 1.0);
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
                                ws.apply_track(idx, v, cx);
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
                    move |bounds: Bounds<Pixels>, _, window, _| paint_hsv_disk(bounds, window),
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
    /// A consistent header icon button (≈2× glyphs), scaled by the menu-bar
    /// slider `s`. The caller adds the glyph child (an emoji via `.child("…")`
    /// or `pane::eq_icon`) and `on_mouse_down`.
    fn hicon_s(th: &theme::Theme, active: bool, s: f32) -> gpui::Div {
        div()
            .px(px(4. * s))
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
        Self::bezel_btn_s(th, label, active, 1.0)
    }

    /// `bezel_btn` scaled by the menu-bar slider `s` — padding + text grow with
    /// the bar so tabs and split/new-tab buttons resize together.
    fn bezel_btn_s(th: &theme::Theme, label: &str, active: bool, s: f32) -> gpui::Div {
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
            .px(px(8. * s))
            .py(px(2. * s))
            .rounded_sm()
            .border_1()
            .text_size(px(11. * s))
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

    /// Open the tab config pane for tab `i`, anchored at `at`. Defaults the wheel
    /// scope to THIS tab and the active pip to Fill.
    fn open_tab_menu(&mut self, i: usize, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.tab_menu = Some(i);
        self.tab_menu_at = Some(at);
        self.group_menu = None;
        self.tab_drag = None;
        self.tab_scope = TabScope::ThisTab;
        self.tab_pip = TabPip::Fill;
        cx.notify();
    }

    /// Open the group's own config menu (right-click its handle / collapsed pill).
    /// The group is a first-class tabby thing: its colour, name, fold, and
    /// disband all live here — never on a member tab's menu. We point the shared
    /// colour wheel at a representative member with Group scope, so the existing
    /// wheel/lightness/swatch machinery edits the group's colours directly.
    fn open_group_menu(&mut self, gid: u32, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.group_menu = Some(gid);
        self.group_menu_at = Some(at);
        self.tab_menu = self.tabs.iter().position(|t| t.group == Some(gid));
        self.tab_scope = TabScope::Group;
        self.tab_pip = TabPip::Fill;
        self.tab_drag = None;
        cx.notify();
    }

    /// Disband a group: clear every member's membership, prune the now-empty
    /// group, and close the menu.
    fn ungroup(&mut self, gid: u32, cx: &mut Context<Self>) {
        for t in self.tabs.iter_mut() {
            if t.group == Some(gid) {
                t.group = None;
            }
        }
        self.prune_groups();
        self.group_menu = None;
        self.tab_menu = None;
        self.tab_scope = TabScope::ThisTab;
        self.save(cx);
        cx.notify();
    }

    /// One mother-bar tab button (or its inline rename box). Tinted by the tab's
    /// resolved fill/text (own override → group lead → bezel default). Right-click
    /// or ctrl+click opens the tab config pane; ✎ / double-click rename.
    fn tab_button(&self, i: usize, cx: &mut Context<Self>) -> gpui::Div {
        let th = theme::theme(cx);
        // tabs ride the menu-bar slider: everything in the tab scales with the bar
        let s = theme::outer_choice(cx).grade.scale;
        let is_active = i == self.active;
        // the ACTIVE tab reads 20% bigger and lifts up out of the strip so the
        // current tab is unmistakable at a glance.
        let ts = if is_active { s * 1.2 } else { s };
        // the inline rename box owns this slot while renaming — full text editor
        // (caret + selection) so arrows / ctrl+arrows / shift navigation all work
        if let Some((_, eb)) = self.renaming.as_ref().filter(|(ri, _)| *ri == i) {
            return div()
                .px(px(8. * s))
                .py(px(2. * s))
                .rounded_sm()
                .border_1()
                .border_color(th.accent)
                .bg(darken(th.bg, 0.8))
                .text_size(px(11. * s))
                .text_color(th.text)
                .flex()
                .flex_row()
                .items_center()
                // clicking the edit box itself keeps editing (don't bubble to the
                // root's commit-on-click-off handler)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(render_edit_buffer(
                    eb,
                    s,
                    th.text,
                    th.cursor,
                    th.accent.alpha(0.4),
                ));
        }
        let label = self.tabs[i]
            .name
            .clone()
            .unwrap_or_else(|| format!("{}", i + 1));
        let (fill, text) = self.resolved_tab_colors(i);
        // the per-tab close affordance — an X in the tab's own frame
        let close_x = div()
            .px(px(4. * ts))
            .text_size(px(12. * ts))
            .text_color(text.unwrap_or(if is_active { th.text } else { th.faint }))
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
        let pencil_col = text.unwrap_or(th.text).alpha(0.8);
        // hover-revealed ✎ affordance: invites the rename without a word
        let pencil = div()
            .id(SharedString::from(format!("tab-pencil-{i}")))
            .text_size(px(10. * ts))
            .text_color(hsla(0., 0., 0., 0.)) // hidden until the tab is hovered
            .group_hover(tab_grp.clone(), move |s| s.text_color(pencil_col))
            .cursor_pointer()
            .child("✎")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let seed = ws.tabs[i].name.clone().unwrap_or_default();
                    ws.renaming = Some((i, EditBuffer::seeded(&seed)));
                    window.focus(&ws.focus_handle, cx);
                    cx.notify();
                }),
            );
        // tint to the resolved fill (tab override or group lead); the resolved
        // text colour rides over the bezel's default label colour
        let mut btn = Self::bezel_btn_s(&th, &label, is_active, ts);
        if let Some(c) = fill {
            btn = btn
                .bg(linear_gradient(
                    135.,
                    linear_color_stop(brighten(c, 1.35), 0.),
                    linear_color_stop(darken(c, 0.6), 1.),
                ))
                .border_color(if is_active { th.accent } else { c });
        }
        if let Some(tc) = text {
            btn = btn.text_color(tc);
        }
        let store = self.tab_bounds.clone();
        let drop_hi =
            matches!(&self.drop_target, Some(DropTarget::Tab { index, .. }) if *index == i);
        btn.group(tab_grp)
            .relative()
            // lift the active tab up out of the strip a touch
            .when(is_active, |d| d.mt(px(-4. * s)))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4. * ts))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, window, cx| {
                    // don't let the click bubble to the root's focus handle, which
                    // would steal focus from the pane
                    cx.stop_propagation();
                    if ev.modifiers.control {
                        // ctrl+click → open this tab's config pane
                        ws.open_tab_menu(i, ev.position, cx);
                    } else if ev.click_count >= 2 {
                        // double-click to rename (the file-manager gesture)
                        let seed = ws.tabs[i].name.clone().unwrap_or_default();
                        ws.renaming = Some((i, EditBuffer::seeded(&seed)));
                        window.focus(&ws.focus_handle, cx);
                        cx.notify();
                    } else {
                        // select now; arm a reorder drag that engages only if the
                        // cursor travels far enough (else it stays a plain click)
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
                cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                    // right-click → open this tab's config pane (rename + colour +
                    // group), like the other configuration trays
                    cx.stop_propagation();
                    ws.open_tab_menu(i, ev.position, cx);
                }),
            )
            .child(
                // measure this tab button's box for "drop onto a tab"
                div().absolute().inset_0().child(
                    canvas(
                        move |bounds, _, _| {
                            store.lock().unwrap().insert(i, bounds);
                        },
                        |_, _, _, _| {},
                    )
                    .size_full(),
                ),
            )
            .when(drop_hi, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .rounded_sm()
                        .border_1()
                        .border_color(th.accent)
                        .bg(th.accent.alpha(0.25)),
                )
            })
            // 🔔 badge: an agent run finished in a pane of this tab and hasn't been
            // acknowledged yet (cleared by the in-terminal ack click).
            .when(self.tab_has_bell(i, cx), |d| {
                d.child(div().text_size(px(11. * s)).child("🔔"))
            })
            .child(pencil)
            .child(close_x)
    }

    /// The handle at the left of an expanded group's band: shows the group name
    /// in its colour; click folds the group.
    fn group_chip(&self, gid: u32, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let th = theme::theme(cx);
        let s = theme::outer_choice(cx).grade.scale;
        let g = self.groups.iter().find(|g| g.id == gid);
        let color = g.map(|g| g.color).unwrap_or(th.accent);
        let name = g.and_then(|g| g.name.clone());
        let glyph_col = if color.l > 0.55 {
            hsla(0., 0., 0.08, 0.95)
        } else {
            white()
        };
        // double-click the chip to rename the group inline
        let mut chip = div()
            .id(SharedString::from(format!("grp-chip-{gid}")))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4. * s))
            .px(px(4. * s))
            .h(px(20. * s))
            // square the bottom so the handle sits flush ON the group's colour
            // rail (rounded top only) — the "parent tab" touching the bar
            .rounded_t_md()
            .bg(color)
            .cursor_pointer()
            .text_size(px(9. * s))
            .font_weight(gpui::FontWeight::EXTRA_BOLD)
            .text_color(glyph_col)
            .child("▾");
        if let Some((_, eb)) = self.group_rename.as_ref().filter(|(rg, _)| *rg == gid) {
            chip = chip.child(render_edit_buffer(
                eb,
                s,
                glyph_col,
                glyph_col,
                glyph_col.alpha(0.35),
            ));
        } else if let Some(n) = name {
            chip = chip.child(n);
        }
        chip.on_mouse_down(
            MouseButton::Left,
            cx.listener(move |ws, ev: &MouseDownEvent, _window, cx| {
                cx.stop_propagation();
                // arm a group drag: a release without travel folds the group, a
                // release after travel reorders the whole group (see on_mouse_up).
                ws.group_drag = Some(GroupDrag {
                    gid,
                    start: ev.position,
                    at: ev.position,
                    engaged: false,
                });
                ws.tab_drop = None;
                cx.notify();
            }),
        )
        // right-click the handle → the group's own config menu (rename, colour,
        // fold, disband). Group properties live on the group, not its members.
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                cx.stop_propagation();
                ws.open_group_menu(gid, ev.position, cx);
            }),
        )
    }

    /// A collapsed group folded into one counted pill; click expands.
    fn group_pill(
        &self,
        gid: u32,
        count: usize,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let th = theme::theme(cx);
        let g = self.groups.iter().find(|g| g.id == gid);
        let color = g.map(|g| g.color).unwrap_or(th.accent);
        let name = g
            .and_then(|g| g.name.clone())
            .unwrap_or_else(|| "group".into());
        let glyph_col = if color.l > 0.55 {
            hsla(0., 0., 0.08, 0.95)
        } else {
            white()
        };
        let s = theme::outer_choice(cx).grade.scale;
        div()
            .id(SharedString::from(format!("grp-pill-{gid}")))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4. * s))
            .px(px(8. * s))
            .py(px(2. * s))
            .rounded_sm()
            .border_1()
            .border_color(color)
            .bg(linear_gradient(
                135.,
                linear_color_stop(brighten(color, 1.2), 0.),
                linear_color_stop(darken(color, 0.6), 1.),
            ))
            .cursor_pointer()
            .text_size(px(11. * s))
            .font_weight(gpui::FontWeight::EXTRA_BOLD)
            .text_color(glyph_col)
            .child("▸")
            .child(name)
            .child(format!("{count}"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    // arm a group drag; a release without travel expands the pill.
                    ws.group_drag = Some(GroupDrag {
                        gid,
                        start: ev.position,
                        at: ev.position,
                        engaged: false,
                    });
                    ws.tab_drop = None;
                    cx.notify();
                }),
            )
            // right-click a folded group → its config menu, same as the handle.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.open_group_menu(gid, ev.position, cx);
                }),
            )
    }

    /// The tab-config wheel: the same HSV disk as the theme breakout, carrying
    /// just TWO pips — ▣ Fill + T Text — scoped to one tab (or its group).
    fn tab_color_wheel(&self, i: usize, cx: &mut Context<Self>) -> gpui::Div {
        const D: f32 = 120.0;
        let r = D / 2.0;
        let store = self.tab_wheel_bounds.clone();
        let markers = self.tab_pip_colors(i, cx);
        let active = self.tab_pip;
        let mut wheel = div()
            .w(px(D))
            .h(px(D))
            .relative()
            .rounded_full()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    let Some(i) = ws.tab_menu else { return };
                    let Some(b) = *ws.tab_wheel_bounds.lock().unwrap() else {
                        return;
                    };
                    let cols: Vec<Hsla> = ws
                        .tab_pip_colors(i, cx)
                        .iter()
                        .map(|(_, _, c)| *c)
                        .collect();
                    if let Some(idx) = disk_grab(b, ev.position.x, ev.position.y, &cols) {
                        let pip = ws.tab_pip_colors(i, cx)[idx].0;
                        ws.tab_pip = pip;
                        ws.tab_wheel_drag = Some(pip);
                        if let Some(hex) =
                            disk_color_at(b, ev.position.x, ev.position.y, cols[idx].l)
                        {
                            ws.tab_set_pip(pip, Some(hex), cx);
                        }
                    }
                }),
            )
            .child(
                canvas(
                    move |bounds, _, _| {
                        *store.lock().unwrap() = Some(bounds);
                    },
                    move |bounds: Bounds<Pixels>, _, window, _| paint_hsv_disk(bounds, window),
                )
                .size_full(),
            );
        // paint the active pip last so it sits on top of a pile (greyscale stack)
        let mut ordered: Vec<_> = markers.into_iter().collect();
        ordered.sort_by_key(|(t, _, _)| *t == active);
        for (t, glyph, c) in ordered {
            let ang = c.h.rem_euclid(1.0) * std::f32::consts::TAU;
            let sat = c.s.clamp(0.0, 1.0);
            let (dx, dy) = (r + ang.cos() * sat * r, r + ang.sin() * sat * r);
            let glyph_col = if c.l > 0.55 {
                hsla(0., 0., 0.08, 0.95)
            } else {
                white()
            };
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

    /// Lightness slider for the active tab pip (dark→light ramp of its hue).
    fn tab_lightness_bar(&self, i: usize, cx: &mut Context<Self>) -> gpui::Div {
        const W: f32 = 120.0;
        let store = self.tab_light_bounds.clone();
        let c = self.tab_pip_color(i, self.tab_pip, cx);
        let l = c.l.clamp(0.0, 1.0);
        div()
            .w(px(W))
            .h(px(14.))
            .relative()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, ev: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.tab_light_drag = true;
                    let Some(i) = ws.tab_menu else { return };
                    let Some(b) = *ws.tab_light_bounds.lock().unwrap() else {
                        return;
                    };
                    let frac = ((f32::from(ev.position.x) - f32::from(b.origin.x))
                        / f32::from(b.size.width).max(1.))
                    .clamp(0., 1.);
                    let cc = ws.tab_pip_color(i, ws.tab_pip, cx);
                    ws.tab_set_pip(
                        ws.tab_pip,
                        Some(hsla_to_hex(hsla(cc.h, cc.s, frac, 1.))),
                        cx,
                    );
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

fn agent_state_glow(th: &theme::Theme, idle: Hsla, state: hud::AgentState) -> Hsla {
    match state {
        hud::AgentState::Working => th.accent,
        hud::AgentState::Blocked => hsla(0.11, 0.85, 0.60, 1.),
        hud::AgentState::Error => hsla(0., 0.75, 0.60, 1.),
        hud::AgentState::Finished => th.complement,
        hud::AgentState::Idle => idle,
    }
}

fn agent_program_glow(fallback: Hsla, label: &str) -> Hsla {
    match label {
        "CLAUDE" => hsla(0.105, 0.9, 0.55, 1.),
        "CODEX" => hsla(0.52, 0.8, 0.6, 1.),
        // Warm matrix green: green/yellow bias, deliberately away from the
        // blue-green Codex lane.
        "SHELL" => hsla(0.285, 0.82, 0.58, 1.),
        _ => fallback,
    }
}

/// Raised row chrome for the Agent Wall and graveyard. It is deliberately the
/// same grammar as focused terminal panes and floating panels: a crisp phosphor
/// rim, soft bloom, downward cast shadow, and tight contact shadow. `live=false`
/// keeps the physical lift but turns the phosphor mostly off for idle/dead rows.
fn agent_card_shadows(glow: Hsla, live: bool) -> Vec<BoxShadow> {
    let (rim, halo, cast, contact) = if live {
        (0.82, 0.38, 0.48, 0.42)
    } else {
        (0.26, 0.10, 0.34, 0.32)
    };
    vec![
        BoxShadow {
            color: glow.alpha(rim),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(1.),
            inset: false,
        },
        BoxShadow {
            color: glow.alpha(halo),
            offset: point(px(0.), px(1.)),
            blur_radius: px(if live { 18. } else { 10. }),
            spread_radius: px(if live { 1. } else { 0. }),
            inset: false,
        },
        BoxShadow {
            color: hsla(0., 0., 0., cast),
            offset: point(px(0.), px(12.)),
            blur_radius: px(28.),
            spread_radius: px(-8.),
            inset: false,
        },
        BoxShadow {
            color: hsla(0., 0., 0., contact),
            offset: point(px(0.), px(3.)),
            blur_radius: px(8.),
            spread_radius: px(-2.),
            inset: false,
        },
    ]
}

/// Shadow stack for any surface that floats ABOVE the workspace (modals + menus).
/// It mirrors the focused pane's lit phosphor border — a crisp accent ring plus a
/// soft accent halo that reads as a *light source* — then casts a broad, dark
/// shadow straight DOWN onto the panes, so the surface clearly hovers and the
/// workspace beneath it sits in shadow. `glow` is the rim colour (the theme
/// accent; a danger hue for destructive dialogs). Pair with `.border_2()` in the
/// same hue, and `.rounded_*()` so the glow follows the corners.
fn float_shadows(glow: Hsla) -> Vec<BoxShadow> {
    vec![
        // the doubled border: a crisp 1px outer ring in the rim colour
        BoxShadow {
            color: glow.alpha(0.9),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(1.),
            inset: false,
        },
        // the lit border casting light: a soft phosphor glow around the rim
        BoxShadow {
            color: glow.alpha(0.5),
            offset: point(px(0.), px(1.)),
            blur_radius: px(22.),
            spread_radius: px(2.),
            inset: false,
        },
        // the shadow that light casts on the workspace below — broad, dark, downward
        BoxShadow {
            color: hsla(0., 0., 0., 0.62),
            offset: point(px(0.), px(30.)),
            blur_radius: px(64.),
            spread_radius: px(-8.),
            inset: false,
        },
        // a tighter contact shadow for crisp separation from the panes
        BoxShadow {
            color: hsla(0., 0., 0., 0.5),
            offset: point(px(0.), px(8.)),
            blur_radius: px(22.),
            spread_radius: px(-2.),
            inset: false,
        },
    ]
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

/// Paint a canvas-rendered HSV disk into `bounds` (hue = angle, saturation =
/// radius, fixed mid lightness). Shared by the theme breakout wheel and the
/// tab-config wheel so the disk looks identical in both.
fn paint_hsv_disk(bounds: Bounds<Pixels>, window: &mut Window) {
    let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
    let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
    let rad = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 2.0;
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
                    Bounds::new(point(px(xx), px(yy)), size(px(cell + 0.6), px(cell + 0.6))),
                    hsla(hue, sat, 0.55, 1.0),
                ));
            }
            xx += cell;
        }
        yy += cell;
    }
}

/// Map a point in a wheel `bounds` to a `#rrggbb` at lightness `l` (polar:
/// angle → hue, radius → saturation). The inverse of where a marker is painted.
fn disk_color_at(bounds: Bounds<Pixels>, x: Pixels, y: Pixels, l: f32) -> Option<String> {
    let cx = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
    let cy = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
    let rad = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 2.0;
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

/// Index of the marker `colors[i]` whose painted position in `bounds` is nearest
/// the press at `(x, y)` — i.e. which pip the user grabbed. `None` if degenerate.
fn disk_grab(bounds: Bounds<Pixels>, x: Pixels, y: Pixels, colors: &[Hsla]) -> Option<usize> {
    let rad = f32::from(bounds.size.width).min(f32::from(bounds.size.height)) / 2.0;
    if rad <= 0.0 {
        return None;
    }
    let cx0 = f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0;
    let cy0 = f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0;
    let (px_, py_) = (f32::from(x), f32::from(y));
    colors
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let ang = c.h.rem_euclid(1.0) * std::f32::consts::TAU;
            let sat = c.s.clamp(0.0, 1.0);
            let mx = cx0 + ang.cos() * sat * rad;
            let my = cy0 + ang.sin() * sat * rad;
            (i, (mx - px_).powi(2) + (my - py_).powi(2))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
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
                // The focused pane has to read as forward even when its accent
                // border matches the theme (orange border on an orange cabinet is
                // invisible). gpui has no CSS scale/z-index, so we get the same
                // "bigger + raised" effect geometrically: INACTIVE panes recess a
                // few px and dim, leaving the focused one flush, full-bright, and
                // lifted by its drop shadow — colour-independent, always legible.
                .when(!is_focused, |d| d.m(px(6.)))
                .overflow_hidden()
                .rounded_md()
                .border_1()
                .border_color(if is_focused { acc } else { th.faint })
                // highlighted pane gets a 2x-thick accent border (drawn inside the
                // flex box, so the grid never reflows — only inner content shifts 1px)
                .when(is_focused, |d| d.border_2())
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
                        // lifted drop shadow: the downward offset is what makes the
                        // pane read as raised off the surface, not just outlined.
                        BoxShadow {
                            color: hsla(0., 0., 0., 0.5),
                            offset: point(px(0.), px(7.)),
                            blur_radius: px(26.),
                            spread_radius: px(1.),
                            inset: false,
                        },
                        // soft accent halo around the live tube
                        BoxShadow {
                            color: acc.alpha(0.55),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(18.),
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
                // dim every pane that isn't focused so the live one is the bright
                // one. Plain bg overlay (no .occlude()) → a click still falls
                // through to the terminal beneath to focus it.
                .when(!is_focused, |d| {
                    d.child(
                        div()
                            .absolute()
                            .inset_0()
                            .rounded_md()
                            .bg(hsla(0., 0., 0., 0.22)),
                    )
                })
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

// Capture/demo hook: TD_SAVINGS_DEMO opens the </> LeanCTX savings overlay once
// with fictional data so it can be screenshotted leak-free. Inert unless set.
static SAVINGS_DEMO_ARMED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// Capture/demo hook: TD_GRAVEYARD_DEMO opens the graveyard once with FICTIONAL
// entries so the dead-agent cards can be screenshotted leak-free.
static GRAVEYARD_DEMO_ARMED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

// When the FOCUS modal opened — drives a ~220ms ease-in of the dim + frosted
// blur so the backdrop melts behind the panel instead of snapping. Set lazily
// in render while the modal is open, cleared when it closes.
static FOCUS_OPEN_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reap(window, cx);
        // Publish the active UI language so panes can localise their own chrome.
        lang::set_current(self.lang);
        let s = self.lang.strings();
        warp::begin_frame(); // visible panes re-register their tube rects below
                             // An open overlay (theme breakout / confirm dialog) flattens the glass:
                             // the warp is a pixel post-process, so a panel over a tube would bow out
                             // of reach of its own flat hit box. Suppress so the menu reads true.
        warp::set_suppressed(
            self.theme_menu.is_some()
                || self.osd_menu.is_some()
                || self.mcp_menu
                || self.plugins_menu
                || self.savings_menu
                || self.dead_menu
                || self.group_menu.is_some()
                || self.confirm_close.is_some()
                || self.help_open
                || self.tab_menu.is_some()
                || self.find.is_some()
                || self.lang_picker.is_some()
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
        // demo/capture hook (TD_CONFIRM_DEMO): auto-open the close-tab confirmation
        // once, so the serious dialog can be screenshotted without a keystroke.
        if std::env::var("TD_CONFIRM_DEMO").is_ok() && self.confirm_close.is_none() {
            self.confirm_close = Some(self.active);
            cx.notify();
        }
        // demo/capture hook (TD_SAVINGS_DEMO): open the </> LeanCTX savings overlay
        // with FICTIONAL data (never the real ~/.lean-ctx ledger), so the surface
        // can be screenshotted for the lean-ctx issue without leaking real agent
        // ids or file paths. Fires once.
        if std::env::var("TD_SAVINGS_DEMO").is_ok()
            && !SAVINGS_DEMO_ARMED.load(std::sync::atomic::Ordering::Relaxed)
        {
            SAVINGS_DEMO_ARMED.store(true, std::sync::atomic::Ordering::Relaxed);
            self.mcp_menu = true;
            self.savings_view = Some(SavingsView {
                tokens_saved: 64_600_000,
                gain_pct: 82.0,
                usd: 161.0,
                score: 70,
                level: "Lv4 Guardian".into(),
                agent_count: 36,
                agents: vec![
                    SavingsAgent { id: "agent-aurora-01".into(), kind: "claude-code".into(), calls: 172, saved_est: 8_910_000, last_seen: "2026-06-22T07:20".into() },
                    SavingsAgent { id: "agent-borealis-02".into(), kind: "claude-code".into(), calls: 141, saved_est: 6_240_000, last_seen: "2026-06-22T07:18".into() },
                    SavingsAgent { id: "agent-cinder-03".into(), kind: "codex-mcp-client".into(), calls: 98, saved_est: 4_010_000, last_seen: "2026-06-22T07:12".into() },
                    SavingsAgent { id: "agent-delta-04".into(), kind: "claude-code".into(), calls: 64, saved_est: 2_770_000, last_seen: "2026-06-22T07:05".into() },
                    SavingsAgent { id: "agent-ember-05".into(), kind: "codex-mcp-client".into(), calls: 41, saved_est: 1_540_000, last_seen: "2026-06-22T06:58".into() },
                    SavingsAgent { id: "agent-flux-06".into(), kind: "claude-code".into(), calls: 22, saved_est: 760_000, last_seen: "2026-06-22T06:40".into() },
                ],
                top_files: vec![
                    SavingsFile { path: "src/main.rs".into(), saved: 10_131_759, pct: 98.2 },
                    SavingsFile { path: "src/app.tsx".into(), saved: 5_163_134, pct: 96.9 },
                    SavingsFile { path: "schema.sql".into(), saved: 2_440_000, pct: 95.1 },
                ],
                note: "context saved by lean-ctx (compression on lean-ctx-touched traffic, not your full provider bill) \u{00b7} per-agent \u{2248} estimated until lean-ctx stamps agent_id into the savings ledger".into(),
            });
            self.savings_status = None;
            self.savings_menu = true;
            eprintln!("terminal-delight: TD_SAVINGS_DEMO — auto-opening </> savings overlay (fictional data)");
            cx.notify();
        }
        // demo/capture hook (TD_GRAVEYARD_DEMO): open the dead-agent manifest with
        // fictional cards (the override lives in the overlay). Fires once.
        if std::env::var("TD_GRAVEYARD_DEMO").is_ok()
            && !GRAVEYARD_DEMO_ARMED.load(std::sync::atomic::Ordering::Relaxed)
        {
            GRAVEYARD_DEMO_ARMED.store(true, std::sync::atomic::Ordering::Relaxed);
            self.dead_menu = true;
            eprintln!("terminal-delight: TD_GRAVEYARD_DEMO — auto-opening graveyard (fictional data)");
            cx.notify();
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

        // ---- tabs + browser-style group bands ----
        // Adjacent tabs sharing a group render under one coloured rail with a
        // handle chip; a collapsed group folds into a counted pill (unless it
        // holds the active tab, which force-expands so you never lose your place).
        // Tabs never own more than 55% of the bar — past that they WRAP onto a
        // fresh row, so the split/window controls on the right are always kept.
        let tabs_max_w = px((f32::from(wb.size.width) * 0.55).max(120.));
        let mut tab_strip = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(4. * scale))
            .items_center()
            // min_w_0 lets the strip shrink BELOW its content so it wraps to extra
            // rows (under tab 1) instead of overrunning into the controls; max_w
            // still caps it at 55% on wide windows.
            .min_w_0()
            .max_w(tabs_max_w);
        // while a tab is being dragged, an accent bar marks the slot it'd land in
        let dragging_tab = self.tab_drag.as_ref().is_some_and(|d| d.engaged)
            || self.group_drag.as_ref().is_some_and(|d| d.engaged);
        let drop_slot = self.tab_drop;
        let new_row_drop = dragging_tab && self.tab_drop_newrow;
        // the between-tabs insertion caret — bold + glowing so it can't be missed
        let drop_marker = || {
            div()
                .w(px(4. * scale))
                .h(px(22. * scale))
                .rounded_full()
                .bg(th.accent)
                .shadow(vec![BoxShadow {
                    color: th.accent.alpha(0.9),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(6. * scale),
                    spread_radius: px(1.),
                    inset: false,
                }])
        };
        let active_group = self.tabs.get(self.active).and_then(|t| t.group);
        let mut i = 0;
        while i < tab_count {
            if dragging_tab && drop_slot == Some(i) {
                tab_strip = tab_strip.child(drop_marker());
            }
            if let Some(g) = self.tabs[i]
                .group
                .filter(|g| self.group_index(*g).is_some())
            {
                // a maximal run [i, j) of adjacent tabs in this group
                let mut j = i;
                while j < tab_count && self.tabs[j].group == Some(g) {
                    j += 1;
                }
                let grp = &self.groups[self.group_index(g).unwrap()];
                let color = grp.color;
                // force-expand the run that holds the active tab
                let collapsed = grp.collapsed && active_group != Some(g);
                if collapsed {
                    tab_strip = tab_strip.child(self.group_pill(g, j - i, cx));
                } else {
                    // The group reads as ONE bound unit: the handle + members all
                    // rest on a shared colour rail (items_end sits everything on the
                    // bar), and extra horizontal margin sets the group apart from
                    // its neighbours — ungrouped tabs keep their tight 4px gap.
                    let mut band = div()
                        .relative()
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap(px(4. * scale))
                        .mx(px(6. * scale))
                        .pb(px(4. * scale))
                        .child(self.group_chip(g, cx));
                    for k in i..j {
                        band = band.child(self.tab_button(k, cx));
                    }
                    // the colour rail the whole run rests on — the handle's square
                    // bottom meets it, so the chip and its tabs read as linked
                    band = band.child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            .h(px(4. * scale))
                            .rounded_b_md()
                            .bg(color),
                    );
                    tab_strip = tab_strip.child(band);
                }
                i = j;
                continue;
            }
            tab_strip = tab_strip.child(self.tab_button(i, cx));
            i += 1;
        }
        // the end caret only when NOT a new-row drop (that gets the wide bar below)
        if dragging_tab && drop_slot == Some(tab_count) && !new_row_drop {
            tab_strip = tab_strip.child(drop_marker());
        }
        tab_strip = tab_strip.child(Self::bezel_btn_s(&th, "+", false, scale).on_mouse_down(
            MouseButton::Left,
            cx.listener(|ws, _: &MouseDownEvent, window, cx| ws.new_tab(window, cx)),
        ));
        // dragging a tab past the bottom of the strip → a VERY obvious full-width
        // bar wraps onto its own line: "drop here to start a new row".
        if new_row_drop {
            tab_strip = tab_strip.child(
                div()
                    .w_full()
                    .h(px(6. * scale))
                    .mt(px(3. * scale))
                    .rounded_full()
                    .bg(th.accent)
                    .shadow(vec![BoxShadow {
                        color: th.accent.alpha(0.9),
                        offset: point(px(0.), px(0.)),
                        blur_radius: px(8. * scale),
                        spread_radius: px(1.),
                        inset: false,
                    }]),
            );
        }

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
            .child(
                div()
                    .text_size(px(9. * scale))
                    .text_color(th.text)
                    .child("▭"),
            )
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
            .child(
                div()
                    .text_size(px(15. * scale))
                    .text_color(th.text)
                    .child("▭"),
            )
            .child(
                div()
                    .text_size(px(10. * scale))
                    .text_color(th.accent)
                    .child(format!("{}%", (scale * 100.).round() as i32)),
            );

        // The split buttons read as primary actions: taller (matched to the
        // window-control buttons), roomier, larger glyph+label — so they never
        // get crowded off the bar or look like an afterthought.
        let split_btn = |label: &str| {
            Self::bezel_btn_s(&th, label, false, scale)
                .h(px(26. * scale))
                .px(px(10. * scale))
                .py(px(0.))
                .text_size(px(13. * scale))
                .flex()
                .items_center()
                .justify_center()
        };
        let cluster = div()
            .flex()
            .flex_row()
            .gap(px(5. * scale))
            .items_center()
            .child(split_btn(&format!("◧ {}", s.ch_split)).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                    ws.split(SplitDir::Row, window, cx)
                }),
            ))
            .child(split_btn(&format!("⬓ {}", s.ch_split)).on_mouse_down(
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
                .w(px(20. * scale))
                .h(px(20. * scale))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .text_size(px(12. * scale))
                .text_color(th.text.alpha(0.7))
                .cursor_pointer()
                .hover(move |s| s.bg(hover.alpha(0.9)).text_color(white()))
                .child(glyph)
        };
        let win_controls = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(4. * scale))
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
                    cx.listener(|ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        // snapshot the LIVE state (current agent sessions + cwds)
                        // before the window dies, so a clean quit reopens exactly
                        // where we left off instead of the last periodic checkpoint.
                        ws.save(cx);
                        window.remove_window();
                    }),
                ))
            });

        let bezel_top = div()
            // min-height (not fixed): wrapped tab rows grow the bar downward.
            .min_h(px(43. * scale))
            .flex_none()
            .flex()
            .flex_row()
            // top-align: the tab strip wraps INTERNALLY (overflow tabs stack under
            // tab 1) and grows DOWNWARD, while the title + controls stay pinned on
            // the top line — the first tab row never moves.
            .items_start()
            .justify_between()
            .px(px(12. * scale))
            .py(px(7. * scale))
            .gap(px(12. * scale))
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
                // LEFT GROUP: title + // SUB-TERMINAL + the tab strip, all INLINE
                // on the top line. There is NO flex_wrap here, so the strip never
                // drops below the title as a block; it wraps INTERNALLY (its own
                // 55% cap) so overflow tabs stack UNDER tab 1. items_start keeps
                // the title on the top line when the strip grows to several rows.
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(px(8. * scale))
                    .child(
                        // title + // SUB-TERMINAL, vertically centred against the
                        // first tab row via a fixed height so they don't ride down
                        // when the tab strip wraps to extra rows.
                        div()
                            .flex_none()
                            .h(px(22. * scale))
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8. * scale))
                            .child(
                                // The mother TITLE — the complement colour (wheel's
                                // `C`; defaults to the accent's / active dynamic's).
                                div()
                                    .flex_none()
                                    .text_size(px(14. * scale))
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .text_color(th.complement)
                                    .child(format!("▸ {}", s.brand)),
                            )
                            .child(
                                // Decoration only — stays a dim foreground tint.
                                div()
                                    .text_size(px(9. * scale))
                                    .text_color(th.text.alpha(0.4))
                                    .child(format!("// {}", s.ch_sub_terminal)),
                            ),
                    )
                    .child(tab_strip),
            )
            .child(
                // never compressed or pushed off — the controls are always kept
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12. * scale))
                    .child(
                        // outer theme: a consistent 🎨 (trigger for the breakout)
                        Self::hicon_s(&th, self.theme_menu.is_some(), scale)
                            .text_size(px(pane::HICON * scale))
                            .line_height(px(pane::HICON * scale))
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
                        // The whole mother bar scales with the menu-bar slider.
                        Self::hicon_s(&th, self.osd_menu.is_some(), scale)
                            .flex()
                            .items_center()
                            .child(pane::eq_icon(th.accent, scale))
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
                        // MCP: a drawn robot — opens the read-only agent-watch
                        // control surface. Lights up when the panel is open OR
                        // the server policy is currently enabled.
                        Self::hicon_s(&th, self.mcp_menu || self.mcp.enabled, scale)
                            .flex()
                            .items_center()
                            .child(pane::robot_icon(th.accent, scale))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.mcp_menu = true;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        // 👻 recover: the dead-agent manifest — resurrect a
                        // closed agent (Claude/Codex) from its saved session.
                        Self::hicon_s(&th, self.dead_menu, scale)
                            .text_size(px(pane::HICON * scale))
                            .line_height(px(pane::HICON * scale))
                            .child("\u{1faa6}")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.dead_filter = None;
                                    ws.dead_menu = true;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        // 🧩 plugins: the MCP plugin host — reach for plugins
                        // (context-delight harvest, …) that act on the agents.
                        Self::hicon_s(&th, self.plugins_menu, scale)
                            .text_size(px(pane::HICON * scale))
                            .line_height(px(pane::HICON * scale))
                            .child("\u{1f9e9}")
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.plugins_menu = true;
                                    cx.notify();
                                }),
                            ),
                    )
                    .child(
                        // help: keys + commands reference, themed by the outer
                        Self::hicon_s(&th, self.help_open, scale)
                            .text_size(px(pane::HICON * scale))
                            .line_height(px(pane::HICON * scale))
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
            .h(px(22. * scale))
            .flex_none()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(12. * scale))
            .text_size(px(10.5 * scale))
            .text_color(th.text)
            .child(div().child(format!("🎨 · {}", focused_title)))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8. * scale))
                    .items_center()
                    .child(format!(
                        "{} {} · {} {}",
                        tab_count,
                        if tab_count == 1 { s.st_tab } else { s.st_tabs },
                        pane_count,
                        if pane_count == 1 {
                            s.st_pane
                        } else {
                            s.st_panes
                        }
                    ))
                    .child(
                        div()
                            .text_color(th.accent)
                            .child(format!("● {}", s.ch_ready)),
                    ),
            );

        // ---- theme breakout: icon grid + seed swatches, per scope ----
        let menu_overlay = self.theme_menu.clone().map(|scope| {
            let is_pane = matches!(scope, MenuScope::Pane(_));
            let cur = self.menu_choice(cx);
            let t = self.lang.strings();
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
                let tlbl = match lbl.as_str() {
                    "quiet" => t.thm_quiet,
                    "field" => t.thm_field,
                    "tactical" => t.thm_tactical,
                    "hacker" => t.thm_hacker,
                    "gamba" => t.thm_gamba,
                    "custom" => t.thm_custom,
                    _ => lbl.as_str(),
                };
                let btn = theme_icon_btn(&th, &icon, tlbl, active)
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
                let cap = match mode {
                    theme::ColorMode::Default => t.cm_ansi,
                    theme::ColorMode::Monochrome => t.cm_mono,
                    theme::ColorMode::OnTheme => t.cm_theme,
                };
                color_row =
                    color_row.child(color_mode_btn(&th, mode.icon(), cap, active).on_mouse_down(
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
                    ));
            }
            // SYNTAX scheme: off, or one of the highlight grammars (code / agentic
            // / logs / markdown). On = recolour default-fg text by the scheme's
            // grammar; PROGRAM COLOUR (above) decides HOW those roles are coloured,
            // and program ANSI still passes through the chosen source mode.
            let mut syntax_row = div().flex().flex_row().flex_wrap().gap_2();
            {
                let cur_c = cur.clone();
                let active = !cur.syntax;
                syntax_row =
                    syntax_row.child(color_mode_btn(&th, "○", t.sc_off, active).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.set_menu_choice(
                                ThemeChoice {
                                    syntax: false,
                                    ..cur_c.clone()
                                },
                                cx,
                            );
                        }),
                    ));
            }
            for scheme in theme::SyntaxScheme::ALL {
                let active = cur.syntax && cur.syntax_scheme == scheme;
                let cur_c = cur.clone();
                let cap = match scheme {
                    theme::SyntaxScheme::Code => t.sc_code,
                    theme::SyntaxScheme::Agentic => t.sc_agentic,
                    theme::SyntaxScheme::Logs => t.sc_logs,
                    theme::SyntaxScheme::Markdown => t.sc_mark,
                };
                syntax_row = syntax_row.child(
                    color_mode_btn(&th, scheme.icon(), cap, active).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.set_menu_choice(
                                ThemeChoice {
                                    syntax: true,
                                    syntax_scheme: scheme,
                                    ..cur_c.clone()
                                },
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
                    // A per-set swatch tints the plain symbol glyphs (❖ ⚡ ☼ …) to
                    // their palette colour so the tray reads at a glance; colour
                    // emoji ignore the tint and keep their own hues.
                    let swatch = d.swatch();
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
                            .text_color(match (swatch, active) {
                                (Some(c), true) => c,
                                (Some(c), false) => c.alpha(0.85),
                                (None, true) => th.text,
                                (None, false) => th.text.alpha(0.7),
                            })
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
                // min-width 0 lets captions wrap inside the column instead of
                // forcing the panel wider than its frame (the overflow bug).
                .min_w(px(0.))
                .child(
                    div()
                        .text_size(px(8.5))
                        .text_color(th.text.alpha(0.45))
                        .child(if is_pane { t.scope_pane } else { t.scope_outer }),
                )
                .child(label(t.t_theme))
                .child(theme_row)
                .child(label(t.t_wheel))
                .child(div().flex().justify_center().py_1().child(wheel))
                .child(div().flex().justify_center().child(lbar))
                .child(div().flex().justify_center().pt_1().child(pick_row))
                .child(seed_row)
                .child(label(t.t_program))
                .child(color_row)
                .child(label(t.t_syntax))
                .child(syntax_row);
            if is_pane {
                // Per-group toggle: on = this pane's theme follows the outer scope
                // live; off = it keeps its own retained theme. Non-destructive.
                let lbl = if following {
                    format!("◉ {}", t.follow_outer)
                } else {
                    format!("◯ {}", t.follow_outer)
                };
                controls = controls.child(Self::bezel_btn(&th, &lbl, following).on_mouse_down(
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
            let vp_h = f32::from(window.viewport_size().height);
            let mut panel = div().id("theme-panel").absolute().w(px(PANEL_W));
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
                .border_2()
                .border_color(th.accent.alpha(0.85))
                .bg(darken(th.surface, 0.6))
                // never spill past the screen: clip horizontally, scroll a tall
                // panel vertically rather than overflowing the bottom edge.
                .max_h(px((vp_h - 16.).max(160.)))
                .overflow_x_hidden()
                .overflow_y_scroll()
                .shadow(float_shadows(th.accent))
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
            let t = self.lang.strings();
            let mut rows = div().flex().flex_col().gap_1();
            for (key, _name) in theme::Grade::CHANNELS {
                let name = match key {
                    theme::GradeKey::TextSize => t.g_text_size,
                    theme::GradeKey::Brightness => t.g_brightness,
                    theme::GradeKey::Contrast => t.g_contrast,
                    theme::GradeKey::Colour => t.g_colour,
                    theme::GradeKey::Text => t.g_text,
                    theme::GradeKey::Background => t.g_background,
                    theme::GradeKey::Gamma => t.g_gamma,
                    theme::GradeKey::Scale => t.g_menu_bar,
                    theme::GradeKey::Warp => t.g_warp,
                    _ => _name,
                };
                rows = rows.child(self.slider_row(key, name, grade.get(key), &th, cx));
            }
            const PANEL_W: f32 = 300.;
            const PANEL_H_EST: f32 = 328.; // 8 slider rows + reset + follow-outer
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
                .border_2()
                .border_color(th.accent.alpha(0.85))
                .bg(darken(th.surface, 0.6))
                .shadow(float_shadows(th.accent))
                .flex()
                .flex_col()
                .gap_2()
                .text_size(px(10.))
                .text_color(th.text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(label(&format!(
                    "{} — {}",
                    t.d_display,
                    if is_pane { t.scope_pane } else { t.scope_outer }
                )))
                .child(rows)
                .child(
                    Self::bezel_btn(&th, t.d_reset, grade.is_neutral()).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.reset_grade(cx);
                        }),
                    ),
                )
                // warp now rides the grade channels above (GradeKey::Warp), so it
                // scopes to pane/outer like the rest of the DISPLAY tray.
                .child(self.track_slider(0, t.d_roll, &th, cx))
                .child(self.track_slider(1, t.d_roll_spd, &th, cx))
                .child(self.track_slider(2, t.d_roll_size, &th, cx))
                .child(
                    div()
                        .id("track-reset")
                        .text_size(px(9.))
                        .text_color(th.text.alpha(0.5))
                        .cursor_pointer()
                        .child(format!("↺ {}", t.d_roll_reset))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                // clear THIS scope's tracking override → back to the
                                // theme's authored roll bar (per-pane via the grade).
                                if let Some(scope) = ws.osd_menu.clone() {
                                    let mut g = ws.choice_for(&scope, cx).grade;
                                    g.tracking = None;
                                    ws.write_grade(&scope, g, cx);
                                }
                            }),
                        ),
                );
            // ---- TEXT CRAWL: per-pane Star-Wars crawl toggle + its two knobs.
            // Rides the grade group like everything else here, so it scopes to
            // pane/outer and inherits via "follow outer". The angle/depth sliders
            // only appear while crawl is on.
            {
                let crawl_on = grade.crawl;
                let mut block = div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(t.d_crawl_hdr))
                    .child(
                        Self::bezel_btn(
                            &th,
                            &if crawl_on {
                                format!("\u{25a3} {}", t.d_crawl_on)
                            } else {
                                format!("\u{25a2} {}", t.d_crawl_off)
                            },
                            crawl_on,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.toggle_crawl(cx);
                            }),
                        ),
                    );
                if crawl_on {
                    block = block
                        .child(self.slider_row(
                            theme::GradeKey::CrawlAngle,
                            t.d_angle,
                            grade.crawl_angle,
                            &th,
                            cx,
                        ))
                        .child(self.slider_row(
                            theme::GradeKey::CrawlDepth,
                            t.d_depth,
                            grade.crawl_depth,
                            &th,
                            cx,
                        ));
                }
                panel = panel.child(block);
            }
            if is_pane {
                // Grade-group toggle, independent of the theme tray's: on = this
                // pane's monitor grade tracks the outer sliders live; off = it
                // keeps its own. Non-destructive (PaneTheme::toggle_grade).
                let lbl = if following {
                    format!("◉ {}", t.follow_outer)
                } else {
                    format!("◯ {}", t.follow_outer)
                };
                panel = panel.child(Self::bezel_btn(&th, &lbl, following).on_mouse_down(
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

        // ---- Find: fuzzy search in this pane (Ctrl+F) or every pane (Ctrl+Shift+F) ----
        let find_overlay = self.render_find(&th, cx);
        let lang_picker_overlay = self.render_lang_picker(&th, cx);
        let plugins_overlay = self.render_plugins_overlay(&th, cx);
        let savings_overlay = self.render_savings_overlay(&th, cx);

        // ---- MCP control: the read-only agent-watch surface (the 🤖 button) ----
        let mcp_overlay = self.mcp_menu.then(|| {
            let cfg = self.mcp.clone();
            let t = self.lang.strings();
            let panes = self.mcp_snapshot(cx);
            let total = panes.len();
            let exposed = panes.iter().filter(|p| p.exposed).count();
            let vp_h = f32::from(window.viewport_size().height);
            let home = std::env::var("HOME").unwrap_or_default();
            let filt = self.mcp_filter;
            let state_filt = self.mcp_state_filter;
            let program_filt = self.mcp_program_filter.clone();
            let label = |s: String| {
                div()
                    .text_size(px(9.))
                    .text_color(th.text.alpha(0.55))
                    .child(s)
            };
            let enable_btn = Self::bezel_btn(
                &th,
                &if cfg.enabled {
                    format!("\u{25c9} {}", t.m_server_on)
                } else {
                    format!("\u{25cb} {}", t.m_server_off)
                },
                cfg.enabled,
            )
            .id("mcp-btn-enable")
            .hover(|s| s.border_color(th.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.mcp.enabled = !ws.mcp.enabled;
                    ws.save(cx);
                    cx.notify();
                }),
            );
            let expose_btn = Self::bezel_btn(
                &th,
                format!(
                    "{} {}",
                    t.m_expose,
                    match cfg.expose {
                        mcp::Expose::AgentsOnly => t.exp_agents,
                        mcp::Expose::All => t.exp_all,
                    }
                )
                .as_str(),
                false,
            )
            .id("mcp-btn-expose")
            .hover(|s| s.border_color(th.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.mcp.expose = ws.mcp.expose.next();
                    ws.save(cx);
                    cx.notify();
                }),
            );
            let events_btn = Self::bezel_btn(
                &th,
                &if cfg.events {
                    format!("\u{25c9} {}", t.m_events_on)
                } else {
                    format!("\u{25cb} {}", t.m_events_off)
                },
                cfg.events,
            )
            .id("mcp-btn-events")
            .hover(|s| s.border_color(th.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.mcp.events = !ws.mcp.events;
                    ws.save(cx);
                    cx.notify();
                }),
            );
            // 🎨 inherit toggle: when on, the Agent Wall wears the outer TD
            // theme as one coherent dashboard instead of mixing every pane's
            // own colours. The chrome stays flat so hit targets remain honest.
            let theme_btn = Self::bezel_btn(
                &th,
                &if self.mcp_theme_preview {
                    format!("\u{1f3a8} {}", t.m_theme_on)
                } else {
                    format!("\u{1f3a8} {}", t.m_theme_off)
                },
                self.mcp_theme_preview,
            )
            .id("mcp-btn-theme")
            .hover(|s| s.border_color(th.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.mcp_theme_preview = !ws.mcp_theme_preview;
                    cx.notify();
                }),
            );

            // Writes opt-in: promotes the read-only watch surface to a
            // remote-control one (set_pane_config). A deliberate second switch —
            // appearance only, never a PTY. Mirrors the TD_MCP_WRITE env var.
            let writes_btn = Self::bezel_btn(
                &th,
                &if cfg.writable {
                    format!("\u{25c9} {}", t.m_writes_on)
                } else {
                    format!("\u{25cb} {}", t.m_writes_off)
                },
                cfg.writable,
            )
            .id("mcp-btn-writes")
            .hover(|s| s.border_color(th.accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.mcp.writable = !ws.mcp.writable;
                    ws.save(cx);
                    cx.notify();
                }),
            );

            // Live pane list — walk the REAL tree (not the wire snapshot) so each
            // row can carry the pane's own resolved colours and focus it on click.
            let preview = self.mcp_theme_preview;
            // ---- pre-pass: whole-fleet counts (unfiltered) + context-aware
            // filter domains. Group chips come from tab groups; program chips
            // come from live pane modes; state chips only come from matching
            // agents. ----
            let (mut n_work, mut n_block, mut n_err, mut n_done, mut n_idle) =
                (0u32, 0u32, 0u32, 0u32, 0u32);
            let mut turn_tok_total = 0u64;
            let mut sess_tok_total = 0u64;
            let mut total_panes = 0u32;
            let mut visible_agent_total = 0u32;
            let (mut v_work, mut v_block, mut v_err, mut v_done, mut v_idle) =
                (0u32, 0u32, 0u32, 0u32, 0u32);
            let mut visible_program_total = 0u32;
            let mut programs_present: Vec<(String, Hsla, u32)> = Vec::new();
            // (group key, display name, band colour, pane count) in first-seen order.
            let mut groups_present: Vec<(Option<u32>, String, Hsla, u32)> = Vec::new();
            // Fully filtered counts per group, used for group headers when a
            // program or state chip is active. Same first-seen order as groups_present.
            let mut group_visible: Vec<(Option<u32>, u32)> = Vec::new();
            for (ti, tab) in self.tabs.iter().enumerate() {
                let grp = self.group_of(ti);
                let key = grp.map(|g| g.id);
                let gname = match grp {
                    Some(g) => g.name.clone().unwrap_or_else(|| format!("group {}", g.id)),
                    None => "Ungrouped".to_string(),
                };
                let gcol = match grp {
                    Some(g) => g.color,
                    None => th.text.alpha(0.45),
                };
                let mut leaves = vec![];
                tab.root.leaves(&mut leaves);
                for leaf in leaves {
                    let p = leaf.read(cx);
                    total_panes += 1;
                    let show_group = match filt {
                        McpFilter::All => true,
                        McpFilter::Group(id) => key == Some(id),
                        McpFilter::Ungrouped => key.is_none(),
                    };
                    let mode_lbl = p.mode.label().to_string();
                    let program_matches = program_filt
                        .as_deref()
                        .is_none_or(|program| program == mode_lbl.as_str());
                    if p.mode.is_agent() {
                        let st = p.agent_status();
                        match st.state {
                            hud::AgentState::Working => n_work += 1,
                            hud::AgentState::Blocked => n_block += 1,
                            hud::AgentState::Error => n_err += 1,
                            hud::AgentState::Finished => n_done += 1,
                            hud::AgentState::Idle => n_idle += 1,
                        }
                        turn_tok_total += st.turn_tokens.unwrap_or(0);
                        sess_tok_total += p.session_tokens();
                        if show_group && program_matches {
                            visible_agent_total += 1;
                            match st.state {
                                hud::AgentState::Working => v_work += 1,
                                hud::AgentState::Blocked => v_block += 1,
                                hud::AgentState::Error => v_err += 1,
                                hud::AgentState::Finished => v_done += 1,
                                hud::AgentState::Idle => v_idle += 1,
                            }
                        }
                        let state_matches = state_filt.is_none_or(|s| st.state == s);
                        if show_group && state_matches {
                            visible_program_total += 1;
                            let color = agent_program_glow(th.text.alpha(0.55), mode_lbl.as_str());
                            match programs_present.iter_mut().find(|e| e.0 == mode_lbl) {
                                Some(e) => e.2 += 1,
                                None => programs_present.push((mode_lbl.clone(), color, 1)),
                            }
                        }
                        if state_matches && program_matches {
                            match group_visible.iter_mut().find(|e| e.0 == key) {
                                Some(e) => e.1 += 1,
                                None => group_visible.push((key, 1)),
                            }
                        }
                    } else {
                        if show_group && state_filt.is_none() {
                            visible_program_total += 1;
                            let color = agent_program_glow(th.text.alpha(0.55), mode_lbl.as_str());
                            match programs_present.iter_mut().find(|e| e.0 == mode_lbl) {
                                Some(e) => e.2 += 1,
                                None => programs_present.push((mode_lbl.clone(), color, 1)),
                            }
                        }
                        if state_filt.is_none() && program_matches {
                            match group_visible.iter_mut().find(|e| e.0 == key) {
                                Some(e) => e.1 += 1,
                                None => group_visible.push((key, 1)),
                            }
                        }
                    }
                    match groups_present.iter_mut().find(|e| e.0 == key) {
                        Some(e) => e.3 += 1,
                        None => groups_present.push((key, gname.clone(), gcol, 1)),
                    }
                }
            }

            // ---- filter chips: ALL · <each tab group> · Ungrouped ----
            let mut chips = div()
                .id("mcp-chips")
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_size(px(9.5));
            {
                let on = filt == McpFilter::All;
                chips = chips.child(
                    div()
                        .id("mcpf-all")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { th.accent } else { th.text.alpha(0.2) })
                        .when(on, |d| d.bg(th.accent.alpha(0.16)))
                        .text_color(if on { th.text } else { th.text.alpha(0.6) })
                        .cursor_pointer()
                        .hover(|s| s.border_color(th.accent.alpha(0.7)))
                        .child("ALL")
                        .child(
                            div()
                                .text_color(th.text.alpha(0.4))
                                .child(format!("{total_panes}")),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.mcp_filter = McpFilter::All;
                                cx.notify();
                            }),
                        ),
                );
            }
            for (key, name, color, count) in &groups_present {
                let f = match key {
                    Some(id) => McpFilter::Group(*id),
                    None => McpFilter::Ungrouped,
                };
                let on = filt == f;
                let color = *color;
                chips = chips.child(
                    div()
                        .id(SharedString::from(format!("mcpf-{key:?}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { color } else { th.text.alpha(0.2) })
                        .when(on, |d| d.bg(color.alpha(0.16)))
                        .text_color(if on { th.text } else { th.text.alpha(0.65) })
                        .cursor_pointer()
                        .hover(move |s| s.border_color(color.alpha(0.7)))
                        .child(
                            div()
                                .w(px(7.))
                                .h(px(7.))
                                .flex_none()
                                .rounded_full()
                                .bg(color),
                        )
                        .child(name.clone())
                        .child(
                            div()
                                .text_color(th.text.alpha(0.4))
                                .child(format!("{count}")),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.mcp_filter = f;
                                cx.notify();
                            }),
                        ),
                );
            }

            // ---- program chips: generated from the pane modes in the current
            // group/state context. Zero-count modes never render. ----
            let mut program_chips = div()
                .id("mcp-program-chips")
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_size(px(9.5));
            {
                let on = program_filt.is_none();
                program_chips = program_chips.child(
                    div()
                        .id("mcpp-all")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { th.accent } else { th.text.alpha(0.2) })
                        .when(on, |d| d.bg(th.accent.alpha(0.16)))
                        .text_color(if on { th.text } else { th.text.alpha(0.6) })
                        .cursor_pointer()
                        .hover(|s| s.border_color(th.accent.alpha(0.7)))
                        .child("ANY PROGRAM")
                        .child(
                            div()
                                .text_color(th.text.alpha(0.4))
                                .child(format!("{visible_program_total}")),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.mcp_program_filter = None;
                                cx.notify();
                            }),
                        ),
                );
            }
            for (program, color, count) in &programs_present {
                if *count == 0 {
                    continue;
                }
                let on = program_filt.as_deref() == Some(program.as_str());
                let color = *color;
                let program_value = program.clone();
                program_chips = program_chips.child(
                    div()
                        .id(SharedString::from(format!("mcpp-{program}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { color } else { color.alpha(0.34) })
                        .when(on, |d| d.bg(color.alpha(0.18)))
                        .text_color(if on { th.text } else { th.text.alpha(0.68) })
                        .cursor_pointer()
                        .hover(move |s| s.border_color(color.alpha(0.78)))
                        .child(
                            div()
                                .w(px(7.))
                                .h(px(7.))
                                .flex_none()
                                .rounded_full()
                                .bg(color),
                        )
                        .child(program.clone())
                        .child(
                            div()
                                .text_color(th.text.alpha(0.4))
                                .child(format!("{count}")),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.mcp_program_filter = Some(program_value.clone());
                                cx.notify();
                            }),
                        ),
                );
            }

            // ---- state chips: generated from matching live agents in the
            // current group/program context. Hidden entirely at zero. ----
            let show_state_chips = visible_agent_total > 0;
            let mut state_chips = div()
                .id("mcp-state-chips")
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_size(px(9.5));
            {
                let on = state_filt.is_none();
                state_chips = state_chips.child(
                    div()
                        .id("mcps-all")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { th.accent } else { th.text.alpha(0.2) })
                        .when(on, |d| d.bg(th.accent.alpha(0.16)))
                        .text_color(if on { th.text } else { th.text.alpha(0.6) })
                        .cursor_pointer()
                        .hover(|s| s.border_color(th.accent.alpha(0.7)))
                        .child("ANY STATE")
                        .child(
                            div()
                                .text_color(th.text.alpha(0.4))
                                .child(format!("{visible_agent_total}")),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.mcp_state_filter = None;
                                cx.notify();
                            }),
                        ),
                );
            }
            for (state, count) in [
                (hud::AgentState::Working, v_work),
                (hud::AgentState::Blocked, v_block),
                (hud::AgentState::Error, v_err),
                (hud::AgentState::Finished, v_done),
                (hud::AgentState::Idle, v_idle),
            ] {
                if count == 0 {
                    continue;
                }
                let on = state_filt == Some(state);
                let color = agent_state_glow(&th, th.text.alpha(0.48), state);
                state_chips = state_chips.child(
                    div()
                        .id(SharedString::from(format!("mcps-{}", state.label())))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { color } else { color.alpha(0.34) })
                        .when(on, |d| d.bg(color.alpha(0.18)))
                        .text_color(if on { th.text } else { th.text.alpha(0.68) })
                        .cursor_pointer()
                        .hover(move |s| s.border_color(color.alpha(0.78)))
                        .child(div().text_color(color).child(state.badge().to_string()))
                        .child(state.label().to_uppercase())
                        .child(
                            div()
                                .text_color(th.text.alpha(0.4))
                                .child(format!("{count}")),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.mcp_state_filter = Some(state);
                                cx.notify();
                            }),
                        ),
                );
            }

            // ---- the grouped, filtered pane list ----
            let mut list = div()
                .id("mcp-pane-list")
                .flex()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_2();
            let mut ri = 0usize;
            let mut last_key: Option<Option<u32>> = None;
            // best-effort model parse from the agent's launch/resume command.
            let parse_model = |cmd: &str| -> Option<String> {
                let toks: Vec<&str> = cmd.split_whitespace().collect();
                for (i, t) in toks.iter().enumerate() {
                    if let Some(v) = t.strip_prefix("--model=") {
                        return Some(v.to_string());
                    }
                    if (*t == "--model" || *t == "-m") && i + 1 < toks.len() {
                        return Some(toks[i + 1].to_string());
                    }
                }
                None
            };
            for (ti, tab) in self.tabs.iter().enumerate() {
                let grp = self.group_of(ti);
                let key = grp.map(|g| g.id);
                let show_tab = match filt {
                    McpFilter::All => true,
                    McpFilter::Group(id) => key == Some(id),
                    McpFilter::Ungrouped => key.is_none(),
                };
                if !show_tab {
                    continue;
                }
                let mut leaves = vec![];
                tab.root.leaves(&mut leaves);
                let tab_visible = leaves.iter().any(|leaf| {
                    let p = leaf.read(cx);
                    let mode_lbl = p.mode.label().to_string();
                    let program_matches = program_filt
                        .as_deref()
                        .is_none_or(|program| program == mode_lbl.as_str());
                    if !program_matches {
                        return false;
                    }
                    if let Some(state) = state_filt {
                        p.mode.is_agent() && p.agent_status().state == state
                    } else {
                        true
                    }
                });
                if !tab_visible {
                    continue;
                }
                let gname = match grp {
                    Some(g) => g.name.clone().unwrap_or_else(|| format!("group {}", g.id)),
                    None => "Ungrouped".to_string(),
                };
                let gcol = match grp {
                    Some(g) => g.color,
                    None => th.text.alpha(0.45),
                };
                // a group section header whenever the section changes.
                if last_key != Some(key) {
                    last_key = Some(key);
                    let gcount = groups_present
                        .iter()
                        .find(|e| e.0 == key)
                        .map(|e| e.3)
                        .unwrap_or(0);
                    let gcount = if state_filt.is_some() || program_filt.is_some() {
                        group_visible
                            .iter()
                            .find(|e| e.0 == key)
                            .map(|e| e.1)
                            .unwrap_or(0)
                    } else {
                        gcount
                    };
                    list = list.child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_1()
                            .pt_1()
                            .text_size(px(8.5))
                            .text_color(th.text.alpha(0.5))
                            .child(div().w(px(8.)).h(px(8.)).flex_none().rounded_sm().bg(gcol))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .child(gname.to_uppercase()),
                            )
                            .child(div().flex_1().min_w(px(0.)))
                            .child(div().text_color(th.text.alpha(0.35)).child(format!(
                                "{gcount} {}",
                                if gcount == 1 { "pane" } else { "panes" }
                            ))),
                    );
                }
                for leaf in leaves {
                    let id = leaf.entity_id();
                    let p = leaf.read(cx);
                    let is_agent = p.mode.is_agent();
                    let mode_lbl = p.mode.label().to_string();
                    let title = p
                        .name
                        .clone()
                        .filter(|n| !n.is_empty())
                        .or_else(|| (!p.title.is_empty()).then(|| p.title.clone()))
                        .unwrap_or_else(|| p.mode.label().to_string());
                    let rt = p.runtime();
                    let exposed = mcp::should_expose(&self.mcp, is_agent);
                    let status = p.agent_status();
                    if let Some(program) = program_filt.as_deref() {
                        if mode_lbl.as_str() != program {
                            continue;
                        }
                    }
                    if let Some(state) = state_filt {
                        if !is_agent || status.state != state {
                            continue;
                        }
                    }
                    let sess_tok = p.session_tokens();
                    // best-effort model from the launch/resume command (often absent).
                    let model = rt.resume.as_deref().and_then(parse_model);

                    // Abbreviate so a long cwd / resume id never spills off the panel.
                    let mut cwd = rt.cwd.clone().unwrap_or_else(|| "\u{2014}".to_string());
                    if !home.is_empty() {
                        cwd = cwd.replacen(&home, "~", 1);
                    }
                    if cwd.chars().count() > 34 {
                        let tail: String = cwd
                            .chars()
                            .rev()
                            .take(32)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .collect();
                        cwd = format!("\u{2026}{tail}");
                    }
                    let sub = match &rt.resume {
                        Some(sess) => {
                            let agent = sess.split_whitespace().next().unwrap_or("agent");
                            let sid = sess
                                .split_whitespace()
                                .last()
                                .filter(|t| t.len() >= 8 && !t.starts_with("--"));
                            match sid {
                                Some(i) => format!("{cwd}   {agent} \u{00b7} {}", &i[..8]),
                                None => format!("{cwd}   {agent}"),
                            }
                        }
                        None => cwd,
                    };
                    // "doing now": when working, lead the 2nd line with the live
                    // gerund (✷ Accomplishing…); otherwise show the cwd / resume.
                    let working = is_agent && status.state == hud::AgentState::Working;
                    let line2_accent = working && status.gerund.is_some();
                    let line2 = if line2_accent {
                        format!(
                            "\u{2737} {}\u{2026}",
                            status.gerund.clone().unwrap_or_default()
                        )
                    } else {
                        sub
                    };

                    // In inherited-theme mode, every card uses the same outer
                    // TD palette. Status/kind colours still carry meaning, but
                    // the board itself no longer jitters between pane skins.
                    let row_text = th.text;
                    let dot_col = if exposed {
                        th.accent
                    } else {
                        row_text.alpha(0.3)
                    };
                    let kind_col = agent_program_glow(row_text.alpha(0.55), mode_lbl.as_str());
                    let mode_col = kind_col;
                    // HUD per-row: state badge colour/glyph + a compact metrics
                    // line (state · effort · elapsed · turn tokens · Σ session).
                    let status_glow = if is_agent {
                        agent_state_glow(&th, row_text.alpha(0.38), status.state)
                    } else {
                        kind_col
                    };
                    let live_glow = is_agent && !matches!(status.state, hud::AgentState::Idle);
                    let card_bg = if preview {
                        darken(th.bg, if live_glow { 0.72 } else { 0.64 })
                    } else if live_glow {
                        darken(th.surface, 0.50)
                    } else {
                        darken(th.surface, 0.42)
                    };
                    let card_border = status_glow.alpha(if live_glow { 0.74 } else { 0.24 });
                    let hover_border = status_glow.alpha(if live_glow { 0.95 } else { 0.48 });
                    let badge_col = if is_agent {
                        status_glow
                    } else {
                        kind_col.alpha(0.78)
                    };
                    let badge_glyph = if is_agent {
                        status.state.badge().to_string()
                    } else {
                        String::new()
                    };
                    let metrics = if is_agent {
                        let mut parts = vec![status.state.label().to_string()];
                        if let Some(ef) = &status.effort {
                            parts.push(ef.clone());
                        }
                        if let Some(e) = &status.elapsed {
                            parts.push(e.clone());
                        }
                        if let Some(tk) = status.turn_tokens {
                            parts.push(hud::fmt_tokens(tk));
                        }
                        parts.push(format!("\u{03a3}{}", hud::fmt_tokens(sess_tok)));
                        parts.join(" \u{00b7} ")
                    } else {
                        String::new()
                    };
                    let model_disp = model.map(|m| {
                        if m.chars().count() > 16 {
                            format!("{}\u{2026}", m.chars().take(15).collect::<String>())
                        } else {
                            m
                        }
                    });

                    let row = div()
                        .id(SharedString::from(format!("mcp-row-{ri}")))
                        .flex()
                        .flex_row()
                        .items_stretch()
                        .gap_2()
                        .w(px(316.))
                        .min_w(px(316.))
                        .max_w(px(316.))
                        .h(px(88.))
                        .flex_none()
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .border_2()
                        .border_color(card_border)
                        .bg(card_bg)
                        .shadow(agent_card_shadows(status_glow, live_glow))
                        .cursor_pointer()
                        // hover intensifies the same state phosphor, so the
                        // click target reads like a terminal pane border.
                        .hover(move |s| s.border_color(hover_border))
                        // click → hop to that tab and focus that exact pane, just
                        // as if the terminal itself had been clicked.
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                if let Some(t) = ws.tabs.get_mut(ti) {
                                    t.focused = Some(id);
                                }
                                ws.mcp_menu = false;
                                ws.activate_tab(ti, window, cx);
                            }),
                        );
                    list = list.child(
                        row.child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .justify_between()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .min_w(px(0.))
                                        .child(
                                            div().flex_none().text_color(dot_col).child(
                                                if exposed { "\u{25cf}" } else { "\u{25cb}" }
                                                    .to_string(),
                                            ),
                                        )
                                        .child(
                                            div()
                                                .w(px(14.))
                                                .flex_none()
                                                .text_color(badge_col)
                                                .child(badge_glyph),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_size(px(8.5))
                                                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                                .text_color(mode_col)
                                                .px_1()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(kind_col.alpha(0.42))
                                                .bg(kind_col.alpha(0.12))
                                                .child(mode_lbl),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(0.))
                                                .overflow_hidden()
                                                .text_size(px(10.5))
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(row_text)
                                                .child(title),
                                        ),
                                )
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_size(px(8.5))
                                        .text_color(if line2_accent {
                                            status_glow.alpha(0.92)
                                        } else {
                                            row_text.alpha(0.55)
                                        })
                                        .child(line2),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .min_w(px(0.))
                                        .when_some(model_disp, |d, m| {
                                            d.child(
                                                div()
                                                    .flex_none()
                                                    .text_size(px(7.5))
                                                    .text_color(kind_col)
                                                    .px_1()
                                                    .rounded_sm()
                                                    .border_1()
                                                    .border_color(kind_col.alpha(0.30))
                                                    .bg(kind_col.alpha(0.10))
                                                    .child(m),
                                            )
                                        })
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(0.))
                                                .overflow_hidden()
                                                .text_size(px(8.5))
                                                .text_color(badge_col)
                                                .when(is_agent && status.state.needs_you(), |d| {
                                                    d.font_weight(gpui::FontWeight::BOLD)
                                                })
                                                .child(metrics),
                                        ),
                                ),
                        ),
                    );
                    ri += 1;
                }
            }
            if ri == 0 {
                list = list.child(label(t.m_no_panes.to_string()));
            }

            // The pane list scrolls within a height cap, so the toggles above and
            // the notes below stay pinned and on-screen no matter how many panes.
            let list = list
                .min_h(px(0.))
                .max_h(px((vp_h * 0.8 - 170.).max(140.)))
                .overflow_y_scroll();

            let (mcp_k1, mcp_k2) = theme::warp_coeffs(th.warp);
            let mcp_glare = th.screen_glare;
            let mcp_preview = preview;
            let vp_w = f32::from(window.viewport_size().width);
            let panel = div()
                .absolute()
                .top(px(vp_h * 0.10))
                .left(px(vp_w * 0.17))
                .right(px(vp_w * 0.17))
                .bottom(px(vp_h * 0.10))
                .overflow_hidden()
                .p_3()
                .rounded_md()
                .border_2()
                .border_color(th.accent.alpha(0.85))
                .bg(if preview {
                    darken(th.bg, 0.78)
                } else {
                    darken(th.surface, 0.6)
                })
                .shadow(float_shadows(th.accent))
                .flex()
                .flex_col()
                .gap_2()
                .text_size(px(10.))
                .text_color(th.text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(label(format!(
                    "MCP {} \u{2014} {}",
                    t.m_control,
                    if cfg.writable {
                        t.m_read_write
                    } else {
                        t.m_read_only
                    }
                )))
                .child(
                    // ---- the agent-wall scoreboard rollup ----
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_3()
                        .text_size(px(12.))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                .text_color(th.complement)
                                .child("AGENT WALL"),
                        )
                        .child(
                            div()
                                .text_color(th.accent)
                                .child(format!("\u{25b6} {n_work}")),
                        )
                        .child(
                            div()
                                .text_color(hsla(0.11, 0.85, 0.60, 1.))
                                .child(format!("\u{23f8} {n_block}")),
                        )
                        .child(
                            div()
                                .text_color(hsla(0., 0.75, 0.60, 1.))
                                .child(format!("\u{2715} {n_err}")),
                        )
                        .child(
                            div()
                                .text_color(th.complement.alpha(0.85))
                                .child(format!("\u{2713} {n_done}")),
                        )
                        .child(
                            div()
                                .text_color(th.text.alpha(0.45))
                                .child(format!("\u{25cb} {n_idle}")),
                        )
                        .child(div().flex_1().min_w(px(0.)))
                        .child(div().text_color(th.text.alpha(0.7)).child(format!(
                            "\u{0394} {} \u{00b7} \u{03a3} {}",
                            hud::fmt_tokens(turn_tok_total),
                            hud::fmt_tokens(sess_tok_total)
                        )))
                        // </> LeanCTX savings: read lean-ctx's precomputed
                        // token-savings rollup over the leanctx-savings plugin
                        // and pop the </> overlay. Own handler + stop_propagation.
                        .child(
                            div()
                                .id("mcp-btn-savings")
                                .flex_none()
                                .px_2()
                                .py_0p5()
                                .rounded_sm()
                                .border_1()
                                .border_color(hsla(0.43, 0.68, 0.55, 0.6))
                                .bg(hsla(0.43, 0.68, 0.55, 0.10))
                                .text_size(px(10.))
                                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                .text_color(hsla(0.43, 0.68, 0.55, 1.))
                                .cursor_pointer()
                                .hover(|s| s.bg(hsla(0.43, 0.68, 0.55, 0.22)))
                                .child("\u{003c}/\u{003e} savings")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        ws.fetch_savings(None, cx);
                                    }),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap_1()
                        .child(enable_btn)
                        .child(expose_btn)
                        .child(events_btn)
                        .child(writes_btn)
                        .child(theme_btn),
                )
                .child(label(format!("{exposed}/{total} {}", t.m_exposed)))
                .child(chips)
                .child(program_chips)
                .when(show_state_chips, |d| d.child(state_chips))
                .child(list)
                .child(
                    div()
                        .text_size(px(8.5))
                        .text_color(th.accent.alpha(0.85))
                        .child(t.m_watches.to_string()),
                )
                .child(label(t.m_transport.to_string()))
                // Theme-on makes the dashboard itself a curved-glass tube
                // using the OUTER display gauges. This mirrors FOCUS: the
                // rect is measured from the real prepaint bounds so the warp
                // and frosted edge stay pixel-aligned as the window resizes.
                .child(
                    div().absolute().inset_0().child(
                        gpui::canvas(
                            move |bounds, window, _cx| {
                                if !mcp_preview {
                                    return;
                                }
                                let sf = window.scale_factor();
                                let rect = [
                                    f32::from(bounds.origin.x) * sf,
                                    f32::from(bounds.origin.y) * sf,
                                    f32::from(bounds.size.width) * sf,
                                    f32::from(bounds.size.height) * sf,
                                ];
                                crate::warp::set_focus_blur(
                                    rect,
                                    28.0 * sf,
                                    16.0 * sf,
                                    0.78,
                                    8.0 * sf,
                                );
                                crate::warp::register_focus_tube(
                                    rect,
                                    mcp_glare,
                                    mcp_k1,
                                    mcp_k2,
                                    [0.0, 1.0, 1.0],
                                );
                            },
                            |_, _, _, _| {},
                        )
                        .size_full(),
                    ),
                );

            // Full-screen scrim with a fixed-position panel. Filters can change
            // the card count, but the dashboard's screen rectangle never moves.
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(th.bg.alpha(if preview { 0.52 } else { 0.70 }))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.mcp_menu = false;
                        cx.notify();
                    }),
                )
                .child(panel)
        });

        // ---- confirm overlay: closing a tab that holds more than one pane ----
        // ---- 🪦 the dead-agent recover manifest ----
        let dead_overlay = self.dead_menu.then(|| {
            let vp_h = f32::from(window.viewport_size().height);
            let home_path = session::home_dir();
            let home_str = home_path.to_string_lossy().into_owned();
            // live = the resume ids of every currently-open agent pane.
            let mut live: std::collections::HashSet<String> = std::collections::HashSet::new();
            for tab in &self.tabs {
                let mut leaves = vec![];
                tab.root.leaves(&mut leaves);
                for leaf in leaves {
                    let p = leaf.read(cx);
                    let rt = p.runtime();
                    if p.mode.is_agent() {
                        if let Some(id) = rt.resume.as_deref().and_then(session::resume_session_id)
                        {
                            live.insert(id);
                        }
                    }
                }
            }
            let mut dead = recover::scan_dead(&live, &home_path, 80);
            // leak-safe demo data for screenshots (never real session paths/ids).
            if std::env::var("TD_GRAVEYARD_DEMO").is_ok() {
                let mk = |kind, sid: &str, cwd: &str, summary: &str, age: &str, kb: u64| {
                    recover::DeadAgent {
                        kind,
                        session_id: sid.into(),
                        cwd: Some(cwd.into()),
                        resume_cmd: String::new(),
                        summary: Some(summary.into()),
                        age: age.into(),
                        bytes: kb * 1024,
                    }
                };
                dead = vec![
                    mk(recover::AgentKind::Claude, "a1b2c3d4", "~/aurora/web", "Wire up the onboarding flow and its tests", "4d ago", 226),
                    mk(recover::AgentKind::Codex, "e5f6a7b8", "~/aurora/api", "Refactor the billing service into modules", "2d ago", 512),
                    mk(recover::AgentKind::Claude, "c9d0e1f2", "~/borealis/ml", "Tune the ranking model hyperparameters", "6h ago", 98),
                ];
            }
            // only offer ⬇ harvest if a discovered plugin advertises the
            // "graveyard" surface (the built-in context-delight one does).
            let can_harvest = plugins::discover(&home_path)
                .iter()
                .any(|m| m.action_for("graveyard").is_some());

            // group dead agents by project (the cwd's last path segment), the way
            // the agent wall groups by tab group — a stable colour per project.
            fn project_of(cwd: Option<&str>) -> String {
                match cwd {
                    Some(c) => c
                        .trim_end_matches('/')
                        .rsplit('/')
                        .find(|s| !s.is_empty())
                        .unwrap_or("~")
                        .to_string(),
                    None => "unknown".to_string(),
                }
            }
            fn proj_hue(s: &str) -> f32 {
                let h = s
                    .bytes()
                    .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
                (h % 360) as f32 / 360.0
            }
            let mut groups: Vec<(String, u32)> = Vec::new();
            for da in &dead {
                let proj = project_of(da.cwd.as_deref());
                match groups.iter_mut().find(|g| g.0 == proj) {
                    Some(g) => g.1 += 1,
                    None => groups.push((proj, 1)),
                }
            }
            let n = dead.len();
            let n_claude = dead
                .iter()
                .filter(|d| d.kind == recover::AgentKind::Claude)
                .count();
            let n_codex = n - n_claude;
            let filt = self.dead_filter.clone();

            // ---- chips: ALL · <each project> ----
            let mut chips = div()
                .id("dead-chips")
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_1()
                .text_size(px(9.5));
            {
                let on = filt.is_none();
                chips = chips.child(
                    div()
                        .id("deadf-all")
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { th.accent } else { th.text.alpha(0.2) })
                        .when(on, |d| d.bg(th.accent.alpha(0.16)))
                        .text_color(if on { th.text } else { th.text.alpha(0.6) })
                        .cursor_pointer()
                        .hover(|s| s.border_color(th.accent.alpha(0.7)))
                        .child("ALL")
                        .child(div().text_color(th.text.alpha(0.4)).child(format!("{n}")))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.dead_filter = None;
                                cx.notify();
                            }),
                        ),
                );
            }
            for (proj, count) in &groups {
                let col = hsla(proj_hue(proj), 0.6, 0.62, 1.);
                let on = filt.as_deref() == Some(proj.as_str());
                let proj_c = proj.clone();
                chips = chips.child(
                    div()
                        .id(SharedString::from(format!("deadf-{proj}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .px_2()
                        .py_0p5()
                        .rounded_full()
                        .border_1()
                        .border_color(if on { col } else { th.text.alpha(0.2) })
                        .when(on, |d| d.bg(col.alpha(0.16)))
                        .text_color(if on { th.text } else { th.text.alpha(0.65) })
                        .cursor_pointer()
                        .hover(move |s| s.border_color(col.alpha(0.7)))
                        .child(div().w(px(7.)).h(px(7.)).flex_none().rounded_full().bg(col))
                        .child(proj.clone())
                        .child(div().text_color(th.text.alpha(0.4)).child(format!("{count}")))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.dead_filter = Some(proj_c.clone());
                                cx.notify();
                            }),
                        ),
                );
            }

            // ---- grouped, filtered list ----
            let mut list = div()
                .id("dead-list")
                .flex()
                .flex_row()
                .flex_wrap()
                .items_start()
                .gap_2();
            if dead.is_empty() {
                list = list.child(
                    div()
                        .w_full()
                        .py_2()
                        .text_color(th.text.alpha(0.6))
                        .child(
                            "No recoverable agents \u{2014} every saved session is either live or already gone.",
                        ),
                );
            }
            // The scan is newest-first with projects interleaved; stable-sort by each
            // project's first-seen order (the `groups` order) so a project's agents
            // are contiguous — one header per project, newest-first within it.
            dead.sort_by_key(|d| {
                groups
                    .iter()
                    .position(|g| g.0 == project_of(d.cwd.as_deref()))
                    .unwrap_or(usize::MAX)
            });
            let mut last_proj: Option<String> = None;
            let mut di = 0usize;
            for da in dead.into_iter() {
                let proj = project_of(da.cwd.as_deref());
                if let Some(f) = filt.as_deref() {
                    if proj != f {
                        continue;
                    }
                }
                let pcol = hsla(proj_hue(&proj), 0.6, 0.62, 1.);
                if last_proj.as_deref() != Some(proj.as_str()) {
                    last_proj = Some(proj.clone());
                    let gcount = groups.iter().find(|g| g.0 == proj).map(|g| g.1).unwrap_or(0);
                    list = list.child(
                        div()
                            .w_full()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_1()
                            .pt_1()
                            .text_size(px(8.5))
                            .text_color(th.text.alpha(0.5))
                            .child(div().w(px(8.)).h(px(8.)).flex_none().rounded_sm().bg(pcol))
                            .child(
                                div()
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .child(proj.to_uppercase()),
                            )
                            .child(div().flex_1().min_w(px(0.)))
                            .child(
                                div()
                                    .text_color(th.text.alpha(0.35))
                                    .child(format!("{gcount} dead")),
                            ),
                    );
                }
                let kind_col = match da.kind {
                    recover::AgentKind::Claude => hsla(0.105, 0.9, 0.55, 1.),
                    recover::AgentKind::Codex => hsla(0.52, 0.8, 0.6, 1.),
                };
                // Graveyard entries are agents with no live pane. Keep the
                // raised physical card, but run the phosphor off: muted project
                // hue, weak rim, no active halo.
                let dead_glow = pcol;
                let dead_border = dead_glow.alpha(0.26);
                let dead_bg = darken(th.surface, 0.40);
                let dead_hover = dead_glow.alpha(0.52);
                let mut cwd_disp = da.cwd.clone().unwrap_or_else(|| "\u{2014}".to_string());
                if !home_str.is_empty() {
                    cwd_disp = cwd_disp.replacen(&home_str, "~", 1);
                }
                if cwd_disp.chars().count() > 34 {
                    let tail: String = cwd_disp
                        .chars()
                        .rev()
                        .take(32)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    cwd_disp = format!("\u{2026}{tail}");
                }
                let title = da
                    .summary
                    .clone()
                    .unwrap_or_else(|| format!("{} session", da.kind.label()));
                let sid_short: String = da.session_id.chars().take(8).collect();
                let meta_line = format!(
                    "{}  \u{00b7}  {}  \u{00b7}  {}  \u{00b7}  {} KB",
                    cwd_disp,
                    da.age,
                    sid_short,
                    da.bytes / 1024
                );
                let cwd_click = da.cwd.clone();
                let resume_click = da.resume_cmd.clone();
                let sid_harvest = da.session_id.clone();
                list = list.child(
                    div()
                        .id(SharedString::from(format!("dead-{di}")))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .w(px(364.))
                        .min_w(px(364.))
                        .max_w(px(364.))
                        .h(px(76.))
                        .flex_none()
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .border_2()
                        .border_color(dead_border)
                        .bg(dead_bg)
                        .shadow(agent_card_shadows(dead_glow, false))
                        .cursor_pointer()
                        .hover(move |s| s.border_color(dead_hover))
                        // ---- content column: a dim dead-dot, the kind chip, the
                        // title, then the meta line. No full-height pill and no
                        // stretched chrome — it reads as the same family as the
                        // live agent-wall card.
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .min_w(px(0.))
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_size(px(7.))
                                                .text_color(dead_glow.alpha(0.7))
                                                .child("\u{25cf}"),
                                        )
                                        .child(
                                            div()
                                                .flex_none()
                                                .text_size(px(8.5))
                                                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                                .text_color(kind_col)
                                                .px_1()
                                                .rounded_sm()
                                                .border_1()
                                                .border_color(kind_col.alpha(0.42))
                                                .bg(kind_col.alpha(0.12))
                                                .child(da.kind.label()),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w(px(0.))
                                                .overflow_hidden()
                                                .text_size(px(10.5))
                                                .font_weight(gpui::FontWeight::BOLD)
                                                .text_color(th.text)
                                                .child(title),
                                        ),
                                )
                                .child(
                                    div()
                                        .overflow_hidden()
                                        .text_size(px(8.))
                                        .text_color(th.text.alpha(0.5))
                                        .child(meta_line),
                                ),
                        )
                        // ---- actions, tucked in: a right-aligned, vertically
                        // centred stack of compact pills (never full-height).
                        .child(
                            div()
                                .flex_none()
                                .flex()
                                .flex_col()
                                .items_end()
                                .justify_center()
                                .gap_1()
                                .when(can_harvest, |col| {
                                    // \u{2b07} harvest this dead session into a portable
                                    // .cdx (context-delight plugin, over MCP). Own handler
                                    // + stop_propagation so it never also resurrects.
                                    col.child(
                                        div()
                                            .id(SharedString::from(format!("harv-{di}")))
                                            .flex_none()
                                            .px_2()
                                            .py_0p5()
                                            .rounded_md()
                                            .border_1()
                                            .border_color(th.complement.alpha(0.5))
                                            .text_size(px(8.5))
                                            .font_weight(gpui::FontWeight::BOLD)
                                            .text_color(th.complement)
                                            .cursor_pointer()
                                            .hover(|s| s.bg(th.complement.alpha(0.16)))
                                            .child("\u{2b07} .cdx")
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                                    cx.stop_propagation();
                                                    ws.harvest_agent(sid_harvest.clone(), cx);
                                                }),
                                            ),
                                    )
                                })
                                .child(
                                    div()
                                        .flex_none()
                                        .px_2()
                                        .py_0p5()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(th.accent.alpha(0.5))
                                        .text_size(px(8.5))
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .text_color(th.accent)
                                        .bg(th.accent.alpha(0.08))
                                        .hover(|s| s.bg(th.accent.alpha(0.18)))
                                        .child("RESURRECT \u{23ce}"),
                                ),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                ws.resurrect_agent(
                                    cwd_click.clone(),
                                    resume_click.clone(),
                                    window,
                                    cx,
                                );
                            }),
                        ),
                );
                di += 1;
            }

            let vp_w = f32::from(window.viewport_size().width);
            let panel = div()
                .absolute()
                .top(px(vp_h * 0.10))
                .left(px(vp_w * 0.17))
                .right(px(vp_w * 0.17))
                .bottom(px(vp_h * 0.10))
                .overflow_hidden()
                .p_3()
                .rounded_md()
                .border_2()
                .border_color(th.accent.alpha(0.85))
                .bg(darken(th.surface, 0.6))
                .shadow(float_shadows(th.accent))
                .flex()
                .flex_col()
                .gap_2()
                .text_size(px(10.))
                .text_color(th.text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_size(px(13.))
                        .child(div().child("\u{1faa6}"))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                .text_color(th.complement)
                                .child("DEAD AGENTS"),
                        )
                        .child(
                            div()
                                .text_size(px(10.))
                                .text_color(th.text.alpha(0.55))
                                .child(format!("{n_claude} claude \u{00b7} {n_codex} codex")),
                        )
                        .child(div().flex_1().min_w(px(0.)))
                        .child(
                            div()
                                .text_color(th.text.alpha(0.7))
                                .child(format!("{n} recoverable")),
                        ),
                )
                .child(
                    div()
                        .text_size(px(8.5))
                        .text_color(th.accent.alpha(0.85))
                        .child(
                            "click a row to resurrect it \u{2014} or \u{2b07} .cdx to harvest its context into a portable package",
                        ),
                )
                .when_some(self.harvest_status.clone(), |d, msg| {
                    d.child(
                        div()
                            .text_size(px(9.))
                            .text_color(th.complement)
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(format!("\u{2b07} {msg}")),
                    )
                })
                .child(chips)
                .child(
                    list.min_h(px(0.))
                        .max_h(px((vp_h * 0.8 - 170.).max(160.)))
                        .overflow_y_scroll(),
                );

            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(th.bg.alpha(0.70))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        ws.dead_menu = false;
                        cx.notify();
                    }),
                )
                .child(panel)
        });

        let confirm_overlay = self.confirm_close.and_then(|i| {
            let name = self
                .tabs
                .get(i)?
                .name
                .clone()
                .unwrap_or_else(|| format!("tab {}", i + 1));
            let n = self.tab_pane_count(i);
            let danger = hsla(0., 0.72, 0.60, 1.);
            // Grammar adapts to a single shell vs a multi-pane tab.
            let (btn_label, body) = if n <= 1 {
                (
                    "CLOSE TAB".to_string(),
                    format!("Closing \u{201c}{name}\u{201d} ends its shell. This can\u{2019}t be undone."),
                )
            } else {
                (
                    format!("CLOSE {n} PANES"),
                    format!(
                        "\u{201c}{name}\u{201d} holds {n} panes \u{2014} closing it ends all {n} shells. This can\u{2019}t be undone."
                    ),
                )
            };
            let confirm_btn = div()
                .px_3()
                .py_1()
                .rounded_sm()
                .border_1()
                .border_color(danger)
                .bg(danger.alpha(0.22))
                .text_color(white().alpha(0.96))
                .text_size(px(12.))
                .font_weight(gpui::FontWeight::BOLD)
                .cursor_pointer()
                .hover(|s| s.bg(danger.alpha(0.4)))
                .child(btn_label)
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
            // A serious modal, framed against the outer bar (th.surface + an accent
            // halo): a solid danger warning banner over a dark body.
            let panel = div()
                .w(px(400.))
                .rounded_lg()
                .overflow_hidden()
                .border_2()
                .border_color(danger.alpha(0.9))
                .bg(darken(th.surface, 0.62))
                .shadow(float_shadows(danger))
                .text_color(th.text)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _: &MouseDownEvent, _w, cx| cx.stop_propagation()),
                )
                .child(
                    div()
                        .w_full()
                        .px_4()
                        .py_2()
                        .bg(danger.alpha(0.18))
                        .border_b_1()
                        .border_color(danger.alpha(0.55))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(div().text_size(px(18.)).child("\u{26a0}"))
                        .child(
                            div()
                                .text_size(px(15.))
                                .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                .text_color(danger)
                                .child("CLOSE TAB?"),
                        ),
                )
                .child(
                    div()
                        .p_4()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(th.text.alpha(0.85))
                                .child(body),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(th.text.alpha(0.45))
                                        .child("Enter to close \u{00b7} Esc to cancel"),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_row()
                                        .gap_2()
                                        .child(cancel_btn)
                                        .child(confirm_btn),
                                ),
                        ),
                );
            // full-window dim scrim; click outside cancels
            Some(
                div()
                    .absolute()
                    .inset_0()
                    .bg(hsla(0., 0., 0., 0.62))
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
        let help_features = self.help_features;
        // The language pack: every chrome string below comes from this table.
        let s = self.lang.strings();
        let cur_lang = self.lang;
        let help_overlay = self.help_open.then(|| {
            let (kc, dc, hc) = (th.accent, th.text.alpha(0.85), th.complement);
            let row = move |k: &str, d: &str| {
                div()
                    .flex()
                    .flex_row()
                    .gap_2()
                    .items_start()
                    .child(
                        div()
                            .min_w(px(150.))
                            .flex_none()
                            .text_color(kc)
                            .text_size(px(11.5))
                            .child(k.to_string()),
                    )
                    .child(
                        // flex_1 + min_w(0) lets a long (e.g. German) description
                        // wrap within the column instead of overflowing the panel.
                        div()
                            .flex_1()
                            .min_w(px(0.))
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
                    s.s_tabs,
                    vec![
                        row("Ctrl+Shift+T", s.new_tab),
                        row("Ctrl+PgUp / PgDn", s.switch_tabs),
                        row("Ctrl+Shift+PgUp / PgDn", s.move_tab),
                        row("Ctrl+Alt+R / D", s.split),
                        row(s.k_alt_arrows, s.move_focus),
                        row(s.k_drag_subtab, s.drag_subtab),
                        row(s.k_rclick_tab, s.rclick_tab),
                    ],
                ))
                .child(section(
                    s.s_edit,
                    vec![
                        row(s.k_rclick, s.rclick),
                        row("Ctrl+Shift+C / V", s.copy_paste),
                        row("Ctrl+X", s.cut),
                        row("Ctrl+F", s.find),
                        row("Ctrl+Shift+F", s.find_all),
                        row(s.k_dbl_click, s.select_wl),
                        row("Shift+Enter", s.newline),
                    ],
                ))
                .child(section(
                    s.s_links,
                    vec![row(s.k_shift_ctrl_click, s.open_link)],
                ));
            let col_b = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap_4()
                .child(section(
                    s.s_scroll,
                    vec![
                        row(s.k_scroll_wheel, s.scroll_hist),
                        row("Ctrl+Shift+K", s.clear_scroll),
                    ],
                ))
                .child(section(
                    s.s_look,
                    vec![
                        row(s.k_theme_icon, s.themes_wheel),
                        row("Ctrl+Shift+D", s.themes_wheel),
                        row(&format!("⛭ {}", s.k_display_tray), s.display_tray),
                        row("Ctrl+Shift+G", s.display_tray),
                        row(s.k_text_size_key, s.text_size),
                        row(s.k_warp_dial, s.warp),
                    ],
                ))
                .child(section(
                    s.s_agents,
                    vec![
                        row("Alt + ↑ / ↓", s.jump_msg),
                        row(&format!("▲ ▼ {}", s.k_pane_header), s.nav_msg),
                        row(&format!("👓 {}", s.k_pane_header), s.focus),
                        row(s.k_focus_inherit_key, s.focus_inherit),
                        row(s.k_wheel_key, s.pan_focus),
                        row(s.k_input_colour, s.input_colour),
                        row(s.k_bell_finish, s.bell),
                        row(&format!("🤖 {}", s.k_mother_bar), s.mcp),
                        row("Ctrl+Shift+A", s.mcp),
                    ],
                ))
                .child(section(s.s_window, vec![row("Ctrl+Alt+T", s.new_window)]));
            // The FEATURES view (toggled from the header) — there are far too many
            // to fit as shortcuts, so this is the "what can it even do" tour.
            let feat_a = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap_4()
                .child(section(
                    s.s_feat,
                    vec![
                        row(s.kf_tiling, s.f_tiling),
                        row(s.kf_groups, s.f_groups),
                        row(s.kf_drag, s.f_drag),
                        row(s.kf_rename, s.f_rename),
                        row(s.kf_popout, s.f_popout),
                    ],
                ))
                .child(section(
                    s.s_theming,
                    vec![
                        row(s.kf_themes, s.f_themes),
                        row(s.kf_perpane, s.f_perpane),
                        row(s.kf_wheel, s.f_wheel),
                        row(s.kf_grade, s.f_grade),
                    ],
                ));
            let feat_b = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w(px(0.))
                .gap_4()
                .child(section(
                    s.s_crt,
                    vec![
                        row(s.kf_warp, s.f_warp),
                        row(s.kf_scan, s.f_scan),
                        row(s.kf_crawl, s.f_crawl),
                        row(&format!("🎰 {}", s.gamba_name), s.f_gamba),
                    ],
                ))
                .child(section(
                    s.s_amcp,
                    vec![
                        row("claude / codex", s.f_detect),
                        row(&format!("👓 {}", s.focus_name), s.f_focus),
                        row(s.kf_restore, s.f_restore),
                        row(&format!("🤖 {}", s.kf_mcp), s.f_mcp),
                    ],
                ));
            // Header pills: SHORTCUTS ⇄ FEATURES. A million features don't fit as
            // keycaps, so a click swaps the body to the feature tour.
            let pill = |label: &str, active: bool, want: bool| {
                div()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .cursor_pointer()
                    .text_size(px(10.5))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(if active { th.bg } else { th.text.alpha(0.75) })
                    .when(active, |d| d.bg(th.accent))
                    .child(label.to_string())
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.help_features = want;
                            cx.notify();
                        }),
                    )
            };
            let tabs = div()
                .flex()
                .flex_row()
                .gap_1()
                .child(pill(s.shortcuts, !help_features, false))
                .child(pill(&format!("✦ {}", s.features), help_features, true));
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
            // Language-pack picker: click opens a fuzzy-search dropdown over all
            // languages (cycling was painful past a few langs).
            let lang_pick = div()
                .px_2()
                .py_0p5()
                .rounded_sm()
                .cursor_pointer()
                .text_size(px(10.5))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(th.complement)
                .border_1()
                .border_color(th.accent.alpha(0.5))
                .child(format!("🌐 {} ▾", cur_lang.native()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        ws.lang_picker = Some(LangPicker {
                            query: EditBuffer::seeded(""),
                            selected: 0,
                        });
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
                .border_2()
                .border_color(th.accent.alpha(0.85))
                .bg(darken(th.surface, 0.45))
                .text_color(th.text)
                .font_family(th.font_family.clone())
                .map(|mut d| {
                    // CJK/Devanagari fallback so a translated UI (中文 menus) renders
                    // real glyphs instead of tofu — only fires on a missing glyph.
                    if let Some(fb) = pane::script_fallbacks() {
                        d.text_style().font_fallbacks = Some(fb);
                    }
                    d
                })
                .shadow(float_shadows(th.accent))
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
                                .child(format!("▸ {} · {}", s.brand, s.help)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .child(tabs)
                                .child(lang_pick)
                                .child(close_x),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .w_full()
                        .gap_8()
                        .child(if help_features { feat_a } else { col_a })
                        .child(if help_features { feat_b } else { col_b }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Self::bezel_btn(
                                &th,
                                &format!("\u{1f5a5}\u{fe0f}  {}", s.demo_btn),
                                false,
                            )
                            .id("help-share-demo")
                            .hover(|s| s.border_color(th.accent))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.share_demo(cx);
                                    ws.help_open = false;
                                    cx.notify();
                                }),
                            ),
                        )
                        .child(
                            div()
                                .text_size(px(9.))
                                .text_color(th.text.alpha(0.45))
                                .child(s.demo_sub),
                        ),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(th.text.alpha(0.5))
                        .child(s.help_footer),
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

        // ---- tab config pane (right-click / ctrl+click a tab) ----
        // Rename, a two-pip colour wheel (▣ fill + T text) scoped to this tab or
        // its group, quick swatches, and group controls — like the other trays.
        let tab_menu_overlay = self
            .tab_menu
            .filter(|_| self.group_menu.is_none())
            .and_then(|i| {
                let tab = self.tabs.get(i)?;
                let label = tab.name.clone().unwrap_or_else(|| format!("tab {}", i + 1));
                let grouped = tab.group.is_some();
                let gid = tab.group;
                let at = self.tab_menu_at.unwrap_or_default();
                let pip = self.tab_pip;

                // rename this tab (closes the pane, opens the inline strip editor)
                let rename_btn = Self::bezel_btn(&th, "✎ rename tab", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        let seed = ws
                            .tabs
                            .get(i)
                            .and_then(|t| t.name.clone())
                            .unwrap_or_default();
                        ws.tab_menu = None;
                        ws.renaming = Some((i, EditBuffer::seeded(&seed)));
                        window.focus(&ws.focus_handle, cx);
                        cx.notify();
                    }),
                );

                // which pip the wheel + lightness slider drive
                let pip_row = div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(
                        Self::bezel_btn(&th, "▣ fill", pip == TabPip::Fill).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.tab_pip = TabPip::Fill;
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        Self::bezel_btn(&th, "T text", pip == TabPip::Text).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.tab_pip = TabPip::Text;
                                cx.notify();
                            }),
                        ),
                    );

                // quick fill swatches
                let mut swatches = div().flex().flex_row().flex_wrap().gap_1().max_w(px(184.));
                for &(h, s, l) in TAB_SWATCHES {
                    let c = hsla(h, s, l, 1.);
                    let hex = hsla_to_hex(c);
                    swatches = swatches.child(
                        div()
                            .id(SharedString::from(format!(
                                "tab-swatch-{i}-{}",
                                (h * 1000.) as i32
                            )))
                            .w(px(18.))
                            .h(px(18.))
                            .rounded_full()
                            .bg(c)
                            .cursor_pointer()
                            .border_1()
                            .border_color(hsla(0., 0., 0., 0.5))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                    cx.stop_propagation();
                                    ws.tab_pip = TabPip::Fill;
                                    ws.tab_set_pip(TabPip::Fill, Some(hex.clone()), cx);
                                }),
                            ),
                    );
                }

                // clear the active pip's override (no-op on a group's fill)
                let clear = Self::bezel_btn(&th, "clear", false).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                        cx.stop_propagation();
                        let p = ws.tab_pip;
                        ws.tab_set_pip(p, None, cx);
                    }),
                );

                // group controls
                let mut group_box = div().flex().flex_col().gap_1().child(
                    div()
                        .text_size(px(9.))
                        .text_color(th.text.alpha(0.7))
                        .child("group"),
                );
                if grouped {
                    // membership only — the group's own colour / name / fold / disband
                    // live on the group's right-click menu, never on a member tab.
                    group_box = group_box.child(
                        Self::bezel_btn(&th, "remove from group", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.remove_from_group(i, cx);
                            }),
                        ),
                    );
                } else {
                    group_box =
                        group_box.child(Self::bezel_btn(&th, "＋ new group", false).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.new_group_from(i, cx);
                            }),
                        ));
                }
                // add to an existing OTHER group
                let others: Vec<(u32, Hsla, String)> = self
                    .groups
                    .iter()
                    .filter(|g| Some(g.id) != gid)
                    .map(|g| {
                        (
                            g.id,
                            g.color,
                            g.name.clone().unwrap_or_else(|| format!("group {}", g.id)),
                        )
                    })
                    .collect();
                if !others.is_empty() {
                    let mut add_row = div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_1()
                        .max_w(px(184.))
                        .child(
                            div()
                                .text_size(px(9.))
                                .text_color(th.text.alpha(0.7))
                                .child("add to:"),
                        );
                    for (g_id, g_col, g_name) in others {
                        add_row = add_row.child(
                            div()
                                .id(SharedString::from(format!("addgrp-{i}-{g_id}")))
                                .px_1()
                                .py_0p5()
                                .rounded_sm()
                                .border_1()
                                .border_color(g_col)
                                .bg(g_col.alpha(0.3))
                                .cursor_pointer()
                                .text_size(px(10.))
                                .text_color(th.text)
                                .child(g_name)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                        cx.stop_propagation();
                                        ws.add_tab_to_group(i, g_id, cx);
                                    }),
                                ),
                        );
                    }
                    group_box = group_box.child(add_row);
                }

                let panel = div()
                    .absolute()
                    .left(px(f32::from(at.x)))
                    .top(px(f32::from(at.y) + 8.))
                    .p_2()
                    .rounded_md()
                    .border_1()
                    .border_color(th.accent.alpha(0.85))
                    .bg(darken(th.surface, 0.6))
                    .shadow(float_shadows(th.accent))
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
                            .text_color(th.text.alpha(0.85))
                            .child(format!("\u{201c}{label}\u{201d}")),
                    )
                    .child(rename_btn)
                    .child(pip_row)
                    .child(self.tab_color_wheel(i, cx))
                    .child(self.tab_lightness_bar(i, cx))
                    .child(swatches)
                    .child(div().flex().flex_row().justify_end().child(clear))
                    .child(group_box);
                // full-window scrim: a click anywhere else dismisses the pane
                Some(
                    div()
                        .absolute()
                        .inset_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                                ws.tab_menu = None;
                                cx.notify();
                            }),
                        )
                        .child(panel),
                )
            });

        // ---- group config menu (right-click a group handle / collapsed pill) ----
        // The group's OWN tabby controls: rename, a two-pip colour wheel (▣ fill +
        // T text) scoped to the GROUP, fold, and disband. `open_group_menu` points
        // the shared wheel at a representative member with Group scope, so the same
        // wheel / lightness / swatch widgets edit the group's colours directly.
        let group_menu_overlay = self.group_menu.and_then(|gid| {
            let g = self.groups.iter().find(|g| g.id == gid)?;
            let gname = g.name.clone().unwrap_or_else(|| format!("group {gid}"));
            let collapsed = g.collapsed;
            let at = self.group_menu_at.unwrap_or_default();
            let pip = self.tab_pip;
            let mi = self.tabs.iter().position(|t| t.group == Some(gid))?;

            let rename_btn = Self::bezel_btn(&th, "✎ rename group", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let seed = ws
                        .groups
                        .iter()
                        .find(|g| g.id == gid)
                        .and_then(|g| g.name.clone())
                        .unwrap_or_default();
                    ws.group_menu = None;
                    ws.tab_menu = None;
                    ws.group_rename = Some((gid, EditBuffer::seeded(&seed)));
                    window.focus(&ws.focus_handle, cx);
                    cx.notify();
                }),
            );
            let fold_btn =
                Self::bezel_btn(&th, if collapsed { "expand" } else { "collapse" }, false)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.toggle_group_collapsed(gid, cx);
                        }),
                    );
            let disband_btn = Self::bezel_btn(&th, "ungroup", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    ws.ungroup(gid, cx);
                }),
            );

            // which pip the wheel + lightness slider drive (fill vs text lead)
            let pip_row = div()
                .flex()
                .flex_row()
                .gap_1()
                .child(
                    Self::bezel_btn(&th, "▣ fill", pip == TabPip::Fill).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.tab_pip = TabPip::Fill;
                            cx.notify();
                        }),
                    ),
                )
                .child(
                    Self::bezel_btn(&th, "T text", pip == TabPip::Text).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            cx.stop_propagation();
                            ws.tab_pip = TabPip::Text;
                            cx.notify();
                        }),
                    ),
                );

            // quick fill swatches — write the group's fill via the shared pip path
            let mut swatches = div().flex().flex_row().flex_wrap().gap_1().max_w(px(184.));
            for &(h, s, l) in TAB_SWATCHES {
                let c = hsla(h, s, l, 1.);
                let hex = hsla_to_hex(c);
                swatches = swatches.child(
                    div()
                        .id(SharedString::from(format!(
                            "grp-swatch-{gid}-{}",
                            (h * 1000.) as i32
                        )))
                        .w(px(18.))
                        .h(px(18.))
                        .rounded_full()
                        .bg(c)
                        .cursor_pointer()
                        .border_1()
                        .border_color(hsla(0., 0., 0., 0.5))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |ws, _: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation();
                                ws.tab_pip = TabPip::Fill;
                                ws.tab_set_pip(TabPip::Fill, Some(hex.clone()), cx);
                            }),
                        ),
                );
            }

            // clear the active pip (a group's fill never clears; its text lead does)
            let clear = Self::bezel_btn(&th, "clear", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                    cx.stop_propagation();
                    let p = ws.tab_pip;
                    ws.tab_set_pip(p, None, cx);
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
                        .text_color(th.text.alpha(0.85))
                        .child(format!("group · {gname}")),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(rename_btn)
                        .child(fold_btn),
                )
                .child(pip_row)
                .child(self.tab_color_wheel(mi, cx))
                .child(self.tab_lightness_bar(mi, cx))
                .child(swatches)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .justify_between()
                        .child(clear)
                        .child(disband_btn),
                );
            Some(
                div()
                    .absolute()
                    .inset_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|ws, _: &MouseDownEvent, _w, cx| {
                            ws.group_menu = None;
                            ws.tab_menu = None;
                            ws.tab_scope = TabScope::ThisTab;
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
        let focus_overlay = if let Some(pane) = self.focus_read.as_ref().and_then(|w| w.upgrade()) {
            let snap = pane.update(cx, |v, cx| v.mirror_snapshot(cx));
            let (ww, wh) = self
                .last_win
                .map(|(_, _, w, h)| (w, h))
                .unwrap_or((1200., 800.));
            let panel_w = (ww * 0.8).max(320.);
            let panel_h = (wh * 0.8).max(240.);
            // Reader padding: a third roomier than the old 16px so the text
            // breathes inside the glass.
            let pad = 21.0_f32;
            let hdr_h = 30.0_f32;
            let avail_w = (panel_w - pad * 2.).max(1.);
            let avail_h = (panel_h - hdr_h - pad * 2.).max(1.);
            let content_w = (snap.cols as f32 * snap.cell_w).max(1.);
            let content_h = (snap.rows as f32 * snap.cell_h).max(1.);
            // Auto-fit (1.0 on the slider). A flat read WRAPS, so its width never
            // constrains the fit — scale to fill the panel HEIGHT so a wide-but-short
            // source (e.g. a directory listing) fills the glass instead of shrinking
            // to a tiny band with a big blank above it. Crawl doesn't wrap, so it
            // keeps the both-axes fit that guarantees each whole row stays visible.
            let fit = if snap.crawl {
                (avail_w / content_w).min(avail_h / content_h)
            } else {
                avail_h / content_h
            }
            .clamp(0.7, 6.0);
            // The header slider rides on top of the fit: 1.0 = fit-to-modal. The
            // reader WRAPS rather than scrolling sideways, so a bigger size never
            // runs lines off the edge — it just wraps more; a smaller size shows a
            // compact column. The floor sits well under 1.0 so a dense read can
            // shrink right down (the old 0.5 floor still felt huge on small panes).
            let ms = (fit * self.focus_zoom).clamp(0.3, 12.0);
            let cell_h = snap.cell_h * ms;
            let glyph_w = (snap.cell_w * ms).max(0.5);
            // "Inherit theme": bend + glare the panel like the pane it mirrors.
            let inherit = self.focus_inherit_theme;
            let (k1, k2, glare) = (snap.k1, snap.k2, snap.glare);
            // FOCUS inherits crawl: the rows are already in the crawl font (baked
            // into the runs) and, in crawl mode, the modal centres each row exactly
            // like the live pane (a flat, readable mirror — the ambient perspective
            // stays on the pane behind the modal). Crawl keeps its own centred,
            // unwrapped rows; only the flat reader wraps.
            let crawl = snap.crawl;
            // Wrap-to-fit: how many glyphs fit across the inner width at this size.
            // Every grid row soft-wraps to this, so a flat read can NEVER overflow
            // horizontally — there is no sideways scroll any more.
            let fit_cols = (avail_w / glyph_w).floor().max(1.0) as usize;
            let vrows = if crawl {
                Vec::new()
            } else {
                pane::wrap_focus_lines(&snap.lines, fit_cols)
            };
            // Exact content height — crawl is one row per grid row, a wrapped read
            // one row per visual (wrapped) row. Counted, never measured.
            let row_count = if crawl { snap.rows } else { vrows.len() };
            let total_h = row_count as f32 * cell_h;
            self.focus_overflow = (total_h - avail_h).max(0.0);
            self.focus_line_h = cell_h;
            self.focus_scroll_y = self.focus_scroll_y.clamp(0.0, self.focus_overflow);
            let scroll_y = self.focus_scroll_y;
            // Centre a short read vertically: when the whole thing fits, split the
            // slack evenly above and below so it sits balanced in the glass — no
            // blank band jammed against the top OR the bottom. Once it overflows
            // this is 0 and the wheel pans `scroll_y` from the top.
            let v_offset = (avail_h - total_h).max(0.0) * 0.5;
            // Horizontal frame: size it to the FULL grid width at this zoom (capped
            // at the panel), NOT to the live content. Sizing to content made the
            // whole block re-centre and JIGGLE sideways every time a line's length
            // changed — fatal for a reader whose eye is locked on one spot. The grid
            // width is fixed, so the left edge never moves. When the grid is narrower
            // than the glass (a zoomed-down read) this fixed frame is centred;
            // otherwise it fills and left-anchors at `pad`.
            let body_w = if crawl {
                avail_w
            } else {
                (snap.cols as f32 * glyph_w).min(avail_w)
            };
            let h_offset = ((avail_w - body_w) * 0.5).max(0.0);
            // Left inset of the content this frame — the hit-test maps clicks against
            // it, so it must include the centring offset.
            let content_left = pad + h_offset;
            let base_size = snap.base_size * ms;
            // Stash this frame's wrapped layout so a click in the reading area maps
            // back to a source cell for selection + copy. Crawl gets an empty map
            // (no selection over the perspective mode).
            let map_rows: Vec<(usize, usize, usize)> = if crawl {
                Vec::new()
            } else {
                vrows
                    .iter()
                    .map(|v| (v.src_row, v.src_col0, v.cols))
                    .collect()
            };
            *self.focus_map.lock().unwrap() = Some(FocusMap {
                rows: map_rows,
                src_lines: snap.lines.iter().map(|(t, _)| t.clone()).collect(),
                line_h: cell_h,
                glyph_w,
                pad: content_left,
                top: pad + v_offset - scroll_y,
                k1,
                k2,
                inherit,
            });
            // The current selection (normalised so start ≤ end) and the tint we
            // paint it with — captured by value for the body closure below.
            let sel = self
                .focus_sel
                .map(|(a, b)| if a <= b { (a, b) } else { (b, a) });
            let sel_hl = snap.accent.alpha(0.30);
            // Crawl self-centres each row across the full inner width; a wrapped
            // flat read fills that width and left-aligns (it has no sideways axis).
            let body = if crawl {
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(body_w))
                    .text_size(px(base_size))
                    .text_color(snap.text)
                    .font_family(snap.font_family.clone())
                    .children(snap.lines.into_iter().map(move |(text, runs)| {
                        match pane::crawl_centered_runs(text, runs) {
                            Some((t, cut)) => div()
                                .h(px(cell_h))
                                .flex()
                                .justify_center()
                                .whitespace_nowrap()
                                .child(gpui::StyledText::new(t).with_runs(cut)),
                            None => div().h(px(cell_h)).whitespace_nowrap(),
                        }
                    }))
            } else {
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .w(px(body_w))
                    .text_size(px(base_size))
                    .text_color(snap.text)
                    .font_family(snap.font_family.clone())
                    .children(vrows.into_iter().map(move |vr| {
                        let line = div().h(px(cell_h)).whitespace_nowrap();
                        if vr.text.is_empty() {
                            return line;
                        }
                        // Paint the selected glyph span on this wrapped row (if the
                        // selection covers it), mapping the source-cell range down to
                        // this visual row's local columns.
                        let runs = match sel {
                            Some((s, en)) if vr.src_row >= s.0 && vr.src_row <= en.0 => {
                                let lo = if vr.src_row == s.0 {
                                    s.1.max(vr.src_col0)
                                } else {
                                    vr.src_col0
                                };
                                let hi = if vr.src_row == en.0 {
                                    en.1 + 1
                                } else {
                                    vr.src_col0 + vr.cols
                                };
                                let lf = lo.saturating_sub(vr.src_col0).min(vr.cols);
                                let lt = hi.saturating_sub(vr.src_col0).min(vr.cols);
                                pane::highlight_runs(&vr.text, &vr.runs, lf, lt, sel_hl)
                            }
                            _ => vr.runs,
                        };
                        line.child(gpui::StyledText::new(vr.text).with_runs(runs))
                    }))
            };
            let header = div()
                .h(px(hdr_h))
                .flex_none()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .px_3()
                .gap_3()
                .text_size(px(12.))
                .text_color(snap.accent)
                .child(
                    // left cluster: title + the persistent inherit-theme toggle
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .child(format!("👓  FOCUS · {}", snap.title))
                        .child(self.focus_inherit_toggle(snap.accent, snap.text, cx)),
                )
                // text-size slider for the pane under scrutiny (live, per-open)
                .child(self.focus_zoom_slider(snap.accent, snap.text, cx))
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
                    // The reading area: a clip box with the mirror absolutely
                    // anchored inside it. Absolute positioning (not a flex child +
                    // margins) pins the vertical origin deterministically:
                    // `top = pad + v_offset - scroll_y`. `v_offset` centres a short
                    // read (slack split top/bottom); once the read overflows it's 0
                    // and the wheel pans `scroll_y` from the top. The fixed-width
                    // frame is centred horizontally at `content_left`. A press here
                    // starts a click-drag text selection (mapped back to a source
                    // cell through the warp); the trailing probe canvas captures this
                    // box's exact on-screen rect so the hit-test stays curve-accurate.
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_hidden()
                        .relative()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|ws, ev: &MouseDownEvent, _w, cx| {
                                cx.stop_propagation(); // don't let the panel/scrim close
                                match ws.focus_cell_at(ev.position) {
                                    Some(cell) => {
                                        ws.focus_sel = Some((cell, cell));
                                        ws.focus_sel_drag = true;
                                    }
                                    None => ws.focus_sel = None,
                                }
                                cx.notify();
                            }),
                        )
                        .child(
                            div()
                                .absolute()
                                .top(px(pad + v_offset - scroll_y))
                                .left(px(content_left))
                                .flex()
                                .flex_col()
                                .child(body),
                        )
                        .child(
                            div().absolute().inset_0().child(
                                gpui::canvas(
                                    {
                                        let store = self.focus_body_bounds.clone();
                                        move |bounds, _window, _cx| {
                                            *store.lock().unwrap() = Some(bounds);
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .size_full(),
                            ),
                        ),
                )
                // Measure the panel's exact on-screen box (physical px) and arm
                // the FOCUS backdrop blur: the CRT post-pass frosts everything
                // outside this rect while the panel itself stays razor-sharp.
                // Using the real prepaint bounds (not an analytic centre) keeps
                // the sharp/blur edge pixel-aligned through the CSD shadow margin.
                // When "Inherit theme" is on, the same rect is also registered as
                // the lone warp tube so the reader bends + glares like its pane.
                .child(
                    div().absolute().inset_0().child(
                        gpui::canvas(
                            move |bounds, window, _cx| {
                                let sf = window.scale_factor();
                                let rect = [
                                    f32::from(bounds.origin.x) * sf,
                                    f32::from(bounds.origin.y) * sf,
                                    f32::from(bounds.size.width) * sf,
                                    f32::from(bounds.size.height) * sf,
                                ];
                                crate::warp::set_focus_blur(
                                    rect,
                                    28.0 * sf * focus_ramp, // blur radius (eases in)
                                    16.0 * sf,              // feather across the panel edge
                                    focus_ramp,             // frosted-glass tint (eases in)
                                    12.0 * sf,              // corner radius — matches rounded(12)
                                );
                                if inherit {
                                    // Warp ONLY the reading area, never the header
                                    // chrome. The slider + "Inherit theme" toggle are
                                    // gpui hit-boxes at their flat layout positions, so
                                    // bending them with the glass makes their clicks
                                    // land off-target (you have to aim well above the
                                    // control). Insetting the tube's top past the
                                    // header keeps the chrome flat + honest — the same
                                    // rule the per-pane tubes follow — while the body
                                    // still curves. crawl stays identity: the body
                                    // already centres crawl rows for readable mirroring.
                                    let body_rect = [
                                        rect[0],
                                        rect[1] + hdr_h * sf,
                                        rect[2],
                                        (rect[3] - hdr_h * sf).max(1.0),
                                    ];
                                    crate::warp::register_focus_tube(
                                        body_rect,
                                        glare,
                                        k1,
                                        k2,
                                        [0.0, 1.0, 1.0],
                                    );
                                }
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
            Some(
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
                    // The reader wraps, so it only ever overflows VERTICALLY; the
                    // wheel pans it so the off-screen rows are reachable. There is no
                    // horizontal axis to pan. At the top/bottom edge (or when the read
                    // already fits) the wheel falls through to the read pane's own
                    // scrollback — the wheel is never lost.
                    .on_scroll_wheel(cx.listener(|ws, ev: &ScrollWheelEvent, _w, cx| {
                        let dy = match ev.delta {
                            gpui::ScrollDelta::Lines(l) => l.y * ws.focus_line_h,
                            gpui::ScrollDelta::Pixels(p) => f32::from(p.y),
                        };
                        if ws.focus_overflow > 0.0 {
                            let next = (ws.focus_scroll_y - dy).clamp(0.0, ws.focus_overflow);
                            if (next - ws.focus_scroll_y).abs() > f32::EPSILON {
                                ws.focus_scroll_y = next;
                                cx.notify();
                                return;
                            }
                        }
                        if let Some(pane) = ws.focus_read.as_ref().and_then(|w| w.upgrade()) {
                            pane.update(cx, |v, cx| v.scroll_by_wheel(ev, cx));
                            cx.notify();
                        }
                    }))
                    .child(panel),
            )
        } else {
            None
        };

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
            // Esc closes whatever popup floats above the workspace — caught in the
            // CAPTURE phase so it fires even while a terminal pane holds focus
            // (the terminal never sees the Esc, so it can't be sent to the shell).
            // With no popup open, Esc falls through to the focused terminal as usual.
            .capture_key_down(cx.listener(|ws, ev: &KeyDownEvent, _w, cx| {
                if ev.keystroke.key.as_str() == "escape" && ws.close_popups() {
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
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
                    .children(mcp_overlay)
                    .children(dead_overlay)
                    .children(plugins_overlay)
                    .children(savings_overlay)
                    .children(confirm_overlay)
                    .children(help_overlay)
                    .children(tab_menu_overlay)
                    .children(group_menu_overlay)
                    .children(drag_chip)
                    // the FOCUS reading modal rides above everything else
                    .children(focus_overlay)
                    // the find panel rides on top too (its own scrim locks input)
                    .children(find_overlay)
                    // the language dropdown rides above even the help modal it opens from
                    .children(lang_picker_overlay),
            );
        // Frameless: wrap the cabinet in client-side decorations (shadow margin,
        // rounded clip, live resize edges) so it runs with no system titlebar.
        csd::decorate(root, window)
    }
}

// Helpers + main() live below the tests by design; silence the stylistic lint.
#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn savings_view_parses_plugin_payload() {
        let json = r#"{
            "ok": true, "tokens_saved": 66080267, "gain_pct": 81.8, "usd": 165.2,
            "score": 70, "level": "Lv4 Guardian", "agent_count": 2,
            "agents": [
              {"id":"mcp-1-abcd","type":"claude-code","calls":41,"saved_est":1541782,"last_seen":"2026-06-22T07:20:07Z"},
              {"id":"mcp-2-ef01","type":"codex-mcp-client","calls":7,"saved_est":12000,"last_seen":"2026-06-22T06:00:00Z"}
            ],
            "top_files": [{"path":"/a/b/main.rs","saved":10131759,"pct":98.2}],
            "note": "context saved by lean-ctx"
        }"#;
        let v = SavingsView::from_json(json).expect("parses");
        assert_eq!(v.tokens_saved, 66080267);
        assert_eq!(v.score, 70);
        assert_eq!(v.agent_count, 2);
        assert_eq!(v.agents.len(), 2);
        assert_eq!(v.agents[0].kind, "claude-code");
        assert_eq!(v.agents[0].saved_est, 1541782);
        assert_eq!(v.top_files.len(), 1);
        assert_eq!(v.top_files[0].saved, 10131759);
        // garbage / wrong shape → None (caller shows raw text instead)
        assert!(SavingsView::from_json("not json").is_none());
    }

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

    fn mods(ctrl: bool, shift: bool) -> gpui::Modifiers {
        gpui::Modifiers {
            control: ctrl,
            shift,
            ..Default::default()
        }
    }

    #[test]
    fn edit_buffer_navigates_selects_and_edits_like_a_text_field() {
        // seeded selects all → first printable replaces the lot
        let mut eb = EditBuffer::seeded("oldname");
        assert!(eb.has_sel());
        eb.apply("x", &mods(false, false), Some("x"), 18);
        assert_eq!(eb.text(), "x");
        assert!(!eb.has_sel());

        // build "foo bar", caret at end
        let mut eb = EditBuffer::default();
        for ch in ["f", "o", "o", " ", "b", "a", "r"] {
            eb.apply(ch, &mods(false, false), Some(ch), 18);
        }
        assert_eq!(eb.text(), "foo bar");
        assert_eq!(eb.cursor, 7);

        // ctrl+left jumps a word; again jumps to the next word boundary
        eb.apply("left", &mods(true, false), None, 18);
        assert_eq!(eb.cursor, 4); // start of "bar"
        eb.apply("left", &mods(true, false), None, 18);
        assert_eq!(eb.cursor, 0); // start of "foo"

        // shift+right extends a char selection; shift+ctrl+right extends by word
        eb.apply("right", &mods(false, true), None, 18);
        assert_eq!(eb.sel_range(), (0, 1));
        eb.apply("right", &mods(true, true), None, 18);
        assert_eq!(eb.sel_range(), (0, 3)); // through "foo"

        // typing over the selection replaces just it
        eb.apply("Z", &mods(false, false), Some("Z"), 18);
        assert_eq!(eb.text(), "Z bar");

        // ctrl+backspace deletes the previous word
        eb.apply("end", &mods(false, false), None, 18);
        eb.apply("backspace", &mods(true, false), None, 18);
        assert_eq!(eb.text(), "Z ");

        // ctrl+a selects all, then backspace clears
        eb.apply("a", &mods(true, false), None, 18);
        assert_eq!(eb.sel_range(), (0, eb.chars.len()));
        eb.apply("backspace", &mods(false, false), None, 18);
        assert_eq!(eb.text(), "");
    }

    #[test]
    fn edit_buffer_caps_inserted_length() {
        let mut eb = EditBuffer::default();
        for _ in 0..30 {
            eb.apply("a", &mods(false, false), Some("a"), 18);
        }
        assert_eq!(eb.chars.len(), 18);
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
    fn scratch_decision_covers_force_seed_and_master() {
        // lone launch, no live master → primary restore, no seed
        let (scratch, seed) = scratch_decision(false, false, None, None);
        assert!(!scratch);
        assert!(seed.is_none());

        // a live master already holds the session → scratch, still no seed
        let (scratch, seed) = scratch_decision(false, true, None, None);
        assert!(scratch);
        assert!(seed.is_none());

        // forced scratch with no master (TD_SCRATCH=1)
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
    fn master_lock_is_exclusive_and_releases() {
        // Unique path per run so the test is parallel-safe and env-free.
        let path = std::env::temp_dir().join(format!(
            "td-master-lock-test-{}-{}.lock",
            std::process::id(),
            line!(),
        ));
        let _ = std::fs::remove_file(&path);

        // First taker wins and holds the fd.
        let held = try_lock_at(&path)
            .expect("open ok")
            .expect("first acquires");
        // While held, a second non-blocking attempt is refused.
        assert!(try_lock_at(&path).is_err(), "second taker must be refused");

        // Releasing (dropping the fd) frees the lock for the next taker.
        drop(held);
        let again = try_lock_at(&path)
            .expect("open ok")
            .expect("re-acquires after release");
        drop(again);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn richness_guard_blocks_unintended_collapse_only() {
        // A catastrophic, UNINTENDED shrink (8→1 panes) is refused.
        assert!(is_catastrophic_shrink(8, 4, 1, 1, false));
        // …but the SAME shrink is allowed when the user explicitly closed.
        assert!(!is_catastrophic_shrink(8, 4, 1, 1, true));
        // A mass tab collapse (10→1 tabs) is refused.
        assert!(is_catastrophic_shrink(10, 10, 1, 1, false));
        // Normal incremental closes are never blocked (8→7, 4→3 tabs).
        assert!(!is_catastrophic_shrink(8, 4, 7, 4, false));
        assert!(!is_catastrophic_shrink(6, 4, 6, 3, false));
        // Growth / steady state never blocked.
        assert!(!is_catastrophic_shrink(3, 2, 9, 5, false));
        // Tiny sessions are exempt (a 2→1 close is normal, not catastrophic).
        assert!(!is_catastrophic_shrink(2, 1, 1, 1, false));
        // The first-ever save (nothing on disk) is never blocked.
        assert!(!is_catastrophic_shrink(0, 0, 3, 3, false));
    }

    #[test]
    fn count_saved_leaves_walks_the_split_tree() {
        let leaf = || SavedNode::Leaf {
            appearance: PaneTheme::default(),
            cwd: None,
            resume: None,
            name: None,
        };
        assert_eq!(count_saved_leaves(&leaf()), 1);
        let split = SavedNode::Split {
            dir: SplitDir::Row,
            ratio: 0.5,
            a: Box::new(leaf()),
            b: Box::new(SavedNode::Split {
                dir: SplitDir::Col,
                ratio: 0.5,
                a: Box::new(leaf()),
                b: Box::new(leaf()),
            }),
        };
        assert_eq!(count_saved_leaves(&split), 3);
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
            text_color: None,
            group: None,
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
                text_color: None,
                group: None,
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
            groups: vec![],
            mcp: None,
            focus_inherit: false,
            lang: lang::Lang::default(),
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
                text_color: None,
                group: None,
                node: SavedNode::Leaf {
                    appearance: PaneTheme::default(),
                    cwd: Some("/home/user/proj".into()),
                    resume: Some("claude --resume 48be90b8-5777-44b6-bb6f-1c6069205c0d".into()),
                    name: None,
                },
            }],
            groups: vec![],
            mcp: None,
            focus_inherit: false,
            lang: lang::Lang::default(),
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
    fn tab_groups_and_per_tab_colours_round_trip() {
        // a grouped tab with its own fill + text override, plus a group carrying
        // its own colours + collapsed state, must all survive serialize→reload.
        let leaf = || SavedNode::Leaf {
            appearance: PaneTheme::default(),
            cwd: None,
            resume: None,
            name: None,
        };
        let state = StateFile {
            active: 0,
            win: None,
            scale: None,
            theme: None,
            warp: theme::WARP_DEFAULT,
            track: None,
            tabs: vec![
                SavedTab {
                    name: Some("HOME".into()),
                    color: Some("#aa3344".into()),
                    text_color: Some("#ffffff".into()),
                    group: Some(7),
                    node: leaf(),
                },
                SavedTab {
                    name: Some("loose".into()),
                    color: None,
                    text_color: None,
                    group: None,
                    node: leaf(),
                },
            ],
            groups: vec![SavedGroup {
                id: 7,
                name: Some("WORK".into()),
                color: "#2d8f4d".into(),
                text_color: Some("#101010".into()),
                collapsed: true,
            }],
            mcp: None,
            focus_inherit: false,
            lang: lang::Lang::default(),
        };
        let body = toml::to_string(&state).expect("serializes");
        let back: StateFile = toml::from_str(&body).expect("round-trips");
        assert_eq!(back.tabs[0].group, Some(7));
        assert_eq!(back.tabs[0].text_color.as_deref(), Some("#ffffff"));
        assert_eq!(back.tabs[1].group, None);
        assert_eq!(back.groups.len(), 1);
        assert_eq!(back.groups[0].id, 7);
        assert_eq!(back.groups[0].name.as_deref(), Some("WORK"));
        assert!(back.groups[0].collapsed);
        assert_eq!(back.groups[0].text_color.as_deref(), Some("#101010"));
    }

    #[test]
    fn pre_feature_state_loads_without_groups_or_text_colour() {
        // a state.toml written before tab groups existed (no `groups`, no
        // per-tab `text_color`/`group`) must still deserialize cleanly.
        let toml = r#"
active = 0
warp = 1.43
[[tabs]]
name = "old"
node = "Leaf"
"#;
        let back: StateFile = toml::from_str(toml).expect("legacy state loads");
        assert_eq!(back.tabs.len(), 1);
        assert_eq!(back.tabs[0].text_color, None);
        assert_eq!(back.tabs[0].group, None);
        assert!(back.groups.is_empty());
        // pre-feature files have no `focus_inherit` → the flat reader default.
        assert!(
            !back.focus_inherit,
            "missing key defaults to the flat reader"
        );
    }

    #[test]
    fn focus_inherit_preference_round_trips() {
        // The global "Inherit theme" reader preference survives a save/load.
        let state = StateFile {
            focus_inherit: true,
            ..Default::default()
        };
        let body = toml::to_string(&state).expect("serializes");
        let back: StateFile = toml::from_str(&body).expect("round-trips");
        assert!(back.focus_inherit, "the inherit-theme toggle persists");
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
    fn lang_picker_fuzzy_filters_by_autonym_english_and_code() {
        use lang::Lang;
        // empty query → all languages, canonical order
        assert_eq!(Workspace::filtered_langs(""), Lang::ALL.to_vec());
        assert_eq!(Workspace::filtered_langs("   "), Lang::ALL.to_vec());
        // English name finds the language
        assert_eq!(Workspace::filtered_langs("german").first(), Some(&Lang::De));
        assert_eq!(
            Workspace::filtered_langs("japanese").first(),
            Some(&Lang::Ja)
        );
        // two-letter code jumps straight to it
        assert_eq!(Workspace::filtered_langs("zh").first(), Some(&Lang::Zh));
        // autonym (its own script) matches
        assert_eq!(Workspace::filtered_langs("中文").first(), Some(&Lang::Zh));
        assert_eq!(
            Workspace::filtered_langs("español").first(),
            Some(&Lang::Es)
        );
        // garbage matches nothing
        assert!(Workspace::filtered_langs("zzzzq").is_empty());
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
                text_color: None,
                group: None,
                node,
            }],
            groups: vec![],
            mcp: None,
            focus_inherit: false,
            lang: lang::Lang::default(),
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

/// Resolve scratch-mode + an optional seed from the inputs. Factored out (pure)
/// so the env/lock plumbing in `main` stays testable. `master_taken` is true when
/// a live MASTER already holds the session lock (see [`acquire_master_lock`]), so
/// this launch must open a non-persisting scratch window instead of restoring.
fn scratch_decision(
    force: bool,
    master_taken: bool,
    cwd: Option<String>,
    resume: Option<String>,
) -> (bool, Option<session::PaneRestore>) {
    let seeded = cwd.is_some() || resume.is_some();
    let seed = if seeded {
        Some(session::PaneRestore { cwd, resume })
    } else {
        None
    };
    (force || seeded || master_taken, seed)
}

/// The MASTER-window lock. Exactly one live terminal-delight process holds this
/// advisory file lock at a time; that process is THE master and the sole owner of
/// the saved session (`state.toml`) — it restores the full layout on boot and is
/// the only window that writes changes back. Every OTHER concurrently-running
/// window — the Ctrl+Alt+T quick window, a torn-off pane, or a fresh launch that
/// races a still-shutting-down master — fails to take this lock and boots as a
/// non-persisting *scratch* window, so it can never clobber the master's layout.
///
/// Held for the process lifetime (the kernel drops it on exit, covering crashes /
/// SIGKILL) and released EARLY at quit-start via [`release_master_lock`] wired
/// into `on_app_quit`. That early release is the whole point: a close → immediate
/// reopen re-acquires cleanly and RESTORES, instead of the old `/proc` comm-scan
/// mistaking the dying master for a live peer and degrading the reopen to a lone
/// scratch terminal.
static MASTER_LOCK: std::sync::Mutex<Option<std::os::fd::OwnedFd>> = std::sync::Mutex::new(None);

/// Per-user path for the master lock. `$XDG_RUNTIME_DIR` (tmpfs, cleared on
/// logout) is ideal; fall back to the temp dir if it is unset.
fn master_lock_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("terminal-delight-master.lock")
}

/// Try to become THE master by taking the advisory lock without blocking.
/// Returns true if we got it (this process is now master and must restore +
/// persist the session); false if a live master already holds it (this launch
/// should open a scratch window). On a lock-file open error we fail OPEN —
/// returning true — so a permissions glitch can never trap the user in a
/// single scratch terminal with no way back to their session.
fn acquire_master_lock() -> bool {
    match try_lock_at(&master_lock_path()) {
        // Got it (or the lock file was unopenable → fail OPEN as master).
        Ok(Some(fd)) => {
            *MASTER_LOCK.lock().unwrap() = Some(fd);
            true
        }
        Ok(None) => true,
        // A live master holds the lock → we are a scratch window.
        Err(()) => false,
    }
}

/// The path-free core of [`acquire_master_lock`], split out so it is unit-testable
/// without touching process env or the `MASTER_LOCK` static. `Ok(Some(fd))` = we
/// took the lock (hold the fd to keep it); `Err(())` = a live holder has it;
/// `Ok(None)` = the lock file could not be opened (caller treats this as master,
/// failing open so a glitch never traps the user in scratch mode).
fn try_lock_at(path: &std::path::Path) -> Result<Option<std::os::fd::OwnedFd>, ()> {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)
    {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    // SAFETY: a valid fd; LOCK_NB guarantees flock never blocks.
    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc != 0 {
        // EWOULDBLOCK → a live master already holds the lock.
        return Err(());
    }
    // Stamp our pid (purely informational for `cat`-ing the lock file).
    let _ = file.set_len(0);
    let mut f = &file;
    let _ = f.write_all(format!("{}\n", std::process::id()).as_bytes());
    Ok(Some(std::os::fd::OwnedFd::from(file)))
}

/// Release the master lock immediately. Called at quit-start (before the slow
/// PTY/GPU teardown that makes a closing instance linger for seconds), so a
/// close → reopen re-elects a master and restores instead of seeing a dying
/// peer. Idempotent.
fn release_master_lock() {
    use std::os::fd::AsRawFd;
    if let Some(fd) = MASTER_LOCK.lock().unwrap().take() {
        // SAFETY: a valid, still-open fd we own.
        unsafe { libc::flock(fd.as_raw_fd(), libc::LOCK_UN) };
        // fd dropped here → file closed.
    }
}

/// Launch a fresh, detached terminal-delight seeded with a torn-off pane's cwd
/// and agent session. It is launched with TD_SCRATCH=1 so it boots as a scratch
/// window (never contends for the master lock); the seed env tells it what to
/// reopen.
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
    // `--td-emit-demo`: this process was spawned as a demo pane's program (see
    // `term::spawn_in` under TD_DEMO). Print a screenful of agentic lorem-ipsum
    // sized to the PTY and block — no window, no shell. Must run before any gpui.
    if std::env::args().nth(1).as_deref() == Some("--td-emit-demo") {
        demo::emit_and_block();
    }

    // Give every shell we spawn a real terminal type. gpui launches us from the
    // desktop/WM with TERM unset, and alacritty_terminal's `tty::new` does NOT
    // set one — so without this, child shells inherit an empty TERM, readline
    // can't look up the `clear_screen` capability, and Ctrl+L silently no-ops
    // (the prompt never pops to the top). `setup_env` picks the `alacritty`
    // terminfo if installed, else the universally-present `xterm-256color`, and
    // advertises 24-bit colour (COLORTERM=truecolor). Must run before any PTY is
    // spawned; it mutates the process env, so keep it ahead of the gpui app/threads.
    alacritty_terminal::tty::setup_env();

    // Decide boot mode before the window opens. An EXPLICITLY-scratch launch —
    // forced scratch (TD_SCRATCH, the Ctrl+Alt+T quick window), a seeded tear-off
    // (TD_SEED_*), or a demo (TD_DEMO_STATE) — opens a small single-terminal
    // window and never contends to own the session. ANY other launch tries to
    // take the MASTER lock: winning means "restore the full session and own
    // `state.toml`"; losing means a live master already has the window open, so
    // this becomes a scratch window too. The lock (held by exactly one live
    // process, released early at quit-start) replaces the old `/proc` comm-scan,
    // which counted a still-shutting-down master as a live peer and dropped a
    // plain close→reopen to a single scratch terminal.
    let force = std::env::var_os("TD_SCRATCH").is_some();
    let seed_cwd = std::env::var("TD_SEED_CWD").ok().filter(|s| !s.is_empty());
    let seed_resume = std::env::var("TD_SEED_RESUME")
        .ok()
        .filter(|s| !s.is_empty());
    // A demo window (spawned by "Share a demo of this layout") restores a cloned
    // layout from TD_DEMO_STATE and fills every pane with the frozen emitter.
    let demo = std::env::var_os("TD_DEMO_STATE").is_some();
    let explicit_scratch = force || seed_cwd.is_some() || seed_resume.is_some() || demo;
    // Only a would-be master takes the lock; explicit-scratch launches leave it
    // untouched so they never steal it from (or wait on) the real master.
    let master_taken = if explicit_scratch {
        false
    } else {
        !acquire_master_lock()
    };
    let (scratch, seed) = scratch_decision(force, master_taken, seed_cwd, seed_resume);

    application().run(move |cx: &mut App| {
        theme::init(cx);
        bell::ensure_seeded(); // populate the sounds dir from bundled defaults if empty
                               // Release the MASTER lock at the very start of shutdown — `on_app_quit`
                               // handlers run BEFORE windows/PTYs tear down (App::shutdown), so this
                               // frees the lock seconds ahead of actual process exit. That is what lets
                               // a close → immediate reopen re-elect a master and restore, instead of
                               // the reopen racing the closing process's PTY teardown linger.
        cx.on_app_quit(|_cx| {
            release_master_lock();
            async move {}
        })
        .detach();
        let bounds = if demo {
            // open the demo at the cloned window's geometry, else a generous centre
            match load_demo_state().win {
                Some((x, y, w, h)) => Bounds {
                    origin: point(px(x), px(y)),
                    size: size(px(w.max(480.)), px(h.max(320.))),
                },
                None => Bounds::centered(None, size(px(1280.), px(720.)), cx),
            }
        } else if scratch {
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
                // Bundle the crawl-mode typeface (News Cycle Bold, SIL OFL — a
                // libre News-Gothic clone) so `crawl` mode has its font even on a
                // box that doesn't ship it. Registered BEFORE the font registry is
                // captured so `all_font_names()` includes it and `resolve_family`
                // can find "News Cycle".
                if let Err(e) = cx.text_system().add_fonts(vec![std::borrow::Cow::Borrowed(
                    include_bytes!("../assets/fonts/NewsCycle-Bold.ttf").as_slice(),
                )]) {
                    eprintln!("terminal-delight: failed to load crawl font: {e}");
                }
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
                if demo {
                    cx.new(|cx| Workspace::new_demo(window, cx))
                } else if scratch {
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
