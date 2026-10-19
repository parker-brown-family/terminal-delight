//! OMARCHY PALETTES — the desktop's on-board colour schemes, worn by a pane.
//!
//! Omarchy publishes every theme it ships as a `colors.toml` of NAMED ROLES —
//! `background`, `foreground`, `accent`, the eight hues and their bright twins —
//! and then stamps those roles into each terminal's own config from a template.
//! Terminal Delight already knows how to wear a colours-only file (that is what
//! `$TD_PALETTE` is), so the bridge between the two is a rename, done once at
//! load: Omarchy's role names into the `[colors]` table [`theme::overlay_palette`]
//! reads. Everything downstream — derived cursor, complement, human colour, the
//! per-pane override, the state file — is machinery that already existed.
//!
//! **The rename is the only opinion in this file, and it is not ours.** It is
//! copied from Omarchy's own `default/themed/alacritty.toml.tpl`: normal black is
//! `background`, bright black is `muted`, white and bright white are `foreground`
//! and `bright_foreground`, and the cursor is `bright_foreground`. Matching the
//! desktop's template rather than inventing a better-looking mapping is the whole
//! point — a pane painted `tokyo-night` then renders ANSI colour for colour the
//! same as every other tokyo-night window on the machine. A prettier private
//! mapping would make Terminal Delight the one window that disagrees.
//!
//! Roots are scanned SYSTEM FIRST, USER LAST, so a theme the user installed into
//! `~/.config/omarchy/themes` shadows a stock one of the same name — the same
//! precedence Omarchy itself applies.
//!
//! **What resolves and what is offered are two lists.** Every palette found —
//! the desktop's, over the copies TD carries — RESOLVES, so a pane wearing one
//! keeps it whatever happens to the desktop. The pickers show the narrower
//! [`offered`] list: Omarchy's own themes minus the ones it hides, which is the
//! list Omarchy's picker shows. Parker, 2026-10-16: *"OVERRIDE the themes picker
//! with the LIVE AND ACTUAL config from Omarchy!"* It is read live — [`init`]
//! rescans whenever a theme is installed, removed, retuned or hidden.

mod current;
pub use current::{current, current_status};

use gpui::{App, Global, Hsla};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// A desktop colour scheme, resolved far enough to draw its own preview.
pub struct Palette {
    /// Directory name — the stable id persisted in the state file (`tokyo-night`).
    pub id: String,
    /// Display label, split on the hyphen so a two-word name fits a paint tile
    /// on two short lines instead of being truncated ("CATPPUCCIN" / "LATTE").
    pub label: (String, String),
    /// `mode = "light"` in the source. Only drives the preview hint; the colours
    /// are applied identically either way (as Omarchy applies them).
    pub light: bool,
    /// The generated Terminal Delight palette file. Kept as source rather than
    /// pre-resolved colours so [`theme::overlay_palette`] stays the single place
    /// a palette turns into a theme — one set of defaulting/derivation rules for
    /// `$TD_PALETTE` and for these alike.
    pub toml: String,
    /// Preview: the scheme's own screen colour, behind three of its hues. The
    /// first chip is the theme's accent.
    pub bg: Hsla,
    pub chips: [Hsla; 3],
    /// Read from one of the desktop's theme folders, rather than from a copy TD
    /// carries in the binary.
    pub desktop: bool,
    /// On offer in the pickers. False for a theme the desktop hides, and — where
    /// Omarchy is installed — for a copy TD carries that the desktop does not
    /// have. Not offered is not gone: [`find`] still resolves it.
    pub offered: bool,
}

/// Every palette that resolves, in display order (dark A–Z, then light).
#[derive(Default)]
pub struct Library {
    pub items: Vec<Palette>,
}
impl Global for Library {}

/// Every palette that RESOLVES: the desktop's, over the copies TD carries. What
/// a picker shows is the narrower [`offered`].
pub fn all(cx: &App) -> &[Palette] {
    cx.try_global::<Library>()
        .map(|l| l.items.as_slice())
        .unwrap_or(&[])
}

/// The palettes the pickers show: this desktop's themes, minus the ones it hides.
pub fn offered(cx: &App) -> impl Iterator<Item = &Palette> {
    all(cx).iter().filter(|p| p.offered)
}

/// Look a palette up by the id persisted in a pane's theme group. Hidden and
/// unoffered palettes resolve too: a picker's list never takes a look off a pane
/// that is already wearing it.
pub fn find<'a>(cx: &'a App, id: &str) -> Option<&'a Palette> {
    all(cx).iter().find(|p| p.id == id)
}

/// [`find`], narrowed to what a picker may show — for a list of the user's own
/// (the favourites) that should hide what the desktop hides.
pub fn find_offered<'a>(cx: &'a App, id: &str) -> Option<&'a Palette> {
    offered(cx).find(|p| p.id == id)
}

impl Palette {
    /// The keyboard chord that paints this palette — its id's first letter,
    /// upper-cased, exactly as a colour set's is.
    ///
    /// Unlike the colour sets these letters are NOT unique and cannot be made so:
    /// the names belong to the desktop, and it ships `catppuccin` beside
    /// `catppuccin-latte`, three palettes starting with `l` and three with `r`.
    /// See [`next_for_letter`] for how a letter still paints exactly one thing.
    pub fn paint_letter(&self) -> char {
        self.id
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
            .unwrap_or('?')
    }
}

