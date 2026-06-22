//! Hot-reloaded theme system. Themes are TOML data files — edit while the app
//! runs, no recompile (PLAN §1: "modify on the fly" is a day-one feature).
//!
//! Resolution: $TD_THEME path → ~/.config/terminal-delight/theme.toml
//! (seeded with the hacker theme on first run) → embedded default.
//! A background task polls mtime (~300ms) and swaps the global on change.

use std::{
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use gpui::{rgb, App, Global, Hsla};
use serde::{Deserialize, Deserializer, Serialize};

pub const DEFAULT_THEME_TOML: &str = include_str!("../themes/hacker.toml");

/// (id, embedded toml) for every built-in. The user file at
/// ~/.config/terminal-delight/theme.toml stays hot-reloaded as id "custom".
const BUILTIN_THEMES: &[(&str, &str)] = &[
    (
        "quiet-command",
        include_str!("../themes/quiet-command.toml"),
    ),
    (
        "field-command",
        include_str!("../themes/field-command.toml"),
    ),
    (
        "tactical-overdrive",
        include_str!("../themes/tactical-overdrive.toml"),
    ),
    ("gamba", include_str!("../themes/gamba.toml")),
    ("hacker", DEFAULT_THEME_TOML),
];

#[derive(Deserialize)]
struct FileColors {
    bg: String,
    surface: String,
    text: String,
    accent: String,
    faint: String,
    cursor: Option<String>,
    /// Optional colour for the user's own input in an agent (claude/codex)
    /// session. Absent → derived from the accent's bright complement.
    human: Option<String>,
    ansi: Vec<String>,
}

#[derive(Deserialize, Default)]
struct FileEffects {
    scanline_opacity: Option<f32>,
    scanline_step: Option<f32>,
    vignette: Option<f32>,
    glow: Option<f32>,
    bloom: Option<f32>,
    tracking: Option<f32>,
    tracking_period: Option<f32>,
    tracking_sweep: Option<f32>,
    flicker: Option<f32>,
    jiggle: Option<f32>,
    screen_glare: Option<f32>,
    bezel: Option<f32>,
}

#[derive(Deserialize, Default)]
struct FileFont {
    family: Option<String>,
    size: Option<f32>,
    cell_height: Option<f32>,
}

#[derive(Deserialize)]
struct ThemeFile {
    name: Option<String>,
    icon: Option<String>,
    colors: FileColors,
    #[serde(default)]
    effects: FileEffects,
    #[serde(default)]
    font: FileFont,
}

#[derive(Clone, Debug)]
pub struct Theme {
    /// Kept for theme-file authors; the UI shows `icon` instead.
    #[allow(dead_code)]
    pub name: String,
    /// Glyph that stands in for the theme everywhere the UI names it.
    pub icon: String,
    pub bg: Hsla,
    pub surface: Hsla,
    pub text: Hsla,
    pub accent: Hsla,
    /// The title's complement colour — a second hue shown alongside the accent in
    /// the mother bar. Defaults to the accent's complement; a dynamic sets it from
    /// its harmony, and the breakout's `C` wheel target overrides it explicitly.
    pub complement: Hsla,
    /// Colour the user's own input is painted in an agent (claude/codex) session,
    /// so your turns stand out from the agent's replies. Set by the wheel's human
    /// (`👤`) target; defaults to the bright complement of the palette.
    pub human: Hsla,
    pub faint: Hsla,
    pub cursor: Hsla,
    pub ansi: [Hsla; 16],
    /// How program text colour is painted (default/monochrome/on-theme).
    pub color_mode: ColorMode,
    /// IDE-style token highlighting overlaid on default-foreground text. An
    /// orthogonal axis to `color_mode`: when on, cells the program left at its
    /// default fg are recoloured by token class, while cells the program gave
    /// an explicit ANSI colour still flow through `color_mode`.
    pub syntax: bool,
    /// Which grammar the overlay highlights when `syntax` is on (the SYNTAX tray).
    pub syntax_scheme: SyntaxScheme,
    /// Monitor-OSD grading baked from the scope's [`ThemeChoice::grade`], applied
    /// to final cell colours at paint time (see `pane::graded`).
    pub grade: Grade,
    pub scanline_opacity: f32,
    pub scanline_step: f32,
    pub vignette: f32,
    pub glow: f32,
    pub bloom: f32,
    pub tracking: f32,
    pub tracking_period: f32,
    pub tracking_sweep: f32,
    /// Barrel-warp amount for THIS pane (`0` = flat). Resolved from the scope's
    /// [`Grade::warp`] so the renderer bends each pane by its own curvature.
    pub warp: f32,
    /// Star-Wars text-crawl mode for THIS pane. When on, the grid renders in the
    /// crawl font ([`CRAWL_FONT_FAMILY`], italic) and the renderer perspective-
    /// warps the tube (text recedes toward the top). Resolved from the scope's
    /// [`Grade::crawl`], so it's per-pane like `warp`.
    pub crawl: bool,
    /// Crawl convergence angle in degrees (`2..=59`). Drives the top-edge width
    /// ratio the renderer tapers to — see [`crawl_coeffs`].
    pub crawl_angle: f32,
    /// Crawl depth: ratio of text height at the BOTTOM vs the TOP of the crawl
    /// (`0.05..=15`). `>1` = classic (near text bigger); `1` = no foreshortening.
    pub crawl_depth: f32,
    pub flicker: f32,
    pub jiggle: f32,
    pub screen_glare: f32,
    /// Raised metallic frame around the pane edge (0 = flat). 0..1 strength.
    pub bezel: f32,
    pub font_family: String,
    pub font_size: f32,
    pub cell_h: f32,
}

pub struct ActiveTheme(pub Arc<Theme>);
impl Global for ActiveTheme {}

pub fn theme(cx: &App) -> Arc<Theme> {
    cx.global::<ActiveTheme>().0.clone()
}

/// The "source" half of the text-colour pair: how a pane renders the colour the
/// program *emitted* (the ANSI byte stream). The orthogonal half — whether plain
/// text is also token-highlighted — lives in `ThemeChoice::syntax`. Travels with
/// the theme choice (follows outer-vs-pane scope like the seed), and is baked
/// onto the resolved `Theme.color_mode` for the renderer to read.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMode {
    /// The real xterm ANSI palette — blues, greens, reds, the lot.
    Default,
    /// Every colour collapses onto the theme's phosphor ramp (the classic look).
    #[default]
    Monochrome,
    /// ANSI hues folded onto a harmonic arc around the seed colour.
    OnTheme,
}

/// Lenient on load: the retired `Syntax`/`code` variant (now the independent
/// `syntax` axis) and any unknown value fold to the monochrome default, so old
/// state files keep deserialising instead of erroring the whole struct.
impl<'de> Deserialize<'de> for ColorMode {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(d)?.as_str() {
            "default" => ColorMode::Default,
            "on-theme" => ColorMode::OnTheme,
            _ => ColorMode::Monochrome,
        })
    }
}

impl ColorMode {
    /// Picker order.
    pub const ALL: [ColorMode; 3] = [
        ColorMode::Default,
        ColorMode::Monochrome,
        ColorMode::OnTheme,
    ];

    /// Glyph shown in the breakout picker.
    pub fn icon(self) -> &'static str {
        match self {
            ColorMode::Default => "◍",
            ColorMode::Monochrome => "●",
            ColorMode::OnTheme => "◉",
        }
    }

    /// `true` for the serde/skip default (monochrome).
    pub fn is_default(&self) -> bool {
        matches!(self, ColorMode::Monochrome)
    }
}

/// serde `skip_serializing_if` for the `syntax` flag — omit it when off (false).
fn is_false(b: &bool) -> bool {
    !*b
}

/// Which grammar the `syntax` overlay highlights, when on. Orthogonal to
/// PROGRAM COLOUR (`ColorMode`): the scheme decides *what* gets a role, the
/// colour mode decides *how* roles are coloured (see `pane::role_color`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SyntaxScheme {
    /// Programming tokens — keywords, strings, numbers, paths, flags, comments.
    #[default]
    Code,
    /// Agent-watch markers — callouts, tool calls, links, structure, lists.
    Agentic,
    /// Log streams — error/warn/ok levels, timestamps, durations, paths.
    Logs,
    /// Markdown — headings, bold/italic, code spans, links, quotes, lists.
    Markdown,
}

/// Lenient on load: an unknown scheme folds to `Code` (the default), so a state
/// file from a newer/older build keeps deserialising instead of erroring.
impl<'de> Deserialize<'de> for SyntaxScheme {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(d)?.as_str() {
            "agentic" => SyntaxScheme::Agentic,
            "logs" => SyntaxScheme::Logs,
            "markdown" => SyntaxScheme::Markdown,
            _ => SyntaxScheme::Code,
        })
    }
}

impl SyntaxScheme {
    /// Picker order for the SYNTAX tray.
    pub const ALL: [SyntaxScheme; 4] = [
        SyntaxScheme::Code,
        SyntaxScheme::Agentic,
        SyntaxScheme::Logs,
        SyntaxScheme::Markdown,
    ];

    /// `true` for the serde/skip default (code).
    pub fn is_code(&self) -> bool {
        matches!(self, SyntaxScheme::Code)
    }

    /// Glyph shown in the picker.
    pub fn icon(self) -> &'static str {
        match self {
            SyntaxScheme::Code => "\u{25c6}",     // ◆
            SyntaxScheme::Agentic => "\u{25cf}",  // ●
            SyntaxScheme::Logs => "\u{2261}",     // ≡
            SyntaxScheme::Markdown => "\u{00b6}", // ¶
        }
    }
}

/// serde `default` for the two `PaneTheme` inherit flags — a fresh pane follows
/// outer for both groups, so an absent flag means "inheriting".
fn yes() -> bool {
    true
}

/// serde `skip_serializing_if` for the inherit flags — omit them when on (the
/// default), so a pristine pane serialises to nothing.
fn is_true(b: &bool) -> bool {
    *b
}

/// One channel of the monitor OSD — the address of a slider, used both to drive
/// the picker loop and to tag an in-flight slider drag.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum GradeKey {
    Brightness,
    Contrast,
    Colour,
    Text,
    Background,
    Gamma,
    /// Menu-bar size multiplier — scales the outer bar + tabs + per-pane header
    /// chrome (NOT the terminal grid). Rides the grade group like the rest.
    Scale,
    /// Terminal text-size multiplier — scales the pane's GRID font + cell metrics
    /// (the terminal reflows), distinct from `Scale` (chrome). Not a paint-time
    /// grade; rides the grade group for the per-pane override + "follow outer".
    TextSize,
    /// Barrel-warp (CRT curvature) amount, `0..=WARP_MAX` (0 = dead flat). Not a
    /// paint grade — it drives the per-pane tube curvature the renderer bends by —
    /// but it rides the grade group so each pane curves by its OWN amount (own
    /// override else inherited outer), instead of one global dial bending all.
    Warp,
    /// Crawl convergence angle in degrees (`2..=59`). Not a paint grade — it
    /// drives the per-pane crawl perspective the renderer warps by — but it
    /// rides the grade group for the per-pane override + "follow outer".
    CrawlAngle,
    /// Crawl depth — text-height ratio bottom:top (`0.05..=15`). Rides the grade
    /// group like [`GradeKey::CrawlAngle`].
    CrawlDepth,
}

impl GradeKey {
    /// `(min, max, neutral)` in stored units. The colour channels live in
    /// `0..1` with `0.5` neutral; the text-size channel lives in `0.7..1.6×`
    /// with `1.0` neutral. Used to map slider position ↔ stored value.
    pub fn range(self) -> (f32, f32, f32) {
        match self {
            GradeKey::Scale => (0.7, 1.6, 1.0),
            GradeKey::TextSize => (0.6, 2.0, 1.0),
            GradeKey::Warp => (0.0, WARP_MAX, 0.0),
            GradeKey::CrawlAngle => (CRAWL_ANGLE_MIN, CRAWL_ANGLE_MAX, CRAWL_ANGLE_DEFAULT),
            GradeKey::CrawlDepth => (CRAWL_DEPTH_MIN, CRAWL_DEPTH_MAX, CRAWL_DEPTH_DEFAULT),
            _ => (0.0, 1.0, 0.5),
        }
    }

    /// Map a stored channel value to a `0..=100` "slider percent" — the single,
    /// uniform unit the MCP config API speaks. Every channel reports the same
    /// `0..100` scale regardless of its idiosyncratic stored range (brightness
    /// `0..1`, crawl-depth `0.05..15`, text-size `0.6..2`), so an agent never has
    /// to know a channel's internal units to read or write it. It is exactly the
    /// OSD slider's track fraction ×100 — what a human sees on the display tray.
    pub fn to_percent(self, stored: f32) -> f32 {
        let (min, max, _) = self.range();
        if (max - min).abs() < f32::EPSILON {
            return 0.0;
        }
        (((stored - min) / (max - min)) * 100.0).clamp(0.0, 100.0)
    }

