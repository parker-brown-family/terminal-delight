//! THEME GLYPHS — one small picture per Omarchy theme, for the theme tray.
//!
//! The tray is a column of glyphs with no captions, so each theme needs a face a
//! person can find without reading. The first proposal was colour emoji, and
//! Parker answered it on 2026-10-16: *"the COLOUR OF THE THEME should be DIRECTLY
//! evident! … liftoff and tokyo liftoff — both as rockets make sense, but the
//! COLOURS of the rockets is different."* An emoji brings its own colours and
//! cannot be re-inked.
//!
//! So a glyph is a SHAPE only. Each is a monochrome icon (Material Design Icons,
//! Apache 2.0, `assets/theme-glyphs/`) that gpui draws as a mask, inked in the
//! theme's own accent on the theme's own background — Liftoff and Tokyo Liftoff
//! are the same rocket in two different themes' colours. Two themes already use
//! a written character as their mark, and keep it: Copper Night's 夜 and Akane's
//! 茜, the madder red the theme is named for.
//!
//! The map is keyed by Omarchy's install name, the folder `omarchy theme install`
//! makes (the repo name less `omarchy-` and `-theme`), which is why tcballard's
//! two rockets are `theme-liftoff` and `theme-tokyo-liftoff`. It covers the 22
//! themes Omarchy ships, the 30 most-starred published themes on 2026-10-16, and
//! seven chosen on purpose. Any other theme draws its first letter in its accent,
//! so a theme published tomorrow still gets a tile; giving it a picture is a line
//! here.

use gpui::{div, px, AnyElement, Hsla, IntoElement, ParentElement, SharedString, Styled};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// What a theme's tile draws.
pub enum Mark {
    /// A monochrome icon, inked at draw time.
    Icon {
        name: &'static str,
        svg: &'static [u8],
    },
    /// A written character the theme already uses as its mark.
    Text(&'static str),
}

macro_rules! icon {
    ($name:literal) => {
        Mark::Icon {
            name: $name,
            svg: include_bytes!(concat!("../assets/theme-glyphs/", $name, ".svg")),
        }
    };
}

const MAP: &[(&str, Mark)] = &[
    // the 22 themes Omarchy ships
    ("catppuccin", icon!("cat")),
    ("catppuccin-latte", icon!("coffee")),
    ("ethereal", icon!("jellyfish")),
    ("everforest", icon!("pine-tree")),
    ("flexoki-light", icon!("script-text")),
    ("gruvbox", icon!("package-variant-closed")),
    ("hackerman", icon!("sunglasses")),
    ("kanagawa", icon!("waves")),
    ("last-horizon", icon!("weather-sunset")),
    ("lumon", icon!("office-building")),
    ("lupine", icon!("dog-side")),
    ("matte-black", icon!("heart")),
    ("miasma", icon!("mushroom")),
    ("nord", icon!("snowflake")),
    ("osaka-jade", icon!("temple-buddhist")),
    ("retro-82", icon!("cassette")),
    ("ristretto", icon!("coffee-maker")),
    ("rose-pine", icon!("flower-tulip")),
    ("solitude", icon!("candle")),
    ("tokyo-night", icon!("transmission-tower")),
    ("vantablack", icon!("square-rounded")),
    ("white", icon!("square-rounded-outline")),
    // the thirty most-starred published themes, by GitHub stars on 2026-10-16
    ("giants", icon!("book-open-page-variant")),
    ("aetheria", icon!("creation")),
    ("akane", Mark::Text("茜")),
    ("coppernight", Mark::Text("夜")),
    ("hermarchy", icon!("feather")),
    ("theme-liftoff", icon!("rocket-launch")),
    ("retropc", icon!("content-save")),
    ("aamis", icon!("umbrella-beach")),
    ("retro-fallout", icon!("radioactive")),
    ("ayaka", icon!("flower")),
    ("void", icon!("blur-radial")),
    ("forest-night", icon!("owl")),
    ("ibm", icon!("laptop")),
    ("felix", icon!("paw")),
    ("blackturq", icon!("diamond-stone")),
    ("one-dark-pro", icon!("atom")),
    ("mechanoonna", icon!("robot")),
    ("blackgold", icon!("crown")),
    ("city-783", icon!("city-variant")),
    ("ash", icon!("volcano")),
    ("last-call", icon!("glass-cocktail")),
    ("evergreen", icon!("leaf")),
    ("rose-pine-dark", icon!("flower-tulip-outline")),
    ("mars", icon!("alien")),
    ("rainynight", icon!("weather-pouring")),
    ("harbordark", icon!("anchor")),
    ("space-monkey", icon!("space-station")),
    ("midnight", icon!("moon-new")),
    ("spectra", icon!("looks")),
    ("batou", icon!("car-sports")),
    // outside the ranking, on purpose
    ("terminal-delight", icon!("television-classic")),
    ("praus", icon!("coach-lamp")),
    ("theme-tokyo-liftoff", icon!("rocket-launch")),
    ("bright-future", icon!("white-balance-sunny")),
    ("highlands", icon!("image-filter-hdr")),
    ("pripyat", icon!("ferris-wheel")),
    ("quattrocento", icon!("pillar")),
];

/// The mark for a theme, by its install name, if the map has one.
pub fn mark(id: &str) -> Option<&'static Mark> {
    MAP.iter().find(|(k, _)| *k == id).map(|(_, m)| m)
}

