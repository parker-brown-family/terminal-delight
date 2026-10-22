//! Looks that remember: each DESIGN keeps the changes made while wearing it.
//!
//! Parker, trying Clean on a window he had tuned: *"ok so we fiddle all the
//! dials and get things just so... but I want to try out clean... click---
//! config bulldozed... how do I get it back!"* A survey of twelve apps
//! (`reports/2026-10-22-looks-that-remember.html`) found his instinct is also
//! the best-attested convention: a preset remembers the changes made while it
//! was worn — Zed's per-theme overrides, VS Code's `[Theme]` blocks, Obsidian's
//! Style Settings, Omarchy's own theme overlay, Figma's extended collections.
//!
//! The rules, as he concurred them on 2026-10-22:
//!
//! 1. **Each design remembers your changes.** Trying another design never
//!    writes to it; picking it again brings it back exactly as you left it.
//! 2. **Some settings are yours everywhere** and never move with a design:
//!    sizes, the picture grade, the palette, program colour, syntax, note
//!    paper. Only the LOOK dials below are remembered per design — plus, for
//!    the window, the skin pin and the UI CONFIG switches. Any look dial can be
//!    PINNED, which moves it to "yours everywhere".
//! 3. **A design you never touched looks as designed**: the house look with
//!    the design file's own `[look]` table over it (Hacker's tube is on).
//! 4. **Getting back**: pick it. A dot marks a tuned design; a reset is per
//!    design and per dial, and the last switch or reset can be undone.
//!
//! A change is stored only where it differs from the design's stock value, so
//! a dial nobody moved keeps following the design when the design changes.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::theme::{Grade, GradeKey};
use crate::uiconfig::UiChoice;

/// Two dials closer than this are the same setting. Stored values round-trip
/// through f32 text and slider fractions, so exact equality would invent
/// changes nobody made.
const NEAR: f32 = 1e-4;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() <= NEAR
}

macro_rules! look_dials {
    ($( $field:ident : $ty:ty => $key:literal, $kind:ident; )*) => {
        /// The look dials of a grade — what a design remembers. Every field is
        /// optional: in a STOCK look `None` is "the design has no opinion"; in
        /// a set of CHANGES `None` is "not changed", which is a different
        /// thing from "changed back to the stock number".
        #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
        pub struct LookDials {
            $( #[serde(default, skip_serializing_if = "Option::is_none")]
               pub $field: Option<$ty>, )*
        }

        impl LookDials {
            /// Every look dial's key, as a set file, a pin and a test name it.
            pub const KEYS: &'static [&'static str] = &[ $( $key, )* ];

            /// What a grade holds right now.
            pub fn of(g: &Grade) -> Self {
                Self { $( $field: look_dials!(@read $kind, g.$field), )* }
            }

            /// Where `self` differs from `stock`: the changes, and nothing else.
            pub fn changes(&self, stock: &Self) -> Self {
                Self { $( $field: match (&self.$field, &stock.$field) {
                    (Some(a), Some(b)) if look_dials!(@same $kind, a, b) => None,
                    (Some(a), _) => Some(*a),
                    (None, _) => None,
                }, )* }
            }

            /// `self` laid over `base`: a value here wins.
            pub fn over(&self, base: &Self) -> Self {
                Self { $( $field: self.$field.or(base.$field), )* }
            }

            /// Write a resolved look into a grade, leaving `keep` untouched —
            /// the pinned dials, which are yours in every design.
            pub fn apply(&self, g: &mut Grade, keep: &BTreeSet<String>) {
                $( if !keep.contains($key) {
                    look_dials!(@write $kind, g.$field, self.$field);
                } )*
            }

            /// How many dials this holds a value for.
            pub fn count(&self) -> usize {
                0 $( + usize::from(self.$field.is_some()) )*
            }

            pub fn is_empty(&self) -> bool {
                self.count() == 0
            }

            /// Whether this holds a value for one dial.
            pub fn has(&self, key: &str) -> bool {
                match key { $( $key => self.$field.is_some(), )* _ => false }
            }

            /// Forget one dial.
            pub fn clear(&mut self, key: &str) {
                match key { $( $key => self.$field = None, )* _ => {} }
            }

            /// Forget every pinned dial: a pin is "yours everywhere", so no
            /// design may remember it.
            pub fn without(mut self, pinned: &BTreeSet<String>) -> Self {
                for k in pinned {
                    self.clear(k);
                }
                self
            }
        }
    };
    // `plain` grade fields always hold a value; `opt` ones may be unset.
    (@read plain, $v:expr) => { Some($v) };
    (@read opt, $v:expr) => { $v };
    (@write plain, $dst:expr, $v:expr) => { if let Some(v) = $v { $dst = v; } };
    (@write opt, $dst:expr, $v:expr) => { $dst = $v };
    (@same $kind:ident, $a:expr, $b:expr) => { Same::same($a, $b) };
}