    /// Inverse of [`Self::to_percent`]: a `0..=100` percent back to the channel's
    /// stored units. The percent is clamped to `0..100` first; the caller should
    /// still pass the result through [`Grade::set`], which clamps into range
    /// again (belt-and-suspenders, and the single point that owns the bounds).
    // Deliberately a method: it needs `self` to pick the channel's range, and is
    // the symmetric partner of `to_percent`. The `from_*`-takes-no-self heuristic
    // doesn't fit a per-key converter, so suppress rather than rename the API.
    #[allow(clippy::wrong_self_convention)]
    pub fn from_percent(self, pct: f32) -> f32 {
        let (min, max, _) = self.range();
        min + (pct.clamp(0.0, 100.0) / 100.0) * (max - min)
    }
}

/// Per-scope "monitor controls": real-display grading applied to the pane's
/// final colours (HSLA, at paint time — see `pane::graded`). Each channel is a
/// slider in `0..=1` with **0.5 = neutral**; `brightness`/`contrast`/`colour`/
/// `gamma` grade both text and background, while `text`/`background` are the
/// independent per-channel lightness levels. Rides on [`ThemeChoice`] so it
/// follows the same outer-vs-pane scope and persistence as the theme.
/// A partial `[theme.grade]` table fills any missing channel from
/// [`Grade::default`] (the shipped house grade), not from the neutral
/// identity — so an absent field means "the default for that channel".
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Grade {
    pub brightness: f32,
    pub contrast: f32,
    pub colour: f32,
    pub text: f32,
    pub background: f32,
    pub gamma: f32,
    /// Menu-bar size multiplier (`0.7..1.6`, neutral `1.0`). Scales the outer
    /// bar + tabs + per-pane header chrome, not the terminal grid.
    pub scale: f32,
    /// Terminal text-size multiplier (`0.6..2.0`, neutral `1.0`). Scales the
    /// pane's grid font + cell metrics, so the terminal reflows.
    pub text_size: f32,
    /// Barrel-warp amount (`0..=WARP_MAX`, `0` = flat). The renderer bends THIS
    /// pane's tube by it, so warp is per-pane (override else inherited outer) —
    /// not the old global dial that curved every pane at once.
    pub warp: f32,
    /// CRT tracking-band dials `[intensity, speed, size]` in `0..1`, or `None` to
    /// use whatever the resolved theme authored. Per-pane via the grade group.
    pub tracking: Option<[f32; 3]>,
    /// Star-Wars text-crawl toggle. On ⇒ this pane's grid renders in the crawl
    /// font and the renderer perspective-warps the tube. Per-pane via the grade
    /// group (like `warp`/`tracking`). Omitted from TOML when off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub crawl: bool,
    /// Crawl convergence angle in degrees (`2..=59`, neutral 12).
    pub crawl_angle: f32,
    /// Crawl depth = text-height ratio bottom:top (`0.05..=15`, neutral 2.5).
    pub crawl_depth: f32,
}

impl Default for Grade {
    /// The shipped "house" grade — Parker's personal monitor look, baked in as
    /// the default for every fresh pane/window (see the breakout DISPLAY tray).
    /// Channels are `0..1` with `0.5` = no-op; these sit deliberately off-neutral.
    /// The *identity* (no grading) is [`Grade::neutral`], which is what `reset`
    /// returns to.
    fn default() -> Self {
        Self {
            // Tuned down from a blown-out phosphor green: the old high
            // background (+31) + gamma (+26) opened every fresh pane to an
            // eye-searing flat green. Parker's dialed-in "tasteful CRT" look —
            // a dim screen with the glow living in the text, not the field.
            brightness: 0.16,   // −34
            contrast: 0.45,     // −5
            colour: 0.69,       // +19
            text: 0.66,         // +16
            background: 0.43,   // −7
            gamma: 0.5,         // 0 (no gamma lift — keeps the field dark)
            scale: 0.99,        // 99%
            text_size: 1.0,     // terminal grid at config size
            warp: WARP_DEFAULT, // the house near-fishbowl bend
            tracking: None,     // defer to the theme's authored roll bar
            crawl: false,       // crawl mode off until toggled
            crawl_angle: CRAWL_ANGLE_DEFAULT,
            crawl_depth: CRAWL_DEPTH_DEFAULT,
        }
    }
}

impl Grade {
    /// Picker order: (channel, label) for the OSD slider rows. Terminal text
    /// size leads — it's the control people reach for most.
    pub const CHANNELS: [(GradeKey, &'static str); 9] = [
        (GradeKey::TextSize, "text size"),
        (GradeKey::Brightness, "brightness"),
        (GradeKey::Contrast, "contrast"),
        (GradeKey::Colour, "colour"),
        (GradeKey::Text, "text"),
        (GradeKey::Background, "background"),
        (GradeKey::Gamma, "gamma"),
        (GradeKey::Scale, "menu bar"),
        (GradeKey::Warp, "warp"),
    ];

    /// The identity grade: every channel at its no-op (`0.5`, scale `1.0`), i.e.
    /// no monitor grading at all. This is what `reset` returns to — distinct
    /// from [`Grade::default`], the off-neutral house look a fresh scope starts at.
    pub fn neutral() -> Self {
        Self {
            brightness: 0.5,
            contrast: 0.5,
            colour: 0.5,
            text: 0.5,
            background: 0.5,
            gamma: 0.5,
            scale: 1.0,
            text_size: 1.0,
            warp: 0.0,      // reset = dead flat
            tracking: None, // reset = defer to the theme's roll bar
            crawl: false,   // reset = crawl off
            crawl_angle: CRAWL_ANGLE_DEFAULT,
            crawl_depth: CRAWL_DEPTH_DEFAULT,
        }
    }

    /// True when every channel sits at neutral — the grade is the identity and
    /// takes `resolve`'s fast path. NB: `warp`/`tracking` are NOT paint grades, so
    /// they're deliberately excluded — a curved-but-ungraded pane still fast-paths.
    pub fn is_neutral(&self) -> bool {
        const EPS: f32 = 1e-3;
        [
            self.brightness,
            self.contrast,
            self.colour,
            self.text,
            self.background,
            self.gamma,
        ]
        .iter()
        .all(|v| (v - 0.5).abs() < EPS)
            && (self.scale - 1.0).abs() < EPS
            && (self.text_size - 1.0).abs() < EPS
    }

    /// True when this grade equals the shipped [`Grade::default`]. Used as the
    /// `skip_serializing_if` for a scope's grade: omit it only when it matches
    /// the compiled default (which reload reconstructs), so a user grade that
    /// happens to be *neutral* still round-trips instead of springing back to
    /// the house default.
    pub fn is_default(&self) -> bool {
        const EPS: f32 = 1e-3;
        let d = Grade::default();
        (self.brightness - d.brightness).abs() < EPS
            && (self.contrast - d.contrast).abs() < EPS
            && (self.colour - d.colour).abs() < EPS
            && (self.text - d.text).abs() < EPS
            && (self.background - d.background).abs() < EPS
            && (self.gamma - d.gamma).abs() < EPS
            && (self.scale - d.scale).abs() < EPS
            && (self.text_size - d.text_size).abs() < EPS
            && (self.warp - d.warp).abs() < EPS
            && self.tracking == d.tracking
            && self.crawl == d.crawl
            && (self.crawl_angle - d.crawl_angle).abs() < EPS
            && (self.crawl_depth - d.crawl_depth).abs() < EPS
    }

    pub fn get(&self, k: GradeKey) -> f32 {
        match k {
            GradeKey::Brightness => self.brightness,
            GradeKey::Contrast => self.contrast,
            GradeKey::Colour => self.colour,
            GradeKey::Text => self.text,
            GradeKey::Background => self.background,
            GradeKey::Gamma => self.gamma,
            GradeKey::Scale => self.scale,
            GradeKey::TextSize => self.text_size,
            GradeKey::Warp => self.warp,
            GradeKey::CrawlAngle => self.crawl_angle,
            GradeKey::CrawlDepth => self.crawl_depth,
        }
    }

    pub fn set(&mut self, k: GradeKey, v: f32) {
        let (min, max, _) = k.range();
        let v = v.clamp(min, max);
        match k {
            GradeKey::Brightness => self.brightness = v,
            GradeKey::Contrast => self.contrast = v,
            GradeKey::Colour => self.colour = v,
            GradeKey::Text => self.text = v,
            GradeKey::Background => self.background = v,
            GradeKey::Gamma => self.gamma = v,
            GradeKey::Scale => self.scale = v,
            GradeKey::TextSize => self.text_size = v,
            GradeKey::Warp => self.warp = v,
            GradeKey::CrawlAngle => self.crawl_angle = v,
            GradeKey::CrawlDepth => self.crawl_depth = v,
        }
    }
}

/// One scope's appearance pick: a theme id plus an optional seed-colour
/// override ("#rrggbb"). Panes carry `Option<ThemeChoice>`; None = follow outer.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeChoice {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(default, skip_serializing_if = "ColorMode::is_default")]
    pub color: ColorMode,
    /// IDE-style token highlighting, an axis orthogonal to `color`: it only
    /// recolours cells the program left at default fg, so app ANSI colour still
    /// flows through `color`. (Was the retired `ColorMode::Syntax` mode.)
    #[serde(default, skip_serializing_if = "is_false")]
    pub syntax: bool,
    /// Which grammar the syntax overlay highlights (code/agentic/logs/markdown).
    #[serde(default, skip_serializing_if = "SyntaxScheme::is_code")]
    pub syntax_scheme: SyntaxScheme,
    /// Monitor-OSD grading for this scope. Starts at the house [`Grade::default`]
    /// and is omitted from the wire form only while it still matches it (see
    /// [`Grade::is_default`]), so a hand-tuned grade — even a neutral one — persists.
    #[serde(default, skip_serializing_if = "Grade::is_default")]
    pub grade: Grade,
    /// How the seed propagates into the palette (the theme-tray glyph). Part of
    /// the theme group; `Plain` (single-hue tint) by default for back-compat.
    #[serde(default, skip_serializing_if = "Dynamic::is_plain")]
    pub dynamic: Dynamic,
    /// Explicit body-text colour override ("#rrggbb"), set by the wheel's `T`
    /// target. `None` = let the theme/dynamic decide the text colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Explicit title-complement colour override, set by the wheel's `C` target.
    /// `None` = derive it from the theme/dynamic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complement: Option<String>,
    /// Explicit colour for the user's own input in an agent session, set by the
    /// wheel's human (`👤`) target. `None` = derive it from the bright complement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human: Option<String>,
}

impl Default for ThemeChoice {
    fn default() -> Self {
        Self {
            id: "custom".into(),
            seed: None,
            color: ColorMode::default(),
            syntax: true,
            syntax_scheme: SyntaxScheme::Code,
            grade: Grade::default(),
            dynamic: Dynamic::default(),
            text: None,
            complement: None,
            human: None,
        }
    }
}

/// The shipped **OUTER** (mother cabinet) look — Parker's "wooden TV set": the
/// warm amber colour set layered over the green `custom` base, which seeds the
/// dark screen into warm brown and paints amber/cream chrome. Used for a fresh
/// install's outer scope (see `main::build`). Real ANSI + code highlighting on;
/// a gentle darken/de-contrast grade and a slightly larger UI. The terminal
/// screens inside stay green — see [`house_terminal`] / [`PaneTheme::house`].
pub fn house_outer() -> ThemeChoice {
    ThemeChoice {
        id: "custom".into(),
        // A warm amber seed tints the dark green base into warm brown chrome with
        // an amber accent and tan text — the "wooden TV" cabinet. (Seed, not the
        // mono Amber colour set, so the accent stays amber rather than greying.)
        seed: Some("#e0913a".into()),
        color: ColorMode::Default, // "ansi"
        syntax: true,
        syntax_scheme: SyntaxScheme::Code,
        grade: Grade {
            brightness: 0.38, // −12
            contrast: 0.21,   // −29
            colour: 0.5,
            text: 0.5,
            background: 0.5,
            gamma: 0.5,
            scale: 1.16,        // 116%
            text_size: 1.0,     // terminal grid at config size
            warp: WARP_DEFAULT, // the house near-fishbowl bend
            tracking: None,     // defer to the theme's authored roll bar
            crawl: false,
            crawl_angle: CRAWL_ANGLE_DEFAULT,
            crawl_depth: CRAWL_DEPTH_DEFAULT,
        },
        dynamic: Dynamic::Plain,
        text: None,
        complement: None,
        human: None,
    }
}

/// The shipped **INNER** (terminal screen) look — the WOOD · HACKER · AGENTIC ·
/// THEME house design: the `hacker` base under the warm Wood colour set, the
/// program emitting OnTheme ("theme") colours, and the agentic syntax overlay
/// for agent-watch markers. The GAUGES (grade) start neutral but carry the house
/// near-fishbowl warp. Fresh panes pin this and deliberately do NOT follow the
/// warm outer cabinet (its own design lives inside the cabinet), see
/// [`PaneTheme::house`].
pub fn house_terminal() -> ThemeChoice {
    ThemeChoice {
        id: "hacker".into(),
        seed: None,                // Wood's signature seed supplies the palette
        color: ColorMode::OnTheme, // "theme" program colour
        syntax: true,
        syntax_scheme: SyntaxScheme::Agentic,
        // GAUGES default = neutral sliders + the house warp (matches the shipped
        // DISPLAY/GAUGES tray: everything +0/100%, warp +143).
        grade: Grade {
            warp: WARP_DEFAULT,
            ..Grade::neutral()
        },
        dynamic: Dynamic::Wood,
        text: None,
        complement: None,
        human: None,
    }
}

