//! FAVOURITES — the shortlist shelf in the paint overlay.
//!
//! The paint overlay speaks two vocabularies: Terminal Delight's own colour
//! sets ([`crate::theme::Dynamic::NAMED`]) and the desktop's palettes
//! ([`crate::palette`]). Both shelves are COMPLETE — every set, every theme the
//! desktop ships — which is exactly what makes them slow to paint from. Nobody
//! wears thirty-three looks; they wear eight, and hunt past the other
//! twenty-five every time.
//!
//! This is the third shelf: a hand-picked list that MIXES the two vocabularies,
//! because a person's shortlist does not respect where a colour came from.
//! `~/.config/terminal-delight/favourites.toml`, one ordered array of tagged
//! ids:
//!
//! ```toml
//! favourites = ["palette:retro-82", "set:army"]
//! ```
//!
//! The tag is not decoration. A desktop is free to ship a theme called `army`
//! beside our colour set of the same name, and an untagged list could not say
//! which one was starred. Order in the file is order on the shelf.
//!
//! A favourite naming something this desktop does not have — an Omarchy theme
//! you had on the old laptop — is UNRESOLVABLE, not absent. It is skipped when
//! the grid is drawn and KEPT in the file, so reinstalling the theme brings the
//! tile back instead of quietly having dropped it months ago.

use gpui::{App, Global, Hsla};
use std::path::{Path, PathBuf};

use crate::theme::Dynamic;

/// One entry on the favourites shelf: a colour set of ours, or one of the
/// desktop's palettes.
///
/// A set is held as the `Dynamic` itself rather than its label because the
/// label is display text that may be re-worded; the variant is what
/// [`crate::theme::ThemeChoice`] actually stores. Only [`Dynamic::NAMED`] sets
/// can be favourited — `Custom` carries a whole palette inline and has no
/// stable name to write down, and `Plain` is the absence of a set.
#[derive(Clone, PartialEq, Debug)]
pub enum Fav {
    Set(Dynamic),
    Palette(String),
}

impl Fav {
    /// The wire form written to the file — `set:army`, `palette:retro-82`.
    pub fn wire(&self) -> String {
        match self {
            Fav::Set(d) => format!("set:{}", d.label()),
            Fav::Palette(id) => format!("palette:{id}"),
        }
    }

    /// Read one wire entry back. Unknown tags and unknown set labels are `None`
    /// — a typo in a hand-edited file drops that line, it does not poison the
    /// whole list. A `palette:` id is taken on trust: the palette may simply not
    /// be installed on this desktop today, which is a different thing from being
    /// misspelled and is resolved later, in [`tiles`].
    pub fn parse(s: &str) -> Option<Fav> {
        let (tag, rest) = s.split_once(':')?;
        match tag.trim() {
            "set" => Dynamic::NAMED
                .iter()
                .find(|d| d.label().eq_ignore_ascii_case(rest.trim()))
                .cloned()
                .map(Fav::Set),
            "palette" if !rest.trim().is_empty() => Some(Fav::Palette(rest.trim().to_string())),
            _ => None,
        }
    }

    /// The keyboard chord — the first letter of the name, exactly as both other
    /// shelves derive theirs. Not unique here (a list may hold `cherry` and
    /// `catppuccin`), which is why [`next_for_letter`] cycles.
    pub fn letter(&self) -> char {
        let name = match self {
            Fav::Set(d) => d.label(),
            Fav::Palette(id) => id.as_str(),
        };
        name.chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
            .unwrap_or('?')
    }
}

/// The starter shortlist, written on first run.
///
/// A curated default rather than an empty shelf: an empty FAVOURITES shelf
/// teaches nothing about what the shelf is for, and the first `⇧F` overwrites
/// this anyway. Six desktop palettes and four colour sets, and — by luck rather
/// than by design — ten distinct first letters, so every one is a single press.
///
/// Most of the palette entries are stock Omarchy themes; `bright-future` is
/// not, and on a desktop without it that line simply does not draw a tile. That
/// is the unresolvable case working as intended, and it is here on purpose as
/// the shipped example of it.
const SEED: &[&str] = &[
    "palette:retro-82",
    "palette:matte-black",
    "palette:osaka-jade",
    "palette:hackerman",
    "palette:bright-future",
    "palette:ethereal",
    "set:army",
    "set:violet",
    "set:wood",
    "set:tide",
];

/// The favourites list as loaded, in shelf order. Entries may be unresolvable.
#[derive(Default)]
pub struct Favourites {
    pub items: Vec<Fav>,
}
impl Global for Favourites {}

pub fn config_path() -> PathBuf {
    crate::instance::config_dir().join("favourites.toml")
}