/// Resolve a letter on the palette shelf to the palette it should paint — the
/// one AFTER whatever the pane is wearing, among those sharing that letter.
///
/// Colour sets can guarantee a unique first letter because we name them. Desktop
/// palettes cannot: `catppuccin` / `catppuccin-latte`, `retro-82` / `ristretto` /
/// `rose-pine`. Rather than give up the keyboard on this shelf, or invent second
/// letters nobody can guess, the letter CYCLES — press `r` three times and the
/// pane walks retro-82 → ristretto → rose-pine → back. Each press paints, so you
/// see every candidate on the way past instead of choosing one blind.
///
/// The cursor is `current` — what the pane already wears — so the cycle needs no
/// stored state and stays correct per pane on a wall being painted in parallel.
/// **`z` never reaches here.** The shelf key is resolved first, so a third-party
/// theme actually named `zenburn` would keep its tile and its click and simply
/// lose its chord — a collision costs one letter, never the shelf.
pub fn next_for_letter(cx: &App, letter: char, current: Option<&str>) -> Option<String> {
    next_in(offered(cx), letter, current)
}

/// [`next_for_letter`] over an explicit list — the whole rule, testable without
/// an app context.
fn next_in<'a>(
    items: impl IntoIterator<Item = &'a Palette>,
    letter: char,
    current: Option<&str>,
) -> Option<String> {
    let group: Vec<&Palette> = items
        .into_iter()
        .filter(|p| p.paint_letter().eq_ignore_ascii_case(&letter))
        .collect();
    let at = current.and_then(|id| group.iter().position(|p| p.id == id));
    let next = match at {
        Some(i) => (i + 1) % group.len(),
        None => 0,
    };
    group.get(next).map(|p| p.id.clone())
}

/// Everything a paint tile draws, owned. The overlay interleaves these with
/// `cx.listener(…)` calls, which take the app context mutably — so it cannot
/// hold a borrow of the library across them, and copying six small fields per
/// palette once per overlay is cheaper than fighting that.
#[derive(Clone)]
pub struct Chip {
    pub id: String,
    /// The chord letter, and the rest of the name split across up to two lines:
    /// `("C", "ATPPUCCIN", "LATTE")`. The tile inks the letter the way it is
    /// pressed, exactly as a colour-set tile does.
    pub letter: char,
    pub rest: String,
    pub second: String,
    pub light: bool,
    pub bg: Hsla,
    pub chips: [Hsla; 3],
}

/// Draw-ready summaries of every offered palette, in display order.
pub fn chips(cx: &App) -> Vec<Chip> {
    offered(cx)
        .map(|p| Chip {
            id: p.id.clone(),
            letter: p.paint_letter(),
            rest: p.label.0.chars().skip(1).collect(),
            second: p.label.1.clone(),
            light: p.light,
            bg: p.bg,
            chips: p.chips,
        })
        .collect()
}

/// The id of the Terminal Delight palette — Omarchy's name for it, and the
/// one a fresh install's outer wears ([`crate::theme::house_outer`]).
pub const TERMINAL_DELIGHT: &str = "terminal-delight";

/// Palettes TD carries itself, `(id, Omarchy colors.toml)`.
///
/// The default look has to exist on a machine with no Omarchy, so the Terminal
/// Delight palette ships inside the binary. These sit BENEATH every desktop
/// root: an installed Omarchy theme of the same name shadows the shipped copy,
/// exactly as a user theme shadows a stock one, so a desktop that has retuned
/// the brand palette is the one TD wears.
const SHIPPED: &[(&str, &str)] = &[(
    TERMINAL_DELIGHT,
    include_str!("../palettes/terminal-delight.toml"),
)];

/// Omarchy's 22 stock themes, each `colors.toml` verbatim from Omarchy 4.0.4
/// (MIT, `palettes/omarchy/LICENSE`).
///
/// They are for a machine with no Omarchy, which still opens on the same themes.
/// Where Omarchy is installed its own copies shadow these and these are never
/// offered: the desktop is the registry, and a copy here must not resurrect a
/// theme Omarchy has dropped. `the_stock_copies_match_the_desktop_they_came_from`
/// compares them with `/usr/share/omarchy/themes` wherever that exists.
const STOCK: &[(&str, &str)] = &[
    (
        "catppuccin",
        include_str!("../palettes/omarchy/catppuccin.toml"),
    ),
    (
        "catppuccin-latte",
        include_str!("../palettes/omarchy/catppuccin-latte.toml"),
    ),
    (
        "ethereal",
        include_str!("../palettes/omarchy/ethereal.toml"),
    ),
    (
        "everforest",
        include_str!("../palettes/omarchy/everforest.toml"),
    ),
    (
        "flexoki-light",
        include_str!("../palettes/omarchy/flexoki-light.toml"),
    ),
    ("gruvbox", include_str!("../palettes/omarchy/gruvbox.toml")),
    (
        "hackerman",
        include_str!("../palettes/omarchy/hackerman.toml"),
    ),
    (
        "kanagawa",
        include_str!("../palettes/omarchy/kanagawa.toml"),
    ),
    (
        "last-horizon",
        include_str!("../palettes/omarchy/last-horizon.toml"),
    ),
    ("lumon", include_str!("../palettes/omarchy/lumon.toml")),
    ("lupine", include_str!("../palettes/omarchy/lupine.toml")),
    (
        "matte-black",
        include_str!("../palettes/omarchy/matte-black.toml"),
    ),
    ("miasma", include_str!("../palettes/omarchy/miasma.toml")),
    ("nord", include_str!("../palettes/omarchy/nord.toml")),
    (
        "osaka-jade",
        include_str!("../palettes/omarchy/osaka-jade.toml"),
    ),
    (
        "retro-82",
        include_str!("../palettes/omarchy/retro-82.toml"),
    ),
    (
        "ristretto",
        include_str!("../palettes/omarchy/ristretto.toml"),
    ),
    (
        "rose-pine",
        include_str!("../palettes/omarchy/rose-pine.toml"),
    ),
    (
        "solitude",
        include_str!("../palettes/omarchy/solitude.toml"),
    ),
    (
        "tokyo-night",
        include_str!("../palettes/omarchy/tokyo-night.toml"),
    ),
    (
        "vantablack",
        include_str!("../palettes/omarchy/vantablack.toml"),
    ),
    ("white", include_str!("../palettes/omarchy/white.toml")),
];