/// The "theme" half of a [`ThemeChoice`] — everything except the monitor-OSD
/// `grade`: theme id, seed override, colour mode and the syntax overlay. Split
/// out so a pane can pin (or inherit) this group independently of its grade,
/// which is what the theme tray's "follow outer" toggle governs.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeGroup {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(default, skip_serializing_if = "ColorMode::is_default")]
    pub color: ColorMode,
    #[serde(default, skip_serializing_if = "is_false")]
    pub syntax: bool,
    #[serde(default, skip_serializing_if = "SyntaxScheme::is_code")]
    pub syntax_scheme: SyntaxScheme,
    #[serde(default, skip_serializing_if = "Dynamic::is_plain")]
    pub dynamic: Dynamic,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub human: Option<String>,
}

impl Default for ThemeGroup {
    fn default() -> Self {
        Self {
            id: "custom".into(),
            seed: None,
            color: ColorMode::default(),
            syntax: true,
            syntax_scheme: SyntaxScheme::Code,
            dynamic: Dynamic::default(),
            text: None,
            complement: None,
            human: None,
        }
    }
}

impl ThemeGroup {
    /// Lift the theme-group fields out of a full choice (drops `grade`).
    pub fn of(c: &ThemeChoice) -> Self {
        Self {
            id: c.id.clone(),
            seed: c.seed.clone(),
            color: c.color,
            syntax: c.syntax,
            syntax_scheme: c.syntax_scheme,
            dynamic: c.dynamic.clone(),
            text: c.text.clone(),
            complement: c.complement.clone(),
            human: c.human.clone(),
        }
    }
}

/// A pane's appearance: its retained per-group overrides plus two independent
/// "follow the outer scope" switches — one for the theme group, one for the
/// grade group. Inheriting is a **live link**: [`PaneTheme::effective`] is
/// recomputed every paint from the current [`OuterChoice`], so editing the outer
/// theme (or nudging one OSD slider) flows into every pane that inherits that
/// group, with no per-pane bookkeeping. A switch is a *non-destructive* toggle:
/// turning "follow outer" on keeps the pane's retained override, so turning it
/// back off restores the pane's own look rather than re-copying outer.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct PaneTheme {
    /// Retained theme-group override (id/seed/colour/syntax). Kept even while
    /// `inherit_theme` is on; `None` only before the group has ever diverged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeGroup>,
    /// Retained grade-group override (the six OSD sliders). Same retention rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<Grade>,
    /// Follow the outer scope's theme group. Default `true` (a fresh pane inherits).
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub inherit_theme: bool,
    /// Follow the outer scope's grade group. Default `true`.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub inherit_grade: bool,
}

impl Default for PaneTheme {
    fn default() -> Self {
        Self {
            theme: None,
            grade: None,
            inherit_theme: true,
            inherit_grade: true,
        }
    }
}

impl PaneTheme {
    /// True when the pane carries nothing of its own — both groups follow outer
    /// and no override is retained. Such a pane is omitted from the state file.
    pub fn is_pristine(&self) -> bool {
        self.inherit_theme && self.inherit_grade && self.theme.is_none() && self.grade.is_none()
    }

    /// The choice this pane actually renders with: each group resolved
    /// independently to the outer scope's value (when inheriting, or before it
    /// has diverged) or the pane's own retained override.
    pub fn effective(&self, outer: &ThemeChoice) -> ThemeChoice {
        let g = match (self.inherit_theme, &self.theme) {
            (false, Some(g)) => g.clone(),
            _ => ThemeGroup::of(outer),
        };
        let grade = match (self.inherit_grade, self.grade) {
            (false, Some(grade)) => grade,
            _ => outer.grade,
        };
        ThemeChoice {
            id: g.id,
            seed: g.seed,
            color: g.color,
            syntax: g.syntax,
            syntax_scheme: g.syntax_scheme,
            grade,
            dynamic: g.dynamic,
            text: g.text,
            complement: g.complement,
            human: g.human,
        }
    }

    /// Pin the theme group to `g` and stop following outer (a theme-tray edit).
    pub fn set_theme(&mut self, g: ThemeGroup) {
        self.theme = Some(g);
        self.inherit_theme = false;
    }

    /// Pin the grade group to `grade` and stop following outer (an OSD edit).
    pub fn set_grade(&mut self, grade: Grade) {
        self.grade = Some(grade);
        self.inherit_grade = false;
    }

    /// Flip the theme group's follow-outer switch. On the *first* detach (no
    /// override retained yet) freeze the current outer look so the pane doesn't
    /// visually jump; on later detaches the retained override is restored.
    pub fn toggle_theme(&mut self, outer: &ThemeChoice) {
        if self.inherit_theme {
            self.theme.get_or_insert_with(|| ThemeGroup::of(outer));
            self.inherit_theme = false;
        } else {
            self.inherit_theme = true;
        }
    }

    /// Flip the grade group's follow-outer switch (see [`Self::toggle_theme`]).
    pub fn toggle_grade(&mut self, outer: &ThemeChoice) {
        if self.inherit_grade {
            self.grade.get_or_insert(outer.grade);
            self.inherit_grade = false;
        } else {
            self.inherit_grade = true;
        }
    }

    /// Migrate a legacy single full-pane override: it pinned *both* groups and
    /// followed outer for neither.
    pub fn from_legacy(c: ThemeChoice) -> Self {
        Self {
            theme: Some(ThemeGroup::of(&c)),
            grade: Some(c.grade),
            inherit_theme: false,
            inherit_grade: false,
        }
    }

    /// A brand-new terminal pane's shipped appearance: pin the green
    /// [`house_terminal`] look and do NOT follow the warm outer cabinet, so a
    /// fresh terminal is the green phosphor CRT regardless of the mother theme.
    /// The "follow outer" toggle still re-attaches it on demand.
    pub fn house() -> Self {
        Self::from_legacy(house_terminal())
    }
}

/// Built-ins + the hot-reloaded user file ("custom").
pub struct ThemeRegistry {
    builtins: Vec<(String, Arc<Theme>)>,
    custom: Arc<Theme>,
}
impl Global for ThemeRegistry {}

/// The outer (workspace) selection; panes without an override follow this.
pub struct OuterChoice(pub ThemeChoice);
impl Global for OuterChoice {}

pub fn outer_choice(cx: &App) -> ThemeChoice {
    cx.global::<OuterChoice>().0.clone()
}

pub fn parse_hex(value: &str) -> Option<Hsla> {
    hex(value)
}

/// (id, icon, label) for the theme picker, in display order. The label is a
/// short caption shown under the glyph; it disambiguates glyph collisions — an
/// unedited `custom` file still carries the `>_` glyph it was seeded from
/// (hacker), so glyph alone can't tell the two apart, but "hacker" vs "custom"
/// can.
pub fn all_themes(cx: &App) -> Vec<(String, String, String)> {
    let reg = cx.global::<ThemeRegistry>();
    let mut out: Vec<_> = reg
        .builtins
        .iter()
        .map(|(id, t)| (id.clone(), t.icon.clone(), short_label(id)))
        .collect();
    // The hot-reloaded user file: always labelled "custom" so it reads as the
    // live-editable slot even when it's still a verbatim copy of a built-in.
    out.push(("custom".into(), reg.custom.icon.clone(), "custom".into()));
    out
}

/// Short picker caption for a theme id: the segment before the first '-'
/// ("tactical-overdrive" → "tactical"), which is unique across the built-ins.
fn short_label(id: &str) -> String {
    id.split('-').next().unwrap_or(id).to_string()
}

/// Hand-picked roles for the [`Dynamic::Custom`] dynamic. Each is an optional
/// "#rrggbb"; an empty slot falls back to a seed-derived default, so a partially
/// filled custom palette still renders coherently.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct CustomPalette {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tertiary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quaternary: Option<String>,
}

/// The signature palette of a named colour set: a default seed plus optional
/// text / title (complement) colours and a program-colour mode. The wheel's
/// seed/T/C overrides tweak on top; an empty slot falls back to the relationship.
pub struct SetSig {
    pub seed: &'static str,
    pub text: Option<&'static str>,
    pub complement: Option<&'static str>,
    pub mode: ColorMode,
}

/// A **colour set**: the palette half of the look, ORTHOGONAL to the theme's
/// texture. The seed (set on the wheel) is the anchor; the set decides how the
/// other roles — title, text, accents — relate to it, and named sets carry a
/// signature palette (a cultural touchstone). Picked by the glyph column in the
/// theme tray; lives in the theme group, so it inherits/overrides per pane.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dynamic {
    /// No colour set — the texture/theme shows its own authored colours. Default,
    /// so old state files are unchanged.
    #[default]
    Plain,
    /// Classic terminal green with a white title and high-contrast green text.
    Greenworks,
    /// Lightning: a deep-purple field with white-blue phosphor text + title.
    Bolt,
    /// Amber monochrome monitor — the warm CRT touchstone.
    Amber,
    /// Gold→green→brown harmony: the seed is the gold anchor; green sits at a
    /// fixed +offset, light brown at a −offset — applied wherever the seed lands.
    Pineapple,
    /// RETRO: a 1970s game-show / slot-machine palette — warm amber-gold anchor,
    /// robin-egg teal complement, vivid game-board greens and cinnabar reds.
    /// Maximum good vibes; the colour half of the GAMBA look (theme `gamba`).
    Retro,
    /// BAT: a nocturnal electric-purple monitor — near-black violet field, bright
    /// magenta phosphor letters, pale-orchid title. The flying-mouse, not baseball.
    Bat,
    /// CHERRY: a dark cherry field with bright cherry-red text and a blush-rose
    /// title — a sweet, glossy crimson monochrome.
    Cherry,
    /// COTTON CLOWNDY: cotton-candy meets clown — a playful pastel SPREAD on a pink
    /// field, fanning pink → sky-blue → violet for confetti accents.
    CottonClowndy,
    /// WOOD: a warm wooden-log monitor — saddle-brown field, light-oak text, birch
    /// title. It's a log; the grain colours stay the same, just a cosy timber glow.
    Wood,
    /// ARMY: olive-drab field with khaki text and a sand title — a muted, sturdy
    /// military monochrome (helmet-and-star).
    Army,
    /// MIDNIGHT: a deep indigo-navy field with icy-periwinkle text and a
    /// moonlight-white title — a calm, starlit blue (moon-and-star).
    Midnight,
    /// SNOWFLAKE: a cold charcoal-blue field with crisp snow-white text and a
    /// pure-white title — the brightest, iciest monochrome.
    Snowflake,
    /// User-defined palette: explicit primary/secondary/tertiary/quaternary.
    /// Boxed so the rare custom palette doesn't bloat every `ThemeChoice` clone.
    Custom(Box<CustomPalette>),
}

impl Dynamic {
    /// The named colour sets shown in the tray, in display order (Custom is
    /// appended separately as the cog).
    pub const NAMED: [Dynamic; 12] = [
        Dynamic::Greenworks,
        Dynamic::Bolt,
        Dynamic::Amber,
        Dynamic::Pineapple,
        Dynamic::Retro,
        // ── themes pack ──
        Dynamic::Bat,
        Dynamic::Cherry,
        Dynamic::CottonClowndy,
        Dynamic::Wood,
        Dynamic::Army,
        Dynamic::Midnight,
        Dynamic::Snowflake,
    ];

