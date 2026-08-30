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

use gpui::{App, Global, Hsla};
use serde::Deserialize;
use std::path::{Path, PathBuf};

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
    /// Preview: the scheme's own screen colour, behind three of its hues.
    pub bg: Hsla,
    pub chips: [Hsla; 3],
}

/// Every palette found on this desktop, in display order (dark A–Z, then light).
#[derive(Default)]
pub struct Library {
    pub items: Vec<Palette>,
}
impl Global for Library {}

/// The palettes available to paint with. Empty when Omarchy isn't installed —
/// which is a legitimate state, not an error: the colour-set shelf still works.
pub fn all(cx: &App) -> &[Palette] {
    cx.try_global::<Library>()
        .map(|l| l.items.as_slice())
        .unwrap_or(&[])
}

/// Look a palette up by the id persisted in a pane's theme group.
pub fn find<'a>(cx: &'a App, id: &str) -> Option<&'a Palette> {
    all(cx).iter().find(|p| p.id == id)
}

/// Everything a paint tile draws, owned. The overlay interleaves these with
/// `cx.listener(…)` calls, which take the app context mutably — so it cannot
/// hold a borrow of the library across them, and copying six small fields per
/// palette once per overlay is cheaper than fighting that.
#[derive(Clone)]
pub struct Chip {
    pub id: String,
    pub label: (String, String),
    pub light: bool,
    pub bg: Hsla,
    pub chips: [Hsla; 3],
}

/// Draw-ready summaries of every palette, in display order.
pub fn chips(cx: &App) -> Vec<Chip> {
    all(cx)
        .iter()
        .map(|p| Chip {
            id: p.id.clone(),
            label: p.label.clone(),
            light: p.light,
            bg: p.bg,
            chips: p.chips,
        })
        .collect()
}

/// Scan the desktop for palettes and publish them. Called once at startup,
/// beside `theme::init`; a desktop with no Omarchy simply publishes none.
pub fn init(cx: &mut App) {
    cx.set_global(Library {
        items: load(&roots()),
    });
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

/// Read every `<root>/*/colors.toml` into a palette, later roots shadowing
/// earlier ones by id. A theme whose file is missing, unreadable or malformed is
/// SKIPPED rather than fatal — one bad third-party theme must not cost the user
/// the other twenty-two.
fn load(roots: &[PathBuf]) -> Vec<Palette> {
    let mut items: Vec<Palette> = Vec::new();
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
fn from_dir(dir: &Path) -> Option<Palette> {
    let id = dir.file_name()?.to_str()?.to_string();
    if id.starts_with('.') {
        return None;
    }
    let source = std::fs::read_to_string(dir.join("colors.toml")).ok()?;
    from_source(&id, &source)
}

/// The translation itself, split out so a test can hold it against a real theme.
pub(crate) fn from_source(id: &str, source: &str) -> Option<Palette> {
    let c: OmarchyColors = toml::from_str(source).ok()?;
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
    Some(Palette {
        label: split_label(id),
        id: id.to_string(),
        light: c.mode.as_deref() == Some("light"),
        bg: px(&bg)?,
        chips: [px(&accent)?, px(&ansi[1])?, px(&ansi[2])?],
        toml,
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
}
