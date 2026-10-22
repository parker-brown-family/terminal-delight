//! What chrome a window draws — the fourth axis, beside palette, finish and
//! trim.
//!
//! Terminal Delight already separates three things a person chooses about how
//! it looks: the PALETTE (what hue everything is, `palette.rs`), the DESIGN
//! (how the glass is finished, `theme.rs`) and the SKIN (the chrome's shape,
//! `skin.rs`). None of them could put a piece of chrome away. A window drew
//! every control it had, all the time; the left bar was the only thing a
//! person could hide.
//!
//! A **UI set** closes that gap the way a skin closed the shape gap: as data.
//! A set is a TOML file of switches, `[outer]` for the frame around the panes
//! and `[panes]` for every pane at once. The built-ins are embedded from
//! `app/ui/*.toml` (`all`, `clean`). A person's own sets come from
//! `~/.config/terminal-delight/ui/*.toml`, so a set can be written and handed
//! to an agent as a file, the way a skin can.
//!
//! ## Who decides a switch
//!
//! Three voices, in order, and the first that speaks wins:
//!
//! 1. **A switch flipped by hand** in UI CONFIG (`UiChoice::outer` / `panes`).
//! 2. **The set** — chosen by hand (`UiChoice::set`), or else the one the
//!    window's DESIGN asks for (`ui = "clean"` in a theme file), or else `all`.
//! 3. **Nobody said** — shown.
//!
//! That order is Parker's rule, 2026-10-22: *"clean mode shouldn't BLOCK
//! elements from being made visible."* A design only asks; a hand always wins.
//!
//! ## One config for every pane
//!
//! The `[panes]` table is ONE config applied to all panes. *"EACH INDIVIDUAL
//! PANE IS NOT IN CHARGE OF ITS OWN CONFIG."* A pane's own 🎨 card has no UI
//! CONFIG, the way it has no SKIN.
//!
//! ## What cannot be switched off
//!
//! CONFIG itself, each pane's title, its state and its ⋯, and the ◀/▶ handle
//! that trades the left bar for tabs across the top. They are the way back and
//! the news, so they are not keys at all: nothing here can name them.

use std::{collections::BTreeMap, fs, path::PathBuf, sync::Arc};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

pub const ALL: &str = "all";
pub const CLEAN: &str = "clean";

/// (id, embedded toml) for every built-in set.
const BUILTIN_SETS: &[(&str, &str)] = &[
    (ALL, include_str!("../ui/all.toml")),
    (CLEAN, include_str!("../ui/clean.toml")),
];

/// Which half of the window a switch belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// The frame around the panes — once per window.
    Outer,
    /// Every pane at once.
    Panes,
}

macro_rules! ui_keys {
    ($( $variant:ident => $layer:ident, $file:literal; )*) => {
        /// One switch. The file key is the TOML key inside its layer's table.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Key { $( $variant, )* }

        impl Key {
            pub const ALL: &'static [Key] = &[ $( Key::$variant, )* ];

            pub fn layer(self) -> Layer {
                match self { $( Key::$variant => Layer::$layer, )* }
            }

            /// The key as a set file spells it, inside its layer's table.
            pub fn file_key(self) -> &'static str {
                match self { $( Key::$variant => $file, )* }
            }

            fn parse(layer: Layer, key: &str) -> Option<Key> {
                Key::ALL
                    .iter()
                    .copied()
                    .find(|k| k.layer() == layer && k.file_key() == key)
            }
        }
    };
}

ui_keys! {
    AppMark        => Outer, "app_mark";
    GitNotice      => Outer, "git_notice";
    AgentCounter   => Outer, "agent_counter";
    LeftBar        => Outer, "left_bar";
    LeftBarHeader  => Outer, "left_bar_header";
    Allowance      => Outer, "allowance";
    Spine          => Outer, "spine";
    SplitButtons   => Outer, "split_buttons";
    Counts         => Outer, "counts";
    Scale          => Outer, "scale";
    Surfaces       => Outer, "surfaces";
    Sliders        => Outer, "sliders";
    Graveyard      => Outer, "graveyard";
    Notifications  => Outer, "notifications";
    BugReport      => Outer, "bug_report";
    Plugins        => Outer, "plugins";
    OptionRows     => Outer, "option_rows";
    PaneLogo       => Panes, "logo";
    PaneMode       => Panes, "mode";
    PaneFace       => Panes, "face";
    PaneAgent      => Panes, "agent";
    PaneHandover   => Panes, "handover";
    PaneSurface    => Panes, "surface";
    PaneSliders    => Panes, "sliders";
    PaneClose      => Panes, "close";
    PaneNotes      => Panes, "notes";
}

