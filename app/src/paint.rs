//! The PAINT overlay's shared vocabulary — one tile shape, one chord table, two
//! scopes that can be painted.
//!
//! `super+alt+P` used to raise an overlay over the WALL alone: every pane drew
//! its own card, a letter painted the focused one, and the cabinet around them —
//! the mother bar, the left bar, the status bar, the bezel — could not be
//! painted at all. The desktop palettes were reachable from nowhere else, so the
//! one surface that is always on screen was the one surface that could never
//! match the rest of the desktop.
//!
//! So the overlay now has two targets ([`crate::theme::Target`]): the wall, and
//! the OUTER, whose card hangs from the top edge of the window. Both draw the
//! same shelves, from this module, because two copies of a tile is how the two
//! surfaces would end up disagreeing about what a shelf contains.
//!
//! What lives here is everything that is the same on both sides: what a tile
//! PAINTS ([`Pick`]), what a scope is WEARING ([`Wearing`]), which tile is lit,
//! what a letter resolves to ([`chord`]), and the tile itself ([`tile`]). What
//! does not live here is the applying: a pane paints through its own
//! `appearance`, the outer through [`apply_outer`], and only the clearing rules
//! they share are pinned in one place ([`stamp`]).

use gpui::prelude::FluentBuilder;
use gpui::{
    div, px, AnyElement, App, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Styled,
};

use crate::fav::{self, Face, Fav};
use crate::skin::Skin;
use crate::theme::{self, Dynamic, Shelf, Theme, ThemeChoice, ThemeGroup};

/// What one tile paints.
///
/// THREE kinds, not four: the favourites shelf is a view over the other two
/// vocabularies rather than a third one of its own, so a starred palette
/// resolves to exactly the `Palette` pick the desktop shelf would have made.
#[derive(Clone, PartialEq, Debug)]
pub enum Pick {
    /// The `⟲ D` tile that leads every shelf: stop deciding. On a pane that is
    /// "follow the outer again"; on the outer it is the house look, because the
    /// cabinet has nothing above it to follow. One chord, two honest meanings —
    /// which is why each scope applies this one itself.
    Default,
    /// One of our own colour sets.
    Set(Dynamic),
    /// One of the desktop's palettes, by Omarchy's id.
    Palette(String),
}

impl Pick {
    /// A favourite, as the pick it really is.
    pub fn of_fav(f: &Fav) -> Pick {
        match f {
            Fav::Set(d) => Pick::Set(d.clone()),
            Fav::Palette(id) => Pick::Palette(id.clone()),
        }
    }
}

/// What a scope is WEARING — the cursor a letter-cycle walks from, the thing
/// `⇧F` stars, and the fact that decides which tile is lit.
///
/// `unpainted` is not "no colour": it is *nothing of its own*. A pane that
/// follows the outer and an outer sitting at the house look are both unpainted,
/// and on both the `⟲` tile is the lit one. Keeping it as its own field rather
/// than inferring it from an empty set/palette is the difference between "not
/// decided" and "decided on plain", which are not the same answer to a cycle.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct Wearing {
    pub unpainted: bool,
    /// The colour set in force, whether or not it is one of ours by name.
    pub set: Option<Dynamic>,
    /// The borrowed desktop palette, if one is painted over the set.
    pub palette: Option<String>,
}

impl Wearing {
    /// The NAMEABLE look actually worn.
    ///
    /// `None` when nothing is worn — and also when what is worn has no name to
    /// write down (a hand-seeded tint, a palette this desktop no longer has).
    /// Absent and unnameable both mean "there is no cursor here", which is the
    /// honest answer for a cycle and for a star alike.
    pub fn fav(&self) -> Option<Fav> {
        if self.unpainted {
            return None;
        }
        // A palette paints OVER a set ([`stamp`] clears the set when one lands),
        // so a scope wearing one is wearing that, whatever else the group holds.
        if let Some(id) = &self.palette {
            return Some(Fav::Palette(id.clone()));
        }
        let d = self.set.as_ref()?;
        Dynamic::NAMED
            .iter()
            .find(|n| n.same_kind(d))
            .cloned()
            .map(Fav::Set)
    }

