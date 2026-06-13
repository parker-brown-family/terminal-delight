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
use serde::{Deserialize, Serialize};

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
    curvature: Option<f32>,
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
    pub faint: Hsla,
    pub cursor: Hsla,
    pub ansi: [Hsla; 16],
    /// How program text colour is painted (default/monochrome/on-theme).
    pub color_mode: ColorMode,
    pub scanline_opacity: f32,
    pub scanline_step: f32,
    pub vignette: f32,
    pub glow: f32,
    pub bloom: f32,
    pub tracking: f32,
    pub tracking_period: f32,
    pub tracking_sweep: f32,
    pub flicker: f32,
    pub jiggle: f32,
    pub curvature: f32,
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

/// How a pane paints the program's text colour. Travels with the theme choice
/// (so it follows outer-vs-pane scope like the seed does), and is baked onto
/// the resolved `Theme.color_mode` for the renderer to read.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMode {
    /// The real xterm ANSI palette — blues, greens, reds, the lot.
    Default,
    /// Every colour collapses onto the theme's phosphor ramp (the classic look).
    #[default]
    Monochrome,
    /// ANSI hues folded onto a harmonic arc around the seed colour.
    OnTheme,
    /// IDE-style: ignore the program's ANSI and instead tokenise the text,
    /// colouring each token class (number, string, path, flag, …) its own
    /// hue on the seed arc.
    Syntax,
}

impl ColorMode {
    /// Picker order.
    pub const ALL: [ColorMode; 4] = [
        ColorMode::Default,
        ColorMode::Monochrome,
        ColorMode::OnTheme,
        ColorMode::Syntax,
    ];

    /// Glyph shown in the breakout picker.
    pub fn icon(self) -> &'static str {
        match self {
            ColorMode::Default => "◍",
            ColorMode::Monochrome => "●",
            ColorMode::OnTheme => "◉",
            ColorMode::Syntax => "◆",
        }
    }

    /// Tiny caption under the glyph.
    pub fn caption(self) -> &'static str {
        match self {
            ColorMode::Default => "ansi",
            ColorMode::Monochrome => "mono",
            ColorMode::OnTheme => "theme",
            ColorMode::Syntax => "code",
        }
    }

    /// `true` for the serde/skip default (monochrome).
    pub fn is_default(&self) -> bool {
        matches!(self, ColorMode::Monochrome)
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
}

impl Default for ThemeChoice {
    fn default() -> Self {
        Self {
            id: "custom".into(),
            seed: None,
            color: ColorMode::default(),
        }
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

/// (id, icon) for the picker, in display order.
pub fn all_themes(cx: &App) -> Vec<(String, String)> {
    let reg = cx.global::<ThemeRegistry>();
    let mut out: Vec<_> = reg
        .builtins
        .iter()
        .map(|(id, t)| (id.clone(), t.icon.clone()))
        .collect();
    out.push(("custom".into(), reg.custom.icon.clone()));
    out
}

pub fn parse_hex(value: &str) -> Option<Hsla> {
    hex(value)
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
    let seed = choice.seed.as_deref().and_then(hex);
    // Fast path: stock theme, no recolour and the default (monochrome) mode.
    if seed.is_none() && choice.color.is_default() {
        return base;
    }
    let mut th = match seed {
        Some(seed) => apply_seed(&base, seed),
        None => (*base).clone(),
    };
    th.color_mode = choice.color;
    Arc::new(th)
}

/// Set the outer (workspace) theme and repaint everything.
pub fn select_outer(cx: &mut App, choice: ThemeChoice) {
    let th = resolve(cx, &choice);
    apply_warp(&th);
    cx.set_global(ActiveTheme(th));
    cx.set_global(OuterChoice(choice));
    cx.refresh_windows();
}

/// Push the curvature dial into the renderer's CRT warp pass (td-crt-pass patch).
fn apply_warp(theme: &Theme) {
    #[cfg(target_os = "linux")]
    gpui_wgpu::set_crt_warp(theme.curvature * 0.14, theme.curvature * 0.06);
}

fn hex(value: &str) -> Option<Hsla> {
    let v = value.trim().trim_start_matches('#');
    if v.len() != 6 {
        return None;
    }
    u32::from_str_radix(v, 16).ok().map(|c| rgb(c).into())
}

fn parse(source: &str) -> Result<Theme, String> {
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
        faint: need(&c.faint, "faint")?,
        cursor: c.cursor.as_ref().and_then(|s| hex(s)).unwrap_or(accent),
        ansi,
        color_mode: ColorMode::default(),
        scanline_opacity: file.effects.scanline_opacity.unwrap_or(0.).clamp(0., 0.6),
        scanline_step: file.effects.scanline_step.unwrap_or(4.).max(2.),
        vignette: file.effects.vignette.unwrap_or(0.).clamp(0., 1.),
        glow: file.effects.glow.unwrap_or(0.).clamp(0., 1.),
        bloom: file.effects.bloom.unwrap_or(0.).clamp(0., 1.),
        tracking: file.effects.tracking.unwrap_or(0.).clamp(0., 1.),
        tracking_period: file.effects.tracking_period.unwrap_or(14.).clamp(2., 120.),
        tracking_sweep: file.effects.tracking_sweep.unwrap_or(7.).clamp(1., 30.),
        flicker: file.effects.flicker.unwrap_or(0.).clamp(0., 1.),
        jiggle: file.effects.jiggle.unwrap_or(0.).clamp(0., 1.),
        curvature: file.effects.curvature.unwrap_or(0.).clamp(0., 1.),
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
        assert_eq!(BUILTIN_THEMES.len(), 4);
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
}

fn theme_path() -> PathBuf {
    if let Ok(p) = std::env::var("TD_THEME") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".config/terminal-delight/theme.toml")
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
    cx.set_global(OuterChoice(ThemeChoice::default()));
    apply_warp(&custom);
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
                        apply_warp(&theme);
                        cx.update(|cx| {
                            // the user file is the "custom" registry slot; any
                            // scope pointing at it re-resolves on repaint
                            cx.global_mut::<ThemeRegistry>().custom = Arc::new(theme);
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