/// The list as it stands, including entries this desktop cannot draw. Callers
/// wanting only what is paintable want [`tiles`].
pub fn all(cx: &App) -> &[Fav] {
    cx.try_global::<Favourites>()
        .map(|f| f.items.as_slice())
        .unwrap_or(&[])
}

/// Is this look starred? Drives the ★ on the other two shelves' tiles.
pub fn contains(cx: &App, f: &Fav) -> bool {
    all(cx).contains(f)
}

/// Is there anything to SHOW on the favourites shelf right now?
///
/// Deliberately cheap and deliberately not `!all().is_empty()`: a list whose
/// every entry is unresolvable would otherwise offer a shelf that draws no
/// tiles, which is the empty-shelf bug [`crate::theme::shelves`] exists to
/// avoid on the palette side.
pub fn any_visible(cx: &App) -> bool {
    all(cx).iter().any(|f| resolvable(cx, f))
}

fn resolvable(cx: &App, f: &Fav) -> bool {
    match f {
        Fav::Set(_) => true,
        Fav::Palette(id) => crate::palette::find(cx, id).is_some(),
    }
}

/// The face a favourite's tile wears. The overlay builds the element; this only
/// says which of the two kinds of face it is, so the drawing code stays in
/// `pane.rs` beside the other shelves' tiles.
#[derive(Clone)]
pub enum Face {
    /// A colour set's glyph (🪖, 🌊, …).
    Glyph(&'static str),
    /// A palette's own screen in miniature, as the desktop shelf draws it.
    Screen {
        bg: Hsla,
        chips: [Hsla; 3],
        light: bool,
    },
}

/// Everything one favourites tile needs, owned — the same contract as
/// [`crate::palette::chips`], and for the same reason: the overlay interleaves
/// these with `cx.listener(…)` calls and cannot hold a borrow across them.
#[derive(Clone)]
pub struct Tile {
    pub fav: Fav,
    pub face: Face,
    pub letter: char,
    /// The name minus its first letter, and a second line for a hyphenated
    /// desktop name (`("ETRO", "82")` for `retro-82`).
    pub rest: String,
    pub second: String,
    pub swatch: Option<Hsla>,
}

/// Draw-ready favourites, in file order, with the unresolvable ones dropped.
pub fn tiles(cx: &App) -> Vec<Tile> {
    all(cx)
        .iter()
        .filter_map(|f| {
            let letter = f.letter();
            match f {
                Fav::Set(d) => Some(Tile {
                    fav: f.clone(),
                    face: Face::Glyph(d.glyph()),
                    letter,
                    rest: d.label()[1..].to_uppercase(),
                    second: String::new(),
                    swatch: d.swatch(),
                }),
                Fav::Palette(id) => crate::palette::find(cx, id).map(|p| Tile {
                    fav: f.clone(),
                    face: Face::Screen {
                        bg: p.bg,
                        chips: p.chips,
                        light: p.light,
                    },
                    letter,
                    rest: p.label.0.chars().skip(1).collect(),
                    second: p.label.1.clone(),
                    swatch: Some(p.chips[0]),
                }),
            }
        })
        .collect()
}

/// Resolve a letter on the favourites shelf to what it should paint — the
/// favourite AFTER whatever the pane is wearing, among those sharing that
/// letter.
///
/// The same cycle rule the desktop shelf uses, and here for the same reason:
/// the list is the user's, so nothing stops them starring `cherry` and
/// `catppuccin` together. Only RESOLVABLE favourites are in the cycle — a
/// letter must never walk onto a tile that isn't drawn.
pub fn next_for_letter(cx: &App, letter: char, current: Option<&Fav>) -> Option<Fav> {
    let group: Vec<&Fav> = all(cx)
        .iter()
        .filter(|f| f.letter().eq_ignore_ascii_case(&letter) && resolvable(cx, f))
        .collect();
    let at = current.and_then(|c| group.iter().position(|f| *f == c));
    let next = match at {
        Some(i) => (i + 1) % group.len(),
        None => 0,
    };
    group.get(next).map(|f| (*f).clone())
}

/// Star or unstar `f`, writing the file and republishing the list. Returns
/// whether it is a favourite AFTER the call, so the caller can say which way it
/// went.
///
/// Read-modify-write of the whole file rather than a write of the in-memory
/// list: a second window may have starred something since we loaded, and losing
/// their star to our stale copy is the one failure a shortlist cannot afford.
/// New stars append — the shelf is chronological, which is at least a reason,
/// where alphabetical would silently reorder a list the user arranged.
pub fn toggle(cx: &mut App, f: Fav) -> bool {
    let path = config_path();
    let mut items = read(&path).unwrap_or_else(|| all(cx).to_vec());
    let now_on = match items.iter().position(|x| *x == f) {
        Some(i) => {
            items.remove(i);
            false
        }
        None => {
            items.push(f);
            true
        }
    };
    write(&path, &items);
    publish(cx, items);
    now_on
}

/// Re-read the file and republish. Called when the overlay is RAISED, so a
/// hand-edit (or another window's star) is picked up without a restart — that
/// is the only moment the list is looked at, so it is the only moment worth
/// paying a stat for.
pub fn reload(cx: &mut App) {
    if let Some(items) = read(&config_path()) {
        if items != all(cx) {
            publish(cx, items);
        }
    }
}

fn publish(cx: &mut App, items: Vec<Fav>) {
    cx.set_global(Favourites { items });
    cx.refresh_windows();
}

/// The file's wire shape. A named table rather than a bare array because TOML
/// has no top-level array document, and because a future `favourites.toml` will
/// want a second key without a migration.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct File {
    #[serde(default)]
    favourites: Vec<String>,
}