    /// The desktop palette worn, if that is what is worn — the palette cycle's
    /// cursor, narrowed from [`Self::fav`] so the two read one fact.
    fn worn_palette(&self) -> Option<String> {
        match self.fav() {
            Some(Fav::Palette(id)) => Some(id),
            _ => None,
        }
    }

    /// Is `pick` the thing this scope is already wearing?
    fn lights(&self, pick: &Pick) -> bool {
        match pick {
            Pick::Default => self.unpainted,
            // A colour set is only "the one you're on" when no palette has since
            // painted over it — otherwise every set would read lit.
            Pick::Set(d) => {
                !self.unpainted
                    && self.palette.is_none()
                    && self.set.as_ref().is_some_and(|s| s.same_kind(d))
            }
            Pick::Palette(id) => !self.unpainted && self.palette.as_deref() == Some(id.as_str()),
        }
    }
}

/// What a PANE wears: its own look, or nothing at all while it still follows the
/// outer.
pub fn pane_wearing(inherit: bool, eff: &ThemeChoice) -> Wearing {
    if inherit {
        return Wearing {
            unpainted: true,
            ..Default::default()
        };
    }
    Wearing {
        unpainted: false,
        set: Some(eff.dynamic.clone()),
        palette: eff.palette.clone(),
    }
}

/// What the OUTER wears.
///
/// The cabinet has nothing above it to follow, so "unpainted" is the house look
/// itself — and it is compared as a WHOLE theme group, not by an empty palette
/// field, so a cabinet hand-tuned in the theme tray is never mistaken for the
/// shipped one and quietly reported as undecided.
pub fn outer_wearing(cx: &App) -> Wearing {
    let c = theme::outer_choice(cx);
    Wearing {
        unpainted: ThemeGroup::of(&c) == ThemeGroup::of(&theme::house_outer()),
        set: Some(c.dynamic.clone()),
        palette: c.palette.clone(),
    }
}

/// Everything one tile draws, owned. Owned for the same reason
/// [`crate::palette::chips`] is: the overlay interleaves these with
/// `cx.listener(…)` calls, which take the app context mutably, so it cannot hold
/// a borrow of the library across them.
#[derive(Clone)]
pub struct Entry {
    pub pick: Pick,
    pub face: Face,
    pub key: char,
    /// The name minus its first letter, and a second line for a hyphenated
    /// desktop name (`("ETRO", "82")` for `retro-82`).
    pub rest: String,
    pub second: String,
    pub swatch: Option<Hsla>,
    /// This is what the scope is wearing right now.
    pub lit: bool,
    /// Also on the favourites shelf — drawn only on the two full shelves, since
    /// on the shortlist itself every tile would carry one and the mark would
    /// stop meaning anything.
    pub star: bool,
}

/// Every tile on `shelf`, in shelf order, with the `⟲` tile leading.
///
/// `lead` is the rest of that first tile's word after its `D`: the wall says
/// DESKTOP (follow the outer again), the outer says DEFAULT (back to the house
/// cabinet). Same chord, same position, and each scope tells the truth about
/// what it does.
pub fn entries(cx: &App, shelf: Shelf, worn: &Wearing, lead: &str) -> Vec<Entry> {
    let mut out = vec![Entry {
        pick: Pick::Default,
        face: Face::Glyph("⟲"),
        key: 'D',
        rest: lead.to_string(),
        second: String::new(),
        swatch: None,
        lit: worn.unpainted,
        star: false,
    }];
    match shelf {
        // The shortlist: both vocabularies, in the order the file lists them,
        // with anything this desktop cannot draw already dropped by `fav::tiles`.
        Shelf::Favourites => {
            for t in fav::tiles(cx) {
                let pick = Pick::of_fav(&t.fav);
                out.push(Entry {
                    lit: worn.lights(&pick),
                    pick,
                    face: t.face,
                    key: t.letter,
                    rest: t.rest,
                    second: t.second,
                    swatch: t.swatch,
                    star: false,
                });
            }
        }
        Shelf::Sets => {
            for d in Dynamic::NAMED.iter() {
                let pick = Pick::Set(d.clone());
                out.push(Entry {
                    lit: worn.lights(&pick),
                    pick,
                    face: Face::Glyph(d.glyph()),
                    key: d.paint_letter(),
                    rest: d.label()[1..].to_uppercase(),
                    second: String::new(),
                    swatch: d.swatch(),
                    star: fav::contains(cx, &Fav::Set(d.clone())),
                });
            }
        }
        Shelf::Palettes => {
            for p in crate::palette::chips(cx) {
                let pick = Pick::Palette(p.id.clone());
                out.push(Entry {
                    lit: worn.lights(&pick),
                    pick,
                    face: Face::Screen {
                        bg: p.bg,
                        chips: p.chips,
                        light: p.light,
                    },
                    key: p.letter,
                    rest: p.rest,
                    second: p.second,
                    swatch: Some(p.chips[0]),
                    star: fav::contains(cx, &Fav::Palette(p.id)),
                });
            }
        }
    }
    out
}