fn shipped() -> Vec<Palette> {
    SHIPPED
        .iter()
        .chain(STOCK)
        .filter_map(|(id, src)| from_source(id, src))
        .collect()
}

/// The ids of every palette TD carries — what a machine with no Omarchy is
/// offered.
#[cfg(test)]
pub(crate) fn shipped_ids() -> impl Iterator<Item = &'static str> {
    SHIPPED.iter().chain(STOCK).map(|(id, _)| *id)
}

/// One read of the desktop: the shipped palettes, the desktop's over them, and
/// the hide list applied. A few dozen small files, so it is simply run again
/// whenever anything it read has changed.
fn scan() -> Vec<Palette> {
    let roots = roots();
    let desktop = roots.iter().any(|r| r.is_dir());
    let mut items = load_over(shipped(), &roots);
    mark_offered(&mut items, desktop, &hidden_names(hidden_path().as_deref()));
    items
}

/// Which palettes a picker shows. With Omarchy installed that is the desktop's
/// own list and nothing TD carries, bar the Terminal Delight palette, the house
/// look, which has to be reachable on any desktop. Without Omarchy it is
/// everything TD carries. Either way a hidden name is dropped, the house look
/// included: hiding is the person's call.
fn mark_offered(items: &mut [Palette], desktop: bool, hidden: &HashSet<String>) {
    for p in items.iter_mut() {
        p.offered = !hidden.contains(&p.id) && (!desktop || p.desktop || p.id == TERMINAL_DELIGHT);
    }
}

/// `~/.config/omarchy/hidden-themes`, one theme name per line.
///
/// Omarchy 4.0.4 has no hide list of its own. This file is the one `theme-veil`
/// keeps — it takes the named themes out of Omarchy's picker — so TD reads the
/// same file the same way, and a theme hidden there is hidden here.
/// `TD_OMARCHY_HIDDEN` points it elsewhere, for tests.
fn hidden_path() -> Option<PathBuf> {
    if let Some(over) = std::env::var("TD_OMARCHY_HIDDEN")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return Some(PathBuf::from(over));
    }
    let config = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })?;
    Some(config.join("omarchy/hidden-themes"))
}

/// The names a hide list holds. A missing or unreadable file hides nothing.
fn hidden_names(path: Option<&Path>) -> HashSet<String> {
    path.and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| parse_hidden(&s))
        .unwrap_or_default()
}

/// `theme-veil`'s own reading of the file: anything after a `#` is a comment,
/// whitespace is removed, and a name holding a `/` or starting with `.` is
/// refused rather than trusted.
fn parse_hidden(src: &str) -> HashSet<String> {
    src.lines()
        .filter_map(|line| {
            let name: String = line
                .split('#')
                .next()
                .unwrap_or("")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            (!name.is_empty() && !name.contains('/') && !name.starts_with('.')).then_some(name)
        })
        .collect()
}

/// What a rescan would read, as modification times: the hide list, each root,
/// and each theme folder's colour files. A missing file is recorded as missing,
/// so a theme gaining or losing its `colors.toml` counts as a change. Equal
/// signatures mean an equal scan.
fn signature() -> Vec<(PathBuf, Option<SystemTime>)> {
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut out = Vec::new();
    if let Some(h) = hidden_path() {
        out.push((h.clone(), mtime(&h)));
    }
    for root in roots() {
        out.push((root.clone(), mtime(&root)));
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            for file in ["colors.toml", "alacritty.toml"] {
                let p = dir.join(file);
                let at = mtime(&p);
                out.push((p, at));
            }
        }
    }
    out
}

/// Scan the desktop for palettes and publish them, over the ones TD ships, then
/// keep them live.
///
/// Omarchy changes under a running window: a theme is installed, removed,
/// retuned in place, or hidden. Twice a second the files the scan read are
/// stat'ed, off the main thread, and the scan runs again only when one of them
/// moved. Every pane then re-resolves, so a pane wearing a theme that was just
/// retuned shows the new colours without anyone touching it.
pub fn init(cx: &mut App) {
    cx.set_global(Library { items: scan() });
    let mut last = signature();
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
        let now = cx.background_executor().spawn(async { signature() }).await;
        if now == last {
            continue;
        }
        last = now;
        let items = cx.background_executor().spawn(async { scan() }).await;
        cx.update(|cx| {
            cx.set_global(Library { items });
            crate::theme::palettes_changed(cx);
        });
    })
    .detach();
    current::init(cx);
}

/// Where Omarchy keeps themes, in precedence order (later roots win). The env
/// override exists for tests and for an Omarchy installed somewhere unusual.
fn roots() -> Vec<PathBuf> {
    if let Some(over) = std::env::var("TD_OMARCHY_THEMES")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return over.split(':').map(PathBuf::from).collect();
    }
    let home = std::env::var("HOME").map(PathBuf::from).ok();
    let mut out = vec![PathBuf::from("/usr/share/omarchy/themes")];
    if let Some(p) = std::env::var("OMARCHY_PATH").ok().filter(|s| !s.is_empty()) {
        out.push(PathBuf::from(p).join("themes"));
    }
    if let Some(h) = home {
        out.push(h.join(".local/share/omarchy/themes"));
        // user-installed themes shadow the stock ones: scanned last
        out.push(h.join(".config/omarchy/themes"));
    }
    out
}

/// The roots alone, with nothing shipped beneath them.
#[cfg(test)]
fn load(roots: &[PathBuf]) -> Vec<Palette> {
    load_over(Vec::new(), roots)
}