/// An icon's file, written once to the runtime directory: gpui draws an SVG from
/// a path ([`crate::art::runtime_asset`]). Remembered per icon, so a tray redrawn
/// every frame does not stat the file every frame.
fn icon_path(name: &'static str, svg: &'static [u8]) -> SharedString {
    static PATHS: OnceLock<Mutex<HashMap<&'static str, SharedString>>> = OnceLock::new();
    let mut paths = PATHS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    paths
        .entry(name)
        .or_insert_with(|| {
            crate::art::runtime_asset(&format!("terminal-delight-glyph-{name}.svg"), svg)
                .to_string_lossy()
                .into_owned()
                .into()
        })
        .clone()
}

/// A theme's glyph, inked `ink` and `size` px across: its icon, its written
/// mark, or — for a theme the map does not know — its first letter.
pub fn element(id: &str, ink: Hsla, size: f32) -> AnyElement {
    match mark(id) {
        Some(Mark::Icon { name, svg }) => gpui::svg()
            .external_path(icon_path(name, svg))
            .w(px(size))
            .h(px(size))
            .text_color(ink)
            .into_any_element(),
        Some(Mark::Text(s)) => div()
            .text_size(px(size * 0.9))
            .text_color(ink)
            .child(*s)
            .into_any_element(),
        None => div()
            .text_size(px(size * 0.85))
            .font_weight(gpui::FontWeight::BOLD)
            .text_color(ink)
            .child(
                id.chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default(),
            )
            .into_any_element(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_theme_appears_once() {
        let mut seen = std::collections::HashSet::new();
        for (id, _) in MAP {
            assert!(seen.insert(*id), "{id} is mapped twice");
        }
        assert_eq!(MAP.len(), 59, "22 stock + 30 most-starred + 7 chosen");
    }

    #[test]
    fn every_theme_td_carries_has_a_glyph() {
        // A machine with no Omarchy is offered exactly these, so none of them
        // may fall back to a letter.
        for id in crate::palette::shipped_ids() {
            assert!(mark(id).is_some(), "{id} ships with TD but has no glyph");
        }
    }

    #[test]
    fn every_icon_is_a_drawing_and_every_drawing_is_used() {
        let mut used = std::collections::HashSet::new();
        for (id, m) in MAP {
            if let Mark::Icon { name, svg } = m {
                let s = std::str::from_utf8(svg).expect("utf-8");
                assert!(
                    s.contains("<svg") && s.contains("<path"),
                    "{id}: {name}.svg is not a drawing"
                );
                used.insert(format!("{name}.svg"));
            }
        }
        // No orphan in the asset folder: an icon nobody draws is a licence
        // obligation carried for nothing.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/theme-glyphs");
        for e in std::fs::read_dir(dir).expect("asset dir").flatten() {
            let f = e.file_name().to_string_lossy().into_owned();
            if f.ends_with(".svg") {
                assert!(
                    used.contains(&f),
                    "{f} is in assets/theme-glyphs but unmapped"
                );
            }
        }
    }

    #[test]
    fn the_two_rockets_are_one_shape() {
        // Parker's own example: the colour, not the picture, tells them apart.
        let name = |id| match mark(id) {
            Some(Mark::Icon { name, .. }) => *name,
            _ => panic!("{id} should be an icon"),
        };
        assert_eq!(name("theme-liftoff"), name("theme-tokyo-liftoff"));
    }
}