/// One letter, resolved against the shelf on show and what the scope wears.
///
/// `d` is answered FIRST, from every shelf, because it is the one entry that
/// belongs to no shelf's own alphabet. After that each shelf resolves its own
/// way: our sets from one table of unique letters, the other two by CYCLING
/// through the entries sharing a letter, which is what makes `catppuccin` and
/// `catppuccin-latte` both reachable from `c`.
pub fn chord(cx: &App, key: &str, shelf: Shelf, worn: &Wearing) -> Option<Pick> {
    if key.eq_ignore_ascii_case("d") {
        return Some(Pick::Default);
    }
    let one = {
        let mut ch = key.chars();
        match (ch.next(), ch.next()) {
            (Some(c), None) => Some(c),
            _ => None, // "escape", "left", … are not chords
        }
    };
    match shelf {
        Shelf::Favourites => {
            fav::next_for_letter(cx, one?, worn.fav().as_ref()).map(|f| Pick::of_fav(&f))
        }
        Shelf::Sets => Dynamic::paint_chord(key).map(|d| match d {
            Some(d) => Pick::Set(d),
            None => Pick::Default,
        }),
        Shelf::Palettes => {
            crate::palette::next_for_letter(cx, one?, worn.worn_palette().as_deref())
                .map(Pick::Palette)
        }
    }
}

/// Stamp a pick onto a theme group — the ONE place the clearing rules live.
///
/// A pick is meant to be the whole statement, not a layer on a pile: the
/// seed/text/complement/human overrides the colour wheel writes would re-derive
/// colours ON TOP of the picked ones, which is exactly what "looks like the rest
/// of the desktop" must not do. The two shelves are also ONE choice — picking on
/// either clears the other — because a colour set works FROM the theme's own
/// colours and a palette still painted over them would silently win.
///
/// Identity is deliberately left alone: texture, CRT effects, grade and warp are
/// not touched. Paint is colour, not look.
///
/// [`Pick::Default`] is a no-op here and is applied by each scope itself: on a
/// pane it means "follow the outer", on the outer "back to the house", and no
/// single stamp can say both.
pub fn stamp(g: &mut ThemeGroup, pick: &Pick) {
    match pick {
        Pick::Default => return,
        Pick::Set(d) => {
            g.dynamic = d.clone();
            g.palette = None;
        }
        Pick::Palette(id) => {
            g.palette = Some(id.clone());
            g.dynamic = Dynamic::Plain;
        }
    }
    g.seed = None;
    g.text = None;
    g.complement = None;
    g.human = None;
}

/// Paint the OUTER — the cabinet the panes sit in.
///
/// The grade is carried across every pick, [`Pick::Default`] included: brightness,
/// warp, tracking and the two size dials are the window's LOOK, and a colour
/// gesture that silently resized the chrome would be a different feature wearing
/// this one's key.
pub fn apply_outer(cx: &mut App, pick: &Pick) {
    let cur = theme::outer_choice(cx);
    let next = match pick {
        Pick::Default => {
            let mut house = theme::house_outer();
            house.grade = cur.grade;
            house
        }
        _ => {
            let mut g = ThemeGroup::of(&cur);
            stamp(&mut g, pick);
            cur.with_group(g)
        }
    };
    theme::select_outer(cx, next);
}