/// Publish only the palettes TD ships, as a desktop with no Omarchy sees them —
/// a test's stand-in for [`init`], which would read whatever Omarchy themes the
/// machine running the tests has.
#[cfg(test)]
pub(crate) fn init_shipped(cx: &mut App) {
    let mut items = shipped();
    mark_offered(&mut items, false, &HashSet::new());
    cx.set_global(Library { items });
}

/// Read every theme folder under `roots` into a palette over `items`, later
/// roots shadowing earlier ones — and `items` — by id. A theme whose files are
/// missing, unreadable or malformed is SKIPPED rather than fatal — one bad
/// third-party theme must not cost the user the other twenty-two.
fn load_over(mut items: Vec<Palette>, roots: &[PathBuf]) -> Vec<Palette> {
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        dirs.sort();
        for dir in dirs {
            let Some(p) = from_dir(&dir) else { continue };
            match items.iter().position(|q| q.id == p.id) {
                Some(i) => items[i] = p, // a later root shadows an earlier one
                None => items.push(p),
            }
        }
    }
    // Dark schemes first, then light, A–Z within each. A light palette turns the
    // whole pane into a bright screen — a real choice, but not the one someone
    // scanning a terminal's paint grid is usually after, so it trails.
    items.sort_by(|a, b| a.light.cmp(&b.light).then_with(|| a.id.cmp(&b.id)));
    items
}

/// One theme directory → one palette, or `None` if it isn't one.
///
/// A theme with no `colors.toml` is read from its `alacritty.toml`. Several
/// published themes ship only that (Retro Fallout, RetroPC, Ayaka, Mars and
/// Spectra among the most-starred), and Omarchy derives their colours when one
/// is SET, into its staging copy rather than the theme's folder — so a
/// `colors.toml` never appears for TD to find, and TD makes the same derivation.
fn from_dir(dir: &Path) -> Option<Palette> {
    let id = dir.file_name()?.to_str()?.to_string();
    if id.starts_with('.') {
        return None;
    }
    let mut p = match std::fs::read_to_string(dir.join("colors.toml")) {
        Ok(source) => from_source(&id, &source),
        Err(_) => from_alacritty(
            &id,
            &std::fs::read_to_string(dir.join("alacritty.toml")).ok()?,
        ),
    }?;
    p.desktop = true;
    Some(p)
}

/// The translation itself, split out so a test can hold it against a real theme.
pub(crate) fn from_source(id: &str, source: &str) -> Option<Palette> {
    from_colors(id, toml::from_str(source).ok()?)
}

/// Omarchy's own `omarchy-theme-colors-from-alacritty`, as a translation into
/// the same roles: all eight normal colours are required, a bright colour falls
/// back to its normal twin, background and foreground fall back to normal black
/// and white, and the accent is normal blue.
fn from_alacritty(id: &str, source: &str) -> Option<Palette> {
    const NAMES: [&str; 8] = [
        "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
    ];
    let v: toml::Value = toml::from_str(source).ok()?;
    let colors = v.get("colors")?;
    let hex = |section: &str, key: &str| first_hex(colors.get(section)?.get(key)?.as_str()?);
    let normal: Vec<String> = NAMES
        .iter()
        .map(|n| hex("normal", n))
        .collect::<Option<_>>()?;
    let bright: Vec<String> = NAMES
        .iter()
        .zip(&normal)
        .map(|(n, base)| hex("bright", n).unwrap_or_else(|| base.clone()))
        .collect();
    let at = |v: &[String], i: usize| Some(v[i].clone());
    from_colors(
        id,
        OmarchyColors {
            accent: at(&normal, 4),
            background: hex("primary", "background").or_else(|| at(&normal, 0)),
            foreground: hex("primary", "foreground").or_else(|| at(&normal, 7)),
            muted: at(&bright, 0),
            bright_foreground: at(&bright, 7),
            red: at(&normal, 1),
            green: at(&normal, 2),
            yellow: at(&normal, 3),
            blue: at(&normal, 4),
            magenta: at(&normal, 5),
            cyan: at(&normal, 6),
            bright_red: at(&bright, 1),
            bright_green: at(&bright, 2),
            bright_yellow: at(&bright, 3),
            bright_blue: at(&bright, 4),
            bright_magenta: at(&bright, 5),
            bright_cyan: at(&bright, 6),
            ..OmarchyColors::default()
        },
    )
}

/// The first run of six hex digits, as Omarchy reads a colour: `"0x1A1B26"`,
/// `"#1a1b26"` and `"1a1b26"` are all `#1a1b26`.
fn first_hex(s: &str) -> Option<String> {
    let b = s.as_bytes();
    (0..b.len().saturating_sub(5))
        .find(|&i| b[i..i + 6].iter().all(u8::is_ascii_hexdigit))
        .map(|i| format!("#{}", s[i..i + 6].to_ascii_lowercase()))
}