impl Key {
    /// The six glyphs of the bottom-right corner, in the order they sit. Hide
    /// any of them and CONFIG takes the corner.
    pub const MENU_GLYPHS: &'static [Key] = &[
        Key::Surfaces,
        Key::Sliders,
        Key::Graveyard,
        Key::Notifications,
        Key::BugReport,
        Key::Plugins,
    ];
}

/// One set, as parsed from its file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiSet {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub switches: BTreeMap<Key, bool>,
    /// Keys the file names that no switch answers to — reported, never fatal,
    /// so a set written for a newer build still loads.
    pub unknown: Vec<String>,
}

impl UiSet {
    /// What this set says about `key`, or `None` for no opinion.
    pub fn get(&self, key: Key) -> Option<bool> {
        self.switches.get(&key).copied()
    }
}

pub fn parse(id: &str, source: &str) -> Result<UiSet, String> {
    let table: toml::Table = toml::from_str(source).map_err(|e| e.to_string())?;
    let mut set = UiSet {
        id: id.to_string(),
        name: table
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(id)
            .to_string(),
        icon: table
            .get("icon")
            .and_then(|v| v.as_str())
            .unwrap_or("▢")
            .to_string(),
        ..Default::default()
    };
    for (section, layer) in [("outer", Layer::Outer), ("panes", Layer::Panes)] {
        let Some(t) = table.get(section).and_then(|v| v.as_table()) else {
            continue;
        };
        for (k, v) in t {
            match (Key::parse(layer, k), v.as_bool()) {
                (Some(key), Some(on)) => {
                    set.switches.insert(key, on);
                }
                _ => set.unknown.push(format!("{section}.{k}")),
            }
        }
    }
    for k in table.keys() {
        if !matches!(k.as_str(), "name" | "icon" | "outer" | "panes") {
            set.unknown.push(k.clone());
        }
    }
    Ok(set)
}

/// What a window chose about its chrome. Persisted in the session file as
/// `[ui]`. Both maps hold only switches flipped BY HAND; everything else is
/// the set's to say.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UiChoice {
    /// A set chosen by hand. `None` follows whatever the design asks for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outer: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panes: BTreeMap<String, bool>,
}

impl UiChoice {
    #[cfg(test)]
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn table(&self, layer: Layer) -> &BTreeMap<String, bool> {
        match layer {
            Layer::Outer => &self.outer,
            Layer::Panes => &self.panes,
        }
    }

    fn table_mut(&mut self, layer: Layer) -> &mut BTreeMap<String, bool> {
        match layer {
            Layer::Outer => &mut self.outer,
            Layer::Panes => &mut self.panes,
        }
    }

    /// The hand's say on `key`, if it has spoken.
    pub fn flipped(&self, key: Key) -> Option<bool> {
        self.table(key.layer()).get(key.file_key()).copied()
    }

    pub fn flip(&mut self, key: Key, on: bool) {
        self.table_mut(key.layer())
            .insert(key.file_key().to_string(), on);
    }

    /// Any switch flipped by hand at all — the set row reads YOURS.
    pub fn has_flips(&self) -> bool {
        !self.outer.is_empty() || !self.panes.is_empty()
    }

    /// The window's old `left_bar` field, read into the new switch. A bar the
    /// person had shut stays shut, by hand. An open bar is what every set but
    /// Clean shows anyway, so it says nothing — no existing window changes.
    pub fn adopt_left_bar(&mut self, left_bar: bool) {
        if !left_bar && self.flipped(Key::LeftBar).is_none() {
            self.flip(Key::LeftBar, false);
        }
    }
}

/// The pure rule: hand, then set, then shown.
pub fn decide(choice: &UiChoice, set: Option<&UiSet>, key: Key) -> bool {
    choice
        .flipped(key)
        .or_else(|| set.and_then(|s| s.get(key)))
        .unwrap_or(true)
}

