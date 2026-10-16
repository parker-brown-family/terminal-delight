//! Which colours a sticky note is drawn in — taken from the theme's OWN palette.
//!
//! The first version of this (PR 970) kept one hue from the theme and painted
//! every note at the same saturation and lightness, so a muted theme and a neon
//! one got the same highlighter-bright paper: praus, a theme of sage, linen and
//! bruised wine, got a yellow note. Parker named what each theme should give
//! instead — praus purple, green and grey; bright-future pink, purple and
//! green; osaka-jade jade, cream and pink; pripyat yellow, green and orange —
//! "all of these TUNED to the actual theme for hue shade etc." The rule here
//! gets all four, from the colours the theme author picked, at their own shade:
//!
//! * **contrast** — the theme's named hue farthest from its accent, weighted by
//!   how vivid it is beside the accent, so a faint hue cannot win on distance
//!   alone (pripyat's teal sits 79° from its yellow but carries a third of its
//!   colour; its terracotta, 70° away at full strength, wins).
//! * **accent** — the theme's accent. If the accent is a grey, or is only the
//!   text colour, the tint of the theme's own surface stands in for it, as the
//!   named hue nearest that tint (kanagawa's wave-blue surface → its blue).
//! * **mute** — the theme's text colour on a dark screen, its surface colour on
//!   a light one.
//!
//! Then every paper is the theme's EXACT colour unless the theme's own ink (its
//! background or its text, whichever reads better) cannot read on it at 4.5:1,
//! or it would vanish into the screen. Only then does it move: to its bright
//! twin, else to the nearest lightness that reads, keeping its hue and as much
//! of its chroma as fits. Parker concurred: "a note that is the theme's colour
//! is the point; a darker or lighter shade of it is the price of reading it."
//!
//! Worked out on 34 themes in `~/Work/reports/2026-10-16-sticky-notes-from-the-theme.html`
//! before a line of this existed; 87 of the 102 papers there are the theme's
//! exact hex. Perceptual arithmetic is OKLab/OKLCH throughout — HSL's
//! "lightness" makes a blue a third as bright as a yellow at the same number.

use gpui::{Hsla, Rgba};

use crate::theme::{NotePaper, Theme};

/// The ink must read on the paper at least this well (WCAG contrast ratio).
const INK_MIN: f32 = 4.5;
/// Below this OKLCH chroma a colour is a grey, not a hue.
const NEUTRAL_C: f32 = 0.03;
/// Two colours closer than this in OKLab are the same colour.
const SAME: f32 = 0.035;
/// A paper this close to the background (OKLab) vanishes into the screen…
const OFF_SCREEN: f32 = 0.15;
/// …except the mute note, which is meant to sit near the screen: its shadow
/// lifts it.
const MUTE_OFF: f32 = 0.06;
/// A grey standing in for a contrast note must be well clear of the screen.
const GREY_OFF: f32 = 0.3;

/// The named hues in TD's 16-colour table: red, green, yellow, blue, magenta,
/// cyan. Their bright twins sit eight places later.
const BASE: [usize; 6] = [1, 2, 3, 4, 5, 6];

/// An sRGB colour, channels `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl Rgb {
    pub fn of(c: Hsla) -> Self {
        let c = Rgba::from(c);
        Rgb {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }

    pub fn hsla(self, a: f32) -> Hsla {
        Rgba {
            r: self.r,
            g: self.g,
            b: self.b,
            a,
        }
        .into()
    }

    #[cfg(test)]
    pub fn hex(s: &str) -> Self {
        let s = s.trim_start_matches('#');
        let ch = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).unwrap() as f32 / 255.0;
        Rgb {
            r: ch(0),
            g: ch(2),
            b: ch(4),
        }
    }

    #[cfg(test)]
    pub fn to_hex(self) -> String {
        let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as u8;
        format!("#{:02x}{:02x}{:02x}", q(self.r), q(self.g), q(self.b))
    }
}