    /// Glyph shown in the tray's vertical box for this colour set.
    pub fn glyph(&self) -> &'static str {
        match self {
            Dynamic::Plain => "○",
            Dynamic::Greenworks => "❖",
            Dynamic::Bolt => "⚡",
            Dynamic::Amber => "☼",
            Dynamic::Pineapple => "🍍",
            Dynamic::Retro => "🎰",
            Dynamic::Bat => "🦇",
            Dynamic::Cherry => "🍒",
            Dynamic::CottonClowndy => "🤡",
            Dynamic::Wood => "🪵",
            Dynamic::Army => "🪖",
            Dynamic::Midnight => "🌙",
            Dynamic::Snowflake => "❄",
            Dynamic::Custom(_) => "⚙",
        }
    }

    /// Human name (tests, accessibility) — the tray itself shows only the glyph.
    pub fn label(&self) -> &'static str {
        match self {
            Dynamic::Plain => "plain",
            Dynamic::Greenworks => "greenworks",
            Dynamic::Bolt => "bolt",
            Dynamic::Amber => "amber",
            Dynamic::Pineapple => "pineapple",
            Dynamic::Retro => "retro",
            Dynamic::Bat => "bat",
            Dynamic::Cherry => "cherry",
            Dynamic::CottonClowndy => "cotton-clowndy",
            Dynamic::Wood => "wood",
            Dynamic::Army => "army",
            Dynamic::Midnight => "midnight",
            Dynamic::Snowflake => "snowflake",
            Dynamic::Custom(_) => "custom",
        }
    }

    /// The signature palette this colour set seeds, if it's a named one. The
    /// wheel's seed/T/C overrides win over these; `Plain`/`Custom` have none.
    pub fn signature(&self) -> Option<SetSig> {
        Some(match self {
            // classic green terminal: white title, bright high-contrast green text
            Dynamic::Greenworks => SetSig {
                seed: "#22c55e",
                text: Some("#7dff9e"),
                complement: Some("#ffffff"),
                mode: ColorMode::Monochrome,
            },
            // lightning: deep purple field, white-blue phosphor letters + title
            Dynamic::Bolt => SetSig {
                seed: "#7c3aed",
                text: Some("#cdd8ff"),
                complement: Some("#eef2ff"),
                mode: ColorMode::Monochrome,
            },
            // amber monochrome monitor
            Dynamic::Amber => SetSig {
                seed: "#ffb000",
                text: Some("#ffce6b"),
                complement: Some("#fff0d0"),
                mode: ColorMode::Monochrome,
            },
            // pineapple gold anchor; text/title fall out of the harmony
            Dynamic::Pineapple => SetSig {
                seed: "#ffcc00",
                text: None,
                complement: None,
                mode: ColorMode::OnTheme,
            },
            // retro game-show: warm amber-gold anchor, cream text, teal title.
            // OnTheme so the vivid ANSI board colours flow through unmolested.
            Dynamic::Retro => SetSig {
                seed: "#f5a623",
                text: Some("#fff1c9"),
                complement: Some("#43c6c3"),
                mode: ColorMode::OnTheme,
            },
            // bat: near-black violet field, bright magenta phosphor + orchid title
            Dynamic::Bat => SetSig {
                seed: "#b026ff",
                text: Some("#f0a6ff"),
                complement: Some("#f7d6ff"),
                mode: ColorMode::Monochrome,
            },
            // cherry: dark cherry field, bright cherry-red text, blush-rose title
            Dynamic::Cherry => SetSig {
                seed: "#e11d48",
                text: Some("#ff8fa8"),
                complement: Some("#ffdbe4"),
                mode: ColorMode::Monochrome,
            },
            // cotton clowndy: candy-pink anchor, sky-blue title; OnTheme so the
            // playful pink→sky→violet spread flows into the ANSI accents.
            Dynamic::CottonClowndy => SetSig {
                seed: "#ff6ec7",
                text: Some("#ffd1ef"),
                complement: Some("#a0e9ff"),
                mode: ColorMode::OnTheme,
            },
            // wood: saddle-brown field, light-oak text, birch-cream title
            Dynamic::Wood => SetSig {
                seed: "#8a5a2b",
                text: Some("#d8b486"),
                complement: Some("#f0dcc0"),
                mode: ColorMode::Monochrome,
            },
            // army: olive-drab field, khaki text, sand title
            Dynamic::Army => SetSig {
                seed: "#6b7d2f",
                text: Some("#cdd6a3"),
                complement: Some("#eef0d2"),
                mode: ColorMode::Monochrome,
            },
            // midnight: deep indigo-navy field, icy-periwinkle text, moonlight title
            Dynamic::Midnight => SetSig {
                seed: "#4361ee",
                text: Some("#b8c9ff"),
                complement: Some("#e6ecff"),
                mode: ColorMode::Monochrome,
            },
            // snowflake: cold charcoal-blue field, crisp snow-white text, white title
            Dynamic::Snowflake => SetSig {
                seed: "#7cc4ff",
                text: Some("#e8f4ff"),
                complement: Some("#ffffff"),
                mode: ColorMode::Monochrome,
            },
            _ => return None,
        })
    }

    /// A legible swatch colour for the tray glyph — the set's signature seed
    /// lifted to a readable lightness/saturation on the dark chip, so the plain
    /// symbol glyphs (❖ ⚡ ☼ …) read as green / purple / amber at a glance.
    /// `None` for Plain/Custom (no signature) → the glyph keeps the theme text
    /// colour. Colour-emoji glyphs (🍍 🎰 🍒 …) ignore this tint and show their
    /// own colours, so it lands only on the monochrome symbol sets.
    pub fn swatch(&self) -> Option<Hsla> {
        let mut c = hex(self.signature()?.seed)?;
        c.s = c.s.max(0.45);
        c.l = c.l.clamp(0.55, 0.72);
        c.a = 1.0;
        Some(c)
    }

    /// `true` for the serde/skip default — no colour set.
    pub fn is_plain(&self) -> bool {
        matches!(self, Dynamic::Plain)
    }

    /// Same variant (ignoring any custom-palette contents) — for tray active state.
    pub fn same_kind(&self, other: &Dynamic) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// Rotate a gpui hue (0..1) by `deg` degrees, wrapping into 0..1.
fn rot(h: f32, deg: f32) -> f32 {
    (h + deg / 360.0).rem_euclid(1.0)
}

/// Shaping parameters for the offset-based named dynamics.
struct Spec {
    sec_deg: f32,
    ter_deg: f32,
    mono: bool,
    text_l: f32,
    text_s: f32,
    prim_l: f32,
    prim_s_floor: f32,
}

/// The four coordinated roles a dynamic derives from an anchor (seed) colour,
/// plus the readable text that goes with them. (The screen background is the
/// theme's, not the dynamic's — see [`apply_dynamic`].)
pub struct Roles {
    pub primary: Hsla,
    pub secondary: Hsla,
    pub tertiary: Hsla,
    pub quaternary: Hsla,
    pub text: Hsla,
}

/// Resolve a dynamic + anchor (seed) into concrete role colours. This is the
/// heart of the "relationship" idea: the named dynamics are pure hue-offset +
/// shaping rules around the anchor; Custom reads explicit colours, falling back
/// to the Prism spread for any empty slot.
pub fn roles(anchor: Hsla, d: &Dynamic) -> Roles {
    if let Dynamic::Custom(c) = d {
        let derived = roles(anchor, &Dynamic::Pineapple);
        let pick = |slot: &Option<String>, fallback: Hsla| {
            slot.as_deref().and_then(hex).unwrap_or(fallback)
        };
        let primary = pick(&c.primary, derived.primary);
        return Roles {
            secondary: pick(&c.secondary, derived.secondary),
            tertiary: pick(&c.tertiary, derived.tertiary),
            quaternary: pick(&c.quaternary, derived.quaternary),
            text: Hsla {
                l: (primary.l + 0.28).min(0.92),
                ..primary
            },
            primary,
        };
    }
    let mono_seed = anchor.s < 0.08;
    let spec = match d {
        Dynamic::Pineapple => Spec {
            sec_deg: 70.,
            ter_deg: -22.,
            mono: false,
            text_l: 0.84,
            text_s: 0.55,
            prim_l: 0.62,
            prim_s_floor: 0.70,
        },
        // retro game-show harmony: amber anchor, robin-egg teal at the far
        // complement, a game-board green between them — saturated, lively.
        Dynamic::Retro => Spec {
            sec_deg: 168.,
            ter_deg: 92.,
            mono: false,
            text_l: 0.86,
            text_s: 0.50,
            prim_l: 0.60,
            prim_s_floor: 0.66,
        },
        // The monochrome colour sets — one hue, bright high-intensity text. The
        // themes-pack monitors (bat/cherry/wood/army/midnight/snowflake) ride here
        // too; their distinct field/text/title come from the signature palette.
        Dynamic::Greenworks
        | Dynamic::Bolt
        | Dynamic::Amber
        | Dynamic::Bat
        | Dynamic::Cherry
        | Dynamic::Wood
        | Dynamic::Army
        | Dynamic::Midnight
        | Dynamic::Snowflake => Spec {
            sec_deg: 0.,
            ter_deg: 0.,
            mono: true,
            text_l: 0.93,
            text_s: 0.12,
            prim_l: 0.72,
            prim_s_floor: 0.30,
        },
        // cotton clowndy: a pastel confetti spread off the candy-pink anchor —
        // secondary fans to sky-blue, tertiary to violet; light + lively.
        Dynamic::CottonClowndy => Spec {
            sec_deg: -130.,
            ter_deg: -60.,
            mono: false,
            text_l: 0.88,
            text_s: 0.45,
            prim_l: 0.70,
            prim_s_floor: 0.55,
        },
        // Plain (and anything else) → single-hue tint, mono when the seed is grey.
        _ => Spec {
            sec_deg: 0.,
            ter_deg: 0.,
            mono: mono_seed,
            text_l: 0.78,
            text_s: 0.40,
            prim_l: 0.60,
            prim_s_floor: 0.45,
        },
    };
    let s = if spec.mono {
        0.0
    } else {
        anchor.s.max(spec.prim_s_floor).clamp(0.0, 1.0)
    };
    let primary = Hsla {
        h: anchor.h,
        s,
        l: spec.prim_l,
        a: 1.,
    };
    let secondary = Hsla {
        h: rot(anchor.h, spec.sec_deg),
        s: if spec.mono { 0. } else { s * 0.92 },
        l: (spec.prim_l + 0.04).min(0.80),
        a: 1.,
    };
    // tertiary leans darker + less saturated — the "brown"/shadow of the harmony.
    let tertiary = Hsla {
        h: rot(anchor.h, spec.ter_deg),
        s: if spec.mono { 0. } else { (s * 0.7).min(0.6) },
        l: (spec.prim_l - 0.20).max(0.22),
        a: 1.,
    };
    let quaternary = Hsla {
        h: rot(anchor.h, (spec.sec_deg + spec.ter_deg) * 0.5),
        s: if spec.mono { 0. } else { s * 0.5 },
        l: (spec.prim_l + 0.10).min(0.85),
        a: 1.,
    };
    let text = Hsla {
        h: anchor.h,
        s: if spec.mono { 0.06 } else { spec.text_s },
        l: spec.text_l,
        a: 1.,
    };
    Roles {
        primary,
        secondary,
        tertiary,
        quaternary,
        text,
    }
}

/// Layer a dynamic over an already-resolved theme. A dynamic is an **orthogonal
/// dimension on top of the base theme**: it re-maps the *relationship* between the
/// seed and the title/text/accents, while the theme's own screen — bg, surface,
/// CRT effects, structural geometry — is left exactly as the theme defines it.
/// `Plain` is the identity (the theme is unchanged), so the four built-in themes
/// keep their default look until a dynamic dimension is chosen.
pub fn apply_dynamic(base: &Theme, anchor: Hsla, d: &Dynamic) -> Theme {
    if d.is_plain() {
        return base.clone();
    }
    let r = roles(anchor, d);
    let mut th = base.clone();
    // Title (accent), its complement, body text and cursor take the dynamic's
    // relationship…
    th.accent = r.primary;
    th.complement = r.secondary;
    th.text = r.text;
    th.cursor = Hsla {
        l: (r.primary.l + 0.12).min(0.9),
        ..r.primary
    };
    th.faint = Hsla {
        l: r.tertiary.l.clamp(0.22, 0.40),
        ..r.tertiary
    };
    th.ansi[7] = r.text;
    // …and the harmony echoes into the accent/link ANSI slots so the seed→colour
    // relationship reads. bg/surface (the theme's screen) are deliberately kept.
    th.ansi[2] = r.secondary;
    th.ansi[3] = r.tertiary;
    th.ansi[6] = r.quaternary;
    th
}

/// Recolour a theme around a seed: structural colours keep their own
/// saturation/lightness, only the hue family moves (grey seeds desaturate).
pub fn apply_seed(base: &Theme, seed: Hsla) -> Theme {
    let mut th = base.clone();
    let mono = seed.s < 0.08;
    let tint = |mut c: Hsla| -> Hsla {
        c.h = seed.h;
        if mono {
            c.s = 0.;
        }
        c
    };
    th.accent = Hsla {
        h: seed.h,
        s: if mono { 0. } else { seed.s.clamp(0.35, 1.) },
        l: seed.l.clamp(0.42, 0.75),
        a: 1.,
    };
    th.bg = tint(th.bg);
    th.surface = tint(th.surface);
    th.text = tint(th.text);
    th.faint = tint(th.faint);
    th.cursor = Hsla {
        l: (th.accent.l + 0.12).min(0.88),
        ..th.accent
    };
    th.ansi[7] = th.text;
    th
}