trait Same {
    fn same(&self, other: &Self) -> bool;
}
impl Same for f32 {
    fn same(&self, other: &Self) -> bool {
        near(*self, *other)
    }
}
impl Same for bool {
    fn same(&self, other: &Self) -> bool {
        self == other
    }
}
impl Same for [f32; 3] {
    fn same(&self, other: &Self) -> bool {
        self.iter().zip(other).all(|(a, b)| near(*a, *b))
    }
}

look_dials! {
    crt: bool => "crt", plain;
    warp: f32 => "warp", plain;
    glare: f32 => "glare", opt;
    tracking: [f32; 3] => "roll", opt;
    crawl: bool => "crawl", plain;
    crawl_angle: f32 => "crawl_angle", plain;
    crawl_depth: f32 => "crawl_depth", plain;
    phosphor: f32 => "phosphor", plain;
    image_glow: f32 => "image_glow", opt;
    inactive_image_glow: f32 => "inactive_image_glow", opt;
    inactive_text: f32 => "inactive_text", opt;
    afterglow_ms: f32 => "afterglow_ms", opt;
    cursor_linger_ms: f32 => "cursor_linger_ms", opt;
    off_cursor_linger_ms: f32 => "off_cursor_linger_ms", opt;
    inactive_cursor_linger_ms: f32 => "inactive_cursor_linger_ms", opt;
    inactive_off_cursor_linger_ms: f32 => "inactive_off_cursor_linger_ms", opt;
}

/// The look dial a GAUGES slider drives, if it drives one. The rest — sizes
/// and the picture grade — are yours in every design.
pub fn look_key(k: GradeKey) -> Option<&'static str> {
    Some(match k {
        GradeKey::Warp => "warp",
        GradeKey::Glare => "glare",
        GradeKey::ImageGlow => "image_glow",
        GradeKey::InactiveImageGlow => "inactive_image_glow",
        GradeKey::InactiveText => "inactive_text",
        GradeKey::AfterglowMs => "afterglow_ms",
        GradeKey::CursorLingerMs => "cursor_linger_ms",
        GradeKey::OffCursorLingerMs => "off_cursor_linger_ms",
        GradeKey::InactiveCursorLingerMs => "inactive_cursor_linger_ms",
        GradeKey::InactiveOffCursorLingerMs => "inactive_off_cursor_linger_ms",
        GradeKey::CrawlAngle => "crawl_angle",
        GradeKey::CrawlDepth => "crawl_depth",
        GradeKey::Phosphor => "phosphor",
        _ => return None,
    })
}

/// What one design remembers of the changes made while wearing it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DesignMemory {
    #[serde(default, skip_serializing_if = "LookDials::is_empty")]
    pub look: LookDials,
    /// The skin pinned while wearing it. `None` follows the design.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin: Option<String>,
    /// UI CONFIG as it was left: the set picked and the switches flipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<UiChoice>,
}

impl DesignMemory {
    /// Nothing worth keeping: no dial changed, no skin pinned, UI as designed.
    pub fn is_empty(&self) -> bool {
        self.look.is_empty()
            && self.skin.is_none()
            && self.ui.as_ref().is_none_or(|u| *u == UiChoice::default())
    }