/// Which set speaks: the hand's, else the design's ask, else `all`. A set id
/// nobody has a file for falls back to `all` rather than to "everything off".
pub fn set_id<'a>(
    choice: &'a UiChoice,
    design_ask: Option<&'a str>,
    sets: &[Arc<UiSet>],
) -> &'a str {
    let want = choice.set.as_deref().or(design_ask).unwrap_or(ALL);
    if sets.iter().any(|s| s.id == want) {
        want
    } else {
        ALL
    }
}

/// The window's UI state, as a gpui global.
pub struct UiState {
    pub choice: UiChoice,
    pub sets: Vec<Arc<UiSet>>,
}
impl Global for UiState {}

/// Every set a person can pick: the built-ins, then their own files.
pub fn load_sets() -> Vec<Arc<UiSet>> {
    let mut out: Vec<Arc<UiSet>> = BUILTIN_SETS
        .iter()
        .map(|(id, src)| Arc::new(parse(id, src).expect("embedded UI set parses")))
        .collect();
    if let Ok(dir) = fs::read_dir(user_dir()) {
        let mut files: Vec<PathBuf> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        files.sort();
        for path in files {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if out.iter().any(|s| s.id == id) {
                continue; // a built-in id stays the built-in
            }
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|src| parse(id, &src))
            {
                Ok(set) => out.push(Arc::new(set)),
                Err(e) => eprintln!("terminal-delight: UI set {}: {e}", path.display()),
            }
        }
    }
    out
}

/// Where a person's own sets live.
pub fn user_dir() -> PathBuf {
    crate::instance::config_dir().join("ui")
}

pub fn init(cx: &mut App, choice: UiChoice) {
    cx.set_global(UiState {
        choice,
        sets: load_sets(),
    });
}

/// The set the design worn at the OUTER scope asks for.
fn design_ask(cx: &App) -> Option<String> {
    crate::theme::theme(cx).ui.clone()
}

/// The id of the set speaking for this window right now.
pub fn active_set(cx: &App) -> String {
    let st = cx.global::<UiState>();
    let ask = design_ask(cx);
    set_id(&st.choice, ask.as_deref(), &st.sets).to_string()
}

/// Whether `key` is drawn.
pub fn shown(cx: &App, key: Key) -> bool {
    let Some(st) = cx.try_global::<UiState>() else {
        return true; // a window built before the global (tests, rigs): everything
    };
    let ask = design_ask(cx);
    let id = set_id(&st.choice, ask.as_deref(), &st.sets);
    let set = st.sets.iter().find(|s| s.id == id).map(|s| s.as_ref());
    decide(&st.choice, set, key)
}

/// Any of the six corner glyphs hidden — CONFIG takes the corner.
pub fn config_in_corner(cx: &App) -> bool {
    Key::MENU_GLYPHS.iter().any(|k| !shown(cx, *k))
        || !shown(cx, Key::Counts)
        || !shown(cx, Key::Scale)
        || !shown(cx, Key::SplitButtons)
}

pub fn choice(cx: &App) -> UiChoice {
    cx.try_global::<UiState>()
        .map(|s| s.choice.clone())
        .unwrap_or_default()
}

pub fn sets(cx: &App) -> Vec<Arc<UiSet>> {
    cx.try_global::<UiState>()
        .map(|s| s.sets.clone())
        .unwrap_or_default()
}

/// Replace the window's whole UI choice — what a design brings back with it
/// when it is picked again (see [`crate::looks`]).
pub fn set_choice(cx: &mut App, choice: UiChoice) {
    cx.global_mut::<UiState>().choice = choice;
}

/// Flip one switch by hand.
pub fn flip(cx: &mut App, key: Key, on: bool) {
    cx.global_mut::<UiState>().choice.flip(key, on);
}

/// Flip a group of switches at once — the menu glyphs, a whole layer.
pub fn flip_all(cx: &mut App, keys: &[Key], on: bool) {
    let st = cx.global_mut::<UiState>();
    for k in keys {
        st.choice.flip(*k, on);
    }
}

/// Pick a set by hand (`Some`), or go back to following the design (`None`).
/// Either way the switches flipped by hand are cleared: picking a set is
/// asking for that set, whole.
pub fn pick_set(cx: &mut App, set: Option<String>) {
    let st = cx.global_mut::<UiState>();
    st.choice.set = set;
    st.choice.outer.clear();
    st.choice.panes.clear();
}