/// Resolve a choice to a concrete theme: registry lookup + seed recolour.
pub fn resolve(cx: &App, choice: &ThemeChoice) -> Arc<Theme> {
    let reg = cx.global::<ThemeRegistry>();
    let base = if choice.id == "custom" {
        reg.custom.clone()
    } else {
        reg.builtins
            .iter()
            .find(|(id, _)| *id == choice.id)
            .map(|(_, t)| t.clone())
            .unwrap_or_else(|| reg.custom.clone())
    };
    // The colour set's signature supplies default seed/text/title/mode; the
    // wheel's seed/T/C overrides win over it.
    let sig = choice.dynamic.signature();
    let seed = choice
        .seed
        .as_deref()
        .or(sig.as_ref().map(|s| s.seed))
        .and_then(hex);
    let text_over = choice
        .text
        .as_deref()
        .or(sig.as_ref().and_then(|s| s.text))
        .and_then(hex);
    let comp_over = choice
        .complement
        .as_deref()
        .or(sig.as_ref().and_then(|s| s.complement))
        .and_then(hex);
    // The human (your-input) colour has no signature default — it's either an
    // explicit wheel override or derived from the final palette below.
    let human_over = choice.human.as_deref().and_then(hex);
    // Default colour mode → the set's signature mode (if any).
    let mode = if choice.color.is_default() {
        sig.as_ref().map(|s| s.mode).unwrap_or(choice.color)
    } else {
        choice.color
    };
    // Plain with no overrides and no signature leaves the theme's colours alone.
    let identity_colour = choice.dynamic.is_plain()
        && seed.is_none()
        && text_over.is_none()
        && comp_over.is_none()
        && human_over.is_none();
    // Fast path: stock theme, no recolour, default mode, no syntax, neutral
    // grade, flat warp, and no tracking override — i.e. nothing to restate, so the
    // shared base Arc is returned untouched. Warp/tracking are excluded from
    // `is_neutral` (they're not paint grades), so guard them explicitly here:
    // a curved or rolling pane must take the full path so `th.warp`/tracking get set.
    if identity_colour
        && mode.is_default()
        && !choice.syntax
        && choice.grade.is_neutral()
        && choice.grade.warp.abs() < 1e-3
        && choice.grade.tracking.is_none()
        && !choice.grade.crawl
    {
        return base;
    }
    // Layer 1 — the THEME: its own seed recolour (the theme's built-in
    // seed→palette behaviour). No seed → the theme as authored.
    let mut th = match seed {
        Some(seed) => apply_seed(&base, seed),
        None => (*base).clone(),
    };
    // Layer 2 — the DYNAMIC dimension: an orthogonal modifier that re-maps the
    // seed→title/text relationship on top of the theme, keeping its screen.
    // `Plain` is the identity, so a theme with no dynamic looks exactly as before.
    if !choice.dynamic.is_plain() {
        let anchor = seed.unwrap_or(base.accent);
        th = apply_dynamic(&th, anchor, &choice.dynamic);
    }
    // Layer 3 — explicit per-colour overrides from the wheel's T/C targets. These
    // win over whatever the theme + dynamic derived.
    if let Some(t) = text_over {
        th.text = t;
        th.ansi[7] = t;
    }
    if let Some(c) = comp_over {
        th.complement = c;
    }
    // Layer 3b — the human (your-input) colour: an explicit wheel override wins;
    // otherwise keep the base theme's value (a file `human` or the bright
    // complement parse() derives), so the file/default is honoured.
    if let Some(h) = human_over {
        th.human = h;
    }
    th.color_mode = mode;
    th.syntax = choice.syntax;
    th.syntax_scheme = choice.syntax_scheme;
    th.grade = choice.grade;
    // Warp + tracking ride the grade group, so they resolve per-pane (own
    // override else inherited outer) — each pane bends by its own curvature
    // instead of one global dial bending every pane.
    th.warp = choice.grade.warp.clamp(0.0, WARP_MAX);
    if let Some(dial) = choice.grade.tracking {
        apply_tracking(&mut th, dial);
    }
    // Crawl rides the grade group too: per-pane perspective + crawl font.
    th.crawl = choice.grade.crawl;
    th.crawl_angle = choice
        .grade
        .crawl_angle
        .clamp(CRAWL_ANGLE_MIN, CRAWL_ANGLE_MAX);
    th.crawl_depth = choice
        .grade
        .crawl_depth
        .clamp(CRAWL_DEPTH_MIN, CRAWL_DEPTH_MAX);
    Arc::new(th)
}

/// Set the outer (workspace) theme and repaint everything.
pub fn select_outer(cx: &mut App, choice: ThemeChoice) {
    let th = resolve(cx, &choice);
    cx.set_global(ActiveTheme(th));
    cx.set_global(OuterChoice(choice));
    cx.refresh_windows();
}

/// Screen-warp (CRT barrel) curvature. Once a single global dial; now a PER-PANE
/// setting that rides the grade group ([`Grade::warp`]) so each pane bends by its
/// own amount (own override else inherited outer) — `resolve` writes it to
/// [`Theme::warp`] and the renderer registers each tube with its own `(k1, k2)`.
/// `0` = dead flat; the slider runs to [`WARP_MAX`] for a full fishbowl.
pub const WARP_DEFAULT: f32 = 1.43;
pub const WARP_MAX: f32 = 1.5;

/// The barrel coefficients `(k1, k2)` the renderer + hit-testing use for a given
/// warp amount — kept here so geometry stays in sync with the shader's scaling.
pub fn warp_coeffs(amount: f32) -> (f32, f32) {
    (amount * 0.14, amount * 0.06)
}

/// The grid font a crawl-mode pane renders in: a libre News-Gothic clone (SIL
/// OFL), the closest freely-licensable match to the iconic crawl typeface. The
/// TTF is bundled and registered at startup (see `main`); `grid_font` swaps to
/// it (italic) when [`Theme::crawl`] is on.
pub const CRAWL_FONT_FAMILY: &str = "News Cycle";

/// Crawl knob ranges (degrees / ratio). Defaults are the look a fresh crawl
/// turns on at: a gentle 12° vergence and a classic 2.5× near:far height.
pub const CRAWL_ANGLE_MIN: f32 = 2.0;
pub const CRAWL_ANGLE_MAX: f32 = 59.0;
pub const CRAWL_ANGLE_DEFAULT: f32 = 12.0;
pub const CRAWL_DEPTH_MIN: f32 = 0.05;
pub const CRAWL_DEPTH_MAX: f32 = 15.0;
pub const CRAWL_DEPTH_DEFAULT: f32 = 2.5;

/// The crawl perspective coefficients the renderer warps each tube by, for a
/// given angle (degrees) and depth ratio — kept here so the geometry stays in
/// sync with the shader's inverse map (`fs_crt` in `crt_pass.wgsl`):
/// - `a` = top-edge width ratio (`1` = no horizontal taper, smaller = sides
///   converge harder toward the top). Driven by `angle`.
/// - `d` = depth ratio = text height at the BOTTOM ÷ at the TOP (`>1` classic,
///   `1` = no vertical foreshortening). Passed straight through, clamped.
pub fn crawl_coeffs(angle_deg: f32, depth: f32) -> (f32, f32) {
    let a = (1.0
        - 1.3
            * angle_deg
                .clamp(CRAWL_ANGLE_MIN, CRAWL_ANGLE_MAX)
                .to_radians()
                .sin())
    .clamp(0.2, 1.0);
    let d = depth.clamp(CRAWL_DEPTH_MIN, CRAWL_DEPTH_MAX);
    (a, d)
}

/// Monotonic counter bumped whenever a global input to [`resolve`] changes that a
/// `ThemeChoice` does NOT itself carry — the custom theme (hot-reload) or the
/// tracking override. Per-pane theme memos key on this (plus the choice/mode) so
/// they recompute exactly when an input changed and can never serve a stale look.
#[derive(Default)]
pub struct ThemeGen(pub u64);
impl Global for ThemeGen {}

/// Current theme generation (0 if never bumped).
pub fn theme_gen(cx: &App) -> u64 {
    cx.try_global::<ThemeGen>().map(|g| g.0).unwrap_or(0)
}
fn bump_theme_gen(cx: &mut App) {
    let n = theme_gen(cx).wrapping_add(1);
    cx.set_global(ThemeGen(n));
}

/// Map the three normalised tracking dials to concrete `Theme` fields:
/// intensity 0..1, speed (high = faster roll = shorter period, 60→6), size →
/// sweep 1..30.
pub fn apply_tracking(th: &mut Theme, dial: [f32; 3]) {
    let [i, sp, sz] = dial;
    th.tracking = i.clamp(0.0, 1.0);
    th.tracking_period = (60.0 - sp.clamp(0.0, 1.0) * 54.0).max(2.0);
    th.tracking_sweep = (1.0 + sz.clamp(0.0, 1.0) * 29.0).max(1.0);
}

/// The current effective tracking dials for a resolved theme, as normalised
/// `0..1` — `th`'s own values inverted back to dial space. Lets a slider start
/// from where the look currently is (warp/tracking are resolved per-pane into
/// `th`, so this reflects the scope already in effect).
pub fn tracking_dials_of(th: &Theme) -> [f32; 3] {
    [
        th.tracking.clamp(0.0, 1.0),
        ((60.0 - th.tracking_period) / 54.0).clamp(0.0, 1.0),
        ((th.tracking_sweep - 1.0) / 29.0).clamp(0.0, 1.0),
    ]
}

fn hex(value: &str) -> Option<Hsla> {
    let v = value.trim().trim_start_matches('#');
    if v.len() != 6 {
        return None;
    }
    u32::from_str_radix(v, 16).ok().map(|c| rgb(c).into())
}