/// Omarchy's roles → a Terminal Delight palette.
fn from_colors(id: &str, c: OmarchyColors) -> Option<Palette> {
    // The five roles with no sensible stand-in. Anything else falls back, so a
    // theme that omits `orange`/`brown` (several stock ones do) still loads.
    let bg = c.background.clone()?;
    let fg = c.foreground.clone()?;
    let accent = c.accent.clone().unwrap_or_else(|| fg.clone());
    let muted = c
        .muted
        .clone()
        .unwrap_or_else(|| c.dark_foreground.clone().unwrap_or_else(|| fg.clone()));
    let bright_fg = c.bright_foreground.clone().unwrap_or_else(|| fg.clone());
    let surface = c.lighter_background.clone().unwrap_or_else(|| bg.clone());
    // A hue with no bright twin falls back to the base hue, never to nothing:
    // a 16-entry ANSI table is a hard requirement of the palette format.
    let hue = |base: &Option<String>| base.clone().unwrap_or_else(|| fg.clone());
    let bright = |b: &Option<String>, base: &Option<String>| {
        b.clone()
            .or_else(|| base.clone())
            .unwrap_or_else(|| fg.clone())
    };
    // Omarchy's own terminal template, verbatim — see the module doc.
    let ansi = [
        bg.clone(),                    // 0  black   = background
        hue(&c.red),                   // 1  red
        hue(&c.green),                 // 2  green
        hue(&c.yellow),                // 3  yellow
        hue(&c.blue),                  // 4  blue
        hue(&c.magenta),               // 5  magenta
        hue(&c.cyan),                  // 6  cyan
        fg.clone(),                    // 7  white   = foreground
        muted.clone(),                 // 8  bright black = muted
        bright(&c.bright_red, &c.red), // 9
        bright(&c.bright_green, &c.green),
        bright(&c.bright_yellow, &c.yellow),
        bright(&c.bright_blue, &c.blue),
        bright(&c.bright_magenta, &c.magenta),
        bright(&c.bright_cyan, &c.cyan),
        bright_fg.clone(), // 15 bright white = bright_foreground
    ];
    let toml = format!(
        "name = \"{id}\"\n\n[colors]\nbg = \"{bg}\"\nsurface = \"{surface}\"\n\
         text = \"{fg}\"\naccent = \"{accent}\"\nfaint = \"{muted}\"\ncursor = \"{bright_fg}\"\n\
         ansi = [\n{}]\n",
        ansi.iter().fold(String::new(), |mut s, c| {
            s.push_str("  \"");
            s.push_str(c);
            s.push_str("\",\n");
            s
        })
    );
    let px = |s: &str| crate::theme::parse_hex(s);
    // Validate every role, not just the preview chips. A malformed unseen ANSI
    // colour must not replace a last-good live palette with unusable data.
    for colour in [&bg, &surface, &fg, &accent, &muted, &bright_fg]
        .into_iter()
        .chain(ansi.iter())
    {
        px(colour)?;
    }
    Some(Palette {
        label: split_label(id),
        id: id.to_string(),
        light: c.mode.as_deref() == Some("light"),
        bg: px(&bg)?,
        chips: [px(&accent)?, px(&ansi[1])?, px(&ansi[2])?],
        toml,
        // Decided by the scan that loaded it: where it came from, then whether
        // the desktop offers it.
        desktop: false,
        offered: false,
    })
}

/// `"catppuccin-latte"` → `("CATPPUCCIN", "LATTE")`; a single-word id gets an
/// empty second line. Splitting on the FIRST hyphen keeps line one short enough
/// for a paint tile while staying unique — `catppuccin` and `catppuccin-latte`
/// differ on line two, which truncation would have eaten.
///
/// Shared with the colour-set shelf, whose labels hyphenate the same way
/// (`retro-sunset`, `cotton-clowndy`) and used to fold mid-word into
/// `RETRO-SUN / SET`.
pub(crate) fn split_label(id: &str) -> (String, String) {
    match id.split_once('-') {
        Some((a, b)) => (a.to_uppercase(), b.replace('-', " ").to_uppercase()),
        None => (id.to_uppercase(), String::new()),
    }
}

/// Omarchy's `colors.toml`. Every field is optional: the stock themes agree on
/// most of them, but `orange` and `brown` are already absent from several, and a
/// third-party theme is under no obligation at all.
#[derive(Deserialize, Default)]
struct OmarchyColors {
    mode: Option<String>,
    accent: Option<String>,
    muted: Option<String>,
    background: Option<String>,
    lighter_background: Option<String>,
    foreground: Option<String>,
    dark_foreground: Option<String>,
    bright_foreground: Option<String>,
    red: Option<String>,
    yellow: Option<String>,
    green: Option<String>,
    cyan: Option<String>,
    blue: Option<String>,
    magenta: Option<String>,
    bright_red: Option<String>,
    bright_yellow: Option<String>,
    bright_green: Option<String>,
    bright_cyan: Option<String>,
    bright_blue: Option<String>,
    bright_magenta: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stock Omarchy theme, verbatim (`/usr/share/omarchy/themes/tokyo-night`).
    const TOKYO: &str = r##"
mode = "dark"
accent = "#7aa2f7"
selection = "#292e42"
muted = "#414868"
background = "#1a1b26"
dark_background = "#13141c"
darker_background = "#0e0e14"
lighter_background = "#24283b"
foreground = "#a9b1d6"
dark_foreground = "#565f89"
light_foreground = "#b4bee6"
bright_foreground = "#c0caf5"
red = "#f7768e"
yellow = "#e0af68"
orange = "#eb927b"
green = "#9ece6a"
cyan = "#449dab"
blue = "#7aa2f7"
magenta = "#ad8ee6"
brown = "#75493d"
bright_red = "#ff7a93"
bright_yellow = "#ff9e64"
bright_green = "#b9f27c"
bright_cyan = "#0db9d7"
bright_blue = "#7da6ff"
bright_magenta = "#bb9af7"
"##;