/// Where a bare arrow lands while the overlay is up — pure, so the one gesture
/// that moves between the wall and the cabinet can be read without a window.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arrow {
    /// Walk the wall in that direction.
    Walk,
    /// Walk UP the wall, and if nothing is up there, aim the outer instead —
    /// the card hangs above the panes, so the gesture that reaches it is the one
    /// that runs out of wall.
    WalkThenOuter,
    /// Come back down to the wall.
    AimWall,
    /// Eaten. It must NOT fall through to the terminal: the pane is behind a
    /// modal and cannot be typed into.
    Nothing,
}

/// The arrow rule, given where the overlay is currently aimed.
pub fn arrow(target: theme::Target, key: &str) -> Arrow {
    match (target, key) {
        (theme::Target::Pane, "up") => Arrow::WalkThenOuter,
        (theme::Target::Pane, "down" | "left" | "right") => Arrow::Walk,
        (theme::Target::Outer, "down") => Arrow::AimWall,
        _ => Arrow::Nothing,
    }
}

/// The legend under a card: the contract that everything named here works, and
/// nothing that works is unnamed.
///
/// Built from what this desktop can actually do, so a machine with no Omarchy
/// and no shortlist is not promised shelves it hasn't got — and `⇧f` is offered
/// only when there is something to star, since naming a key that would do
/// nothing is exactly what this line promises never to do.
pub fn legend(cx: &App, walk: &[&str], lead_verb: &str, worn: &Wearing) -> String {
    let mut parts: Vec<&str> = walk.to_vec();
    parts.push("letter paints");
    let shelves = theme::shelves(cx);
    if shelves.len() > 1 {
        parts.push("z shelf");
    }
    if shelves.contains(&Shelf::Favourites) {
        parts.push("f favourites");
    }
    let worn_fav = worn.fav();
    if let Some(w) = &worn_fav {
        parts.push(if fav::contains(cx, w) {
            "⇧f unstar"
        } else {
            "⇧f star"
        });
    }
    parts.push(lead_verb);
    parts.push("esc done");
    parts.join(" · ")
}

/// What a scope has ON, as one word — for a header that reads label on the left,
/// live value on the right, the way the menu-bar popup says "menu bar … 85%".
///
/// THREE answers, and they are genuinely different. The name of the look; the
/// house default, when nothing has been painted; and CUSTOM, for a look with no
/// name to say — a hand-seeded tint, or a palette this desktop no longer has
/// installed. Folding that third case into "default" would report a cabinet
/// somebody hand-tuned in the theme tray as the one we shipped.
pub fn worn_label(w: &Wearing) -> String {
    if w.unpainted {
        return "DEFAULT".into();
    }
    match w.fav() {
        Some(Fav::Palette(id)) => id.to_uppercase(),
        Some(Fav::Set(d)) => d.label().to_uppercase(),
        None => "CUSTOM".into(),
    }
}

/// A tile's face: a colour set's glyph, or a palette's own screen in miniature.
///
/// Omarchy themes ship no emoji, and a name alone cannot tell gruvbox from
/// everforest — the miniature can.
pub fn face_el(face: &Face, th: &Theme, sk: &Skin) -> AnyElement {
    match face {
        Face::Glyph(g) => div()
            .text_size(px(17.))
            .child((*g).to_string())
            .into_any_element(),
        Face::Screen { bg, chips, light } => div()
            .relative()
            .w(px(30.))
            .h(px(18.))
            .rounded(sk.rad_raw(3.))
            .border_1()
            .border_color(th.text.alpha(0.25))
            .bg(*bg)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(3.))
            .children((*chips).map(|c| div().w(px(4.)).h(px(4.)).rounded(sk.radius_pill()).bg(c)))
            // A LIGHT scheme turns the whole surface into a bright screen —
            // worth knowing BEFORE the key is pressed, not after.
            .when(*light, |d| {
                d.child(
                    div()
                        .absolute()
                        .top(px(-1.))
                        .right(px(1.))
                        .text_size(px(7.))
                        .text_color(chips[0])
                        .child("☀"),
                )
            })
            .into_any_element(),
    }
}

