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

use gpui::{App, Global, Hsla, rgb};
use serde::Deserialize;

pub const DEFAULT_THEME_TOML: &str = include_str!("../themes/hacker.toml");

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
    flicker: Option<f32>,
    jiggle: Option<f32>,
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
    colors: FileColors,
    #[serde(default)]
    effects: FileEffects,
    #[serde(default)]
    font: FileFont,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub bg: Hsla,
    pub surface: Hsla,
    pub text: Hsla,
    pub accent: Hsla,
    pub faint: Hsla,
    pub cursor: Hsla,
    pub ansi: [Hsla; 16],
    pub scanline_opacity: f32,
    pub scanline_step: f32,
    pub vignette: f32,
    pub glow: f32,
    pub bloom: f32,
    pub tracking: f32,
    pub tracking_period: f32,
    pub flicker: f32,
    pub jiggle: f32,
    pub font_family: String,
    pub font_size: f32,
    pub cell_h: f32,
}

pub struct ActiveTheme(pub Arc<Theme>);
impl Global for ActiveTheme {}

pub fn theme(cx: &App) -> Arc<Theme> {
    cx.global::<ActiveTheme>().0.clone()
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
        return Err(format!("colors.ansi must have 16 entries, got {}", c.ansi.len()));
    }
    let mut ansi = [Hsla::default(); 16];
    for (i, s) in c.ansi.iter().enumerate() {
        ansi[i] = need(s, &format!("ansi[{i}]"))?;
    }
    let accent = need(&c.accent, "accent")?;
    Ok(Theme {
        name: file.name.unwrap_or_else(|| "unnamed".into()),
        bg: need(&c.bg, "bg")?,
        surface: need(&c.surface, "surface")?,
        text: need(&c.text, "text")?,
        accent,
        faint: need(&c.faint, "faint")?,
        cursor: c.cursor.as_ref().and_then(|s| hex(s)).unwrap_or(accent),
        ansi,
        scanline_opacity: file.effects.scanline_opacity.unwrap_or(0.).clamp(0., 0.6),
        scanline_step: file.effects.scanline_step.unwrap_or(4.).max(2.),
        vignette: file.effects.vignette.unwrap_or(0.).clamp(0., 1.),
        glow: file.effects.glow.unwrap_or(0.).clamp(0., 1.),
        bloom: file.effects.bloom.unwrap_or(0.).clamp(0., 1.),
        tracking: file.effects.tracking.unwrap_or(0.).clamp(0., 1.),
        tracking_period: file.effects.tracking_period.unwrap_or(7.).clamp(2., 60.),
        flicker: file.effects.flicker.unwrap_or(0.).clamp(0., 1.),
        jiggle: file.effects.jiggle.unwrap_or(0.).clamp(0., 1.),
        font_family: file.font.family.unwrap_or_else(|| "JetBrains Mono".into()),
        font_size: file.font.size.unwrap_or(14.).clamp(8., 32.),
        cell_h: file.font.cell_height.unwrap_or(20.).clamp(10., 48.),
    })
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
    cx.set_global(ActiveTheme(Arc::new(initial)));

    let mut last = mtime(&path);
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            let now = mtime(&path);
            if now != last {
                last = now;
                if let Ok(source) = fs::read_to_string(&path) {
                    match parse(&source) {
                        Ok(theme) => {
                            let _ = cx.update(|cx| {
                                cx.set_global(ActiveTheme(Arc::new(theme)));
                                cx.refresh_windows();
                            });
                        }
                        Err(err) => eprintln!("theme reload error (keeping current): {err}"),
                    }
                }
            }
        }
    })
    .detach();
}