// ---------------------------------------------------------------------------
// New windows
// ---------------------------------------------------------------------------

/// What a brand-new window starts as: a saved look, or nothing saved — the
/// house look, which is Clean. Parker, 2026-10-21: new-window settings are
/// *"CUSTOMIZABLE, but at a default -- CLEAN"*.
#[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NewWindow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<crate::theme::ThemeChoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiChoice>,
}

impl NewWindow {
    pub fn path() -> PathBuf {
        crate::instance::config_dir().join("new-window.toml")
    }

    /// The saved template, or `None` when there is none — new windows are
    /// Clean. A file that will not parse is reported and treated as none.
    pub fn load() -> Option<NewWindow> {
        let src = fs::read_to_string(Self::path()).ok()?;
        match toml::from_str(&src) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("terminal-delight: {}: {e}", Self::path().display());
                None
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let body = toml::to_string(self).map_err(std::io::Error::other)?;
        fs::write(path, format!("# terminal-delight — what a new window starts as.\n# Written by UI CONFIG's \"use this window\"; delete it to start new windows Clean.\n{body}"))
    }

    /// Back to Clean: the file goes.
    pub fn clear() -> std::io::Result<()> {
        match fs::remove_file(Self::path()) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// `terminal-delight ui` — UI sets without a window, so a set can be written,
/// checked and handed to an agent as a file, the way a skin can.
///
/// ```text
/// terminal-delight ui --list
/// terminal-delight ui --template all > mine.toml
/// terminal-delight ui --check mine.toml
/// ```
pub fn run_cli(args: &[String]) -> i32 {
    match args {
        [flag] if flag == "--list" => {
            for set in load_sets() {
                println!("{}\t{}\t{}", set.id, set.icon, set.name);
            }
            0
        }
        [flag, id] if flag == "--template" => match BUILTIN_SETS.iter().find(|(i, _)| i == id) {
            Some((_, src)) => {
                print!("{src}");
                0
            }
            None => {
                eprintln!("no built-in UI set {id:?} — have: {}", ids().join(", "));
                2
            }
        },
        [flag, path] if flag == "--check" => {
            let src = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{path}: {e}");
                    return 2;
                }
            };
            let id = std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("set");
            match parse(id, &src) {
                Ok(set) => {
                    for k in Key::ALL {
                        let layer = match k.layer() {
                            Layer::Outer => "outer",
                            Layer::Panes => "panes",
                        };
                        let said = match set.get(*k) {
                            Some(true) => "shown",
                            Some(false) => "hidden",
                            None => "shown (no opinion)",
                        };
                        println!("{layer}.{}\t{said}", k.file_key());
                    }
                    for u in &set.unknown {
                        eprintln!("unknown key: {u}");
                    }
                    i32::from(!set.unknown.is_empty())
                }
                Err(e) => {
                    eprintln!("{path}: {e}");
                    2
                }
            }
        }
        _ => {
            eprintln!("usage: terminal-delight ui --list | --template <id> | --check <file.toml>");
            2
        }
    }
}