/// The width one tile occupies, and the gap between two — what a caller needs to
/// decide how many will fit on a row before it has drawn any.
pub const TILE_W: f32 = 62.;
pub const TILE_GAP: f32 = 6.;

/// The tile grid's width on a card CENTRED over a pane: seven tiles, the width
/// the wall's card has always used.
const NARROW_W: f32 = 430.;
/// The widest the card's grid goes — EIGHT tiles.
///
/// It was sixteen for one build, and on a wide monitor that drew a banner across
/// the whole window: at that width the card stops being a panel floating over the
/// glass and becomes a stripe painted on it. Eight keeps the footprint in the same
/// family as the queue panel and the menu-bar popup, which are the two surfaces
/// this one is meant to read as a sibling of — a fifteen-entry shortlist is two
/// rows, the full desktop shelf three.
///
/// Unscaled, like the tiles it caps. Scaling the cap while the tiles it holds stay
/// fixed is how a row silently loses a tile at 85%.
const WIDE_W: f32 = 560.;
/// Window edge to card edge, so the shade never runs corner to corner.
const CARD_MARGIN: f32 = 48.;

/// How wide the hanging card's grid may be, in a window `win_w` across.
///
/// `None` — a window that has not reported its size yet — is UNKNOWN, not wide.
/// It falls back to the width the wall's own card uses, which fits any window
/// this program can open, rather than guessing wide and hanging tiles off the
/// edge for a frame on every cold start.
pub fn grid_w(win_w: Option<f32>) -> f32 {
    match win_w {
        Some(w) => (w - 2. * CARD_MARGIN).clamp(2. * (TILE_W + TILE_GAP), WIDE_W),
        None => NARROW_W,
    }
}

/// ONE tile shape serves every shelf and both scopes: a face, the chord letter
/// with the rest of the name beside it, an optional second name line, and the
/// colour the pick paints with.
///
/// The caller attaches the click — a pane paints itself, the outer card paints
/// the cabinet — which is the only thing the two surfaces do differently.
pub fn tile(e: &Entry, th: &Theme, sk: &Skin) -> Div {
    let (acc, surf, txt, faint) = (th.accent, th.surface, th.text, th.faint);
    let swatch = e.swatch;
    let lit = e.lit;
    div()
        .relative()
        .w(px(TILE_W))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(3.))
        .py(px(6.))
        .rounded(sk.rad_raw(8.))
        .border_1()
        .border_color(if lit { acc } else { acc.alpha(0.28) })
        .bg(if lit {
            acc.alpha(0.16)
        } else {
            surf.alpha(0.92)
        })
        .cursor_pointer()
        .hover(move |s| s.bg(acc.alpha(0.20)))
        .when(e.star, |d| {
            d.child(
                div()
                    .absolute()
                    .top(px(2.))
                    .right(px(4.))
                    .text_size(px(8.))
                    .text_color(acc)
                    .child("★"),
            )
        })
        // Face and name sit in FIXED-height boxes so every tile is the same
        // height whether its name takes one line or two — otherwise the rows
        // stagger and the grid reads as scrunched.
        .child(
            div()
                .h(px(21.))
                .flex()
                .items_center()
                .child(face_el(&e.face, th, sk)),
        )
        .child(
            div()
                .h(px(21.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .child(
                    // The name, with its chord worn loud — bigger, heavier,
                    // underlined, and inked in the TILE'S OWN colour, so the key
                    // you press also previews the colour it applies. Two children
                    // on a shared baseline rather than one styled string: the
                    // initial needs its own size, weight and underline, and gpui
                    // styles per element, not per run.
                    div()
                        .flex()
                        .flex_row()
                        .items_baseline()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::BLACK)
                                .text_color(swatch.unwrap_or(acc))
                                .underline()
                                .text_decoration_2()
                                .text_decoration_color(swatch.unwrap_or(acc))
                                .child(e.key.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(8.))
                                .text_color(if lit { txt.alpha(0.9) } else { faint })
                                .child(e.rest.clone()),
                        ),
                )
                // A desktop palette's name is the desktop's, not ours: it breaks
                // on the hyphen onto a second line rather than folding mid-word
                // (CATPPUCCIN / LATTE).
                .when(!e.second.is_empty(), |d| {
                    d.child(
                        div()
                            .text_size(px(8.))
                            .text_color(if lit { txt.alpha(0.9) } else { faint })
                            .child(e.second.clone()),
                    )
                }),
        )
        .child(
            div()
                .h(px(3.))
                .w(px(36.))
                .rounded(sk.rad_raw(2.))
                .bg(swatch.unwrap_or(acc.alpha(0.0))),
        )
}