    /// How many things differ from the design as shipped — the count a reset
    /// names.
    pub fn count(&self) -> usize {
        self.look.count()
            + usize::from(self.skin.is_some())
            + self.ui.as_ref().map_or(0, |u| {
                u.outer.len() + u.panes.len() + usize::from(u.set.is_some())
            })
    }
}

/// Every design's memory, and the pins. Persisted with the window as `[looks]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Looks {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub designs: BTreeMap<String, DesignMemory>,
    /// Look dials pinned to every design.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub pinned: BTreeSet<String>,
}

impl Looks {
    pub fn is_empty(&self) -> bool {
        self.designs.is_empty() && self.pinned.is_empty()
    }

    /// Keep `memory` for `design`, or forget it when there is nothing in it.
    pub fn keep(&mut self, design: &str, memory: DesignMemory) {
        if memory.is_empty() {
            self.designs.remove(design);
        } else {
            self.designs.insert(design.to_string(), memory);
        }
    }
}

/// What a window is wearing, in the pieces a design switch moves.
#[derive(Clone, Debug, PartialEq)]
pub struct Wearing {
    pub grade: Grade,
    pub skin: Option<String>,
    pub ui: UiChoice,
}

/// Leave `from` for `to`: what `from` keeps, and what the window wears next.
///
/// `stock` gives a design's look as shipped. Pinned dials never move.
pub fn switch(
    looks: &mut Looks,
    from: &str,
    now: &Wearing,
    to: &str,
    stock: impl Fn(&str) -> LookDials,
) -> Wearing {
    let left = DesignMemory {
        look: LookDials::of(&now.grade)
            .changes(&stock(from))
            .without(&looks.pinned),
        skin: now.skin.clone(),
        ui: Some(now.ui.clone()),
    };
    looks.keep(from, left);
    let memory = looks.designs.get(to).cloned().unwrap_or_default();
    let mut grade = now.grade;
    memory
        .look
        .over(&stock(to))
        .apply(&mut grade, &looks.pinned);
    Wearing {
        grade,
        skin: memory.skin,
        ui: memory.ui.unwrap_or_default(),
    }
}