fn lin(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn unlin(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// OKLab `[L, a, b]`.
fn oklab(c: Rgb) -> [f32; 3] {
    let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
    let l = 0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b;
    let m = 0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b;
    let s = 0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b;
    let (l, m, s) = (l.cbrt(), m.cbrt(), s.cbrt());
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// OKLCH `(L, C, H°)`.
fn oklch(c: Rgb) -> (f32, f32, f32) {
    let [l, a, b] = oklab(c);
    (l, a.hypot(b), b.atan2(a).to_degrees().rem_euclid(360.0))
}

/// Back from OKLCH, or `None` when that colour is outside sRGB.
fn from_oklch(l: f32, c: f32, h: f32) -> Option<Rgb> {
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
    let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
    let bb = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
    let ok = |v: f32| (-1e-4..=1.0 + 1e-4).contains(&v);
    (ok(r) && ok(g) && ok(bb)).then(|| Rgb {
        r: unlin(r),
        g: unlin(g),
        b: unlin(bb),
    })
}

/// WCAG relative luminance.
fn lum(c: Rgb) -> f32 {
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio, `1..=21`.
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// Perceptual distance (OKLab).
fn de(a: Rgb, b: Rgb) -> f32 {
    let (p, q) = (oklab(a), oklab(b));
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
}

fn hue_dist(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// The colours a theme hands Terminal Delight, which is all the rule may use:
/// an Omarchy palette's `orange`, `brown` and `selection` never arrive, so a
/// rule that read them would draw notes no window could.
#[derive(Clone, Copy, Debug)]
pub struct ThemeColours {
    pub bg: Rgb,
    pub surface: Rgb,
    pub text: Rgb,
    pub accent: Rgb,
    pub faint: Rgb,
    pub ansi: [Rgb; 16],
}

impl ThemeColours {
    pub fn of(th: &Theme) -> Self {
        ThemeColours {
            bg: Rgb::of(th.bg),
            surface: Rgb::of(th.surface),
            text: Rgb::of(th.text),
            accent: Rgb::of(th.accent),
            faint: Rgb::of(th.faint),
            ansi: th.ansi.map(Rgb::of),
        }
    }

    /// A light screen. Measured rather than declared: TD's resolved theme
    /// carries no mode, and an inverted dark theme is a light one.
    fn light(&self) -> bool {
        oklch(self.bg).0 > 0.6
    }

    /// The theme's own inks: its background, its bright white, its text.
    fn inks(&self) -> [Rgb; 3] {
        [self.bg, self.ansi[15], self.text]
    }

    fn best_ink(&self, paper: Rgb) -> Rgb {
        self.inks()
            .into_iter()
            .max_by(|a, b| contrast(paper, *a).total_cmp(&contrast(paper, *b)))
            .unwrap_or(self.text)
    }

    fn readable(&self, paper: Rgb, off: f32) -> bool {
        contrast(paper, self.best_ink(paper)) >= INK_MIN && de(paper, self.bg) >= off
    }

    /// The named hues, base and bright, that have a hue at all, by ANSI index.
    fn hues(&self) -> impl Iterator<Item = usize> + '_ {
        BASE.into_iter()
            .chain(BASE.map(|i| i + 8))
            .filter(|&i| oklch(self.ansi[i]).1 >= NEUTRAL_C)
    }
}

/// How far a paper had to move from the theme's own colour. Kept because a
/// moved paper is a different claim from an exact one, and the tests (and
/// anyone reading a surprising note) need to tell the two apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Moved {
    /// The theme's exact colour.
    Exact,
    /// Its bright twin from the 16-colour table.
    Twin,
    /// Same hue, lightness moved from → to (OKLCH L).
    Lightness(f32, f32),
    /// Nothing near it reads; drawn as the theme had it anyway.
    Unreadable,
}

/// One note's paper and the theme ink it is written in.
#[derive(Clone, Copy, Debug)]
pub struct Sheet {
    pub paper: Rgb,
    pub ink: Rgb,
    /// Read by the tests, which pin "exact" for Parker's four themes; the
    /// renderer only needs the colour. Kept on the sheet rather than in a
    /// test-only twin so the claim tested is the one the window draws.
    #[cfg_attr(not(test), allow(dead_code))]
    pub moved: Moved,
}

/// The same hue and as much of the same chroma as fits, at lightness `l`.
fn at_lightness(c: Rgb, l: f32) -> Rgb {
    let (_, mut ch, h) = oklch(c);
    loop {
        if let Some(rgb) = from_oklch(l, ch, h) {
            return rgb;
        }
        ch -= 0.002;
        if ch <= 0.0 {
            return from_oklch(l, 0.0, h).unwrap_or(c);
        }
    }
}

/// The theme's exact colour when its own ink reads on it; else its bright twin;
/// else the same hue at the nearest lightness that reads. On a dark screen a
/// note is a LIGHT object, so lightening is tried all the way before darkening;
/// on a light screen the shorter move wins.
fn legible(t: &ThemeColours, c: Rgb, twin: Option<Rgb>, off: f32) -> (Rgb, Moved) {
    if t.readable(c, off) {
        return (c, Moved::Exact);
    }
    if let Some(tw) = twin.filter(|tw| *tw != c && t.readable(*tw, off)) {
        return (tw, Moved::Twin);
    }
    let l = oklch(c).0;
    let step = |i: usize| 0.005 * i as f32;
    let tries: Vec<f32> = if t.light() {
        (1..120).flat_map(|i| [l + step(i), l - step(i)]).collect()
    } else {
        (1..120)
            .map(|i| l + step(i))
            .chain((1..120).map(|i| l - step(i)))
            .collect()
    };
    for l2 in tries.into_iter().filter(|l2| (0.05..=0.98).contains(l2)) {
        let p = at_lightness(c, l2);
        if t.readable(p, off) {
            return (p, Moved::Lightness(l, l2));
        }
    }
    (c, Moved::Unreadable)
}

/// What the accent note is drawn from: the accent itself, or — when the accent
/// is a grey or just the text colour — the named hue nearest the tint of the
/// theme's own surface. `None` for a theme with no hue anywhere.
fn accent_source(t: &ThemeColours) -> Option<Rgb> {
    if oklch(t.accent).1 >= NEUTRAL_C && de(t.accent, t.text) > SAME {
        return Some(t.accent);
    }
    let hues: Vec<usize> = t.hues().collect();
    if hues.is_empty() {
        return None;
    }
    for surf in [t.surface, t.bg] {
        let (_, c, h) = oklch(surf);
        if c >= 0.02 {
            return hues
                .iter()
                .min_by(|&&a, &&b| {
                    hue_dist(oklch(t.ansi[a]).2, h).total_cmp(&hue_dist(oklch(t.ansi[b]).2, h))
                })
                .map(|&i| t.ansi[i]);
        }
    }
    hues.iter()
        .max_by(|&&a, &&b| oklch(t.ansi[a]).1.total_cmp(&oklch(t.ansi[b]).1))
        .map(|&i| t.ansi[i])
}

/// One of the theme's own greys, as far in lightness from the other two notes
/// as its palette allows — for a theme whose palette has no second hue.
fn grey_apart(t: &ThemeColours, taken: [Rgb; 2]) -> (Rgb, Moved) {
    let ls = taken.map(|c| oklch(c).0);
    let pool = [t.text, t.ansi[15], t.faint, t.surface, t.accent]
        .into_iter()
        .chain(BASE.map(|i| t.ansi[i]))
        .filter(|c| oklch(*c).1 < NEUTRAL_C + 0.02 && de(*c, t.bg) >= GREY_OFF);
    let far = |c: &Rgb| {
        let l = oklch(*c).0;
        ls.iter().map(|t| (l - t).abs()).fold(f32::MAX, f32::min)
    };
    match pool.max_by(|a, b| far(a).total_cmp(&far(b))) {
        Some(g) => legible(t, g, None, OFF_SCREEN),
        None => legible(t, t.text, None, OFF_SCREEN),
    }
}

/// The three papers, in tray order.
pub struct Papers {
    pub contrast: Sheet,
    pub accent: Sheet,
    pub mute: Sheet,
}

impl Papers {
    pub fn get(&self, mode: NotePaper) -> Sheet {
        match mode {
            NotePaper::Contrast => self.contrast,
            NotePaper::Accent => self.accent,
            NotePaper::Mute => self.mute,
        }
    }
}

pub fn papers(t: &ThemeColours) -> Papers {
    let sheet = |(paper, moved): (Rgb, Moved)| Sheet {
        paper,
        ink: t.best_ink(paper),
        moved,
    };
    let mute_src = if t.light() { t.surface } else { t.text };
    let mute = legible(t, mute_src, None, MUTE_OFF);
    let Some(acc_src) = accent_source(t) else {
        // No hue anywhere (vantablack, white): three of the theme's own greys.
        let accent = legible(t, t.accent, None, OFF_SCREEN);
        let contrast = grey_apart(t, [accent.0, mute.0]);
        return Papers {
            contrast: sheet(contrast),
            accent: sheet(accent),
            mute: sheet(mute),
        };
    };
    let accent = legible(t, acc_src, None, OFF_SCREEN);
    let (_, ca, ha) = oklch(acc_src);
    // Only the NAMED base hues compete: a bright twin is a lighter shade of its
    // base, used only when the base cannot be read — and some themes' "bright
    // magenta" is not magenta at all (osaka-jade's is a teal).
    let mut ranked: Vec<(f32, usize)> = BASE
        .into_iter()
        .filter(|&i| oklch(t.ansi[i]).1 >= NEUTRAL_C)
        .map(|i| {
            let (_, c, h) = oklch(t.ansi[i]);
            let vivid = (c / ca.max(1e-6)).min(1.0).sqrt();
            (hue_dist(h, ha) * vivid, i)
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let contrast = ranked
        .into_iter()
        .map(|(_, i)| legible(t, t.ansi[i], Some(t.ansi[i + 8]), OFF_SCREEN))
        .find(|(p, _)| de(*p, accent.0) > SAME * 2.0 && de(*p, mute.0) > SAME * 2.0)
        .unwrap_or_else(|| grey_apart(t, [accent.0, mute.0]));
    Papers {
        contrast: sheet(contrast),
        accent: sheet(accent),
        mute: sheet(mute),
    }
}

/// The pushpin's head: the theme's red, unless the paper is red-ish, then its
/// accent — a pin should stand out from its own sheet.
pub fn pin(t: &ThemeColours, paper: Rgb) -> Rgb {
    let red = t.ansi[1];
    if hue_dist(oklch(red).2, oklch(paper).2) < 40.0 {
        t.accent
    } else {
        red
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A theme exactly as a window gets it: Omarchy's `colors.toml` through the
    /// palette bridge (`palette::from_source`) onto the shipped base theme.
    fn omarchy(id: &str, colors_toml: &str) -> ThemeColours {
        let base = crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).unwrap();
        let p = crate::palette::from_source(id, colors_toml).expect("a palette");
        ThemeColours::of(&crate::theme::overlay_palette(&base, &p.toml).unwrap())
    }

    fn hexes(p: &Papers) -> [String; 3] {
        [p.contrast, p.accent, p.mute].map(|s| s.paper.to_hex())
    }

    const PRAUS: &str = r##"
mode = "dark"
accent = "#86917B"
muted = "#536049"
background = "#000000"
lighter_background = "#151915"
foreground = "#D4D0BF"
bright_foreground = "#EFE8D6"
red = "#B05F55"
orange = "#A8774B"
yellow = "#A18C60"
green = "#8A9963"
cyan = "#6F8D87"
blue = "#71829A"
magenta = "#937180"
bright_red = "#C47A6E"
bright_yellow = "#CDA369"
bright_green = "#AAB886"
bright_cyan = "#97B1AB"
bright_blue = "#98A7BA"
bright_magenta = "#B597A5"
"##;

    const BRIGHT_FUTURE: &str = r##"
mode = "dark"
accent = "#ff4fd1"
muted = "#414780"
background = "#131637"
foreground = "#d9cfe8"
bright_foreground = "#fbf0fb"
red = "#ef637e"
yellow = "#e8c77a"
orange = "#e99a70"
green = "#55cfa2"
cyan = "#38d2f5"
blue = "#738ee8"
magenta = "#ff4fd1"
bright_red = "#ff8aa4"
bright_yellow = "#f6dda2"
bright_green = "#7ae3bd"
bright_cyan = "#7deeff"
bright_blue = "#9badf5"
bright_magenta = "#ff8ae2"
"##;

    const OSAKA_JADE: &str = r##"
mode = "dark"
accent = "#509475"
muted = "#53685B"
background = "#111c18"
foreground = "#C1C497"
bright_foreground = "#F7E8B2"
red = "#FF5345"
yellow = "#459451"
orange = "#a2734b"
green = "#549e6a"
cyan = "#2DD5B7"
blue = "#509475"
magenta = "#D2689C"
bright_red = "#db9f9c"
bright_yellow = "#E5C736"
bright_green = "#63b07a"
bright_cyan = "#8CD3CB"
bright_blue = "#ACD4CF"
bright_magenta = "#75bbb3"
"##;

    const PRIPYAT: &str = r##"
mode = "dark"
accent = "#D6D85F"
foreground = "#D7DBC5"
background = "#111714"
muted = "#637268"
bright_foreground = "#F2F4E6"
red = "#C06F52"
green = "#94B86B"
yellow = "#D6D85F"
blue = "#6F9C98"
magenta = "#A6A17D"
cyan = "#83B8AD"
orange = "#C79558"
bright_red = "#E08A67"
bright_green = "#B8DA82"
bright_yellow = "#EFF078"
bright_blue = "#91C1BB"
bright_magenta = "#C8C39A"
bright_cyan = "#A5DDD1"
"##;

    /// Parker's four, in his words, from his own themes' files: every paper is
    /// the theme's exact colour, none re-brightened.
    #[test]
    fn the_four_themes_give_what_parker_named() {
        for (id, src, want, said) in [
            (
                "praus",
                PRAUS,
                ["#937180", "#86917b", "#d4d0bf"],
                "purple, green, grey",
            ),
            (
                "bright-future",
                BRIGHT_FUTURE,
                ["#55cfa2", "#ff4fd1", "#d9cfe8"],
                "green, pink, purple",
            ),
            (
                "osaka-jade",
                OSAKA_JADE,
                ["#d2689c", "#509475", "#c1c497"],
                "pink, jade, cream",
            ),
            (
                "pripyat",
                PRIPYAT,
                ["#c06f52", "#d6d85f", "#d7dbc5"],
                "orange, yellow, green",
            ),
        ] {
            let p = papers(&omarchy(id, src));
            assert_eq!(
                hexes(&p),
                want,
                "{id} should be {said} (contrast, accent, mute)"
            );
            for s in [p.contrast, p.accent, p.mute] {
                assert_eq!(s.moved, Moved::Exact, "{id}: the theme's own shade");
            }
        }
    }

    /// Pripyat is the case the vividness weight exists for: its teal is the
    /// farthest hue from its yellow, but faint beside it.
    #[test]
    fn a_faint_hue_cannot_win_contrast_on_distance_alone() {
        let t = omarchy("pripyat", PRIPYAT);
        let teal = t.ansi[4];
        let (_, ca, ha) = oklch(t.accent);
        let (_, ct, ht) = oklch(teal);
        assert!(
            hue_dist(ht, ha) > hue_dist(oklch(t.ansi[1]).2, ha),
            "the teal IS farther"
        );
        assert!(ct < ca * 0.4, "and it is faint beside the accent");
        assert_eq!(papers(&t).contrast.paper.to_hex(), "#c06f52");
    }

    /// The ink is the theme's own — praus writes in its black, bright-future in
    /// its indigo — and reads at 4.5:1 or better.
    #[test]
    fn the_ink_is_the_themes_own_and_reads() {
        let t = omarchy("bright-future", BRIGHT_FUTURE);
        let p = papers(&t);
        assert_eq!(p.accent.ink.to_hex(), "#131637", "indigo on hot pink");
        for s in [p.contrast, p.accent, p.mute] {
            assert!(contrast(s.paper, s.ink) >= INK_MIN);
            assert!(t.inks().contains(&s.ink));
        }
    }

    /// Kanagawa's accent IS its text colour, so the accent note takes the tint
    /// of its own wave-blue surface instead of duplicating the mute note.
    #[test]
    fn a_neutral_accent_gives_way_to_the_surface_tint() {
        let t = omarchy(
            "kanagawa",
            r##"
mode = "dark"
accent = "#dcd7ba"
background = "#1f1f28"
lighter_background = "#223249"
foreground = "#dcd7ba"
muted = "#54546D"
red = "#c34043"
yellow = "#c0a36e"
green = "#76946a"
cyan = "#6a9589"
blue = "#7e9cd8"
magenta = "#957fb8"
"##,
        );
        let p = papers(&t);
        assert_eq!(p.accent.paper.to_hex(), "#7e9cd8", "its crystal blue");
        assert!(de(p.accent.paper, p.mute.paper) > SAME * 2.0);
    }

    /// A theme with no hue anywhere still gets three notes, all its own greys,
    /// none lost against its black screen.
    #[test]
    fn a_colourless_theme_gets_three_of_its_own_greys() {
        let t = omarchy(
            "vantablack",
            r##"
mode = "dark"
accent = "#8d8d8d"
background = "#000000"
lighter_background = "#1a1a1a"
foreground = "#ffffff"
muted = "#7a7a7a"
red = "#a4a4a4"
yellow = "#cecece"
green = "#b6b6b6"
cyan = "#b0b0b0"
blue = "#8d8d8d"
magenta = "#9b9b9b"
"##,
        );
        let p = papers(&t);
        let s = [p.contrast, p.accent, p.mute];
        for (i, a) in s.iter().enumerate() {
            assert!(oklch(a.paper).1 < NEUTRAL_C, "grey");
            assert!(de(a.paper, t.bg) >= OFF_SCREEN, "clear of the screen");
            for b in &s[i + 1..] {
                assert!(de(a.paper, b.paper) > SAME, "three different greys");
            }
        }
    }

    /// On a light screen the mute note is its quiet surface, written in its
    /// dark text, and a mid-tone accent moves only as far as it must.
    #[test]
    fn a_light_theme_mutes_to_its_surface() {
        let t = omarchy(
            "catppuccin-latte",
            r##"
mode = "light"
accent = "#1e66f5"
background = "#eff1f5"
lighter_background = "#dce0e8"
foreground = "#4c4f69"
muted = "#acb0be"
red = "#d20f39"
yellow = "#df8e1d"
green = "#40a02b"
cyan = "#179299"
blue = "#1e66f5"
magenta = "#ea76cb"
"##,
        );
        assert!(t.light());
        let p = papers(&t);
        assert!(de(p.mute.paper, t.surface) < 0.03, "its surface");
        assert_eq!(p.mute.ink.to_hex(), "#4c4f69", "in its text colour");
        match p.accent.moved {
            Moved::Lightness(a, b) => assert!((a - b).abs() < 0.05, "a small move"),
            m => panic!("latte's blue cannot carry 4.5:1 ink as it is, got {m:?}"),
        }
    }

    /// Terminal Delight's own six designs: three distinct papers each, every one
    /// readable in the theme's own ink.
    #[test]
    fn every_shipped_design_gets_three_distinct_readable_notes() {
        for (id, src) in crate::theme::BUILTIN_THEMES {
            let th = crate::theme::parse(src).unwrap();
            let t = ThemeColours::of(&th);
            let p = papers(&t);
            let s = [p.contrast, p.accent, p.mute];
            for (i, a) in s.iter().enumerate() {
                assert!(contrast(a.paper, a.ink) >= INK_MIN, "{id}: unreadable note");
                assert_ne!(a.moved, Moved::Unreadable, "{id}");
                for b in &s[i + 1..] {
                    assert!(
                        de(a.paper, b.paper) > SAME,
                        "{id}: two notes the same colour"
                    );
                }
            }
        }
    }

    /// The colour maths round-trips, so "exact" really is the theme's hex.
    #[test]
    fn oklch_round_trips_srgb() {
        for h in [
            "#86917b", "#ff4fd1", "#000000", "#ffffff", "#c06f52", "#1e66f5",
        ] {
            let c = Rgb::hex(h);
            let (l, ch, hue) = oklch(c);
            assert_eq!(from_oklch(l, ch, hue).expect("in gamut").to_hex(), h);
        }
    }
}