    #[test]
    fn the_ansi_table_is_omarchys_own_terminal_template() {
        // The contract this whole module exists to keep: a pane painted
        // tokyo-night must agree, slot for slot, with what Omarchy writes into
        // alacritty/foot/ghostty from `default/themed/*.tpl`.
        let p = from_source("tokyo-night", TOKYO).expect("stock theme loads");
        let ansi: Vec<&str> = p
            .toml
            .lines()
            .filter_map(|l| l.trim().strip_prefix('"'))
            .filter_map(|l| l.split('"').next())
            .collect();
        assert_eq!(
            ansi,
            vec![
                "#1a1b26", // black  = background, NOT a darker shade
                "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#ad8ee6", "#449dab",
                "#a9b1d6", // white  = foreground
                "#414868", // bright black = muted
                "#ff7a93", "#b9f27c", "#ff9e64", "#7da6ff", "#bb9af7", "#0db9d7",
                "#c0caf5", // bright white = bright_foreground
            ],
            "ANSI order is r,g,y,b,m,c — Omarchy's colors.toml lists them y,g,c,b,m"
        );
        assert!(
            p.toml.contains("cursor = \"#c0caf5\""),
            "cursor = bright_foreground"
        );
        assert!(p.toml.contains("bg = \"#1a1b26\""));
        assert!(
            p.toml.contains("surface = \"#24283b\""),
            "surface = lighter_background"
        );
        assert!(p.toml.contains("faint = \"#414868\""), "faint = muted");
        assert!(!p.light);
    }