/// How many changes `design` carries right now: from the window itself when
/// it is the one being worn, else from its memory.
pub fn changes_in(
    looks: &Looks,
    design: &str,
    worn: Option<&Wearing>,
    stock: impl Fn(&str) -> LookDials,
) -> usize {
    match worn {
        Some(now) => DesignMemory {
            look: LookDials::of(&now.grade)
                .changes(&stock(design))
                .without(&looks.pinned),
            skin: now.skin.clone(),
            ui: Some(now.ui.clone()),
        }
        .count(),
        None => looks.designs.get(design).map_or(0, DesignMemory::count),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uiconfig::Key;

    fn stock(id: &str) -> LookDials {
        let house = LookDials::of(&crate::theme::house_outer().grade);
        let own = match id {
            "hacker" => LookDials {
                crt: Some(true),
                ..Default::default()
            },
            _ => LookDials::default(),
        };
        own.over(&house)
    }

    fn wearing(id: &str) -> Wearing {
        let mut grade = crate::theme::house_outer().grade;
        stock(id).apply(&mut grade, &BTreeSet::new());
        Wearing {
            grade,
            skin: None,
            ui: UiChoice::default(),
        }
    }

    /// Parker's case: Hacker tuned just so, Clean tried, Hacker back exactly.
    #[test]
    fn trying_another_design_never_writes_to_the_one_you_tuned() {
        let mut looks = Looks::default();
        let mut hacker = wearing("hacker");
        hacker.grade.warp = 1.37;
        hacker.grade.phosphor = 2.5;
        hacker.skin = Some("deco".into());
        hacker.ui.flip(Key::Counts, false);

        let clean = switch(&mut looks, "hacker", &hacker, "clean", stock);
        assert!(!clean.grade.crt, "Clean as designed: tube off");
        assert!(near(clean.grade.warp, stock("clean").warp.unwrap()));
        assert_eq!(clean.skin, None, "follows Clean's own trim");
        assert_eq!(clean.ui, UiChoice::default(), "Clean's UI as designed");

        let mut tweaked = clean.clone();
        tweaked.ui.flip(Key::AgentCounter, true);
        let back = switch(&mut looks, "clean", &tweaked, "hacker", stock);
        assert!(back.grade.crt);
        assert!(near(back.grade.warp, 1.37) && near(back.grade.phosphor, 2.5));
        assert_eq!(back.skin.as_deref(), Some("deco"));
        assert_eq!(back.ui.flipped(Key::Counts), Some(false));
        assert_eq!(
            back.ui.flipped(Key::AgentCounter),
            None,
            "Clean's flip stayed with Clean"
        );
        assert_eq!(
            looks.designs["clean"]
                .ui
                .as_ref()
                .unwrap()
                .flipped(Key::AgentCounter),
            Some(true)
        );
    }

    /// Only what differs from stock is kept, so an untouched dial keeps
    /// following the design — and an untouched design keeps no entry at all.
    #[test]
    fn only_changes_are_kept() {
        let mut looks = Looks::default();
        let plain = wearing("hacker");
        switch(&mut looks, "hacker", &plain, "clean", stock);
        assert!(looks.designs.is_empty(), "nothing changed, nothing kept");

        let mut one = wearing("hacker");
        one.grade.warp = 0.5;
        switch(&mut looks, "hacker", &one, "clean", stock);
        let kept = &looks.designs["hacker"].look;
        assert_eq!(kept.count(), 1, "{kept:?}");
        assert!(kept.has("warp"));
    }

    /// A pinned dial is yours everywhere: it rides across the switch and no
    /// design remembers it.
    #[test]
    fn a_pinned_dial_never_moves_with_a_design() {
        let mut looks = Looks::default();
        looks.pinned.insert("phosphor".into());
        let mut hacker = wearing("hacker");
        hacker.grade.phosphor = 3.3;
        let clean = switch(&mut looks, "hacker", &hacker, "clean", stock);
        assert!(near(clean.grade.phosphor, 3.3), "pinned: carried");
        assert!(looks.designs.is_empty(), "and not remembered by Hacker");
    }

    /// The count a reset names, from the window or from memory.
    #[test]
    fn a_design_says_how_many_changes_it_carries() {
        let mut looks = Looks::default();
        let mut hacker = wearing("hacker");
        hacker.grade.warp = 0.5;
        hacker.skin = Some("ops".into());
        assert_eq!(changes_in(&looks, "hacker", Some(&hacker), stock), 2);
        switch(&mut looks, "hacker", &hacker, "clean", stock);
        assert_eq!(changes_in(&looks, "hacker", None, stock), 2);
        assert_eq!(changes_in(&looks, "deco", None, stock), 0);
    }

    /// Every look dial a slider drives has a key the memory answers to, and
    /// no picture or size dial is mistaken for one.
    #[test]
    fn every_look_slider_maps_to_a_remembered_dial() {
        for k in [
            GradeKey::Warp,
            GradeKey::Glare,
            GradeKey::ImageGlow,
            GradeKey::Phosphor,
            GradeKey::CursorLingerMs,
            GradeKey::InactiveOffCursorLingerMs,
        ] {
            let key = look_key(k).unwrap();
            assert!(LookDials::KEYS.contains(&key), "{key}");
        }
        for k in [
            GradeKey::Brightness,
            GradeKey::Contrast,
            GradeKey::TextSize,
            GradeKey::Scale,
        ] {
            assert_eq!(look_key(k), None, "{k:?} is yours everywhere");
        }
    }

    /// The memory round-trips through the session file and writes only what
    /// was kept.
    #[test]
    fn the_memory_round_trips_and_writes_only_what_was_kept() {
        assert_eq!(toml::to_string(&Looks::default()).unwrap().trim(), "");
        let mut looks = Looks::default();
        let mut hacker = wearing("hacker");
        hacker.grade.warp = 0.5;
        switch(&mut looks, "hacker", &hacker, "clean", stock);
        looks.pinned.insert("phosphor".into());
        let wire = toml::to_string(&looks).unwrap();
        let back: Looks = toml::from_str(&wire).unwrap();
        assert_eq!(back, looks, "{wire}");
    }
}