fn ids() -> Vec<&'static str> {
    BUILTIN_SETS.iter().map(|(i, _)| *i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin(id: &str) -> UiSet {
        let src = BUILTIN_SETS.iter().find(|(i, _)| *i == id).unwrap().1;
        parse(id, src).unwrap()
    }

    /// Every built-in set parses with nothing unknown, and `all` names every
    /// switch there is, as `true` — it is the template a person copies.
    #[test]
    fn the_builtin_sets_parse_and_all_names_every_switch() {
        for (id, _) in BUILTIN_SETS {
            let set = builtin(id);
            assert!(set.unknown.is_empty(), "{id}: {:?}", set.unknown);
        }
        let all = builtin(ALL);
        for k in Key::ALL {
            assert_eq!(all.get(*k), Some(true), "all.toml says nothing about {k:?}");
        }
    }

    /// Clean is the set Parker drew: top tabs, the pill kept, the menus folded
    /// into CONFIG, each pane down to its title, state and notes.
    #[test]
    fn clean_is_top_tabs_the_pill_and_each_panes_name_and_state() {
        let clean = builtin(CLEAN);
        let choice = UiChoice::default();
        let on = |k| decide(&choice, Some(&clean), k);
        assert!(!on(Key::LeftBar), "clean prefers top tabs");
        assert!(on(Key::Spine), "the attention pill stays");
        assert!(on(Key::PaneNotes), "notes stay");
        for k in Key::MENU_GLYPHS {
            assert!(!on(*k), "{k:?} folds into CONFIG");
        }
        for k in [
            Key::PaneLogo,
            Key::PaneMode,
            Key::PaneFace,
            Key::PaneAgent,
            Key::PaneHandover,
            Key::PaneSurface,
            Key::PaneSliders,
            Key::PaneClose,
        ] {
            assert!(!on(k), "{k:?} folds into ⋯");
        }
        assert!(!on(Key::OptionRows), "rows fold to their selection");
    }

    /// The hand outranks the set, the set outranks nobody, nobody means shown.
    /// "clean mode shouldn't BLOCK elements from being made visible."
    #[test]
    fn a_switch_flipped_by_hand_outranks_the_set() {
        let clean = builtin(CLEAN);
        let mut choice = UiChoice::default();
        assert!(!decide(&choice, Some(&clean), Key::AgentCounter));
        choice.flip(Key::AgentCounter, true);
        assert!(
            decide(&choice, Some(&clean), Key::AgentCounter),
            "Clean never blocks"
        );
        choice.flip(Key::Counts, false);
        assert!(
            !decide(&choice, Some(&builtin(ALL)), Key::Counts),
            "and the hand hides too"
        );
        assert!(
            decide(&UiChoice::default(), None, Key::Counts),
            "nobody said: shown"
        );
    }

    /// The hand's set, else the design's ask, else all — and a set nobody has
    /// a file for is `all`, never "everything off".
    #[test]
    fn the_hands_set_then_the_designs_ask_then_all() {
        let sets: Vec<Arc<UiSet>> = BUILTIN_SETS
            .iter()
            .map(|(id, src)| Arc::new(parse(id, src).unwrap()))
            .collect();
        let mut c = UiChoice::default();
        assert_eq!(set_id(&c, None, &sets), ALL);
        assert_eq!(set_id(&c, Some(CLEAN), &sets), CLEAN, "the design asks");
        c.set = Some(ALL.into());
        assert_eq!(
            set_id(&c, Some(CLEAN), &sets),
            ALL,
            "the hand outranks the ask"
        );
        c.set = Some("gone".into());
        assert_eq!(set_id(&c, Some(CLEAN), &sets), ALL, "a missing set is all");
    }

    /// An old window's shut left bar stays shut; an open one says nothing, so
    /// no window that exists today changes when this build arrives.
    #[test]
    fn the_old_left_bar_field_becomes_a_hand_flip_only_when_shut() {
        let mut shut = UiChoice::default();
        shut.adopt_left_bar(false);
        assert_eq!(shut.flipped(Key::LeftBar), Some(false));
        let mut open = UiChoice::default();
        open.adopt_left_bar(true);
        assert!(open.is_default(), "an open bar says nothing");
        let mut kept = UiChoice::default();
        kept.flip(Key::LeftBar, true);
        kept.adopt_left_bar(false);
        assert_eq!(kept.flipped(Key::LeftBar), Some(true), "a [ui] flip wins");
    }

    /// The choice writes only what was said, and reads back the same.
    #[test]
    fn a_choice_round_trips_and_writes_only_what_was_said() {
        assert_eq!(toml::to_string(&UiChoice::default()).unwrap().trim(), "");
        let mut c = UiChoice {
            set: Some(CLEAN.into()),
            ..Default::default()
        };
        c.flip(Key::PaneClose, true);
        c.flip(Key::Counts, false);
        let wire = toml::to_string(&c).unwrap();
        let back: UiChoice = toml::from_str(&wire).unwrap();
        assert_eq!(back, c, "{wire}");
        assert!(
            wire.contains("close = true") && wire.contains("counts = false"),
            "{wire}"
        );
    }

    /// A key a newer build added is reported, not fatal.
    #[test]
    fn an_unknown_key_is_reported_not_fatal() {
        let set = parse("x", "name = \"x\"\n[outer]\nnope = false\ncounts = false\n").unwrap();
        assert_eq!(set.unknown, vec!["outer.nope".to_string()]);
        assert_eq!(set.get(Key::Counts), Some(false));
    }
}