/// Read the list, or `None` when the file is missing or unreadable — which the
/// callers treat differently: [`toggle`] falls back to the live list rather
/// than silently writing an empty file over a list it failed to read.
fn read(path: &Path) -> Option<Vec<Fav>> {
    let body = std::fs::read_to_string(path).ok()?;
    let file: File = toml::from_str(&body).ok()?;
    Some(
        file.favourites
            .iter()
            .filter_map(|s| Fav::parse(s))
            .collect(),
    )
}

fn write(path: &Path, items: &[Fav]) {
    let file = File {
        favourites: items.iter().map(|f| f.wire()).collect(),
    };
    if let Ok(body) = toml::to_string(&file) {
        let _ = crate::session::write_atomic(path, &body);
    }
}

/// Load the list, seeding the starter shortlist on first run. Must follow
/// `palette::init` only in spirit — nothing here resolves a palette — but it is
/// called beside it so the two shelves are stood up together.
pub fn init(cx: &mut App) {
    let path = config_path();
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        write(
            &path,
            &SEED
                .iter()
                .filter_map(|s| Fav::parse(s))
                .collect::<Vec<_>>(),
        );
    }
    let items = read(&path).unwrap_or_default();
    cx.set_global(Favourites { items });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_form_round_trips_both_vocabularies() {
        for f in [
            Fav::Set(Dynamic::Army),
            Fav::Set(Dynamic::Ocean),
            Fav::Palette("retro-82".into()),
        ] {
            assert_eq!(Fav::parse(&f.wire()), Some(f.clone()), "{}", f.wire());
        }
    }

    #[test]
    fn the_seed_parses_and_spells_ten_distinct_letters() {
        let seed: Vec<Fav> = SEED.iter().filter_map(|s| Fav::parse(s)).collect();
        assert_eq!(seed.len(), SEED.len(), "every seed line must parse");
        let mut letters: Vec<char> = seed.iter().map(|f| f.letter()).collect();
        letters.sort_unstable();
        letters.dedup();
        assert_eq!(
            letters.len(),
            seed.len(),
            "the shipped shortlist is one press per favourite; a collision would \
             make two of them share a cycle"
        );
    }

    /// The two letters the overlay owns as VERBS. A favourite spelled with one
    /// would keep its tile and lose its chord — survivable, but never something
    /// we should SHIP, so the seed is held to a stricter bar than a user's list.
    #[test]
    fn no_seeded_favourite_takes_a_shelf_verb() {
        for f in SEED.iter().filter_map(|s| Fav::parse(s)) {
            let l = f.letter();
            assert!(
                l != 'Z' && l != 'F' && l != 'D',
                "{} spells a verb ({l})",
                f.wire()
            );
        }
    }

    #[test]
    fn a_bad_line_drops_itself_and_not_the_list() {
        let file: File = toml::from_str(
            r#"favourites = ["set:army", "set:nosuchset", "banana", "palette:", "palette:gruvbox"]"#,
        )
        .unwrap();
        let got: Vec<Fav> = file
            .favourites
            .iter()
            .filter_map(|s| Fav::parse(s))
            .collect();
        assert_eq!(
            got,
            vec![Fav::Set(Dynamic::Army), Fav::Palette("gruvbox".into())]
        );
    }

    #[test]
    fn a_letter_cycles_the_favourites_sharing_it() {
        // `catppuccin` and `cherry` both spell C — the collision a user's own
        // list is free to contain, and the reason this shelf cycles like the
        // desktop one instead of assuming unique letters.
        let items = [
            Fav::Palette("catppuccin".into()),
            Fav::Set(Dynamic::Cherry),
            Fav::Set(Dynamic::Army),
        ];
        let group: Vec<&Fav> = items.iter().filter(|f| f.letter() == 'C').collect();
        assert_eq!(group.len(), 2);
        assert_eq!(
            group.iter().position(|f| **f == items[1]),
            Some(1),
            "cherry follows catppuccin, in file order"
        );
    }
}