/// One shelf pill — the visible half of `z`. The caller attaches the click.
///
/// [`Skin::chip`] rather than a hand-rolled pill: this is a small tag saying
/// which shelf is showing, which is the chrome's chip in every other surface,
/// and going through the verb is what makes a square skin square THESE corners
/// too. It also means the overlay's selected-thing follows the same emphasis
/// strategy as the rest of the app instead of being the one filled pill left
/// behind when that strategy changes.
pub fn pill(shelf: Shelf, on: bool, sk: &Skin) -> Div {
    sk.chip(on).cursor_pointer().child(shelf.label())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(d: Dynamic) -> Wearing {
        Wearing {
            unpainted: false,
            set: Some(d),
            palette: None,
        }
    }

    /// Nothing is lit but `⟲` while a scope wears nothing of its own — and that
    /// is a different state from wearing the plain set, which is what an
    /// inferred "unpainted" would have collapsed it into.
    #[test]
    fn an_unpainted_scope_lights_only_the_returning_tile() {
        let worn = Wearing {
            unpainted: true,
            set: Some(Dynamic::Plain),
            palette: Some("gruvbox".into()),
        };
        assert!(worn.lights(&Pick::Default));
        assert!(!worn.lights(&Pick::Palette("gruvbox".into())));
        assert!(!worn.lights(&Pick::Set(Dynamic::Plain)));
        assert_eq!(worn.fav(), None, "nothing worn is nothing to star");
    }

    /// A palette paints over a set, so the set beneath it must not read lit —
    /// otherwise two tiles claim to be what the scope is wearing.
    #[test]
    fn a_palette_hides_the_set_underneath_it() {
        let worn = Wearing {
            unpainted: false,
            set: Some(Dynamic::NAMED[0].clone()),
            palette: Some("nord".into()),
        };
        assert!(worn.lights(&Pick::Palette("nord".into())));
        assert!(!worn.lights(&Pick::Set(Dynamic::NAMED[0].clone())));
        assert_eq!(worn.fav(), Some(Fav::Palette("nord".into())));
    }

    /// A worn set lights its own tile and is the thing `⇧F` would star.
    #[test]
    fn a_worn_set_lights_its_own_tile() {
        let d = Dynamic::NAMED[0].clone();
        let worn = set(d.clone());
        assert!(worn.lights(&Pick::Set(d.clone())));
        assert!(!worn.lights(&Pick::Default));
        assert_eq!(worn.fav(), Some(Fav::Set(d)));
    }

    /// A pane that follows the outer wears nothing OF ITS OWN, whatever the
    /// outer it is following happens to be wearing.
    #[test]
    fn a_following_pane_wears_nothing_of_its_own() {
        let eff = ThemeChoice {
            palette: Some("tokyo-night".into()),
            ..Default::default()
        };
        assert!(pane_wearing(true, &eff).unpainted);
        assert_eq!(pane_wearing(true, &eff).fav(), None);
        assert_eq!(
            pane_wearing(false, &eff).fav(),
            Some(Fav::Palette("tokyo-night".into()))
        );
    }

    /// The clearing rules, in one place: a set clears a palette, a palette
    /// clears the set, and both clear the wheel's overrides — or a pick would be
    /// a layer on a pile instead of the whole statement.
    #[test]
    fn a_pick_is_the_whole_statement() {
        let mut g = ThemeGroup {
            palette: Some("nord".into()),
            seed: Some("#ff0000".into()),
            text: Some("#ffffff".into()),
            complement: Some("#00ff00".into()),
            human: Some("#0000ff".into()),
            ..Default::default()
        };
        stamp(&mut g, &Pick::Set(Dynamic::NAMED[0].clone()));
        assert_eq!(g.palette, None, "a set clears the palette over it");
        assert_eq!(g.seed, None);
        assert_eq!(g.text, None);
        assert_eq!(g.complement, None);
        assert_eq!(g.human, None);

        let mut g = ThemeGroup {
            dynamic: Dynamic::NAMED[0].clone(),
            seed: Some("#ff0000".into()),
            ..Default::default()
        };
        stamp(&mut g, &Pick::Palette("nord".into()));
        assert_eq!(g.palette.as_deref(), Some("nord"));
        assert!(
            matches!(g.dynamic, Dynamic::Plain),
            "a palette stands up clean, with no set re-deriving colours on top"
        );
        assert_eq!(g.seed, None);
    }

    /// `Default` is each scope's own business — stamping it must not half-apply
    /// something on the way past.
    #[test]
    fn the_returning_pick_stamps_nothing() {
        let before = ThemeGroup {
            dynamic: Dynamic::NAMED[0].clone(),
            palette: Some("nord".into()),
            seed: Some("#ff0000".into()),
            ..Default::default()
        };
        let mut g = before.clone();
        stamp(&mut g, &Pick::Default);
        assert!(
            g == before,
            "Default is applied by the scope, not the stamp"
        );
    }

    /// The header's live value tells a named look, the shipped default and a
    /// look with no name apart — three states, because the third one is exactly
    /// the hand-tuned cabinet that must not be reported as the shipped one.
    #[test]
    fn the_header_value_keeps_custom_apart_from_default() {
        let d = Dynamic::NAMED[0].clone();
        assert_eq!(
            worn_label(&Wearing {
                unpainted: true,
                ..Default::default()
            }),
            "DEFAULT"
        );
        assert_eq!(worn_label(&set(d.clone())), d.label().to_uppercase());
        assert_eq!(
            worn_label(&Wearing {
                unpainted: false,
                set: Some(Dynamic::Plain),
                palette: None,
            }),
            "CUSTOM",
            "a seeded tint with no set name is not the house default"
        );
    }

    /// A window that has not said how big it is gets the narrow card, never a
    /// wide one — the difference between "not measured" and "measured wide" is
    /// a row of tiles hanging off the edge of the screen.
    #[test]
    fn an_unmeasured_window_is_not_a_wide_one() {
        assert_eq!(grid_w(None), NARROW_W);
        assert_eq!(
            grid_w(Some(4000.)),
            WIDE_W,
            "a panel on a wide screen, not a banner"
        );
        assert_eq!(
            grid_w(Some(600.)),
            600. - 2. * CARD_MARGIN,
            "between the floor and the cap it is simply the window, less its margins"
        );
        assert!(
            grid_w(Some(200.)) >= 2. * (TILE_W + TILE_GAP),
            "a narrow window gets a narrow column, never a negative width"
        );
    }

    /// The one gesture that moves between the wall and the cabinet: up off the
    /// top of the wall aims the outer, down comes back, and nothing else moves
    /// the aim — a left/right press while aiming the outer must not silently
    /// drop back onto the wall.
    #[test]
    fn the_arrow_rule_only_moves_between_the_two_scopes_deliberately() {
        use theme::Target;
        assert_eq!(arrow(Target::Pane, "up"), Arrow::WalkThenOuter);
        assert_eq!(arrow(Target::Pane, "down"), Arrow::Walk);
        assert_eq!(arrow(Target::Pane, "left"), Arrow::Walk);
        assert_eq!(arrow(Target::Pane, "right"), Arrow::Walk);
        assert_eq!(arrow(Target::Outer, "down"), Arrow::AimWall);
        assert_eq!(arrow(Target::Outer, "up"), Arrow::Nothing);
        assert_eq!(arrow(Target::Outer, "left"), Arrow::Nothing);
        assert_eq!(arrow(Target::Outer, "right"), Arrow::Nothing);
    }
}