pub(crate) fn parse(source: &str) -> Result<Theme, String> {
    let file: ThemeFile = toml::from_str(source).map_err(|e| e.to_string())?;
    let c = &file.colors;
    let need = |s: &String, what: &str| hex(s).ok_or(format!("bad color for {what}: {s}"));
    if c.ansi.len() != 16 {
        return Err(format!(
            "colors.ansi must have 16 entries, got {}",
            c.ansi.len()
        ));
    }
    let mut ansi = [Hsla::default(); 16];
    for (i, s) in c.ansi.iter().enumerate() {
        ansi[i] = need(s, &format!("ansi[{i}]"))?;
    }
    let accent = need(&c.accent, "accent")?;
    let name = file.name.unwrap_or_else(|| "unnamed".into());
    let default_screen_glare = if name == "hacker" { 0.42 } else { 0.0 };
    Ok(Theme {
        name,
        icon: file.icon.unwrap_or_else(|| "◈".into()),
        bg: need(&c.bg, "bg")?,
        surface: need(&c.surface, "surface")?,
        text: need(&c.text, "text")?,
        accent,
        // Default complement = the accent's opposite hue; resolve() may restate it
        // from the active dynamic or an explicit `C` override.
        complement: Hsla {
            h: (accent.h + 0.5).rem_euclid(1.0),
            ..accent
        },
        // Default human (your-input) colour: a bright, lively complement so the
        // user's turns pop against the agent's text. A theme file may override it.
        human: c.human.as_ref().and_then(|s| hex(s)).unwrap_or(Hsla {
            h: (accent.h + 0.5).rem_euclid(1.0),
            s: (accent.s * 0.8).clamp(0.45, 0.85),
            l: 0.76,
            a: 1.0,
        }),
        faint: need(&c.faint, "faint")?,
        cursor: c.cursor.as_ref().and_then(|s| hex(s)).unwrap_or(accent),
        ansi,
        color_mode: ColorMode::default(),
        syntax: false,
        syntax_scheme: SyntaxScheme::Code,
        grade: Grade::default(),
        scanline_opacity: file.effects.scanline_opacity.unwrap_or(0.).clamp(0., 0.6),
        scanline_step: file.effects.scanline_step.unwrap_or(4.).max(2.),
        vignette: file.effects.vignette.unwrap_or(0.).clamp(0., 1.),
        glow: file.effects.glow.unwrap_or(0.).clamp(0., 1.),
        bloom: file.effects.bloom.unwrap_or(0.).clamp(0., 1.),
        tracking: file.effects.tracking.unwrap_or(0.).clamp(0., 1.),
        tracking_period: file.effects.tracking_period.unwrap_or(14.).clamp(2., 120.),
        tracking_sweep: file.effects.tracking_sweep.unwrap_or(7.).clamp(1., 30.),
        // Resolved per-pane from the scope's Grade::warp in resolve(); the base
        // theme is flat until then.
        warp: 0.0,
        // Resolved per-pane from the scope's Grade in resolve(); off by default.
        crawl: false,
        crawl_angle: CRAWL_ANGLE_DEFAULT,
        crawl_depth: CRAWL_DEPTH_DEFAULT,
        flicker: file.effects.flicker.unwrap_or(0.).clamp(0., 1.),
        jiggle: file.effects.jiggle.unwrap_or(0.).clamp(0., 1.),
        screen_glare: file
            .effects
            .screen_glare
            .unwrap_or(default_screen_glare)
            .clamp(0., 1.),
        bezel: file.effects.bezel.unwrap_or(0.).clamp(0., 1.),
        font_family: file.font.family.unwrap_or_else(|| "JetBrains Mono".into()),
        font_size: file.font.size.unwrap_or(14.).clamp(8., 32.),
        cell_h: file.font.cell_height.unwrap_or(20.).clamp(10., 48.),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_themes_parse_with_distinct_ids_and_icons() {
        let mut icons = vec![];
        for (id, src) in BUILTIN_THEMES {
            let th = parse(src).unwrap_or_else(|e| panic!("{id} failed to parse: {e}"));
            assert_eq!(&th.name, id, "theme file name must match registry id");
            icons.push(th.icon);
        }
        assert_eq!(BUILTIN_THEMES.len(), 5);
    }

    #[test]
    fn dynamic_tray_entries_have_distinct_glyphs_and_labels() {
        // The tray is glyph-only (no captions/hover), so each entry must be
        // visually distinct — the cog (Custom) is appended after the named sets.
        let mut entries: Vec<&Dynamic> = Dynamic::NAMED.iter().collect();
        let custom = Dynamic::Custom(Box::default());
        entries.push(&custom);
        let glyphs: std::collections::HashSet<_> = entries.iter().map(|d| d.glyph()).collect();
        let labels: std::collections::HashSet<_> = entries.iter().map(|d| d.label()).collect();
        assert_eq!(
            glyphs.len(),
            entries.len(),
            "every dynamic needs its own glyph"
        );
        assert_eq!(
            labels.len(),
            entries.len(),
            "every dynamic needs its own label"
        );
        assert_eq!(
            entries.len(),
            13,
            "twelve named dynamics plus the custom cog"
        );
    }

    #[test]
    fn every_named_set_carries_a_parseable_signature() {
        // A tray set with no signature would click to a no-op (Plain-like) look;
        // every NAMED set must seed a real, parseable palette.
        for d in Dynamic::NAMED.iter() {
            let sig = d
                .signature()
                .unwrap_or_else(|| panic!("{} has no signature", d.label()));
            assert!(
                hex(sig.seed).is_some(),
                "{} seed {} does not parse",
                d.label(),
                sig.seed
            );
            // …and every named set must carry a legible tray swatch so the glyph
            // reads as its palette colour on the dark chip.
            let sw = d
                .swatch()
                .unwrap_or_else(|| panic!("{} has no swatch", d.label()));
            assert!(
                (0.55..=0.72).contains(&sw.l),
                "{} swatch lightness {} not in legible band",
                d.label(),
                sw.l
            );
        }
    }

    fn outer_named(id: &str, brightness: f32) -> ThemeChoice {
        ThemeChoice {
            id: id.into(),
            grade: Grade {
                brightness,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn pane_theme_inherits_each_group_independently() {
        let outer = outer_named("field-command", 0.8);
        // Inherit theme, but pin a divergent grade.
        let mut p = PaneTheme::default();
        p.set_grade(Grade {
            brightness: 0.2,
            ..Default::default()
        });
        let eff = p.effective(&outer);
        assert_eq!(eff.id, "field-command", "theme group still follows outer");
        assert!(!p.inherit_theme || (eff.grade.brightness - 0.2).abs() < 1e-6);
        assert!(
            (eff.grade.brightness - 0.2).abs() < 1e-6,
            "grade group uses the pane's own pin"
        );
        // Now also pin a divergent theme; both groups are the pane's own.
        p.set_theme(ThemeGroup {
            id: "hacker".into(),
            ..Default::default()
        });
        let eff = p.effective(&outer);
        assert_eq!(eff.id, "hacker");
        assert!((eff.grade.brightness - 0.2).abs() < 1e-6);
    }

    #[test]
    fn pane_theme_follows_outer_live_until_it_diverges() {
        let p = PaneTheme::default();
        assert!(p.is_pristine());
        // A pristine pane mirrors whatever outer currently is — the link is live.
        assert_eq!(p.effective(&outer_named("hacker", 0.5)).id, "hacker");
        assert_eq!(
            p.effective(&outer_named("field-command", 0.5)).id,
            "field-command"
        );
    }

    #[test]
    fn follow_outer_toggle_is_non_destructive() {
        let outer = outer_named("field-command", 0.5);
        let mut p = PaneTheme::default();
        // Diverge to an explicit theme.
        p.set_theme(ThemeGroup {
            id: "hacker".into(),
            ..Default::default()
        });
        assert_eq!(p.effective(&outer).id, "hacker");
        // Toggle ON: follows outer live, but the pick is RETAINED, not discarded.
        p.toggle_theme(&outer);
        assert!(p.inherit_theme);
        assert_eq!(p.effective(&outer).id, "field-command");
        assert_eq!(
            p.theme.as_ref().unwrap().id,
            "hacker",
            "retained, not wiped"
        );
        // Toggle OFF again: the pane's own theme returns — nothing was lost.
        p.toggle_theme(&outer);
        assert!(!p.inherit_theme);
        assert_eq!(p.effective(&outer).id, "hacker");
    }

    #[test]
    fn first_detach_freezes_the_current_outer_look() {
        let mut p = PaneTheme::default();
        // Detaching a pristine group freezes outer-at-that-moment so nothing jumps.
        p.toggle_theme(&outer_named("hacker", 0.5));
        assert!(!p.inherit_theme);
        // A later outer change no longer affects this now-detached pane.
        assert_eq!(p.effective(&outer_named("field-command", 0.5)).id, "hacker");
    }

    #[test]
    fn warp_is_per_pane_via_the_grade_group_and_does_not_leak_globally() {
        // The reported regression: setting warp on one terminal bent EVERY
        // terminal (warp was a single global). The fix makes warp ride the grade
        // group, so it resolves per-pane (own override else inherited outer) and
        // an override never touches the outer or its siblings.
        let mut outer = outer_named("field-command", 0.5);
        outer.grade.warp = 1.2;

        // a pristine pane inherits the outer warp (grade group follows outer)
        let inheriting = PaneTheme::default();
        assert!(
            (inheriting.effective(&outer).grade.warp - 1.2).abs() < 1e-6,
            "an un-overridden pane inherits the outer warp"
        );

        // a pane that pins a flat grade bends by its OWN (zero) warp — and the
        // outer stays bent: warp no longer leaks globally (the whole bug).
        let mut flat = PaneTheme::default();
        flat.set_grade(Grade {
            warp: 0.0,
            ..Grade::default()
        });
        assert!(
            flat.effective(&outer).grade.warp.abs() < 1e-6,
            "an overriding pane bends by its own (flat) warp"
        );
        assert!(
            (outer.grade.warp - 1.2).abs() < 1e-6,
            "the outer warp is untouched — warp no longer leaks across panes"
        );

        // the inverse: a bent pane keeps its bend even beside a FLAT outer cabinet
        let mut bent = PaneTheme::default();
        bent.set_grade(Grade {
            warp: 1.4,
            ..Grade::default()
        });
        let mut flat_outer = outer_named("field-command", 0.5);
        flat_outer.grade.warp = 0.0;
        assert!(
            (bent.effective(&flat_outer).grade.warp - 1.4).abs() < 1e-6,
            "a bent pane bends even when the outer cabinet is flat"
        );
    }

    #[test]
    fn tracking_is_per_pane_via_the_grade_group() {
        let mut outer = outer_named("field-command", 0.5);
        outer.grade.tracking = Some([0.9, 0.5, 0.3]);

        // inherit: a pristine pane follows the outer roll bar
        assert_eq!(
            PaneTheme::default().effective(&outer).grade.tracking,
            Some([0.9, 0.5, 0.3]),
            "tracking inherits the outer dial"
        );

        // override (drop): a pane can pin a grade that clears the roll override
        // (None) → it defers to its theme's authored roll, not the outer's dial
        let mut quiet = PaneTheme::default();
        quiet.set_grade(Grade {
            tracking: None,
            ..Grade::default()
        });
        assert_eq!(
            quiet.effective(&outer).grade.tracking,
            None,
            "an overriding pane can drop the outer roll dial"
        );

        // override (pin): a pane rolls by its OWN dial while the outer differs
        let mut rolled = PaneTheme::default();
        rolled.set_grade(Grade {
            tracking: Some([0.2, 0.8, 0.1]),
            ..Grade::default()
        });
        assert_eq!(
            rolled.effective(&outer).grade.tracking,
            Some([0.2, 0.8, 0.1]),
            "a pane rolls by its own dial, not the outer's"
        );
    }

    #[test]
    fn grade_default_carries_the_house_warp_and_neutral_resets_flat() {
        // a fresh scope starts at the house bend; reset (neutral) is dead flat.
        assert!((Grade::default().warp - WARP_DEFAULT).abs() < 1e-6);
        assert_eq!(Grade::default().tracking, None);
        assert!(Grade::neutral().warp.abs() < 1e-6, "reset = flat");
        // warp/tracking are NOT paint grades — a curved-but-ungraded pane still
        // takes resolve()'s neutral fast path (is_neutral ignores them).
        let curved = Grade {
            warp: 1.4,
            ..Grade::neutral()
        };
        assert!(
            curved.is_neutral(),
            "warp is excluded from the paint-grade neutral check"
        );
    }

    #[test]
    fn hex_accepts_themed_forms_and_rejects_junk() {
        assert!(hex("#22c55e").is_some());
        assert!(hex("22c55e").is_some());
        assert!(hex("  #22c55e  ").is_some());
        assert!(hex("#22c5").is_none());
        assert!(hex("#22c55ea1").is_none());
        assert!(hex("#zzzzzz").is_none());
        assert!(hex("").is_none());
    }

    #[test]
    fn seed_recolour_moves_hue_and_handles_grey() {
        let base = parse(DEFAULT_THEME_TOML).unwrap();
        let cyan = hex("#31d7ff").unwrap();
        let seeded = apply_seed(&base, cyan);
        assert!((seeded.accent.h - cyan.h).abs() < 0.01);
        assert!((seeded.bg.h - cyan.h).abs() < 0.01);
        let grey = hex("#828282").unwrap();
        let mono = apply_seed(&base, grey);
        assert!(mono.accent.s < 0.01 && mono.bg.s < 0.01);
    }

    /// Shortest distance between two gpui hues (0..1), in degrees.
    fn hue_deg_gap(a: f32, b: f32) -> f32 {
        let d = (a - b).rem_euclid(1.0) * 360.0;
        d.min(360.0 - d)
    }

    #[test]
    fn pineapple_spreads_gold_green_brown_and_travels_with_the_seed() {
        let gold = hex("#ffcc00").unwrap();
        let r = roles(gold, &Dynamic::Pineapple);
        // primary keeps the seed (the gold anchor)
        assert!(hue_deg_gap(r.primary.h, gold.h) < 1.0);
        // green ~70° one way, brown ~22° the other — the pineapple relationship
        assert!((hue_deg_gap(r.secondary.h, gold.h) - 70.0).abs() < 3.0);
        assert!((hue_deg_gap(r.tertiary.h, gold.h) - 22.0).abs() < 3.0);
        // the "brown" tertiary is darker than the gold primary
        assert!(r.tertiary.l < r.primary.l);
        // the SAME relationship applies wherever the seed lands (rotate to blue)
        let blue = hex("#3366ff").unwrap();
        let rb = roles(blue, &Dynamic::Pineapple);
        assert!((hue_deg_gap(rb.secondary.h, blue.h) - 70.0).abs() < 3.0);
    }

    #[test]
    fn mono_colour_sets_are_single_hue_with_blazing_text() {
        // Greenworks / Bolt / Amber all use the monochrome relationship.
        for set in [Dynamic::Greenworks, Dynamic::Bolt, Dynamic::Amber] {
            let r = roles(hex("#22c55e").unwrap(), &set);
            assert!(
                r.secondary.s < 0.02 && r.tertiary.s < 0.02,
                "{} roles are mono",
                set.label()
            );
            assert!(r.text.l > 0.9 && r.text.s < 0.2, "high-intensity letters");
        }
    }

    #[test]
    fn tracking_dial_maps_normalised_dials_to_theme_fields() {
        let mut th = parse(DEFAULT_THEME_TOML).unwrap();
        apply_tracking(&mut th, [1.0, 1.0, 1.0]);
        assert!((th.tracking - 1.0).abs() < 1e-6, "intensity 1");
        assert!(th.tracking_period <= 6.5, "max speed → short period");
        assert!(
            (th.tracking_sweep - 30.0).abs() < 0.5,
            "max size → 30 sweep"
        );
        apply_tracking(&mut th, [0.0, 0.0, 0.0]);
        assert_eq!(th.tracking, 0.0, "intensity 0 = roll off");
        assert!(
            (th.tracking_period - 60.0).abs() < 0.5,
            "speed 0 → long period"
        );
        // round-trips back to dial space (invert)
        apply_tracking(&mut th, [0.4, 0.6, 0.5]);
        let back = [
            th.tracking,
            (60.0 - th.tracking_period) / 54.0,
            (th.tracking_sweep - 1.0) / 29.0,
        ];
        for (a, b) in back.iter().zip([0.4, 0.6, 0.5]) {
            assert!((a - b).abs() < 0.02, "dial round-trips: {a} vs {b}");
        }
    }

    #[test]
    fn named_colour_sets_carry_a_signature_palette() {
        // Greenworks: green seed, white title, no signature mode change (mono).
        let g = Dynamic::Greenworks.signature().unwrap();
        assert_eq!(g.seed, "#22c55e");
        assert_eq!(g.complement, Some("#ffffff"), "white title");
        assert_eq!(g.mode, ColorMode::Monochrome);
        // Bolt: deep purple seed, white-blue text.
        let b = Dynamic::Bolt.signature().unwrap();
        assert_eq!(b.seed, "#7c3aed");
        assert_eq!(b.text, Some("#cdd8ff"));
        // Plain / Custom carry no signature.
        assert!(Dynamic::Plain.signature().is_none());
        assert!(Dynamic::Custom(Box::default()).signature().is_none());
    }

    #[test]
    fn custom_uses_explicit_roles_and_falls_back_for_empty_slots() {
        let c = CustomPalette {
            primary: Some("#ff0000".into()),
            secondary: Some("#00ff00".into()),
            tertiary: None,
            quaternary: None,
        };
        let r = roles(hex("#888888").unwrap(), &Dynamic::Custom(Box::new(c)));
        assert!(hue_deg_gap(r.primary.h, hex("#ff0000").unwrap().h) < 1.0);
        assert!(hue_deg_gap(r.secondary.h, hex("#00ff00").unwrap().h) < 1.0);
        // an empty slot falls back to a derived role — not the explicit red
        assert!(hue_deg_gap(r.tertiary.h, hex("#ff0000").unwrap().h) > 5.0);
    }

    #[test]
    fn apply_dynamic_pops_the_title_off_the_base_and_the_body() {
        let base = parse(DEFAULT_THEME_TOML).unwrap();
        let gold = hex("#ffcc00").unwrap();
        let th = apply_dynamic(&base, gold, &Dynamic::Pineapple);
        // the title (accent) takes the vivid gold anchor, away from the base green
        assert!(hue_deg_gap(th.accent.h, gold.h) < 2.0);
        assert!(hue_deg_gap(th.accent.h, base.accent.h) > 5.0);
        // title and body text stay distinct in lightness so the title reads
        assert!((th.accent.l - th.text.l).abs() > 0.08);
    }

    #[test]
    fn a_dynamic_is_a_layer_that_keeps_the_theme_screen() {
        // The blunder-fix invariant: a dynamic is an orthogonal dimension ON TOP
        // of the theme — it re-maps title/text/accents but must NOT touch the
        // theme's own screen (bg/surface), and Plain must be the identity.
        let base = parse(DEFAULT_THEME_TOML).unwrap();
        let gold = hex("#ffcc00").unwrap();
        let dynamic = apply_dynamic(&base, gold, &Dynamic::Pineapple);
        assert_eq!(
            dynamic.bg, base.bg,
            "dynamic must keep the theme background"
        );
        assert_eq!(
            dynamic.surface, base.surface,
            "dynamic must keep the theme surface"
        );
        assert_ne!(dynamic.accent, base.accent, "but the title is re-mapped");
        assert_ne!(dynamic.text, base.text, "and the body text is re-mapped");
        // Plain leaves the theme exactly itself.
        let plain = apply_dynamic(&base, gold, &Dynamic::Plain);
        assert_eq!(plain.accent, base.accent);
        assert_eq!(plain.text, base.text);
        assert_eq!(plain.bg, base.bg);
    }

    #[test]
    fn text_and_complement_overrides_round_trip_and_ride_the_theme_group() {
        let c = ThemeChoice {
            text: Some("#ff8800".into()),
            complement: Some("#0088ff".into()),
            ..Default::default()
        };
        // wire round-trip
        let back: ThemeChoice = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.text.as_deref(), Some("#ff8800"));
        assert_eq!(back.complement.as_deref(), Some("#0088ff"));
        // omitted from the wire when unset
        let d = toml::to_string(&ThemeChoice::default()).unwrap();
        assert!(!d.contains("text") && !d.contains("complement"));
        // they travel with the theme group, so a pane override carries them
        let pane = PaneTheme {
            theme: Some(ThemeGroup::of(&c)),
            inherit_theme: false,
            ..Default::default()
        };
        let eff = pane.effective(&ThemeChoice::default());
        assert_eq!(eff.text.as_deref(), Some("#ff8800"));
        assert_eq!(eff.complement.as_deref(), Some("#0088ff"));
    }

    #[test]
    fn human_override_round_trips_and_rides_the_theme_group() {
        let c = ThemeChoice {
            human: Some("#22e0ff".into()),
            ..Default::default()
        };
        // wire round-trip
        let back: ThemeChoice = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert_eq!(back.human.as_deref(), Some("#22e0ff"));
        // omitted from the wire when unset (no churn for the common case)
        let d = toml::to_string(&ThemeChoice::default()).unwrap();
        assert!(!d.contains("human"), "default human is omitted: {d}");
        // an explicit override rides the theme group onto a pinned pane
        let pinned = PaneTheme {
            theme: Some(ThemeGroup::of(&c)),
            inherit_theme: false,
            ..Default::default()
        };
        assert_eq!(
            pinned.effective(&ThemeChoice::default()).human.as_deref(),
            Some("#22e0ff"),
            "a pinned pane keeps its own human colour"
        );
        // …and a pane that follows outer INHERITS the outer human colour
        let outer = ThemeChoice {
            human: Some("#abcdef".into()),
            ..Default::default()
        };
        assert_eq!(
            PaneTheme::default().effective(&outer).human.as_deref(),
            Some("#abcdef"),
            "an inheriting pane takes the outer human colour"
        );
    }

    #[test]
    fn parsed_human_defaults_bright_and_honours_the_file() {
        // No `human` key → derived as a bright complement of the accent so the
        // user's input pops against the agent's text.
        let th = parse(DEFAULT_THEME_TOML).expect("parses");
        let comp_h = (th.accent.h + 0.5).rem_euclid(1.0);
        assert!(
            (th.human.h - comp_h).abs() < 1e-4,
            "human takes the complement hue"
        );
        assert!(
            th.human.l > 0.6,
            "derived human is bright (l={})",
            th.human.l
        );
        // A theme file may pin it explicitly under [colors].
        let doctored = DEFAULT_THEME_TOML.replacen("[colors]", "[colors]\nhuman = \"#ff2299\"", 1);
        let th2 = parse(&doctored).expect("parses with explicit human");
        let want = hex("#ff2299").unwrap();
        assert!(
            (th2.human.h - want.h).abs() < 2e-3 && (th2.human.l - want.l).abs() < 2e-3,
            "explicit file human is honoured"
        );
    }

    #[test]
    fn house_outer_resolves_to_a_warm_amber_cabinet() {
        // The shipped outer is the green base seeded amber — the resolved chrome
        // must be WARM (amber accent, dark warm bg), never grey or green.
        let base = parse(DEFAULT_THEME_TOML).expect("embedded theme parses");
        let c = house_outer();
        assert_eq!(c.color, ColorMode::Default, "ansi");
        assert!(c.syntax, "code highlighting on");
        assert!(
            c.dynamic.is_plain(),
            "warmth is from the seed, not a colour set"
        );
        let warm = |h: f32| (15.0..=55.0).contains(&(h * 360.0)); // orange/amber band
        let seed = hex(c.seed.as_deref().expect("outer carries an amber seed")).unwrap();
        assert!(warm(seed.h), "seed is amber, got {}°", seed.h * 360.0);
        let th = apply_seed(&base, seed);
        assert!(warm(th.accent.h), "accent warms to amber (was green)");
        assert!(th.accent.s > 0.3, "accent stays saturated, not grey");
        assert!(
            warm(th.bg.h) && th.bg.l < 0.22,
            "cabinet bg is warm and dark"
        );
        // the warm outer grade differs from the green house default, so it persists
        assert!(
            !c.grade.is_default(),
            "outer grade is the warm cabinet grade"
        );
    }

    #[test]
    fn house_terminal_is_the_wood_design_and_does_not_follow_the_warm_outer() {
        // The shipped INNER design: WOOD colour set · HACKER base · AGENTIC
        // syntax · THEME (OnTheme) program colour, GAUGES neutral but warped.
        let t = house_terminal();
        assert_eq!(t.id, "hacker");
        assert!(
            t.seed.is_none(),
            "the Wood set supplies the seed, not an override"
        );
        assert_eq!(t.color, ColorMode::OnTheme, "theme program colour");
        assert!(t.syntax && t.syntax_scheme == SyntaxScheme::Agentic);
        assert_eq!(t.dynamic, Dynamic::Wood);
        assert_eq!(t.grade.warp, WARP_DEFAULT, "carries the house warp");
        assert!(
            (t.grade.brightness - 0.5).abs() < f32::EPSILON,
            "GAUGES sliders start neutral"
        );

        let p = PaneTheme::house();
        assert!(
            !p.inherit_theme && !p.inherit_grade,
            "a fresh terminal is pinned, NOT following the warm cabinet"
        );
        assert!(!p.is_pristine());
        // rendered against the amber cabinet, the pane keeps its own Wood design
        let eff = p.effective(&house_outer());
        assert_eq!(
            eff.dynamic,
            Dynamic::Wood,
            "the pane's own Wood design inside the cabinet"
        );
        assert_eq!(eff.grade.warp, WARP_DEFAULT, "pane keeps the warped GAUGES");
    }

    #[test]
    fn dynamic_round_trips_and_plain_is_omitted() {
        // Plain is the default → skipped on the wire (old state files unchanged)
        let plain = toml::to_string(&ThemeChoice::default()).unwrap();
        assert!(
            !plain.contains("dynamic"),
            "default Plain dynamic is skipped"
        );
        // a named dynamic survives a round-trip
        let named = ThemeChoice {
            dynamic: Dynamic::Pineapple,
            ..Default::default()
        };
        let back: ThemeChoice = toml::from_str(&toml::to_string(&named).unwrap()).unwrap();
        assert_eq!(back.dynamic, Dynamic::Pineapple);
        // a custom palette round-trips with its slots intact
        let cust = ThemeChoice {
            dynamic: Dynamic::Custom(Box::new(CustomPalette {
                primary: Some("#abcdef".into()),
                ..Default::default()
            })),
            ..Default::default()
        };
        let back: ThemeChoice = toml::from_str(&toml::to_string(&cust).unwrap()).unwrap();
        assert!(
            matches!(back.dynamic, Dynamic::Custom(p) if p.primary.as_deref() == Some("#abcdef")),
            "custom palette survives the wire"
        );
    }

    #[test]
    fn syntax_is_an_orthogonal_axis_and_round_trips() {
        // source + syntax serialise as two independent fields
        let c = ThemeChoice {
            id: "hacker".into(),
            color: ColorMode::OnTheme,
            syntax: true,
            syntax_scheme: SyntaxScheme::Code,
            ..Default::default()
        };
        let toml = toml::to_string(&c).unwrap();
        let back: ThemeChoice = toml::from_str(&toml).unwrap();
        assert_eq!(back.color, ColorMode::OnTheme);
        assert!(back.syntax, "syntax flag survives a round-trip");

        // syntax=false is omitted from the wire form (the serde skip), even
        // though the shipped default is now on
        let off = toml::to_string(&ThemeChoice {
            syntax: false,
            ..Default::default()
        })
        .unwrap();
        assert!(!off.contains("syntax"), "syntax=false is skipped");
    }

    #[test]
    fn default_grade_is_omitted_but_a_neutral_override_persists() {
        // the shipped house grade is deliberately off-neutral, yet it is the
        // default — so it is omitted from the wire form (the is_default skip)
        assert!(
            !Grade::default().is_neutral(),
            "the house grade is off-neutral"
        );
        assert!(Grade::default().is_default());
        let off = toml::to_string(&ThemeChoice::default()).unwrap();
        assert!(
            !off.contains("grade"),
            "the default grade is skipped on the wire"
        );

        // a user who resets to the neutral identity has DIVERGED from the
        // default, so the grade MUST be written — otherwise reload would spring
        // back to the house grade
        let neutral = ThemeChoice {
            grade: Grade::neutral(),
            ..Default::default()
        };
        let wire = toml::to_string(&neutral).unwrap();
        assert!(wire.contains("grade"), "a neutral override is persisted");
        let back: ThemeChoice = toml::from_str(&wire).unwrap();
        assert!(
            back.grade.is_neutral(),
            "neutral round-trips, not the house default"
        );
    }

    #[test]
    fn grade_percent_round_trips_across_every_channel() {
        // The MCP config API's uniform 0..100 unit must round-trip every channel
        // back to its stored value — including the channels with non-`0..1`
        // ranges (text-size 0.6..2, scale 0.7..1.6, warp, crawl angle/depth) —
        // or a `get → set` would silently drift the look.
        let keys = [
            GradeKey::Brightness,
            GradeKey::Contrast,
            GradeKey::Colour,
            GradeKey::Text,
            GradeKey::Background,
            GradeKey::Gamma,
            GradeKey::Scale,
            GradeKey::TextSize,
            GradeKey::Warp,
            GradeKey::CrawlAngle,
            GradeKey::CrawlDepth,
        ];
        for k in keys {
            let (min, max, _) = k.range();
            // sweep the channel's whole range in stored units
            for i in 0..=20 {
                let stored = min + (max - min) * (i as f32 / 20.0);
                let pct = k.to_percent(stored);
                assert!(
                    (0.0..=100.0).contains(&pct),
                    "{k:?}: percent {pct} out of 0..100 for stored {stored}"
                );
                let back = k.from_percent(pct);
                assert!(
                    (back - stored).abs() < (max - min) * 1e-4 + 1e-6,
                    "{k:?}: round-trip {stored} → {pct}% → {back} drifted"
                );
            }
            // the endpoints map to the bookends, and out-of-range percent saturates
            assert!(k.to_percent(min).abs() < 1e-3, "{k:?}: min should be 0%");
            assert!(
                (k.to_percent(max) - 100.0).abs() < 1e-3,
                "{k:?}: max should be 100%"
            );
            assert!(
                (k.from_percent(-50.0) - min).abs() < 1e-6,
                "{k:?}: below 0% clamps to min"
            );
            assert!(
                (k.from_percent(150.0) - max).abs() < 1e-6,
                "{k:?}: above 100% clamps to max"
            );
        }
    }

    #[test]
    fn grade_round_trips_and_partial_toml_fills_default() {
        // a hand-tuned grade survives a round-trip on ThemeChoice
        let mut g = Grade::default();
        g.set(GradeKey::Brightness, 0.8);
        g.set(GradeKey::Gamma, 0.3);
        let c = ThemeChoice {
            id: "hacker".into(),
            grade: g,
            ..Default::default()
        };
        let back: ThemeChoice = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert!((back.grade.brightness - 0.8).abs() < 1e-6);
        assert!((back.grade.gamma - 0.3).abs() < 1e-6);
        // a partial [grade] table fills the *missing* channels from the house
        // default (Grade::default), not f32's 0.0 — guards the container
        // `#[serde(default)]` wiring.
        let partial: ThemeChoice =
            toml::from_str("id = \"hacker\"\n[grade]\nbrightness = 0.9\n").unwrap();
        let d = Grade::default();
        assert!((partial.grade.brightness - 0.9).abs() < 1e-6);
        assert!(
            (partial.grade.contrast - d.contrast).abs() < 1e-6,
            "omitted channel = house default"
        );
        assert!((partial.grade.colour - d.colour).abs() < 1e-6);
    }

    #[test]
    fn text_size_rides_the_grade_group_under_its_own_range() {
        // The identity scale is 1.0× (Grade::neutral); the shipped house grade
        // sits just under at 0.99× and is the default, not the identity.
        assert!((Grade::neutral().scale - 1.0).abs() < 1e-6);
        assert!(Grade::neutral().is_neutral());
        assert!((Grade::default().scale - 0.99).abs() < 1e-6);
        assert!(!Grade::default().is_neutral());

        // Stored in real units, clamped to 0.7..1.6 (not the colour 0..1).
        let mut g = Grade::neutral();
        g.set(GradeKey::Scale, 1.3);
        assert!((g.get(GradeKey::Scale) - 1.3).abs() < 1e-6);
        assert!(!g.is_neutral(), "a non-1.0 scale breaks neutrality");
        g.set(GradeKey::Scale, 9.0);
        assert!((g.scale - 1.6).abs() < 1e-6, "clamps to the 1.6 ceiling");
        g.set(GradeKey::Scale, 0.0);
        assert!((g.scale - 0.7).abs() < 1e-6, "clamps to the 0.7 floor");
        assert_eq!(GradeKey::Scale.range(), (0.7, 1.6, 1.0));

        // A non-neutral scale survives a round-trip; an absent `scale` (legacy
        // state written before this channel existed) defaults to 1.0×.
        let c = ThemeChoice {
            grade: g,
            ..Default::default()
        };
        let back: ThemeChoice = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert!((back.grade.scale - 0.7).abs() < 1e-6);
        let legacy: ThemeChoice =
            toml::from_str("id = \"hacker\"\n[grade]\nbrightness = 0.9\n").unwrap();
        assert!(
            (legacy.grade.scale - Grade::default().scale).abs() < 1e-6,
            "absent scale = house default"
        );

        // A pristine pane inherits the outer text size live (the "Mother" rule);
        // a detached grade keeps its own.
        let mut outer = ThemeChoice::default();
        outer.grade.scale = 1.45;
        let pristine = PaneTheme::default();
        assert!((pristine.effective(&outer).grade.scale - 1.45).abs() < 1e-6);
        let mut own = Grade::default();
        own.set(GradeKey::Scale, 0.85);
        let detached = PaneTheme {
            grade: Some(own),
            inherit_grade: false,
            ..Default::default()
        };
        assert!((detached.effective(&outer).grade.scale - 0.85).abs() < 1e-6);
    }

    #[test]
    fn crawl_rides_the_grade_group_and_clamps() {
        // Off by default; angle/depth start at the shipped neutral look.
        assert!(!Grade::neutral().crawl);
        assert!(!Grade::default().crawl);
        assert!((Grade::neutral().crawl_angle - CRAWL_ANGLE_DEFAULT).abs() < 1e-6);
        assert!((Grade::neutral().crawl_depth - CRAWL_DEPTH_DEFAULT).abs() < 1e-6);
        // Crawl is NOT a paint grade, so it must not break the fast-path neutral
        // check (like warp/tracking) — an otherwise-neutral grade stays neutral.
        let mut g = Grade::neutral();
        g.crawl = true;
        assert!(
            g.is_neutral(),
            "crawl is excluded from the paint-neutral check"
        );
        // ...but it IS a divergence from the default, so the scope persists it.
        assert!(!g.is_default(), "crawl on diverges from the house default");

        // Angle/depth ride their own ranges and clamp.
        assert_eq!(
            GradeKey::CrawlAngle.range(),
            (CRAWL_ANGLE_MIN, CRAWL_ANGLE_MAX, CRAWL_ANGLE_DEFAULT)
        );
        g.set(GradeKey::CrawlAngle, 99.0);
        assert!((g.crawl_angle - CRAWL_ANGLE_MAX).abs() < 1e-6);
        g.set(GradeKey::CrawlDepth, 0.0);
        assert!((g.crawl_depth - CRAWL_DEPTH_MIN).abs() < 1e-6);

        // Round-trips through TOML; an absent crawl flag reads as off.
        let c = ThemeChoice {
            grade: g,
            ..Default::default()
        };
        let back: ThemeChoice = toml::from_str(&toml::to_string(&c).unwrap()).unwrap();
        assert!(back.grade.crawl);
        assert!((back.grade.crawl_angle - CRAWL_ANGLE_MAX).abs() < 1e-6);
        let legacy: ThemeChoice =
            toml::from_str("id = \"hacker\"\n[grade]\nbrightness = 0.9\n").unwrap();
        assert!(!legacy.grade.crawl, "absent crawl flag = off");

        // A pristine pane inherits the outer crawl live; a detached pane keeps its own.
        let mut outer = ThemeChoice::default();
        outer.grade.crawl = true;
        assert!(PaneTheme::default().effective(&outer).grade.crawl);
    }

    #[test]
    fn crawl_coeffs_map_angle_to_taper_and_pass_depth() {
        // Bigger angle ⇒ harder convergence (smaller top-edge width ratio).
        let (a_small, _) = crawl_coeffs(2.0, 2.5);
        let (a_big, d) = crawl_coeffs(30.0, 2.5);
        assert!(a_small > a_big, "more angle ⇒ narrower top edge");
        assert!(a_big > 0.2 - 1e-6 && a_small <= 1.0);
        assert!((d - 2.5).abs() < 1e-6, "depth passes straight through");
        // Out-of-range knobs clamp to the band.
        assert!((crawl_coeffs(0.0, 99.0).1 - CRAWL_DEPTH_MAX).abs() < 1e-6);
    }

    // A Rust mirror of the WGSL crawl inverse-map in `fs_crt` (crt_pass.wgsl):
    // given a screen-local point in a crawling tube and the (a, depth) coeffs,
    // return the content texel to sample, or None for the letterboxed starfield.
    // The shader is the runtime authority; this copy locks the math by test.
    fn crawl_sample(lx: f32, ly: f32, a: f32, depth: f32) -> Option<(f32, f32)> {
        let vb = 1.0 - ly; // 0 = bottom/near, 1 = top/far
        let tc = if (depth - 1.0).abs() < 1e-3 {
            vb
        } else {
            (depth.powf(vb) - 1.0) / (depth - 1.0)
        };
        let width = (1.0 - tc) + a * tc;
        let cx = 0.5 + (lx - 0.5) / width;
        if !(0.0..=1.0).contains(&cx) {
            None
        } else {
            Some((cx, 1.0 - tc))
        }
    }

    #[test]
    fn crawl_perspective_inverse_map_is_correct() {
        // Identity: no taper, no foreshortening ⇒ content == screen.
        for &(lx, ly) in &[(0.1f32, 0.2f32), (0.5, 0.5), (0.9, 0.8)] {
            let (cx, cy) = crawl_sample(lx, ly, 1.0, 1.0).unwrap();
            assert!((cx - lx).abs() < 1e-5 && (cy - ly).abs() < 1e-5);
        }

        let (a, depth) = crawl_coeffs(18.0, 3.0);

        // The bottom row (near) samples full width at the very bottom of content.
        let (cx_b, cy_b) = crawl_sample(0.5, 1.0, a, depth).unwrap();
        assert!((cx_b - 0.5).abs() < 1e-5 && (cy_b - 1.0).abs() < 1e-5);
        // Edges of the bottom row are still in-bounds (width == 1 there).
        assert!(crawl_sample(0.0, 1.0, a, depth).is_some());

        // Vertical foreshortening (depth > 1): screen-mid maps PAST content-mid,
        // i.e. rows bunch toward the far (top) edge.
        let (_, cy_mid) = crawl_sample(0.5, 0.5, a, depth).unwrap();
        assert!(
            cy_mid > 0.5,
            "depth>1 bunches rows toward the top, got {cy_mid}"
        );

        // Horizontal taper (a < 1): near the top the sides converge, so the
        // outer columns fall outside the trapezoid and letterbox to black.
        assert!(
            crawl_sample(0.02, 0.0, a, depth).is_none(),
            "top-left letterboxes"
        );
        assert!(
            crawl_sample(0.5, 0.0, a, depth).is_some(),
            "top-centre stays in"
        );

        // cy is monotonic in screen-y (no folding): higher on screen (smaller
        // ly) recedes further toward the content TOP (smaller cy).
        let (_, cy_up) = crawl_sample(0.5, 0.25, a, depth).unwrap();
        let (_, cy_down) = crawl_sample(0.5, 0.75, a, depth).unwrap();
        assert!(
            cy_up < cy_down,
            "screen-up ⇒ nearer the content top (smaller cy)"
        );
    }

    #[test]
    fn legacy_syntax_colour_mode_folds_to_mono() {
        // Old state files stored the retired `color = "syntax"` mode; it must
        // still load (folding onto the monochrome default) rather than erroring.
        let c: ThemeChoice = toml::from_str("id = \"hacker\"\ncolor = \"syntax\"\n").unwrap();
        assert_eq!(c.color, ColorMode::Monochrome);
        assert!(!c.syntax);
    }
}

/// Absolute path of the hot-reloaded "custom" theme file for THIS machine —
/// `$TD_THEME` if set, else `~/.config/terminal-delight/theme.toml`. Public so
/// the UI can show the user exactly where their editable theme lives and open it.
pub fn theme_path() -> PathBuf {
    if let Ok(p) = std::env::var("TD_THEME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/terminal-delight/theme.toml")
}

/// Open a path in the user's default handler (their editor, for a `.toml`).
/// Best-effort and detached — the spawned process outlives this call, and any
/// failure (no `xdg-open`, no handler) is swallowed so the UI never blocks.
/// Linux-only: terminal-delight targets the freedesktop desktop, so the opener
/// is `xdg-open` (macOS would want `open`, Windows `start`). The `cfg` lives in
/// the body — mirroring `apply_warp` — so the signature exists on every target.
pub fn open_in_default_app(path: &std::path::Path) {
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    #[cfg(not(target_os = "linux"))]
    let _ = path; // other platforms: no-op (see doc note)
}

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Load the theme, seed the user config on first run, start the hot-reload watcher.
pub fn init(cx: &mut App) {
    let path = theme_path();
    // first-run seed so "edit your theme" has a file to edit
    if std::env::var("TD_THEME").is_err() && !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let _ = fs::write(&path, DEFAULT_THEME_TOML);
    }
    let initial = fs::read_to_string(&path)
        .ok()
        .and_then(|s| parse(&s).ok())
        .unwrap_or_else(|| parse(DEFAULT_THEME_TOML).expect("embedded theme parses"));
    let custom = Arc::new(initial);
    let builtins = BUILTIN_THEMES
        .iter()
        .map(|(id, src)| {
            (
                (*id).to_string(),
                Arc::new(parse(src).expect("embedded theme parses")),
            )
        })
        .collect();
    cx.set_global(ThemeRegistry {
        builtins,
        custom: custom.clone(),
    });
    cx.set_global(OuterChoice(house_outer())); // warm cabinet; state restore may change it
    cx.set_global(ActiveTheme(custom));

    let mut last = mtime(&path);
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        let now = mtime(&path);
        if now != last {
            last = now;
            if let Ok(source) = fs::read_to_string(&path) {
                match parse(&source) {
                    Ok(theme) => {
                        // Warp is a global toggle now, independent of the theme —
                        // a hot-reload only restates colours/effects.
                        cx.update(|cx| {
                            // the user file is the "custom" registry slot; any
                            // scope pointing at it re-resolves on repaint
                            cx.global_mut::<ThemeRegistry>().custom = Arc::new(theme);
                            bump_theme_gen(cx); // invalidate per-pane theme memos
                            let outer = outer_choice(cx);
                            if outer.id == "custom" {
                                let th = resolve(cx, &outer);
                                cx.set_global(ActiveTheme(th));
                            }
                            cx.refresh_windows();
                        });
                    }
                    Err(err) => eprintln!("theme reload error (keeping current): {err}"),
                }
            }
        }
    })
    .detach();
}