    #[test]
    fn the_generated_file_is_one_the_theme_layer_accepts() {
        // The translation is only worth anything if `overlay_palette` takes it —
        // this is the seam, so assert across it rather than trusting the shape.
        let p = from_source("tokyo-night", TOKYO).expect("stock theme loads");
        let base = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).expect("base parses");
        let out = crate::theme::overlay_palette(&base, &p.toml).expect("palette applies");
        assert_eq!(out.bg, crate::theme::parse_hex("#1a1b26").unwrap());
        assert_eq!(out.ansi[8], crate::theme::parse_hex("#414868").unwrap());
        assert_eq!(out.name, "tokyo-night", "the palette names itself");
    }

    #[test]
    fn a_theme_missing_optional_hues_still_loads() {
        // Several stock themes ship no `orange`/`brown`; `solitude` and `white`
        // are the live proof. Absent hues fall back, never fail.
        let thin = "mode = \"light\"\nbackground = \"#ffffff\"\nforeground = \"#111111\"\n";
        let p = from_source("white", thin).expect("a minimal theme still loads");
        assert!(p.light);
        assert_eq!(
            p.toml.matches('"').count() / 2,
            1 + 6 + 16,
            "16 ansi entries + 6 roles + name"
        );
    }

    #[test]
    fn a_theme_with_no_background_is_not_a_palette() {
        assert!(from_source("junk", "mode = \"dark\"\n").is_none());
        assert!(from_source("junk", "not toml at all {{{").is_none());
    }

    #[test]
    fn the_label_splits_on_the_first_hyphen_so_near_names_stay_distinct() {
        assert_eq!(split_label("tokyo-night"), ("TOKYO".into(), "NIGHT".into()));
        assert_eq!(
            split_label("catppuccin"),
            ("CATPPUCCIN".into(), String::new())
        );
        // truncation would render both of these "CATPPUCCIN…"; the split doesn't
        assert_eq!(
            split_label("catppuccin-latte"),
            ("CATPPUCCIN".into(), "LATTE".into())
        );
        assert_eq!(split_label("a-b-c"), ("A".into(), "B C".into()));
    }

    /// The real desktop, when there is one. Skipped on a machine without Omarchy
    /// (CI, another distro) rather than failing — but where the themes DO exist,
    /// this is the only test that proves the translation survives contact with
    /// files we didn't write, including the several that omit `orange`/`brown`.
    #[test]
    fn every_theme_this_desktop_actually_ships_loads() {
        let root = PathBuf::from("/usr/share/omarchy/themes");
        let Ok(dirs) = std::fs::read_dir(&root) else {
            return; // no Omarchy here
        };
        let on_disk = dirs
            .filter_map(|e| e.ok())
            .filter(|e| e.path().join("colors.toml").is_file())
            .count();
        let loaded = load(&[root]);
        assert_eq!(
            loaded.len(),
            on_disk,
            "every shipped theme became a palette"
        );
        for p in &loaded {
            let base = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).expect("base");
            crate::theme::overlay_palette(&base, &p.toml)
                .unwrap_or_else(|e| panic!("{} does not apply: {e}", p.id));
        }
    }

    #[test]
    fn a_letter_cycles_the_palettes_that_share_it() {
        // The property that lets this shelf keep a keyboard at all: the desktop's
        // names collide, so `r` walks the r-group instead of picking one blind.
        let mk = |id: &str| from_source(id, "background = \"#000000\"\nforeground = \"#ffffff\"\n");
        let items: Vec<Palette> = ["retro-82", "ristretto", "rose-pine", "nord"]
            .iter()
            .filter_map(|id| mk(id))
            .collect();
        assert_eq!(items.len(), 4, "fixtures build");
        // nothing worn yet → the first of the group
        assert_eq!(next_in(&items, 'r', None).as_deref(), Some("retro-82"));
        // …then each press walks one on, and wraps back to the top
        assert_eq!(
            next_in(&items, 'r', Some("retro-82")).as_deref(),
            Some("ristretto")
        );
        assert_eq!(
            next_in(&items, 'r', Some("ristretto")).as_deref(),
            Some("rose-pine")
        );
        assert_eq!(
            next_in(&items, 'r', Some("rose-pine")).as_deref(),
            Some("retro-82"),
            "the cycle wraps rather than dead-ending on the last one"
        );
        // wearing something from ANOTHER group restarts this one from the top
        assert_eq!(
            next_in(&items, 'r', Some("nord")).as_deref(),
            Some("retro-82")
        );
        // a group of one is a plain pick that stays put
        assert_eq!(next_in(&items, 'n', Some("nord")).as_deref(), Some("nord"));
        // case-insensitive, and a letter nothing is spelled with paints nothing
        assert_eq!(next_in(&items, 'N', None).as_deref(), Some("nord"));
        assert_eq!(next_in(&items, 'q', None), None);
    }

    #[test]
    fn the_shelf_key_is_not_a_chord_on_either_shelf() {
        // `z` is allowed to be a VERB (turn the shelf) only because nothing is
        // spelled with one. The palettes are the desktop's, so check what it
        // actually ships — and what TD carries for a desktop without it.
        for p in shipped() {
            assert_ne!(p.paint_letter(), 'Z', "{} takes the shelf key", p.id);
        }
        for p in load(&[PathBuf::from("/usr/share/omarchy/themes")]) {
            assert_ne!(
                p.paint_letter(),
                'Z',
                "{} collides with the shelf key — see `paint_key`, which resolves \
                 `z` to the shelf first and would leave this palette mouse-only",
                p.id
            );
        }
    }

    /// `f` is the overlay's SECOND verb (jump to favourites / star what is
    /// worn), and unlike `z` it was not free — it costs `flexoki-light` its
    /// chord on the desktop shelf.
    ///
    /// This test is the price tag, not an alarm. It asserts what we chose: `f`
    /// is a chord on no shelf of OURS, and the desktop palettes it does shadow
    /// are named here, so the day a theme called `firewatch` is installed the
    /// list changes and somebody re-reads the decision instead of discovering it
    /// through a key that quietly stopped working.
    #[test]
    fn the_favourites_verb_shadows_only_what_we_agreed_it_would() {
        // TD's own copies are the desktop a machine without Omarchy sees, so
        // they are held to the same price as the real one.
        let shadowed: Vec<String> = shipped()
            .into_iter()
            .chain(load(&[PathBuf::from("/usr/share/omarchy/themes")]))
            .filter(|p| p.paint_letter() == 'F')
            .map(|p| p.id)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        // Stock Omarchy, as of this writing, ships exactly one `f`.
        assert!(
            shadowed == ["flexoki-light"],
            "the favourites key now shadows {shadowed:?} — these palettes keep \
             their tile and their click but lose their chord. Still the right \
             trade? See `paint_key`."
        );
    }

    #[test]
    fn a_user_theme_shadows_a_stock_one_of_the_same_name() {
        let dir = std::env::temp_dir().join(format!("td-pal-{}", std::process::id()));
        let (sys, usr) = (dir.join("sys"), dir.join("usr"));
        for (root, bg) in [(&sys, "#111111"), (&usr, "#222222")] {
            let t = root.join("tokyo-night");
            std::fs::create_dir_all(&t).expect("mkdir");
            std::fs::write(
                t.join("colors.toml"),
                format!("background = \"{bg}\"\nforeground = \"#eeeeee\"\n"),
            )
            .expect("write");
        }
        // a directory that is not a theme is skipped, not fatal
        std::fs::create_dir_all(sys.join("not-a-theme")).expect("mkdir");
        let items = load(&[sys, usr]);
        assert_eq!(items.len(), 1, "same id → one entry");
        assert_eq!(
            items[0].bg,
            crate::theme::parse_hex("#222222").unwrap(),
            "the LAST root wins — user themes shadow stock ones"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_terminal_delight_palette_ships_and_an_installed_one_shadows_it() {
        // No desktop at all: the fresh-install default still has its colours.
        let bare = load_over(shipped(), &[]);
        let td = bare
            .iter()
            .find(|p| p.id == TERMINAL_DELIGHT)
            .expect("the shipped copy loads with no Omarchy installed");
        assert_eq!(
            td.bg,
            crate::theme::parse_hex("#030708").unwrap(),
            "Void, from the brand palette"
        );
        assert_eq!(
            td.chips[0],
            crate::theme::parse_hex("#67F454").unwrap(),
            "Signal Green is the accent"
        );

        // An Omarchy theme of the same name, retuned on this desktop, wins.
        let dir = std::env::temp_dir().join(format!("td-pal-td-{}", std::process::id()));
        let t = dir.join(TERMINAL_DELIGHT);
        std::fs::create_dir_all(&t).expect("mkdir");
        std::fs::write(
            t.join("colors.toml"),
            "background = \"#101010\"\nforeground = \"#eeeeee\"\n",
        )
        .expect("write");
        let over = load_over(shipped(), std::slice::from_ref(&dir));
        let mine: Vec<_> = over.iter().filter(|p| p.id == TERMINAL_DELIGHT).collect();
        assert_eq!(mine.len(), 1, "shadowed, not duplicated");
        assert_eq!(
            mine[0].bg,
            crate::theme::parse_hex("#101010").unwrap(),
            "the desktop's copy beats the shipped one"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A scratch desktop: `<dir>/<id>/colors.toml` for each id, dark.
    fn desktop(tag: &str, ids: &[&str]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-pal-{tag}-{}", std::process::id()));
        for id in ids {
            let t = dir.join(id);
            std::fs::create_dir_all(&t).expect("mkdir");
            std::fs::write(
                t.join("colors.toml"),
                "background = \"#101010\"\nforeground = \"#eeeeee\"\naccent = \"#ff8800\"\n",
            )
            .expect("write");
        }
        dir
    }

    fn offered_ids(items: &[Palette]) -> Vec<&str> {
        items
            .iter()
            .filter(|p| p.offered)
            .map(|p| p.id.as_str())
            .collect()
    }

    #[test]
    fn with_omarchy_installed_the_desktop_list_is_what_is_offered() {
        // Parker's desktop in miniature: two stock themes, one of his own, and
        // one he has hidden.
        let dir = desktop("offer", &["nord", "praus", "catppuccin", "tokyo-night"]);
        let mut items = load_over(shipped(), std::slice::from_ref(&dir));
        let hidden = parse_hidden("catppuccin\n");
        mark_offered(&mut items, true, &hidden);
        assert_eq!(
            offered_ids(&items),
            vec!["nord", "praus", "terminal-delight", "tokyo-night"],
            "the desktop's themes minus the hidden one, plus the house look — and \
             none of the stock copies the desktop does not have (gruvbox, kanagawa…)"
        );
        // Hidden is not gone: a pane wearing it still resolves its colours.
        let cat = items
            .iter()
            .find(|p| p.id == "catppuccin")
            .expect("resolves");
        assert!(!cat.offered && cat.desktop);
        // And a stock copy the desktop lacks still resolves too.
        let gruv = items.iter().find(|p| p.id == "gruvbox").expect("resolves");
        assert!(!gruv.offered && !gruv.desktop);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_omarchy_the_stock_themes_td_carries_are_offered() {
        let mut items = load_over(shipped(), &[]);
        mark_offered(&mut items, false, &HashSet::new());
        assert_eq!(
            offered_ids(&items).len(),
            23,
            "22 stock themes + the house look"
        );
        assert!(offered_ids(&items).contains(&"tokyo-night"));
    }

    #[test]
    fn hiding_the_house_look_hides_it_too() {
        let mut items = load_over(shipped(), &[]);
        mark_offered(&mut items, true, &parse_hidden("terminal-delight\n"));
        assert!(offered_ids(&items).is_empty());
    }

    #[test]
    fn the_hide_list_is_read_the_way_theme_veil_reads_it() {
        // Parker's own file, verbatim in shape: a comment header, then names.
        let src = "# Themes kept installed but hidden from the Omarchy theme picker.\n\
                   # One name per line.\n\
                   ristretto\n  rose-pine  \nlupine # the wolf one\n\n\
                   ../etc\nsub/dir\n.hidden\nwhite\n";
        let got = parse_hidden(src);
        let want: HashSet<String> = ["ristretto", "rose-pine", "lupine", "white"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(got, want);
        assert!(hidden_names(Some(Path::new("/nonexistent/hidden-themes"))).is_empty());
        assert!(hidden_names(None).is_empty());
    }

    #[test]
    fn a_theme_with_only_alacritty_colours_loads_as_omarchy_would_derive_it() {
        // RetroPC's shape: no colors.toml, an alacritty.toml with 0x colours and
        // no bright section, so every bright colour falls back to its twin.
        let dir = std::env::temp_dir().join(format!("td-pal-ala-{}", std::process::id()));
        let t = dir.join("retropc");
        std::fs::create_dir_all(&t).expect("mkdir");
        std::fs::write(
            t.join("alacritty.toml"),
            "[colors.primary]\nbackground = '0x0A0A08'\nforeground = '0xD0D0C0'\n\
             [colors.normal]\nblack = '0x000000'\nred = '0xAA0000'\ngreen = '0x00AA00'\n\
             yellow = '0xAAAA00'\nblue = '0x0000AA'\nmagenta = '0xAA00AA'\n\
             cyan = '0x00AAAA'\nwhite = '0xAAAAAA'\n",
        )
        .expect("write");
        let items = load(std::slice::from_ref(&dir));
        assert_eq!(items.len(), 1, "an alacritty-only theme is still a theme");
        let p = &items[0];
        assert!(p.desktop);
        let px = |s: &str| crate::theme::parse_hex(s).unwrap();
        assert_eq!(p.bg, px("#0a0a08"), "primary background");
        assert_eq!(p.chips[0], px("#0000aa"), "the accent is normal blue");
        assert!(
            p.toml.contains("faint = \"#000000\""),
            "muted is bright black, falling back to normal black"
        );
        // Without all eight normal colours Omarchy derives nothing, and neither do we.
        std::fs::write(
            t.join("alacritty.toml"),
            "[colors.primary]\nbackground = '#000000'\n[colors.normal]\nred = '#ff0000'\n",
        )
        .expect("write");
        assert!(load(std::slice::from_ref(&dir)).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_colour_is_its_first_six_hex_digits() {
        assert_eq!(first_hex("0x1A1B26").as_deref(), Some("#1a1b26"));
        assert_eq!(first_hex("#1a1b26").as_deref(), Some("#1a1b26"));
        assert_eq!(first_hex("1a1b26").as_deref(), Some("#1a1b26"));
        assert_eq!(first_hex("#fff"), None);
        assert_eq!(first_hex(""), None);
    }

    #[test]
    fn every_stock_copy_loads_on_a_machine_with_no_omarchy() {
        assert_eq!(STOCK.len(), 22);
        for (id, src) in STOCK {
            let p = from_source(id, src).unwrap_or_else(|| panic!("{id} does not load"));
            let base = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).expect("base");
            crate::theme::overlay_palette(&base, &p.toml)
                .unwrap_or_else(|e| panic!("{id} does not apply: {e}"));
        }
    }

    /// The copies TD carries against the desktop they were copied from. Skipped
    /// where Omarchy is not installed; where it is, a theme Omarchy has retuned
    /// fails here by name instead of drifting. `palettes/omarchy/README.md`
    /// has the one-line refresh.
    #[test]
    fn the_stock_copies_match_the_desktop_they_came_from() {
        let root = PathBuf::from("/usr/share/omarchy/themes");
        if !root.is_dir() {
            return;
        }
        let drifted: Vec<&str> = STOCK
            .iter()
            .filter(|(id, src)| {
                std::fs::read_to_string(root.join(id).join("colors.toml"))
                    .is_ok_and(|on_disk| on_disk != *src)
            })
            .map(|(id, _)| *id)
            .collect();
        assert!(
            drifted.is_empty(),
            "Omarchy has changed {drifted:?}; refresh app/palettes/omarchy/ (see its README)"
        );
    }
}
