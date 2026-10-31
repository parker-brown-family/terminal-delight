//! The chrome's SHAPE, as data — the third themeable axis.
//!
//! Terminal Delight already separates two axes and hot-reloads both: a
//! `[colors]` PALETTE (what hue everything is) and `[effects]` TEXTURE (how much
//! CRT you get). Neither reaches the chrome. The chrome — bars, tabs, the left
//! bar, chips, rules, panels — computes its colours at the call site (268
//! `darken(th.surface, 0.3)` / `th.text.alpha(0.45)` expressions over six palette
//! roles) and compiles in all of its geometry (141 radius calls, 197 border
//! calls, 62 distinct `px()` literals). So a theme file can RETINT Terminal
//! Delight and cannot RESTYLE it.
//!
//! That gap is what a skin closes. A look like art deco is not a tint: it is
//! square corners where there were round ones, a twin rule where there was a
//! hairline, a bracket where there was a filled pill, tracked caps where there
//! was sentence case. None of that is reachable from a colour.
//!
//! Three things live here, in dependency order:
//!
//! 1. **Tokens** — [`Inks`] (semantic chrome colours), [`Metrics`] (geometry),
//!    [`Shapes`] (strategy enums). Data, resolved once per region.
//! 2. **A vocabulary** — [`Skin::panel`], [`Skin::chip`], [`Skin::rule_h`] and
//!    friends. Every call site asks for an ELEMENT; the strategy branch happens
//!    in here, once, never at the call site. That is what makes a second skin a
//!    file rather than a patch.
//! 3. **Files** — `app/skins/*.toml`, embedded as builtins and hot-reloaded from
//!    `$TD_SKIN` / `~/.config/terminal-delight/skin.toml`, exactly like themes.
//!
//! ## A skin declares RECIPES, not colours
//!
//! The point of `rule = { from = "surface", l = 0.30 }` rather than
//! `rule = "#1a2226"` is that the first one is still right under a palette the
//! skin author never saw. Five palettes times two skins is ten working looks,
//! not ten hand-authored files — and that, not the token list, is what "themeable"
//! has to mean to be worth the layer.
//!
//! ## Absent is not zero
//!
//! Every field a file can carry is an `Option`. A skin that says only
//! `corner = "square"` is a complete, valid skin: every ink it did not mention
//! resolves to its DEFAULT RECIPE over the live palette. A missing token is never
//! black, never `0.0`, and never silently the same as a declared one — see
//! `ink_tokens!` below, where the `None` arm reaches for the recipe rather than
//! for a default value.

use std::{
    collections::BTreeMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use gpui::{
    div, hsla, px, App, Div, Global, Hsla, InteractiveElement, ParentElement, Pixels, Stateful,
    Styled,
};
use serde::Deserialize;

use crate::theme::{self, Theme};
mod catalogue;
pub(crate) mod parts;
mod words;

/// Today's chrome, stated as tokens. Must stay pixel-faithful to what main.rs
/// draws — `default_skin_reproduces_todays_chrome` is the gate.
pub const DEFAULT_SKIN_TOML: &str = include_str!("../skins/default.toml");

/// (id, embedded toml) for every built-in skin, mirroring `theme::BUILTIN_THEMES`.
const BUILTIN_SKINS: &[(&str, &str)] = &[
    ("chalk", include_str!("../skins/chalk.toml")),
    ("default", DEFAULT_SKIN_TOML),
    ("deco", include_str!("../skins/deco.toml")),
    ("ops", include_str!("../skins/ops.toml")),
    ("clean", include_str!("../skins/clean.toml")),
    ("terminimalist", include_str!("../skins/terminimalist.toml")),
];

// ---------------------------------------------------------------------------
// Roles — the palette a recipe is allowed to draw from
// ---------------------------------------------------------------------------

/// A name a recipe can point at. Deliberately only the PALETTE's own roles plus
/// the two absolutes: a recipe that could point at another ink would let a skin
/// file build a cycle, and the resolver would have to become a graph walk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Bg,
    Surface,
    Text,
    Accent,
    Complement,
    Human,
    Faint,
    Cursor,
    /// Pure white — the ground of every "lift on hover" tint in the chrome.
    White,
    Black,
    /// The lighter of this palette's foreground and background, for stock.
    Light,
    /// One of the sixteen ANSI slots, so state inks (ok/warn/danger) track the
    /// palette's own idea of green/yellow/red instead of being invented here.
    Ansi(u8),
}

impl Role {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "bg" => Role::Bg,
            "surface" => Role::Surface,
            "text" => Role::Text,
            "accent" => Role::Accent,
            "complement" => Role::Complement,
            "human" => Role::Human,
            "faint" => Role::Faint,
            "cursor" => Role::Cursor,
            "white" => Role::White,
            "black" => Role::Black,
            "light" => Role::Light,
            other => {
                let n: u8 = other.strip_prefix("ansi")?.parse().ok()?;
                if n > 15 {
                    return None;
                }
                Role::Ansi(n)
            }
        })
    }

    /// Resolve the role against a theme. `pub(crate)` because the workbench
    /// borrows this vocabulary rather than inventing a second one — see
    /// [`crate::benchdraw::ink`].
    pub(crate) fn of(self, th: &Theme) -> Hsla {
        match self {
            Role::Bg => th.bg,
            Role::Surface => th.surface,
            Role::Text => th.text,
            Role::Accent => th.accent,
            Role::Complement => th.complement,
            Role::Human => th.human,
            Role::Faint => th.faint,
            Role::Cursor => th.cursor,
            Role::White => hsla(0., 0., 1., 1.),
            Role::Black => hsla(0., 0., 0., 1.),
            Role::Light => {
                if th.bg.l < th.text.l {
                    th.text
                } else {
                    th.bg
                }
            }
            Role::Ansi(n) => th.ansi[(n as usize).min(15)],
        }
    }
}

// ---------------------------------------------------------------------------
// Recipes — how an ink is built out of a role
// ---------------------------------------------------------------------------

/// A small, closed set of operations over one palette role. Closed on purpose:
/// every operation here already exists at a call site in main.rs (`darken` is
/// `l`, `brighten` is `l` with `max_l`, `.alpha(x)` is `a`, `mix(a, b, t)` is
/// `toward`/`t`), so the recipe language is a transcription of what the chrome
/// was doing by hand rather than a new thing to learn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Recipe {
    from: Role,
    /// Multiply lightness — `darken(c, 0.3)` is `l = 0.3`.
    l: Option<f32>,
    /// Multiply saturation.
    s: Option<f32>,
    /// SET alpha (not multiply) — `.alpha(0.45)` is `a = 0.45`.
    a: Option<f32>,
    /// Ceiling applied after `l`, which is the only thing `brighten` adds over
    /// `darken`: it refuses to run a colour up into white.
    max_l: Option<f32>,
    /// Blend toward a second role by `t`.
    toward: Option<Role>,
    t: f32,
}

impl Recipe {
    pub fn of(from: Role) -> Self {
        Self {
            from,
            l: None,
            s: None,
            a: None,
            max_l: None,
            toward: None,
            t: 0.,
        }
    }
    pub fn l(mut self, v: f32) -> Self {
        self.l = Some(v);
        self
    }
    pub fn s(mut self, v: f32) -> Self {
        self.s = Some(v);
        self
    }
    pub fn a(mut self, v: f32) -> Self {
        self.a = Some(v);
        self
    }
    pub fn max_l(mut self, v: f32) -> Self {
        self.max_l = Some(v);
        self
    }
    pub fn toward(mut self, r: Role, t: f32) -> Self {
        self.toward = Some(r);
        self.t = t;
        self
    }

    /// Apply the recipe to a live palette. Order matters and matches the order
    /// the same expressions run in at the call sites: lighten, then saturate,
    /// then blend, then alpha last — alpha is a property of the DRAWN thing, so
    /// blending a half-transparent colour toward an opaque one would otherwise
    /// quietly restore its opacity.
    pub fn bake(&self, th: &Theme) -> Hsla {
        let mut c = self.from.of(th);
        if let Some(l) = self.l {
            c.l = (c.l * l).clamp(0., 1.);
        }
        if let Some(cap) = self.max_l {
            c.l = c.l.min(cap);
        }
        if let Some(s) = self.s {
            c.s = (c.s * s).clamp(0., 1.);
        }
        if let Some(r) = self.toward {
            c = crate::mix(c, r.of(th), self.t);
        }
        if let Some(a) = self.a {
            c.a = a.clamp(0., 1.);
        }
        c
    }
}

/// What a skin file said about one ink: a fixed colour, or a recipe over the
/// palette. The two are different in the type because they are different claims
/// — "this look is gold whatever the palette is" versus "this look tracks
/// whatever accent it is given".
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InkSpec {
    Pinned(Hsla),
    Mixed(Recipe),
}

impl InkSpec {
    fn bake(&self, th: &Theme) -> Hsla {
        match self {
            InkSpec::Pinned(c) => *c,
            InkSpec::Mixed(r) => r.bake(th),
        }
    }
}

// ---------------------------------------------------------------------------
// The ink token set
// ---------------------------------------------------------------------------

/// Generates the three parallel shapes from ONE list, so adding a chrome colour
/// is a single line rather than four edits that can drift apart.
macro_rules! ink_tokens {
    ($( $field:ident => $key:literal, $default:expr, $doc:literal; )*) => {
        /// Every chrome colour, resolved against a live palette.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Inks { $( #[doc = $doc] pub $field: Hsla, )* }

        impl Inks {
            /// One resolved ink, by its file key. Chrome code reads the field
            /// directly; this is for the `skin` verb, which has to walk the whole
            /// set without naming any of it.
            pub fn by_key(&self, key: &str) -> Option<Hsla> {
                match key { $( $key => Some(self.$field), )* _ => None }
            }
        }

        /// What a skin FILE said. `None` means the file did not say — which
        /// resolves to the token's default recipe, never to a zero value.
        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct InkSpecs { $( pub $field: Option<InkSpec>, )* }

        impl InkSpecs {
            /// `false` when the key is not an ink — the caller reports it as a
            /// typo rather than dropping it, because a silently ignored token is
            /// exactly the failure mode theming systems die of.
            fn set(&mut self, key: &str, spec: InkSpec) -> bool {
                match key { $( $key => { self.$field = Some(spec); true } )* _ => false }
            }

            fn bake(&self, th: &Theme) -> Inks {
                Inks { $( $field: match &self.$field {
                    Some(spec) => spec.bake(th),
                    None => $default.bake(th),
                }, )* }
            }
        }

        /// The default recipe for one ink, by key — what a file gets when it
        /// stays silent. Reached only from the template-drift test today, which a
        /// binary crate's dead-code pass cannot see; it is also the lookup any
        /// future skin validator or editor needs, so it stays public.
        #[allow(dead_code)]
        pub fn default_ink(key: &str) -> Option<Recipe> {
            match key { $( $key => Some($default), )* _ => None }
        }

        pub const INK_KEYS: &[&str] = &[ $($key,)* ];
    };
}

ink_tokens! {
    // ---- grounds ----
    panel        => "panel",        Recipe::of(Role::Bg),                          "the ground a chrome region sits on";
    panel_raised => "panel_raised", Recipe::of(Role::Surface),                     "a surface lifted off the ground: a bar, a card";
    panel_sunken => "panel_sunken", Recipe::of(Role::Surface).l(0.40),             "a well: a track, an input field, an inset";
    panel_glass  => "panel_glass",  Recipe::of(Role::Bg).a(0.70),                  "the scrim an overlay lays over the workspace";
    btn_face     => "btn_face",     Recipe::of(Role::Surface).l(0.80),             "the face of a button at rest";

    // ---- boundaries ----
    rule         => "rule",         Recipe::of(Role::Faint).a(0.25),               "an ordinary divider inside a region";
    rule_strong  => "rule_strong",  Recipe::of(Role::Surface).l(0.30),             "the line between two regions — a panel's own edge";
    edge         => "edge",         Recipe::of(Role::Accent).a(0.35),              "the edge of something interactive";
    edge_strong  => "edge_strong",  Recipe::of(Role::Accent).a(0.50),              "the edge of a pane's own chrome — its header, its trays";
    focus        => "focus",        Recipe::of(Role::Accent),                      "the ring on the thing holding the keyboard";

    // ---- text ----
    ink          => "ink",          Recipe::of(Role::Text),                        "primary chrome text";
    ink_dim      => "ink_dim",      Recipe::of(Role::Text).a(0.70),                "secondary text: a subtitle, an inactive label";
    ink_off      => "ink_off",      Recipe::of(Role::Faint),                       "a label that is present but not currently in effect";
    // 0.45 → 0.60 on 2026-09-18, and the number is the floor's, not a taste.
    // Small text wants 4.5:1; at 0.45 the worst builtin palette (quiet-command)
    // gave 2.88:1 and hacker 3.55:1. 0.60 is the first rung on which ALL SIX
    // clear it, the worst of them at 4.54:1. It sits only 0.10 off `ink_dim`
    // and that gap is honest: on palettes this dark there is room for one
    // readable rung below the body text, not two.
    ink_faint    => "ink_faint",    Recipe::of(Role::Text).a(0.60),                "meta text: counts, ages, paths";
    ink_ghost    => "ink_ghost",    Recipe::of(Role::Text).a(0.20),                "text that is present but not for reading yet";
    ink_on_mark  => "ink_on_mark",  Recipe::of(Role::White).a(0.95),               "text drawn ON the accent";

    // ---- a control's two states ----
    // THE LIT LABEL IS NOT THE LIT COLOUR. Drawing the selected tab's word in
    // `select` put the glyphs, the border around them and the bloom outside it
    // all in one hue, and a shape whose figure and ground are the same colour
    // has no figure. Parker, on the bench's shelf tabs: *"The workbench right
    // bar tabs are unreadable... compare to model and effort which look fine"*
    // — and the model/effort strip he was pointing at draws its value in the
    // foreground at 0.92 and lets the EDGE carry the hue. These three are that
    // strip's own numbers, lifted into the token table so the strip and every
    // tab in the window cannot drift apart again.
    ink_lit      => "ink_lit",      Recipe::of(Role::Text).a(0.92),                "the label of the control you are on";
    edge_rest    => "edge_rest",    Recipe::of(Role::Text).a(0.30),                "the edge of a control that is not the lit one";
    face_rest    => "face_rest",    Recipe::of(Role::Text).a(0.06),                "the face of a control that is not the lit one";

    // ---- the accent, by job ----
    mark         => "mark",         Recipe::of(Role::Accent),                      "the accent at full strength: what is selected";
    mark_soft    => "mark_soft",    Recipe::of(Role::Accent).a(0.85),              "the accent carrying text";
    mark_dim     => "mark_dim",     Recipe::of(Role::Accent).a(0.40),              "the accent at rest";
    mark_wash    => "mark_wash",    Recipe::of(Role::Accent).a(0.14),              "the accent as a background tint";
    // SELECTION IS NOT THE ACCENT. The accent is the chrome's furniture colour —
    // rules, edges, the frame — so lighting the selected thing in it means the
    // one row you are standing on is painted the same colour as everything
    // around it. `human` is the palette's own answer to "you": the colour your
    // OWN input is drawn in inside an agent session, derived as the accent's
    // bright complement when a theme does not state it, and already dialable
    // live from the wheel's 👤 pip. So the selection colour is adjustable from a
    // control that exists, without editing a skin file.
    select       => "select",       Recipe::of(Role::Human),                       "the colour of the thing you have selected";
    // Was 0.22 — the fill under a selected row. Dropped to a seat rather than a
    // highlight on 2026-09-12: with the phosphor ring carrying the marking, a
    // wash that strong is what made a lit row read as a selected list item in a
    // web page. The focused pane, whose recipe the ring copies, carries none.
    row_active   => "row_active",   Recipe::of(Role::Human).a(0.08),               "the ground under the row you are standing on";
    hover        => "hover",        Recipe::of(Role::White).a(0.12),               "the lift under the pointer";

    // ---- states ----
    // The BRIGHT ansi slots, not the normal ones. A terminal palette very often
    // points its accent at one of its own normal colours — the field-command
    // theme we shipped until 0.4.0 had `#8fa85f` as both its accent and its
    // `ansi2` — so `ok = ansi2` would have painted "healthy" in exactly the
    // colour of the furniture. The brights are both further from the accent and better signals
    // against a dark ground, and
    // `no_state_ink_collapses_onto_the_accent_in_any_builtin_pairing` is what
    // keeps the next palette from reintroducing the collision.
    live         => "live",         Recipe::of(Role::Complement),                  "something is running right now";
    ok           => "ok",           Recipe::of(Role::Ansi(10)),                    "finished, healthy, within budget";
    warn         => "warn",         Recipe::of(Role::Ansi(11)),                    "close to a limit";
    danger       => "danger",       Recipe::of(Role::Ansi(9)),                     "failed, over, or about to be destroyed";
}

// ---------------------------------------------------------------------------
// The metric token set
// ---------------------------------------------------------------------------

macro_rules! metric_tokens {
    ($( $field:ident => $key:literal, $default:expr, $doc:literal; )*) => {
        /// Chrome geometry, in unscaled logical pixels. The window's scale is
        /// applied once, by [`Skin::px`], rather than at 688 call sites.
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Metrics { $( #[doc = $doc] pub $field: f32, )* }

        impl Metrics {
            /// One resolved metric, by its file key — see [`Inks::by_key`].
            pub fn by_key(&self, key: &str) -> Option<f32> {
                match key { $( $key => Some(self.$field), )* _ => None }
            }
        }

        #[derive(Clone, Debug, Default, PartialEq)]
        pub struct MetricSpecs { $( pub $field: Option<f32>, )* }

        impl MetricSpecs {
            fn set(&mut self, key: &str, v: f32) -> bool {
                match key { $( $key => { self.$field = Some(v); true } )* _ => false }
            }
            fn bake(&self) -> Metrics {
                Metrics { $( $field: self.$field.unwrap_or($default), )* }
            }
        }

        /// The default for one metric, by key — see [`default_ink`].
        #[allow(dead_code)]
        pub fn default_metric(key: &str) -> Option<f32> {
            match key { $( $key => Some($default), )* _ => None }
        }

        pub const METRIC_KEYS: &[&str] = &[ $($key,)* ];
    };
}

metric_tokens! {
    // gpui's own scale, which today's chrome spells as rounded_sm / _md / _lg.
    radius      => "radius",      4.0,   "corner radius of a small element (gpui rounded_sm)";
    radius_lg   => "radius_lg",   10.0,  "corner radius of a whole region — the left bar's own frame";
    // The largest corner anything draws, including the sites that spell their
    // radius by hand (the window's frame at 14, a menu at 8). Effectively no cap
    // by default; Clean's "not perfectly square, but close" is a cap of 3.
    corner_cap  => "corner_cap",  9999.0, "the largest corner radius any element draws";
    hairline    => "hairline",    1.0,   "the thinnest line the chrome draws";
    border      => "border",      1.0,   "the width of an element's border";
    rule_gap    => "rule_gap",    2.0,   "the space between the two lines of a double rule";
    gap         => "gap",         4.0,   "the space between two things in a row";
    pad_x       => "pad_x",       6.0,   "horizontal padding inside a region";
    pad_y       => "pad_y",       4.0,   "vertical padding inside a region";
    chip_px     => "chip_px",     5.0,   "horizontal padding inside a chip";
    chip_py     => "chip_py",     1.0,   "vertical padding inside a chip";
    row_h       => "row_h",       15.0,  "the height of a control on a bar";
    label_size  => "label_size",  9.0,   "the type size of a small caps label";
    bracket     => "bracket",     5.0,   "the arm length of a corner bracket";
    rail        => "rail",        2.0,   "the width of the rail marking an active row";
    // Taken down 25% from 7.0/0.55 on 2026-09-12, on the running build: the
    // first numbers were calibrated on a still window and read as haze once the
    // crisp spread-ring was added beside the halo. Two bright devices need less
    // of each than one did.
    //
    // …and then to 0.11 on 2026-09-18, which is the number this line is really
    // about. 0.41 was a MARKING alpha — right for the one thing on a surface
    // that is shouting, wrong for a strip where one of four tabs is always lit.
    // Paired with the crisp spread-ring it doubled the border inward over
    // 9-to-10 point glyphs and the lit tab came out as a coloured blob. The
    // spread-ring is gone (see [`Skin::ring`]) and what is left is one soft
    // halo, deliberately just past the model/effort strip's zero: Parker,
    // handing over that strip as the reference, *"dialing back the phosphor to
    // the level of the model and effort buttons but like +10% more phosphor"*.
    glow        => "glow",        5.25,  "how far the phosphor ring blooms past its border";
    glow_a      => "glow_a",      0.11,  "how hot the bloom is, 0..1 — NOT a length";
    // Read only under `corner = "cut"`; every other corner strategy ignores it.
    cut         => "cut",         7.0,   "how far a cut corner reaches along each edge";
    grain       => "grain",       1.0,   "material grain strength, 0..1";
    frost       => "frost",       0.0,   "Gaussian blur of the grid beneath smoked groupings, in logical pixels";
    roughness   => "roughness",   1.0,   "paper edge irregularity, 0..1";
    lift        => "lift",        1.0,   "paper contact shadow and stock depth, 0..2";
    marker_alpha => "marker_alpha", 0.22, "highlighter opacity, 0..1";
}

// ---------------------------------------------------------------------------
// The shape strategies — the part that makes a LOOK rather than a tint
// ---------------------------------------------------------------------------

/// How a corner is cut.
///
/// There is no `Chamfer`, and its absence is the one thing about this enum worth
/// knowing. The cut corner is the deco move a reader actually pictures, and gpui
/// cannot draw it: corner styling takes radii only, so a chamfer needs a rendered
/// path and therefore a `canvas` behind every element that wants one. `Square`
/// plus [`Skin::brackets`] gets most of the read for none of that risk.
///
/// **Parked and expected back** — issue #407 carries the cost estimate and the
/// criterion that would close it `invalid`. It is filed rather than remembered
/// because the person who parked it said he would forget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Corner {
    #[default]
    Round,
    /// Deco, and every "serious instrument" look: no radius anywhere. Emphasis
    /// has to come from the boundary instead, which is the whole point.
    Square,
    /// The chamfer issue #407 parked: square, with the top-left and
    /// bottom-right corners cut off by the `cut` metric. gpui has no property
    /// for it, so a region that wears it is drawn as a plate
    /// ([`crate::deck::plate`]) by [`Skin::panel`] and [`Skin::finish`];
    /// everything that only asks for a radius gets square.
    Cut,
}

/// How the edge of a region is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Boundary {
    #[default]
    Hairline,
    /// Two lines with a gap — deco's twin rule. Structural, not decorative: it
    /// reads as "this is a bounded thing" at a glance where one line reads as
    /// "these two things are adjacent".
    Double,
    /// One line, set in from the edge.
    Inset,
    None,
}

/// How "this one is active" is signalled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Emphasis {
    /// A tinted background. What the chrome did before the glow.
    #[default]
    Fill,
    /// A line under it.
    Underline,
    /// Four corner ticks. Marks without filling, so a dense list stays readable
    /// — in theory. **Shipped in deco on 2026-09-12 and withdrawn the same day**:
    /// at the size a tab label actually is, four disconnected ticks read as
    /// debris around the text rather than as a bracket around it. The device
    /// needs more room than a 16px row has. Kept as a strategy because it is
    /// correct at larger sizes, and because a look nobody is running costs one
    /// match arm.
    Bracket,
    /// A bar down the leading edge.
    Rail,
    /// A lit border that blooms — the phosphor ring.
    ///
    /// A full border in the accent plus an outer glow of the same colour, so the
    /// marked thing reads as *energised* rather than as *outlined*. This is the
    /// one device that survives being small: a continuous line is a single shape
    /// the eye resolves at any size, where four corner ticks are four shapes it
    /// has to assemble.
    ///
    /// It also compounds with what the terminal already does. TD's CRT pass has
    /// glow and bloom of its own, so a bright accent edge under a phosphor theme
    /// is lit twice — the skin draws the ring and the renderer blooms it. That is
    /// why this belongs in the DEFAULT skin and not only in deco: the retro look
    /// is the one it was always going to suit best.
    Glow,
    /// A solid bar the selected thing sits on — Fallout 4's Pip-Boy. A row's
    /// bar ends in an arrow tip ([`crate::deck::bar_tip`]) and its text turns
    /// to [`Inks::ink_on_mark`] ([`Skin::lit_label`]); a chip is a solid fill;
    /// a tab is cradled by the bracket that rises around Pip-Boy's active tab.
    /// The loudest marking in the vocabulary, so it suits a skin that keeps
    /// everything else quiet.
    ///
    /// The SLAB is the accent (`mark`), and that is not the collision the
    /// selection rule guards against: the rule exists because a ring or an
    /// edge in the accent vanishes into accent furniture, and a filled slab
    /// with dark text on it is a different shape from any hairline. The EDGE
    /// devices this strategy still draws — the tab's cradle, a slider's lit
    /// half — keep `select`, which stays apart from the accent.
    Bar,
}

/// The personality of a horizontal divider.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Divider {
    #[default]
    Line,
    Double,
    None,
}

/// What happens to a label's letters. Layered OVER what the call site asked for,
/// not instead of it: a site that already draws an uppercase label keeps drawing
/// one under `Off`, and the skin's only say is whether it is also tracked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Caps {
    /// Leave the string alone — the identity, and what today's chrome does.
    #[default]
    Off,
    /// Uppercase.
    Upper,
    /// Uppercase, with a thin space between letters. gpui has no letter-spacing,
    /// so tracking is done in the string — which is why it lives behind
    /// [`Skin::caps`] and not at a call site.
    Tracked,
}

/// Whether a surface is allowed to be lit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Shine {
    /// A soft vertical gradient on raised surfaces — what a pane header does
    /// today, and what makes the chrome read as a physical bar.
    #[default]
    Gradient,
    /// Flat. A gradient and a hard boundary are two different claims about the
    /// same edge: a look that draws its edges structurally has already said the
    /// surface is a plane, and lighting it then contradicts that. Every serious
    /// instrument panel is flat for this reason and not as an aesthetic.
    Flat,
}

/// How a measured share is drawn — an allowance, a budget, a level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Gauge {
    /// A continuous rail filled to the share — what every skin drew before OPS.
    #[default]
    Rail,
    /// A row of separate slanted pips, lit up to the share (XCOM's health bar).
    /// Counting replaces estimating: "nine of twelve" is read, "about
    /// three-quarters" is guessed. An unknown share draws hollow pips, never
    /// dark ones (see [`crate::deck::Pip`]).
    Pips,
}

/// How a NAME that heads something is framed — the notice on the top band,
/// the focused pane's title.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Title {
    #[default]
    Plain,
    /// Between chevrons, `» like this «` — XCOM 2's ability name. The chevrons
    /// take the `warn` ink, the palette's signal colour, so the frame is lit
    /// and the name stays the body colour.
    Chevron,
}

/// What marks the pane that has the keyboard, beyond its lit frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Focus {
    #[default]
    Plain,
    /// A hatched strip along the top edge of the focused pane's header — the
    /// dotted rails beside XCOM's ability name. Drawn above the header's
    /// content line, never under its text.
    Hatch,
}

/// The substrate of chrome, independent of palette and terminal effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Material {
    #[default]
    Plain,
    Paper,
    /// A faint square grid in the accent, washed under a region's content —
    /// the KELOWNA OPS page's 44px grid at the deck's scale. Under the text,
    /// never over it.
    Grid,
}

/// Terminal glass is a content policy, independent of the chrome substrate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tube {
    #[default]
    Glass,
    Stock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NoteDecoration {
    #[default]
    Plain,
    Flower,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Shapes {
    pub material: Material,
    pub tube: Tube,
    pub corner: Corner,
    pub boundary: Boundary,
    pub emphasis: Emphasis,
    pub divider: Divider,
    pub caps: Caps,
    pub shine: Shine,
    pub gauge: Gauge,
    pub title: Title,
    pub focus: Focus,
    pub controls: Controls,
}

/// Native controls can name their action instead of drawing an icon. Content
/// renderers never consult this policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Controls {
    #[default]
    Icons,
    Text,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShapeSpecs {
    pub material: Option<Material>,
    pub tube: Option<Tube>,
    pub corner: Option<Corner>,
    pub boundary: Option<Boundary>,
    pub emphasis: Option<Emphasis>,
    pub divider: Option<Divider>,
    pub caps: Option<Caps>,
    pub shine: Option<Shine>,
    pub gauge: Option<Gauge>,
    pub title: Option<Title>,
    pub focus: Option<Focus>,
    pub controls: Option<Controls>,
}

impl ShapeSpecs {
    fn bake(&self) -> Shapes {
        Shapes {
            material: self.material.unwrap_or_default(),
            tube: self
                .tube
                .unwrap_or(if self.material == Some(Material::Paper) {
                    Tube::Stock
                } else {
                    Tube::Glass
                }),
            corner: self.corner.unwrap_or_default(),
            boundary: self.boundary.unwrap_or_default(),
            emphasis: self.emphasis.unwrap_or_default(),
            divider: self.divider.unwrap_or_default(),
            caps: self.caps.unwrap_or_default(),
            shine: self.shine.unwrap_or_default(),
            gauge: self.gauge.unwrap_or_default(),
            title: self.title.unwrap_or_default(),
            focus: self.focus.unwrap_or_default(),
            controls: self.controls.unwrap_or_default(),
        }
    }
}

// ---------------------------------------------------------------------------
// SkinSpec — what a file says. Skin — what a region draws with.
// ---------------------------------------------------------------------------

/// A parsed skin file: every token still optional, nothing resolved. This is the
/// thing that is hot-reloaded and stored; it holds no palette, so one spec
/// serves every pane whatever theme each pane is wearing.
#[derive(Clone, Debug, PartialEq)]
pub struct SkinSpec {
    pub font_family: Option<String>,
    pub font_scale: Option<f32>,
    pub font_min_px: Option<f32>,
    pub font_line_height: Option<f32>,
    pub note_decoration: Option<NoteDecoration>,
    pub note_stock: Option<bool>,
    /// `[font] heading` — names: a project, a pane, a panel's title. Absent
    /// means the same face as `family`, which is every skin before OPS.
    pub font_heading: Option<String>,
    /// `[font] data` — numbers: a percentage, a counter, a zoom level.
    pub font_data: Option<String>,
    pub name: String,
    pub icon: String,
    pub ink: InkSpecs,
    pub metric: MetricSpecs,
    pub shape: ShapeSpecs,
    /// Keys the file carried that this build does not know. Kept rather than
    /// dropped: a misspelled token is the single most common way a theme "does
    /// nothing" for reasons nobody can see.
    pub unknown: Vec<String>,
}

impl Default for SkinSpec {
    fn default() -> Self {
        Self {
            font_family: None,
            font_scale: None,
            font_min_px: None,
            font_line_height: None,
            note_decoration: None,
            note_stock: None,
            font_heading: None,
            font_data: None,
            name: "default".into(),
            icon: "▢".into(),
            ink: InkSpecs::default(),
            metric: MetricSpecs::default(),
            shape: ShapeSpecs::default(),
            unknown: Vec::new(),
        }
    }
}

/// A skin resolved against one palette at one scale — what chrome code holds
/// while it builds elements. Cheap to make (about forty float ops); made once
/// per region per frame, never per element.
#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    /// UI only. Grid shaping continues to use Theme::font_family.
    pub font_family: String,
    pub type_scale: f32,
    pub type_min_px: f32,
    pub line_height: f32,
    pub note_decoration: NoteDecoration,
    pub note_stock: bool,
    /// The face for names, resolved: the file's `heading`, else `font_family`.
    /// Ask through [`Skin::heading`], which also sets its weight.
    pub heading_family: String,
    /// The face for numbers, resolved: the file's `data`, else `font_family`.
    pub data_family: String,
    pub name: String,
    pub icon: String,
    pub ink: Inks,
    pub m: Metrics,
    pub shape: Shapes,
    /// The window's UI scale, folded in here so no call site multiplies by `s`.
    pub scale: f32,
    /// The TYPE gauge, folded in for exactly the same reason `scale` is.
    ///
    /// Two different dials, and the difference matters: `scale` is the menu-bar
    /// slider, which sizes chrome — the outer bar, the tabs, a pane's header.
    /// This is `grade.text_size`, the slider that sizes what a person READS,
    /// and it is neutral on a freshly baked skin. Only a surface that is
    /// content rather than chrome asks for it, by taking a copy through
    /// [`Skin::with_type`]; the pane header, which is chrome, deliberately
    /// does not.
    pub ty: crate::workbench::Type,
    /// The phosphor gauge of the theme this was baked from — the scope's
    /// `grade.phosphor`. The metrics keep the skin's own bloom (`glow`,
    /// `glow_a`) as authored; this is the gain [`crate::theme::phosphor`]
    /// applies to it at draw time, so a control's ring moves with the frames
    /// around it on one dial.
    pub phosphor: f32,
}

impl SkinSpec {
    /// Open text furniture asks for the existing Clean UI set. A person's
    /// explicit set or individual switches still take precedence.
    pub fn ui_default(&self) -> Option<&'static str> {
        (self.shape.controls == Some(Controls::Text) && self.shape.boundary == Some(Boundary::None))
            .then_some(crate::uiconfig::CLEAN)
    }

    pub fn bake(&self, th: &Theme, scale: f32) -> Skin {
        let mut ink = self.ink.bake(th);
        if self.shape.bake().material == Material::Paper {
            let light = if th.bg.l < th.text.l { th.text } else { th.bg };
            if self.ink.btn_face.is_none() {
                ink.btn_face = crate::mix(th.surface, light, 0.12);
            }
            // Pigment on pale stock needs a darker value than chalk on slate.
            if th.bg.l > th.text.l && !matches!(self.ink.select, Some(InkSpec::Pinned(_))) {
                ink.select.l = ink.select.l.min(0.36);
            }
        }
        let font_family = self
            .font_family
            .clone()
            .unwrap_or_else(|| th.font_family.clone());
        Skin {
            note_decoration: self.note_decoration.unwrap_or(
                if self.shape.material == Some(Material::Paper) {
                    NoteDecoration::Flower
                } else {
                    NoteDecoration::Plain
                },
            ),
            note_stock: self
                .note_stock
                .unwrap_or(self.shape.material == Some(Material::Paper)),
            // Legacy Caveat files keep their optical fit. Material never selects
            // a font policy: a mono face on paper retains mono measurements.
            type_scale: self
                .font_scale
                .unwrap_or(if font_family == "Caveat" { 1.45 } else { 1. }),
            type_min_px: self
                .font_min_px
                .unwrap_or(if font_family == "Caveat" { 16. } else { 0. }),
            line_height: self.font_line_height.unwrap_or(1.15),
            heading_family: self
                .font_heading
                .clone()
                .unwrap_or_else(|| font_family.clone()),
            data_family: self
                .font_data
                .clone()
                .unwrap_or_else(|| font_family.clone()),
            font_family,
            name: self.name.clone(),
            icon: self.icon.clone(),
            ink,
            m: self.metric.bake(),
            shape: self.shape.bake(),
            scale,
            ty: crate::workbench::Type::neutral(),
            phosphor: th.grade.phosphor,
        }
    }
}

impl Skin {
    pub fn text_controls(&self) -> bool {
        self.shape.controls == Controls::Text
    }

    /// Open native furniture is an explicit combination; icon skins retain
    /// their existing borders even when their panel boundary is None.
    pub fn unboxed(&self) -> bool {
        self.text_controls() && self.shape.boundary == Boundary::None
    }

    pub fn box_border(&self, legacy: f32) -> Pixels {
        px(if self.unboxed() { 0. } else { legacy })
    }

    /// A small monochrome state mark. Its caller owns the meaning and colour.
    pub fn quiet_mark(&self, glyph: &'static str, ink: Hsla, scale: f32) -> Div {
        self.chrome(div())
            .flex_none()
            .w(px(12. * scale))
            .flex()
            .items_center()
            .justify_center()
            .text_size(self.text_px(11. * scale))
            .text_color(ink.alpha(0.85))
            .child(glyph)
    }

    /// Keep a popup sharp while gently diffusing the scene around it. The
    /// existing reader pass measures physical pixels and resets every frame.
    pub fn popup<E: Styled + ParentElement>(&self, d: E) -> E {
        if !self.unboxed() {
            return d;
        }
        d.child(
            div().absolute().inset_0().child(
                gpui::canvas(
                    |bounds, window, _| {
                        let sf = window.scale_factor();
                        crate::warp::set_focus_blur(
                            [
                                f32::from(bounds.origin.x) * sf,
                                f32::from(bounds.origin.y) * sf,
                                f32::from(bounds.size.width) * sf,
                                f32::from(bounds.size.height) * sf,
                            ],
                            7. * sf,
                            0.,
                            0.,
                            0.,
                        );
                    },
                    |_, _, _, _| {},
                )
                .size_full(),
            ),
        )
    }

    /// Only native labels pass here. Never use this on terminal cells, user
    /// titles, authored documents, code or workbench prose.
    pub fn control_text(&self, label: &str) -> String {
        if self.text_controls() {
            words::label(label)
        } else {
            label.to_string()
        }
    }

    pub fn control<'a>(&self, icon: &'a str, word: &'a str) -> &'a str {
        if self.text_controls() {
            word
        } else {
            icon
        }
    }

    /// Resolve the default terminal field at the presentation edge. This is
    /// separate from program-authored cell backgrounds and the legacy grade.
    pub fn terminal_theme(&self, mut th: Theme) -> Theme {
        th.field_background = th
            .grade
            .field_colour
            .or_else(|| self.unboxed().then_some(1.))
            .map(|level| crate::mix(gpui::black(), self.ink.panel, level.clamp(0., 1.)));
        th
    }

    /// Project the skin's native ink into legacy chrome. This value is local
    /// to a native renderer; never store it as a Design or pass it to a grid.
    pub fn chrome_theme(&self, th: &Theme) -> Theme {
        let mut chrome = th.clone();
        if self.text_controls() {
            chrome.bg = self.ink.panel;
            chrome.surface = self.ink.panel_raised;
            chrome.text = self.ink.ink;
            chrome.faint = self.ink.ink_faint;
            chrome.accent = self.ink.mark;
            if !self.unboxed() {
                chrome.complement = self.ink.ink;
            }
            chrome.glow = 0.;
            chrome.grade.phosphor = 0.;
        }
        chrome
    }

    pub fn glass(&self) -> bool {
        self.shape.tube == Tube::Glass
    }

    pub fn terminal_stock<E: Styled + ParentElement>(&self, d: E, face: Hsla) -> E {
        if self.glass() {
            d
        } else {
            d.bg(face).children(self.grain())
        }
    }

    pub fn note_palette(&self, th: &Theme) -> crate::sticky::Paper {
        let mut paper = crate::sticky::paper_for(th);
        if self.note_stock && th.note_paper == crate::theme::NotePaper::Contrast {
            paper.base = if self.text_controls() {
                self.ink.btn_face
            } else {
                crate::mix(self.ink.btn_face, th.accent, 0.07)
            };
            paper.ink = if self.text_controls() {
                self.ink.ink
            } else {
                th.text
            };
            paper.pin = th.ansi[1];
        }
        paper
    }

    pub fn marked_note_palette(&self, th: &Theme, marked: bool) -> crate::sticky::Paper {
        let mut paper = self.note_palette(th);
        if self.unboxed() {
            paper.pin = self.ink.edge;
            if marked {
                paper.base = Hsla {
                    h: th.complement.h,
                    ..crate::mix(self.ink.panel, th.complement, 0.20)
                };
                paper.ink = self.ink.ink;
                paper.pin = th.complement.alpha(0.6);
            }
        }
        paper
    }

    pub fn reading_header(&self, d: Div) -> Div {
        if self.type_scale > 1. {
            d.flex_col().items_start().gap(self.px(4.))
        } else {
            d
        }
    }
    fn stock(&self) -> crate::paper::Stock {
        crate::paper::Stock {
            grain: self.m.grain,
            roughness: self.m.roughness,
            lift: self.m.lift,
            marker_alpha: self.m.marker_alpha,
        }
    }

    /// Restore the UI type policy on a region that otherwise inherits content type.
    pub fn chrome<E: Styled>(&self, d: E) -> E {
        d.font_family(self.font_family.clone())
            .line_height(gpui::relative(self.line_height))
    }

    /// Fit compact chrome to the optical type policy; these are the two
    /// established geometry endpoints, independent of the substrate.
    pub fn fit(&self, normal: f32, large_type: f32) -> f32 {
        normal + (large_type - normal) * ((self.type_scale - 1.) / 0.45).clamp(0., 2.)
    }

    pub fn caption(&self, text: &str, size: f32, selected: bool) -> Div {
        if self.is_paper() {
            self.paper_caption(text, size, selected)
        } else {
            div()
                .text_size(self.text_px(size))
                .whitespace_nowrap()
                .child(text.to_owned())
        }
    }

    pub fn marker(&self, selected: bool) -> Option<Div> {
        (selected && self.is_paper())
            .then(|| crate::paper::selection_with(self.ink.select, self.stock()))
    }

    pub fn caption_ink(&self, legacy: Hsla) -> Hsla {
        if self.is_paper() {
            self.ink.ink_dim
        } else {
            legacy
        }
    }

    pub fn region_face(&self, legacy: Hsla) -> Hsla {
        if self.text_controls() || self.is_paper() {
            self.ink.panel
        } else {
            legacy
        }
    }

    /// Edge only, safe after a caller has added its text or hosted content.
    fn stock_rim(&self, edge: Hsla, accent: Hsla, active: bool) -> Div {
        crate::paper::rim_with(edge, accent, active, self.stock())
    }

    pub fn rim(&self, edge: Hsla, accent: Hsla, active: bool) -> Option<Div> {
        self.is_paper()
            .then(|| crate::paper::rim_with(edge, accent, active, self.stock()))
    }

    pub fn frame<E: Styled>(&self, d: E) -> E {
        if self.unboxed() {
            self.open_control(d).rounded(self.radius())
        } else if self.text_controls() {
            d.rounded(self.radius())
                .border(px(self.m.border))
                .shadow(Vec::new())
        } else if self.is_paper() {
            d.rounded(px(0.))
                .border_color(gpui::transparent_black())
                .shadow(crate::paper::shadows_with(self.stock()))
        } else {
            d
        }
    }

    /// A slider's decoration; the caller retains its measured hit target,
    /// handlers and value mapping. Both strategies share those exact bounds.
    pub fn scrubber(&self, fraction: f32) -> Div {
        let fraction = fraction.clamp(0., 1.);
        if self.is_paper() {
            return crate::paper::gauge(
                fraction,
                self.ink.btn_face,
                self.ink.ink_dim,
                self.ink.select,
                self.stock(),
            );
        }
        div()
            .absolute()
            .inset_0()
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(gpui::relative(0.5))
                    .h(px(3.))
                    .rounded(self.radius_pill())
                    .bg(self.ink.mark.alpha(0.18)),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(gpui::relative(0.5))
                    .h(px(3.))
                    .w(gpui::relative(fraction))
                    .rounded(self.radius_pill())
                    .bg(self.ink.select),
            )
            .child(
                div()
                    .absolute()
                    .left(gpui::relative(fraction))
                    .top(gpui::relative(0.5))
                    .ml(px(-5.))
                    .mt(px(-4.))
                    .size(px(10.))
                    .rounded(self.radius_pill())
                    .bg(self.ink.mark),
            )
    }

    /// Apply a choice's material without altering its input geometry or semantic ink.
    pub fn choice<E: Styled + ParentElement>(&self, d: E, active: bool) -> E {
        if self.is_paper() {
            self.paper_option(d, active)
        } else {
            self.tab(
                self.open_control(d).relative().rounded(self.radius()),
                active,
                self.ink.select,
            )
            .font_family(self.font_family.clone())
            .text_color(if active {
                self.lit_label()
            } else {
                self.ink.ink_dim
            })
        }
    }

    fn open_control<E: Styled>(&self, d: E) -> E {
        if self.unboxed() {
            d.border(px(0.)).shadow(Vec::new())
        } else {
            d
        }
    }

    /// An action has a raised backing; a choice is writing on the parent region.
    /// Apply this before adding content: ParentElement appends visual children.
    pub fn action<E: Styled + ParentElement>(&self, d: E, active: bool) -> E {
        let d = self.open_control(d);
        if self.is_paper() {
            self.paper_control(d, active)
        } else if let Some(glass) = self.smoked_backing(
            if active {
                self.ink.select
            } else {
                self.ink.edge
            },
            false,
        ) {
            d.relative()
                .bg(gpui::transparent_black())
                .border_color(gpui::transparent_black())
                .child(glass)
        } else {
            d.relative().rounded(self.radius()).children(self.grain())
        }
    }

    /// A semantic meter track/fill keeps its caller's data colour.
    pub fn meter_part(&self, d: Div) -> Div {
        d.relative()
            .rounded(self.radius_pill())
            .children(self.grain())
    }

    /// An allowance may use discrete marks; the owner still chooses its count and semantic inks.
    pub fn discrete_meter(
        &self,
        reading: Option<f32>,
        count: usize,
        ink: Hsla,
        track: Hsla,
        unknown: Hsla,
    ) -> Option<Div> {
        (self.shape.gauge == Gauge::Pips)
            .then(|| crate::deck::pips(crate::deck::pips_for(reading, count), ink, track, unknown))
    }

    /// A floating plain frame keeps its phosphor shadow; stock and cut frames own their depth.
    pub fn floating_shadow<E: Styled>(&self, d: E, ink: Hsla, phosphor: f32) -> E {
        if self.is_paper() || self.is_cut() {
            d
        } else {
            d.shadow(crate::float_shadows(ink, phosphor))
        }
    }

    /// A bounded reading. Absence draws hollow pips or an explicitly unknown rail.
    pub fn meter(
        &self,
        reading: Option<f32>,
        ink: Hsla,
        track: Hsla,
        unknown: Hsla,
        height: f32,
    ) -> Div {
        let reading = reading.filter(|v| v.is_finite()).map(|v| v.clamp(0., 1.));
        if let Some(pips) = self.discrete_meter(reading, 24, ink, track, unknown) {
            return div().relative().w_full().h(px(height * 1.8)).child(pips);
        }
        let track = self.meter_part(div().w_full().h(px(height)).bg(track));
        match reading {
            Some(f) => track.overflow_hidden().child(
                self.meter_part(
                    div()
                        .h_full()
                        .w(gpui::relative(if f > 0. { f.max(0.02) } else { 0. }))
                        .bg(ink),
                ),
            ),
            None => track
                .border_1()
                .border_color(unknown)
                .text_color(unknown)
                .child("—"),
        }
    }

    /// Set a NAME in the skin's heading face: a project, a pane title, a
    /// panel's title, the notice on the top band.
    ///
    /// A no-op unless the skin names a heading face of its own, so every skin
    /// written before `[font] heading` existed keeps drawing exactly what it
    /// drew. A separate heading face is set bold: it is there to be the loud
    /// voice beside the interface text, and a regular cut of a display face
    /// reads as a typo for the body face.
    pub fn heading<E: Styled>(&self, d: E) -> E {
        if self.heading_family == self.font_family {
            return d;
        }
        d.font_family(self.heading_family.clone())
            .font_weight(gpui::FontWeight::BOLD)
    }

    /// A name one rung quieter than a heading — an initiative under its
    /// project, a section inside a panel: the heading face, semibold. A no-op
    /// under a skin with no heading face of its own.
    pub fn subheading<E: Styled>(&self, d: E) -> E {
        if self.heading_family == self.font_family {
            return d;
        }
        d.font_family(self.heading_family.clone())
            .font_weight(gpui::FontWeight::SEMIBOLD)
    }

    /// A name's letters, as the heading face sets them: capitals, when the skin
    /// has a heading face of its own, and the string untouched otherwise. Kept
    /// apart from [`Skin::caps`], which is the LABEL strategy and already has
    /// its own answer in deco and ops.
    pub fn heading_text(&self, text: &str) -> String {
        if self.heading_family == self.font_family {
            text.to_string()
        } else {
            text.to_uppercase()
        }
    }

    /// Set a NUMBER in the skin's data face: a percentage, a count, a zoom.
    /// A no-op unless the skin names a data face of its own.
    pub fn data<E: Styled>(&self, d: E) -> E {
        if self.data_family == self.font_family {
            return d;
        }
        d.font_family(self.data_family.clone())
    }

    /// Optical chrome type, independent of material and measured terminal cells.
    pub fn text_px(&self, size: f32) -> Pixels {
        px((size * self.type_scale).max(self.type_min_px * self.scale))
    }

    /// An unpadded sheet: callers retain their geometry and interaction.
    pub fn sheet(&self) -> Div {
        if self.is_paper() {
            div().relative().bg(self.ink.panel).children(self.grain())
        } else if self.has_material() {
            // The wash only: a band keeps the ground it already had.
            div().relative().children(self.grain())
        } else {
            div()
        }
    }

    /// Finish a region before its content is added, replacing legacy chrome.
    pub fn finish<E: Styled + ParentElement>(&self, d: E) -> E {
        if self.unboxed() {
            self.chrome(self.open_control(d))
                .bg(self.ink.panel)
                .rounded(self.radius())
        } else {
            self.finish_edged(d, self.ink.edge_strong)
        }
    }

    /// A grouping inside an overlay. Its backing stays independent of the
    /// overlay's opaque frame, so the glass never changes the frame's material.
    pub fn finish_group<E: Styled + ParentElement>(&self, d: E) -> E {
        if self.unboxed() {
            self.chrome(self.open_control(d))
                .bg(gpui::transparent_black())
        } else {
            self.finish_group_edged(d, self.ink.edge_strong)
        }
    }

    /// A foreground grouping whose edge carries selection, severity or identity.
    /// Apply before adding content, as with `finish_group`.
    pub fn finish_group_edged<E: Styled + ParentElement>(&self, d: E, edge: Hsla) -> E {
        if self.unboxed() {
            return self
                .chrome(self.open_control(d))
                .bg(gpui::transparent_black())
                .border_l(px(self.m.hairline))
                .border_color(edge);
        }
        if let Some(glass) = self.smoked_backing(edge, false) {
            self.chrome(d)
                .relative()
                .bg(gpui::transparent_black())
                .border_color(gpui::transparent_black())
                .shadow(Vec::new())
                .child(glass)
        } else {
            self.finish_edged(d, edge)
        }
    }

    fn smoked_backing(&self, edge: Hsla, grid: bool) -> Option<Div> {
        (self.is_cut() && self.shape.material == Material::Grid && self.m.frost > 0.).then(|| {
            crate::deck::smoked_plate(
                self.ink.panel_sunken,
                edge,
                self.m.cut,
                grid.then_some((self.ink.mark.alpha(0.16), 22. * self.scale)),
                self.m.frost * self.scale,
            )
        })
    }

    /// [`Skin::finish`], for a region whose edge colour is a MEANING rather
    /// than furniture — the red rim of a destructive confirm. A cut plate draws
    /// its edge in `edge`; every other strategy behaves exactly as `finish`.
    pub fn finish_edged<E: Styled + ParentElement>(&self, d: E, edge: Hsla) -> E {
        if self.unboxed() {
            return self
                .chrome(self.open_control(d))
                .bg(self.ink.panel)
                .rounded(self.radius())
                .border_l(px(self.m.hairline))
                .border_color(edge);
        }
        if self.text_controls() {
            return self
                .chrome(d)
                .bg(self.ink.panel)
                .rounded(self.radius())
                .border(px(self.m.border))
                .border_color(edge)
                .shadow(Vec::new());
        }
        let d = self.chrome(d);
        if let Some(glass) = self.smoked_backing(edge, true) {
            return d
                .bg(gpui::transparent_black())
                .border_color(gpui::transparent_black())
                .shadow(Vec::new())
                .child(glass);
        }
        if self.is_cut() {
            // A floating region: the caller's fill, border and rectangular
            // shadow give way to one cut plate with a hard shadow of the same
            // shape. The border keeps its width, transparent, so the caller's
            // geometry does not move.
            return d
                .bg(hsla(0., 0., 0., 0.))
                .border_color(hsla(0., 0., 0., 0.))
                .shadow(Vec::new())
                .child(crate::deck::plate(
                    self.ink.panel_raised,
                    edge,
                    self.m.cut,
                    true,
                ))
                .children(self.grain());
        }
        if self.is_paper() {
            d.bg(self.ink.panel)
                .border_color(hsla(0., 0., 0., 0.))
                .rounded(self.radius())
                .shadow(crate::paper::shadows_with(self.stock()))
                .children(self.grain())
                .child(self.stock_rim(edge, self.ink.select, false))
        } else {
            d
        }
    }
    fn is_paper(&self) -> bool {
        self.shape.material == Material::Paper
    }

    /// Whether regions are drawn as cut plates rather than as styled quads.
    /// Paper wins where both are asked for: a sheet of cotton is not machined.
    fn is_cut(&self) -> bool {
        self.shape.corner == Corner::Cut && !self.is_paper()
    }

    /// Optional material layer, added before a region's own content: paper's
    /// grain, or the grid wash. Absolutely positioned, so its parent must be
    /// `relative()` — every caller already is, because paper needed the same.
    pub fn grain(&self) -> Option<Div> {
        match self.shape.material {
            Material::Paper => Some(crate::paper::grain_with(self.stock())),
            Material::Grid => Some(crate::deck::grid(
                self.ink.mark.alpha(0.045),
                22. * self.scale,
            )),
            Material::Plain => None,
        }
    }

    /// Whether this skin lays a material under its regions at all.
    fn has_material(&self) -> bool {
        self.shape.material != Material::Plain
    }

    /// The chevron either side of a heading name, under `title = "chevron"`:
    /// `»` before and `«` after, in the palette's signal ink and the data face.
    /// `None` under every other title strategy, so a caller writes
    /// `.children(sk.chevron("»")).child(name).children(sk.chevron("«"))` and
    /// draws exactly the name everywhere else.
    pub fn chevron(&self, glyph: &'static str) -> Option<Div> {
        (self.shape.title == Title::Chevron).then(|| {
            div()
                .flex_none()
                .mx(self.px(3.))
                .font_family(self.data_family.clone())
                .text_color(self.ink.warn)
                .child(glyph)
        })
    }

    /// The hatched strip that marks the focused pane, under `focus = "hatch"`.
    /// Absolutely positioned along its parent's top edge, inset from the
    /// corners; `None` for an unfocused pane and under every other strategy.
    pub fn focus_hatch(&self, focused: bool) -> Option<Div> {
        (focused && self.shape.focus == Focus::Hatch).then(|| {
            div()
                .absolute()
                .top_0()
                .left(self.px(28.))
                .right(self.px(28.))
                .h(self.px(3.))
                .child(crate::deck::hatch(self.ink.mark.alpha(0.70)))
        })
    }

    /// A paper control's backing is laid down before its caller adds text.
    fn paper_control<E: Styled + ParentElement>(&self, d: E, active: bool) -> E {
        self.paper_option(d, false)
            .child(crate::paper::backing_with(
                self.ink.btn_face,
                self.ink.select,
                active,
                self.stock(),
            ))
    }

    /// Choices are writing on the sheet, marked once rather than laid over a
    /// second filled tile. Action buttons retain their raised cotton backing.
    fn paper_option<E: Styled + ParentElement>(&self, d: E, active: bool) -> E {
        d.relative()
            .bg(hsla(0., 0., 0., 0.))
            .border_color(hsla(0., 0., 0., 0.))
            .font_family(self.font_family.clone())
            .text_size(self.text_px(11. * self.scale))
            .line_height(gpui::relative(self.line_height))
            .text_color(self.ink.ink)
            .children(active.then(|| crate::paper::selection_with(self.ink.select, self.stock())))
    }
    /// A caption owns its marker: icons above it must not move the mark away
    /// from the handwriting. A relative line height prevents inherited leading
    /// from overflowing compact two-line controls.
    fn paper_caption(&self, text: &str, size: f32, active: bool) -> Div {
        div()
            .relative()
            .flex_none()
            .text_size(self.text_px(size))
            .line_height(gpui::relative(self.line_height))
            .whitespace_nowrap()
            .text_color(self.ink.ink)
            .children(active.then(|| crate::paper::selection_with(self.ink.select, self.stock())))
            .child(text.to_string())
    }
    /// This skin, wearing a pane's text-size gauge.
    ///
    /// A copy rather than a mutation: one bake per pane per frame feeds both
    /// the header and the bench, and they want different answers — see
    /// [`Skin::ty`].
    pub fn with_type(&self, gauge: f32) -> Skin {
        Skin {
            ty: crate::workbench::Type::at(gauge),
            ..self.clone()
        }
    }

    /// A rung of the type ramp, in points, with this skin's gauge applied.
    ///
    /// Spelled on the skin so a call site reads `sk.pt(Step::Body)` beside
    /// `sk.rad(…)` and never multiplies by a factor itself — the same bargain
    /// `scale` already struck with every corner in the chrome.
    pub fn pt(&self, step: crate::workbench::Step) -> f32 {
        f32::from(self.text_px(self.ty.pt(step)))
    }

    /// A box that holds text, scaled the same way the text in it is.
    ///
    /// Type that grows inside a column that does not is how an exact fit starts
    /// wrapping down a row, and a screenshot is the only thing that catches it.
    pub fn tpx(&self, base: f32) -> f32 {
        self.ty.px(base) * self.type_scale
    }
}

// ---------------------------------------------------------------------------
// The element vocabulary
// ---------------------------------------------------------------------------

/// What a ringed thing looks like when it is NOT the lit one.
///
/// Not a style preference — the two answers belong to two different shapes, and
/// giving either one the other's answer is a bug you can see from across the
/// room.
///
/// A **row** in a list of twenty carries its selection alone. Box the other
/// nineteen and a tree becomes a grid; the border is merely reserved,
/// transparent, so that moving the selection moves nothing.
///
/// A **tab** is one of three or four side by side, and it is a control whether
/// or not it is the one you are standing on. Drawn bare it comes out as two
/// grey words floating beside a lit pill — which is exactly what `overview` and
/// `artifacts` were next to `decisions`, and why the strip read as one button
/// and some text rather than as three tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rest {
    /// Nothing but the reserved, transparent border.
    Bare,
    /// A quiet bordered face — [`Inks::edge_rest`] around [`Inks::face_rest`].
    Face,
}

/// gpui spells border widths as `border_0` … `border_8` (1px steps) rather than
/// taking a length, so a token width has to be routed to one of them.
///
/// Widths are deliberately NOT multiplied by the UI scale. A hairline that
/// thickens with the scale stops being a hairline, and today's chrome already
/// spells every border as an unscaled `border_1()` — scaling them here would be
/// a look change smuggled in under a refactor.
fn w8(w: f32) -> u8 {
    w.round().clamp(0., 8.) as u8
}

macro_rules! width_router {
    ($name:ident, $($n:literal => $m:ident),+ $(,)?) => {
        /// Generic over the element so the vocabulary works on a `Stateful<Div>`
        /// (anything the chrome gave an id to) as well as a bare `Div`.
        fn $name<E: Styled>(d: E, w: f32) -> E {
            match w8(w) { $( $n => d.$m(), )+ _ => d }
        }
    };
}

width_router!(b_all, 0 => border_0, 1 => border_1, 2 => border_2, 3 => border_3,
    4 => border_4, 5 => border_5, 6 => border_6, 7 => border_7, 8 => border_8);
width_router!(b_t, 0 => border_t_0, 1 => border_t_1, 2 => border_t_2, 3 => border_t_3,
    4 => border_t_4, 5 => border_t_5, 6 => border_t_6, 7 => border_t_7, 8 => border_t_8);
width_router!(b_b, 0 => border_b_0, 1 => border_b_1, 2 => border_b_2, 3 => border_b_3,
    4 => border_b_4, 5 => border_b_5, 6 => border_b_6, 7 => border_b_7, 8 => border_b_8);
width_router!(b_l, 0 => border_l_0, 1 => border_l_1, 2 => border_l_2, 3 => border_l_3,
    4 => border_l_4, 5 => border_l_5, 6 => border_l_6, 7 => border_l_7, 8 => border_l_8);
width_router!(b_r, 0 => border_r_0, 1 => border_r_1, 2 => border_r_2, 3 => border_r_3,
    4 => border_r_4, 5 => border_r_5, 6 => border_r_6, 7 => border_r_7, 8 => border_r_8);

/// The vocabulary. Every method here is meant to be called from chrome code, and
/// adoption is staged one surface at a time (see `docs/plans/chrome-skin/`), so
/// the ones no surface has reached yet look dead to a binary crate's dead-code
/// pass. They are not: shipping `bar` without a caller is the point — the next
/// slice converts a call site rather than designing a primitive under deadline.
#[allow(dead_code)]
impl Skin {
    /// Scale one unscaled metric. The ONLY place the UI scale is applied.
    pub fn px(&self, v: f32) -> Pixels {
        px(v * self.scale)
    }

    /// The radius of a small element, after the corner strategy has had its say.
    ///
    /// **Unscaled**, like border widths and unlike lengths. That is not a
    /// simplification: it is what the chrome already does. `rounded_sm()` is a
    /// fixed 4px and appears 55 times; the three sites that spell a scaled radius
    /// keep it through [`Skin::rad`]. A radius describes the SHAPE of a corner
    /// rather than the size of the thing it is on, so it belongs with the
    /// hairline, and making it scale here would have quietly rounded every corner
    /// in the app by 0.8px the moment the house scale (0.80) was applied.
    pub fn radius(&self) -> Pixels {
        self.rad_raw(self.m.radius)
    }

    /// The radius of a whole region — the left bar's own frame, at `px(10.)`.
    pub fn radius_lg(&self) -> Pixels {
        self.rad_raw(self.m.radius_lg)
    }

    /// A pill's radius — the chrome's `rounded_full`, which a square skin flattens
    /// like everything else.
    ///
    /// For pills: chips, tracks, badges, dots. **Not for circles the content
    /// requires** — the HSV colour disk in the tab wheel keeps a literal
    /// `rounded_full()`, because there hue is the angle and saturation the radius,
    /// so the circle is the data structure. The skin owns the shapes the chrome
    /// chose; it does not own the shapes the content is.
    pub fn radius_pill(&self) -> Pixels {
        match self.shape.corner {
            Corner::Round => self.capped(9999.),
            Corner::Square | Corner::Cut => px(0.),
        }
    }

    /// An arbitrary radius the chrome already spells as `rounded(px(v * scale))`.
    /// Kept rather than folded into [`Skin::radius`] because the chrome genuinely
    /// has several — a chip is not a card is not a window — and flattening them
    /// all to one token would be a look change smuggled in under a refactor.
    pub fn rad(&self, v: f32) -> Pixels {
        match self.shape.corner {
            Corner::Round => px(f32::from(self.px(v)).min(self.m.corner_cap.max(0.))),
            Corner::Square | Corner::Cut => px(0.),
        }
    }

    /// The same, for the sites that spell it `rounded(px(v))` with no scale.
    pub fn rad_raw(&self, v: f32) -> Pixels {
        match self.shape.corner {
            Corner::Round => self.capped(v),
            Corner::Square | Corner::Cut => px(0.),
        }
    }

    /// A radius some site spells by hand and keeps under EVERY corner strategy
    /// — the window's frame, the screen well, the pane frame — held under the
    /// skin's `corner_cap`. Square skins keep those corners exactly as before;
    /// only a skin with a cap (Clean's 3 px) draws them nearly square.
    pub fn capped(&self, v: f32) -> Pixels {
        px(v.min(self.m.corner_cap.max(0.)))
    }

    /// A TAB on the strip, marked or not. `tint` is the tab's own colour where
    /// somebody chose one, so a deliberately coloured tab still reads as itself.
    ///
    /// Tabs are not rows. The strip is the surface a look gets judged on, because
    /// it is the one a person looks at most, and it has a constraint a row does
    /// not: **the rule is drawn in every state and merely goes transparent when
    /// the tab is inactive.** A border that appeared on selection would shift the
    /// entire strip by its own width every time you changed tabs. That trick is
    /// in here rather than at the call site precisely because it is the kind of
    /// thing a later strategy would quietly break.
    ///
    /// `Fill` and `Underline` are the same device here — the bottom rule the
    /// strip already draws. That is not a fudge: the default skin has to be
    /// pixel-identical to today, and today's tab is underlined whatever the rest
    /// of the chrome does with its selected things.
    pub fn tab<E: Styled + ParentElement>(&self, d: E, active: bool, tint: Hsla) -> E {
        if self.is_paper() {
            return b_all(d.relative(), self.m.border)
                .bg(self.ink.btn_face)
                .border_color(hsla(0., 0., 0., 0.))
                .text_color(self.ink.ink)
                .child(self.stock_rim(self.ink.edge_rest, tint, active));
        }
        let clear = hsla(0., 0., 0., 0.);
        let lit = |on: bool| if on { tint } else { clear };
        match self.shape.emphasis {
            Emphasis::Fill | Emphasis::Underline => b_b(d, self.m.rail).border_color(lit(active)),
            Emphasis::Rail => b_l(d, self.m.rail).border_color(lit(active)),
            // The ring takes the whole edge, so the strip's reserved bottom rule
            // is not drawn as well — two devices on one tab is one too many, and
            // `ring` reserves its own border in every state regardless.
            //
            // `Rest::Face` because a tab strip is a row of controls: an inactive
            // tab that is only a dim word reads as a caption, and the strip
            // stops looking like somewhere you can click.
            Emphasis::Glow => {
                let d = self.ring(d, active, tint, Rest::Face);
                if active {
                    // The seat, in the TAB'S OWN hue rather than the accent's.
                    // A wash mixed from one colour under a ring lit in another
                    // is two claims about the same box.
                    d.bg(tint.alpha(self.ink.mark_wash.a))
                } else {
                    d
                }
            }
            // The rule still occupies its two pixels, transparent, so switching
            // tabs under a bracketed skin moves nothing either.
            Emphasis::Bracket => {
                let d = b_b(d, self.m.rail).border_color(clear);
                if active {
                    self.brackets(d, tint)
                } else {
                    d
                }
            }
            // The strip's rule stays reserved and transparent, so marking a
            // tab moves nothing; the cradle hangs from the tab's own box.
            Emphasis::Bar => {
                let d = b_b(d.relative(), self.m.rail).border_color(clear);
                if active {
                    self.cradle(d.text_color(self.ink.ink_lit), tint)
                } else {
                    d
                }
            }
        }
    }

    /// A skin at a different scale — for the handful of controls that already
    /// draw themselves at `s * 0.85`.
    pub fn at(&self, factor: f32) -> Skin {
        let mut s = self.clone();
        s.scale *= factor;
        s
    }

    /// Apply the caps strategy to a label. Pure, so the tracking rule is testable
    /// without a window.
    pub fn caps(&self, text: &str) -> String {
        match self.shape.caps {
            Caps::Off => text.to_string(),
            Caps::Upper => text.to_uppercase(),
            // U+2009 THIN SPACE. A real space would read as word breaks; the
            // thin space reads as tracking, which is what deco small caps are.
            Caps::Tracked => text
                .to_uppercase()
                .chars()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("\u{2009}"),
        }
    }

    /// A whole chrome region: its ground, its corner, its edge, and the inset
    /// that keeps its contents off all three. `Boundary::Double` hangs a ring
    /// inside it, which is why the returned div is already `relative()` — a
    /// caller's own absolute children still position against it.
    ///
    /// **The inset is here and not at the call site**, because the call site is
    /// where it kept not happening. Fourteen places on the bench ask for a
    /// panel; seven of them padded it and seven did not, and the seven that did
    /// not drew an eleven-point line of text with its descenders sitting on a
    /// lit border and a ten-pixel corner arcing through its first word. Parker,
    /// on the decision card: *"the border clips the text"*. A rule that lives
    /// only at the call sites is a rule half the call sites will miss, and the
    /// half that missed is not discoverable from the source — only from a
    /// photograph.
    ///
    /// A region that genuinely wants something bled to its own edge still says
    /// so: the seven that already pad pass their own `px`/`py`/`p` after this
    /// and overwrite it, and `.p(px(0.))` is the way back to none.
    ///
    /// Safe for the ring: taffy resolves an absolute child's insets against the
    /// container size **minus border** and never subtracts padding
    /// (`perform_absolute_layout_on_absolute_children`), so the double rule goes
    /// on hugging the edge rather than sliding in with the text.
    pub fn panel(&self) -> Div {
        self.panel_with_grid(false)
    }

    /// The navigation spine frosts the deck grid rather than desktop content.
    pub fn spine_panel(&self) -> Div {
        self.panel_with_grid(true)
    }

    fn panel_with_grid(&self, grid: bool) -> Div {
        let d = div()
            .relative()
            .bg(self.ink.panel)
            .rounded(self.radius_lg())
            .px(self.px(self.m.pad_x))
            .py(self.px(self.m.pad_y));
        if self.is_cut() {
            // The plate goes in FIRST and the material over it, so every child
            // the caller adds paints over both. The border is reserved,
            // transparent, at the width a hairline skin would draw, so swapping
            // skins moves nothing.
            let edge = match self.shape.boundary {
                Boundary::None => hsla(0., 0., 0., 0.),
                _ => self.ink.rule_strong,
            };
            let d =
                b_all(d.bg(hsla(0., 0., 0., 0.)), self.m.border).border_color(hsla(0., 0., 0., 0.));
            if let Some(glass) = self.smoked_backing(edge, grid) {
                return d.child(glass);
            }
            return d
                .border_color(hsla(0., 0., 0., 0.))
                .child(crate::deck::plate(self.ink.panel, edge, self.m.cut, false))
                .children(self.grain());
        }
        let d = d.children(self.grain());
        if self.is_paper() {
            return b_all(
                d.shadow(crate::paper::shadows_with(self.stock())),
                self.m.border,
            )
            .border_color(hsla(0., 0., 0., 0.))
            .child(self.stock_rim(self.ink.rule_strong, self.ink.select, false));
        }
        // The inner line of a double/inset boundary is an absolutely positioned
        // ring rather than a second border, because one div carries one border.
        // It is added FIRST so a caller's own children paint over it.
        let ring = |c: Hsla| {
            let g = self.px(self.m.rule_gap);
            b_all(
                div()
                    .absolute()
                    .top(g)
                    .bottom(g)
                    .left(g)
                    .right(g)
                    .rounded(self.radius()),
                self.m.hairline,
            )
            .border_color(c)
        };
        match self.shape.boundary {
            Boundary::None => d,
            Boundary::Hairline => b_all(d, self.m.border).border_color(self.ink.rule_strong),
            Boundary::Inset => d.child(ring(self.ink.rule_strong)),
            Boundary::Double => b_all(d, self.m.border)
                .border_color(self.ink.rule_strong)
                .child(ring(self.ink.rule)),
        }
    }

    /// The ground of a raised surface, lit or flat according to the skin.
    ///
    /// Takes both stops because the caller has already computed its own lighter
    /// tone from its own palette (a pane header's `lighter` is derived from the
    /// pane's surface, not the window's), and inventing a second one here would
    /// give a retinted pane two different headers.
    pub fn ground(&self, lit: Hsla, base: Hsla) -> gpui::Background {
        match self.shape.shine {
            Shine::Gradient => gpui::linear_gradient(
                180.,
                gpui::linear_color_stop(lit, 0.),
                gpui::linear_color_stop(base, 1.),
            ),
            Shine::Flat => base.into(),
        }
    }

    /// A horizontal band across a region — a header, a footer, a toolbar.
    pub fn bar(&self) -> Div {
        let d = div();
        let d = if self.has_material() {
            d.relative().children(self.grain())
        } else {
            d
        };
        d.font_family(self.font_family.clone())
            .flex()
            .flex_row()
            .items_center()
            .gap(self.px(self.m.gap))
            .px(self.px(self.m.pad_x))
            .py(self.px(self.m.pad_y))
            .bg(self.ink.panel_raised)
    }

    /// The divider between two stacked things. Returns a container rather than a
    /// bare line so `Divider::Double` can be two lines without the call site
    /// knowing there are two.
    ///
    /// Line thickness does NOT take the UI scale, for the same reason border
    /// widths do not: a hairline is a hairline at every scale, and today's chrome
    /// already spells this one `px(1.)` next to an `mx(px(6. * s))`.
    pub fn rule_h(&self) -> Div {
        let line = |c: Hsla| div().h(px(self.m.hairline)).w_full().bg(c);
        match self.shape.divider {
            Divider::None => div().h(px(0.)),
            Divider::Line => line(self.ink.rule),
            Divider::Double => div()
                .flex()
                .flex_col()
                .child(line(self.ink.rule_strong))
                .child(div().h(self.px(self.m.rule_gap)))
                .child(line(self.ink.rule)),
        }
    }

    /// The divider between two side-by-side things.
    pub fn rule_v(&self) -> Div {
        let line = |c: Hsla| div().w(px(self.m.hairline)).h_full().bg(c);
        match self.shape.divider {
            Divider::None => div().w(px(0.)),
            Divider::Line => line(self.ink.rule),
            Divider::Double => div()
                .flex()
                .flex_row()
                .child(line(self.ink.rule_strong))
                .child(div().w(self.px(self.m.rule_gap)))
                .child(line(self.ink.rule)),
        }
    }

    /// A row of controls that does NOT carry its own ground — a header inside a
    /// panel, a strip of buttons. Distinct from [`Skin::bar`] on purpose: a deco
    /// header is banded by the RULE under it, not by a fill, and conflating the
    /// two would give every header a surface it does not have today.
    pub fn row(&self) -> Div {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(self.px(self.m.gap))
            .px(self.px(self.m.pad_x))
            .py(self.px(self.m.pad_y))
    }

    /// A bare glyph target: no ground, no border, a hover lift. The commonest
    /// control in the chrome and the one most often hand-rolled.
    ///
    /// Takes the element id because gpui's `hover` lives on the STATEFUL half of
    /// the interactive traits — a div has to be identified before it can be said
    /// to be hovered — and the hover lift is part of what this control IS.
    pub fn icon_btn(&self, id: &'static str) -> Stateful<Div> {
        let hover = self.ink.hover;
        let d = div();
        let d = if self.is_paper() {
            self.paper_control(d, false)
        } else {
            d
        };
        d.id(id)
            .flex()
            .items_center()
            .justify_center()
            .h(self.px(self.m.row_h))
            .rounded(self.radius())
            .cursor_pointer()
            .hover(move |st| st.bg(hover))
    }

    /// A small tag: a status, a scope, a count. `active` routes through the
    /// emphasis strategy — this is the single branch that turns "selected" from
    /// a filled pill into a bracket or a rail across the whole app.
    pub fn chip(&self, active: bool) -> Div {
        let base = div()
            .relative()
            .px(self.px(self.m.chip_px))
            .py(self.px(self.m.chip_py))
            .rounded(self.radius())
            .text_size(self.px(self.m.label_size))
            .whitespace_nowrap();
        if self.is_paper() {
            return b_all(self.paper_control(base, active), self.m.border);
        }
        // The ring reserves its border in every state, so an unlit chip under a
        // glow skin is the same size as a lit one.
        //
        // NEITHER ink is the select colour any more. Lit was `select`, which put
        // the word, its border and its bloom in one hue; unlit was `ink_off`,
        // which is Faint — a legitimate colour for a label that is genuinely not
        // in effect, and the wrong one for a tab you are meant to press. The
        // pair is now the foreground at two weights, and the hue is spent on the
        // edge, the seat and the halo, which is how the model/effort strip has
        // always done it.
        if matches!(self.shape.emphasis, Emphasis::Glow) {
            let ink = if active {
                self.ink.ink_lit
            } else {
                self.ink.ink_dim
            };
            let r = self.ring(base.text_color(ink), active, self.ink.select, Rest::Face);
            return if active && self.ink.mark_wash.a > 0.02 {
                r.bg(self.ink.select.alpha(self.ink.mark_wash.a))
            } else {
                r
            };
        }
        if !active {
            // A bar skin's unlit chip is still a control you can press, so it
            // reads at the dimmed body weight rather than at `ink_off`, the
            // colour of a label that is not in effect.
            let rest = if self.shape.emphasis == Emphasis::Bar {
                self.ink.ink_dim
            } else {
                self.ink.ink_off
            };
            return base.text_color(rest);
        }
        match self.shape.emphasis {
            Emphasis::Fill => base.bg(self.ink.mark_wash).text_color(self.ink.mark),
            Emphasis::Underline => {
                b_b(base.text_color(self.ink.mark), self.m.border).border_color(self.ink.mark)
            }
            Emphasis::Rail => {
                b_l(base.text_color(self.ink.mark), self.m.rail).border_color(self.ink.mark)
            }
            Emphasis::Bracket => self.brackets(base.text_color(self.ink.mark), self.ink.mark),
            Emphasis::Bar => base.bg(self.ink.mark).text_color(self.ink.ink_on_mark),
            Emphasis::Glow => unreachable!("handled above, before the inactive early-return"),
        }
    }

    /// A two-state slider: one bordered track, and the half you are on filled.
    ///
    /// TERM ⇄ BENCH was two chips with two pixels between them. Under the ring
    /// each reserved its own border, so one binary choice put four vertical
    /// edges on the header — and a pair of adjacent buttons is not a switch,
    /// it is a pair of adjacent buttons. Parker: *"TERM and BENCH — combine
    /// into a single bordered slider toggler, also pretty unreadable"*. The
    /// count of edges is now the count of decisions in the control.
    ///
    /// The track wears the [`Rest::Face`] treatment — the same quiet edge every
    /// tab at rest wears — so the toggle reads as a member of the chrome's
    /// family rather than as a shape of its own. Feed it [`Skin::slider_half`].
    pub fn slider(&self) -> Div {
        if self.is_paper() {
            return self.sheet().flex().flex_row().items_center().p(px(2.));
        }
        b_all(
            div()
                .flex()
                .flex_row()
                .items_center()
                .p(px(self.m.hairline))
                .rounded(self.radius())
                .bg(self.ink.face_rest),
            self.m.border,
        )
        .border_color(self.ink.edge_rest)
    }

    /// One half of a [`Skin::slider`].
    ///
    /// The thumb is a FILL and not a second border: a bordered thing inside a
    /// bordered thing is the four-edge problem again at half the size. It also
    /// carries the control's entire phosphor budget, so throwing the switch
    /// MOVES the glow rather than lighting a second one — which is the whole
    /// read of a slider, and the reason this is one control instead of two.
    pub fn slider_half(&self, lit: bool, tint: Hsla) -> Div {
        let d = div()
            .px(self.px(self.m.chip_px))
            .py(self.px(self.m.chip_py))
            .rounded(self.rad_raw((self.m.radius - self.m.border).max(0.)))
            .whitespace_nowrap()
            .cursor_pointer();
        if self.is_paper() {
            return self.paper_control(d, lit);
        }
        if !lit {
            return d.text_color(self.ink.ink_dim);
        }
        // Heavier than `mark_wash` on purpose. Everywhere else the fill is a
        // seat under a lit border and the border does the marking; here there is
        // no border to do it, so the fill IS the thumb and has to survive being
        // read at a glance against the track it sits in.
        d.bg(tint.alpha((self.ink.mark_wash.a * 1.6).min(0.28)))
            .text_color(self.ink.ink_lit)
            .shadow(self.bloom(tint))
    }

    /// This skin's control bloom in `tint`, through the phosphor gauge — the one
    /// halo [`Skin::halo`] and the lit half of a slider both wear. Empty when
    /// the gauge is all the way down.
    fn bloom(&self, tint: Hsla) -> Vec<gpui::BoxShadow> {
        crate::theme::phosphor(
            self.phosphor,
            gpui::BoxShadow {
                color: tint.alpha(self.m.glow_a.clamp(0., 1.)),
                offset: gpui::point(px(0.), px(0.)),
                blur_radius: px(self.m.glow),
                spread_radius: px(0.),
                inset: false,
            },
        )
        .into_iter()
        .collect()
    }

    /// The phosphor ring: a lit border that blooms outward.
    ///
    /// `tint` is what the marked thing is lit in — the accent, or the thing's own
    /// colour where it has one. The border is drawn at EVERY state and merely
    /// goes transparent when unlit, so switching which row or tab is active never
    /// moves anything by a border width. That reservation is the whole reason
    /// this lives here rather than at a call site.
    ///
    /// **One halo, and no fill.** It used to be two shadows — a crisp 1px
    /// spread ring at `glow_a` *plus* a wide soft halo — copied down from the
    /// focused pane, where it is right: a tube is a large object and the thing
    /// it is competing with is another tube. A tab is a nine-point word in a
    /// box five pixels wider than itself, and a spread ring on a box that small
    /// closes over the glyphs from every side at once. The lit shelf tab came
    /// out as a coloured blob with something written in it.
    ///
    /// So the crisp ring is gone and the border does that job, which is what a
    /// border is for:
    ///
    /// - the **border** is the hard bright line, at the strip's own 0.85;
    /// - the **halo** (wide blur, `glow_a`) is all the phosphor there is;
    /// - the **absence of fill** is why the text underneath keeps its contrast.
    ///
    /// A first pass used one soft shadow and a background wash. The wash is what
    /// made it read as a selected row in a web list rather than as a lit edge —
    /// the glow was doing half the work and the fill was undoing it.
    ///
    /// No inset shadow: an inset reads as a bevel, and a bevel claims the thing
    /// is raised, which is the opposite of what a glow says.
    pub fn ring<E: Styled + ParentElement>(&self, d: E, lit: bool, tint: Hsla, rest: Rest) -> E {
        if self.unboxed() {
            return self
                .open_control(d)
                .border_b(px(self.m.hairline))
                .border_color(if lit { tint } else { gpui::transparent_black() });
        }
        if self.is_paper() {
            let d = b_all(d.relative(), self.m.border)
                .bg(hsla(0., 0., 0., 0.))
                .border_color(hsla(0., 0., 0., 0.))
                .text_color(self.ink.ink);
            return if lit {
                d.child(crate::paper::selection_with(tint, self.stock()))
            } else {
                d
            };
        }
        let clear = hsla(0., 0., 0., 0.);
        if !lit {
            // The border is drawn at every state and merely goes transparent,
            // so switching which tab is lit never moves anything by a border
            // width. `Rest::Face` spends those same pixels on a visible edge.
            let d = b_all(d, self.m.border);
            return match rest {
                Rest::Bare => d.border_color(clear),
                Rest::Face => d.border_color(self.ink.edge_rest).bg(self.ink.face_rest),
            };
        }
        self.halo(b_all(d, self.m.border).border_color(tint.alpha(0.85)), tint)
    }

    /// The phosphor a CONTROL is allowed — one soft halo, at this skin's own
    /// `glow`/`glow_a`, and no spread.
    ///
    /// Split out of [`Skin::ring`] so that a control which is not a chip can ask
    /// for the same bloom instead of reaching for `benchdraw::aglow`, which is a
    /// REGION's bloom and four times the spread at three times the heat. That is
    /// not a hypothetical distinction: APPROVE on a decision card and the launch
    /// verb on the strip were both word-sized boxes wearing a card's halo, and
    /// the tube's own bloom pass then multiplies whatever this emits. Parker:
    /// *"about 3x or 4 to much extra!!!! dial it WAY back"*.
    pub fn halo<E: Styled>(&self, d: E, tint: Hsla) -> E {
        d.shadow(self.bloom(tint))
    }

    /// Four corner ticks around whatever the div holds. One div per corner, each
    /// carrying two borders — cheaper than eight lines and it reads as a machined
    /// bracket rather than a box.
    pub fn brackets<E: Styled + ParentElement>(&self, d: E, tint: Hsla) -> E {
        let arm = self.px(self.m.bracket);
        let w = self.m.hairline;
        let tick = || div().absolute().w(arm).h(arm);
        d.child(b_t(b_l(tick().left_0().top_0(), w), w).border_color(tint))
            .child(b_t(b_r(tick().right_0().top_0(), w), w).border_color(tint))
            .child(b_b(b_l(tick().left_0().bottom_0(), w), w).border_color(tint))
            .child(b_b(b_r(tick().right_0().bottom_0(), w), w).border_color(tint))
    }

    /// The Pip-Boy cradle under an active tab: a rule along the bottom and two
    /// short arms rising at its ends. One continuous shape, which is why it
    /// survives at tab size where four separate corner ticks did not.
    pub fn cradle<E: Styled + ParentElement>(&self, d: E, tint: Hsla) -> E {
        let w = self.m.hairline.max(1.5);
        let arm = self.px(self.m.bracket);
        d.child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(arm)
                .border_l(px(w))
                .border_r(px(w))
                .border_b(px(w))
                .border_color(tint),
        )
    }

    /// The colour of the label on the thing you are standing on. Under a bar
    /// emphasis the label sits ON the selection colour and goes dark; under
    /// every other strategy it is [`Inks::ink_lit`].
    pub fn lit_label(&self) -> Hsla {
        if self.shape.emphasis == Emphasis::Bar && !self.is_paper() {
            self.ink.ink_on_mark
        } else {
            self.ink.ink_lit
        }
    }

    /// Mark a ROW as the one you are standing on.
    ///
    /// Rows are not chips, and the difference is not decoration: a row is the
    /// full width of its panel, so a wash light enough to sit under text is too
    /// light to find at a glance in a list of twenty. Today's chrome answers that
    /// by pairing the wash with a rail down the leading edge, and `Fill` here
    /// means that pair — the wash alone was never the whole device.
    ///
    /// `Bracket` is the deco answer to the same problem and the reason the
    /// strategy is worth having: it marks without tinting, so a row whose task
    /// already carries a colour is not asked to wear two.
    pub fn active_row<E: Styled + ParentElement>(&self, d: E, active: bool) -> E {
        if self.is_paper() {
            let d = b_all(d.relative(), self.m.border)
                .bg(hsla(0., 0., 0., 0.))
                .border_color(hsla(0., 0., 0., 0.));
            return if active {
                d.child(crate::paper::selection_with(self.ink.select, self.stock()))
            } else {
                d
            };
        }
        // The ring carries it. An earlier pass added `row_active` underneath on
        // the theory that a ring alone finds a row without reading it against
        // twenty neighbours — but the fill is exactly what made the result look
        // like a selected web row instead of a lit edge, and the focused pane
        // this recipe comes from carries no fill at all. A tint is still
        // available to a skin that wants one; it is simply not the default.
        if matches!(self.shape.emphasis, Emphasis::Glow) {
            // `Rest::Bare`, and this is the whole reason that argument exists: a
            // bar of twenty rows with nineteen quiet boxes in it is a grid, and
            // a tree that looks like a grid has stopped saying which rows hang
            // off which.
            let r = self.ring(d, active, self.ink.select, Rest::Bare);
            return if active && self.ink.row_active.a > 0.02 {
                r.bg(self.ink.row_active)
            } else {
                r
            };
        }
        if !active {
            return d;
        }
        match self.shape.emphasis {
            Emphasis::Fill => {
                b_l(d.bg(self.ink.row_active), self.m.rail).border_color(self.ink.mark)
            }
            Emphasis::Rail => b_l(d, self.m.rail).border_color(self.ink.mark),
            Emphasis::Underline => b_b(d, self.m.border).border_color(self.ink.mark),
            Emphasis::Bracket => self.brackets(d, self.ink.mark),
            // FIRST child, so the row's own content paints over the bar. The
            // row keeps its padding; the bar fills its box and points right.
            Emphasis::Bar => d.relative().child(crate::deck::bar_tip(
                self.ink.mark,
                self.ink.mark.alpha(0.22),
            )),
            Emphasis::Glow => unreachable!("handled above, before the inactive early-return"),
        }
    }

    /// The chrome's main pressable control — the split buttons, new-tab, the left
    /// bar's adopt and `+`. `s` is the absolute scale the bar is running at,
    /// which the call sites already compute (some ask for `s * 0.85`).
    ///
    /// It is called a bezel because that is what it is under the default skin: a
    /// raised key with a white glint along its top-left inside edge and a shadow
    /// seated under it. Both of those are claims that the button is a physical
    /// object above the surface — and a skin that has said [`Shine::Flat`] has
    /// said the surface is a plane. So the glint and the seat are not "turned
    /// off" as a style preference; they are removed because they contradict what
    /// the rest of the skin is asserting. This is the single most visible place
    /// the shine strategy earns its keep.
    pub fn bezel(&self, active: bool, s: f32) -> Div {
        let base = div()
            .px(px(8. * s))
            .py(px(2. * s))
            .rounded(self.radius())
            .border_1()
            .text_size(px(11. * s))
            // A button's label is one line: never let a narrow flex row shrink it
            // below its content and char-wrap it into a vertical column (the War
            // Room relationship panel's From/To boxes did exactly that). A label
            // too long for its box clips; callers that expect long text add
            // `max_w` + `text_ellipsis` for a clean "…".
            .whitespace_nowrap()
            .cursor_pointer();
        if self.unboxed() {
            return self
                .open_control(base)
                .bg(gpui::transparent_black())
                .text_color(if active {
                    self.lit_label()
                } else {
                    self.ink.ink_dim
                });
        }
        if self.is_paper() {
            return self.paper_control(base, active);
        }
        match self.shape.shine {
            Shine::Gradient => {
                let glint = gpui::BoxShadow {
                    color: gpui::white().alpha(0.22),
                    offset: gpui::point(px(1.), px(1.)),
                    blur_radius: px(0.),
                    spread_radius: px(0.),
                    inset: true,
                };
                let seat = gpui::BoxShadow {
                    color: hsla(0., 0., 0., 0.55),
                    offset: gpui::point(px(2.), px(2.)),
                    blur_radius: px(3.),
                    spread_radius: px(0.),
                    inset: false,
                };
                let b = base.shadow(vec![glint, seat]);
                if active {
                    b.bg(gpui::linear_gradient(
                        135.,
                        gpui::linear_color_stop(self.ink.mark.alpha(0.42), 0.),
                        gpui::linear_color_stop(self.ink.mark.alpha(0.12), 1.),
                    ))
                    .border_color(self.ink.mark)
                    .text_color(gpui::white().alpha(0.92))
                } else {
                    b.bg(gpui::linear_gradient(
                        135.,
                        gpui::linear_color_stop(crate::brighten(self.ink.panel_raised, 1.7), 0.),
                        gpui::linear_color_stop(crate::darken(self.ink.panel_raised, 0.7), 1.),
                    ))
                    .border_color(self.ink.mark.alpha(0.4))
                    .text_color(self.ink.ink)
                }
            }
            // Flat: the edge carries the whole state, which is what a bar of
            // square buttons needs anyway — eight lit keys in a row read as a
            // texture, eight edged ones read as eight buttons.
            Shine::Flat => {
                if active {
                    base.bg(self.ink.mark_wash)
                        .border_color(self.ink.mark)
                        .text_color(self.ink.mark)
                } else {
                    base.bg(self.ink.btn_face)
                        .border_color(self.ink.edge)
                        .text_color(self.ink.ink_dim)
                }
            }
        }
    }

    /// A pressable control on a bar.
    pub fn btn(&self, active: bool) -> Div {
        let base = div()
            .flex()
            .items_center()
            .justify_center()
            .h(self.px(self.m.row_h))
            .px(self.px(self.m.chip_px))
            .rounded(self.radius())
            .cursor_pointer();
        if self.is_paper() {
            return b_all(self.paper_control(base, active), self.m.border);
        }
        if active {
            b_all(base.bg(self.ink.mark), self.m.border)
                .text_color(self.ink.ink_on_mark)
                .border_color(self.ink.mark)
        } else {
            b_all(base.bg(self.ink.btn_face), self.m.border)
                .text_color(self.ink.ink)
                .border_color(self.ink.edge)
        }
    }

    /// A well: an input, a track, anything the chrome means as "recessed".
    pub fn field(&self) -> Div {
        b_all(
            div().bg(self.ink.panel_sunken).rounded(self.radius()),
            self.m.hairline,
        )
        .border_color(self.ink.rule_strong)
    }

    /// The type treatment of a small meta label. Pair with [`Skin::caps`] for the
    /// string itself — the two are separate because the caller usually already has
    /// an owned `String` and should not be made to allocate twice.
    pub fn label(&self) -> Div {
        div()
            .font_family(self.font_family.clone())
            .text_size(self.px(self.m.label_size))
            .text_color(self.ink.ink_faint)
            .whitespace_nowrap()
    }
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(untagged)]
enum FileInk {
    /// `rule = "#1a2226"` — this look is this colour, whatever the palette is.
    Hex(String),
    /// `rule = { from = "surface", l = 0.3 }` — this look tracks the palette.
    Recipe(FileRecipe),
}

#[derive(Deserialize)]
struct FileRecipe {
    from: String,
    l: Option<f32>,
    s: Option<f32>,
    a: Option<f32>,
    max_l: Option<f32>,
    toward: Option<String>,
    t: Option<f32>,
    #[serde(flatten)]
    unknown: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct FileShape {
    controls: Option<String>,
    material: Option<String>,
    tube: Option<String>,
    corner: Option<String>,
    boundary: Option<String>,
    emphasis: Option<String>,
    divider: Option<String>,
    caps: Option<String>,
    shine: Option<String>,
    gauge: Option<String>,
    title: Option<String>,
    focus: Option<String>,
    #[serde(flatten)]
    unknown: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct SkinFile {
    font: Option<FileFont>,
    name: Option<String>,
    icon: Option<String>,
    #[serde(default)]
    ink: BTreeMap<String, FileInk>,
    #[serde(default)]
    metric: BTreeMap<String, f32>,
    shape: Option<FileShape>,
    note: Option<FileNote>,
    #[serde(flatten)]
    unknown: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct FileNote {
    decoration: Option<String>,
    stock: Option<bool>,
    #[serde(flatten)]
    unknown: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct FileFont {
    family: Option<String>,
    heading: Option<String>,
    data: Option<String>,
    scale: Option<f32>,
    min_px: Option<f32>,
    line_height: Option<f32>,
    #[serde(flatten)]
    unknown: BTreeMap<String, toml::Value>,
}

fn enum_of<T>(
    value: Option<&String>,
    table: &[(&str, T)],
    what: &str,
    unknown: &mut Vec<String>,
) -> Option<T>
where
    T: Copy,
{
    let v = value?;
    match table.iter().find(|(k, _)| k == &v.as_str()) {
        Some((_, t)) => Some(*t),
        None => {
            unknown.push(format!("{what} = \"{v}\""));
            None
        }
    }
}

/// The known key nearest to `typo`, when one is near enough to be worth naming.
///
/// A bare "no such token" tells an author their file is wrong and nothing about
/// how; `rule_stong (no such token — did you mean rule_strong?)` ends the
/// problem on the line where it is read. Levenshtein with a distance cap of 3,
/// which on a fixed twenty-five-key vocabulary is exact enough and cheap enough
/// to run only on the failure path.
fn nearest(typo: &str, keys: &[&'static str]) -> Option<&'static str> {
    fn distance(a: &str, b: &str) -> usize {
        let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
        let mut prev: Vec<usize> = (0..=b.len()).collect();
        let mut cur = vec![0usize; b.len() + 1];
        for (i, ca) in a.iter().enumerate() {
            cur[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let sub = prev[j] + usize::from(ca != cb);
                cur[j + 1] = sub.min(prev[j + 1] + 1).min(cur[j] + 1);
            }
            std::mem::swap(&mut prev, &mut cur);
        }
        prev[b.len()]
    }
    keys.iter()
        .map(|k| (distance(typo, k), *k))
        .filter(|(d, _)| *d <= 3)
        .min_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

fn no_such(kind: &str, key: &str, keys: &[&'static str]) -> String {
    match nearest(key, keys) {
        Some(near) => format!("{kind}.{key} (no such token — did you mean {near}?)"),
        None => format!("{kind}.{key} (no such token)"),
    }
}

/// Parse a skin file. Never fails on an unknown token — it collects them, so a
/// typo shows up as a reported name instead of a look that silently did nothing.
pub fn parse(source: &str) -> Result<SkinSpec, String> {
    let f: SkinFile = toml::from_str(source).map_err(|e| e.to_string())?;
    let mut unknown = f
        .unknown
        .keys()
        .map(|k| format!("{k} (unknown section or key)"))
        .collect::<Vec<_>>();
    if let Some(font) = &f.font {
        unknown.extend(
            font.unknown
                .keys()
                .map(|k| format!("font.{k} (unknown key)")),
        );
    }
    if let Some(shape) = &f.shape {
        unknown.extend(
            shape
                .unknown
                .keys()
                .map(|k| format!("shape.{k} (unknown key)")),
        );
    }
    if let Some(note) = &f.note {
        unknown.extend(
            note.unknown
                .keys()
                .map(|k| format!("note.{k} (unknown key)")),
        );
    }
    let named = |v: Option<String>| v.filter(|name| !name.trim().is_empty());
    let (family, heading, data, scale, min_px, line_height) = match f.font {
        Some(font) => (
            named(font.family),
            named(font.heading),
            named(font.data),
            font.scale,
            font.min_px,
            font.line_height,
        ),
        None => (None, None, None, None, None, None),
    };
    for (key, value, lo, hi) in [
        ("scale", scale, 0.5, 3.),
        ("min_px", min_px, 0., 32.),
        ("line_height", line_height, 0.8, 2.),
    ] {
        if value.is_some_and(|v| !v.is_finite() || v < lo || v > hi) {
            return Err(format!(
                "font.{key} must be finite and between {lo} and {hi}"
            ));
        }
    }
    let mut spec = SkinSpec {
        font_family: family,
        font_heading: heading,
        font_data: data,
        font_scale: scale,
        font_min_px: min_px,
        font_line_height: line_height,
        name: f.name.unwrap_or_else(|| "unnamed".into()),
        icon: f.icon.unwrap_or_else(|| "▢".into()),
        unknown,
        ..SkinSpec::default()
    };

    if let Some(note) = f.note {
        spec.note_stock = note.stock;
        spec.note_decoration = enum_of(
            note.decoration.as_ref(),
            &[
                ("plain", NoteDecoration::Plain),
                ("flower", NoteDecoration::Flower),
            ],
            "note.decoration",
            &mut spec.unknown,
        );
    }

    for (key, value) in &f.ink {
        if let FileInk::Recipe(recipe) = value {
            if [recipe.l, recipe.s, recipe.a, recipe.max_l, recipe.t]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite())
            {
                return Err(format!("ink.{key} recipe values must be finite"));
            }
            spec.unknown.extend(
                recipe
                    .unknown
                    .keys()
                    .map(|k| format!("ink.{key}.{k} (unknown key)")),
            );
        }
        let parsed = match value {
            FileInk::Hex(h) => match theme::parse_hex(h) {
                Some(c) => Some(InkSpec::Pinned(c)),
                None => {
                    spec.unknown
                        .push(format!("ink.{key} = \"{h}\" (not a colour)"));
                    None
                }
            },
            FileInk::Recipe(r) => match Role::parse(&r.from) {
                None => {
                    spec.unknown.push(format!(
                        "ink.{key}.from = \"{}\" (not a palette role)",
                        r.from
                    ));
                    None
                }
                Some(from) => {
                    let mut rec = Recipe::of(from);
                    if let Some(v) = r.l {
                        rec = rec.l(v);
                    }
                    if let Some(v) = r.s {
                        rec = rec.s(v);
                    }
                    if let Some(v) = r.a {
                        rec = rec.a(v);
                    }
                    if let Some(v) = r.max_l {
                        rec = rec.max_l(v);
                    }
                    if let Some(name) = &r.toward {
                        match Role::parse(name) {
                            Some(role) => rec = rec.toward(role, r.t.unwrap_or(0.5)),
                            None => spec.unknown.push(format!(
                                "ink.{key}.toward = \"{name}\" (not a palette role)"
                            )),
                        }
                    }
                    Some(InkSpec::Mixed(rec))
                }
            },
        };
        if let Some(p) = parsed {
            if !spec.ink.set(key, p) {
                spec.unknown.push(no_such("ink", key, INK_KEYS));
            }
        }
    }

    for (key, value) in &f.metric {
        if !value.is_finite() {
            return Err(format!("metric.{key} must be finite"));
        }
        let max = match key.as_str() {
            "grain" | "roughness" | "marker_alpha" => Some(1.),
            "lift" => Some(2.),
            "frost" => Some(12.),
            _ => None,
        };
        if max.is_some_and(|max| *value < 0. || *value > max) {
            return Err(format!("metric.{key} is outside its material range"));
        }
        if !spec.metric.set(key, *value) {
            spec.unknown.push(no_such("metric", key, METRIC_KEYS));
        }
    }

    if let Some(sh) = &f.shape {
        let u = &mut spec.unknown;
        spec.shape.tube = enum_of(
            sh.tube.as_ref(),
            &[("glass", Tube::Glass), ("stock", Tube::Stock)],
            "shape.tube",
            u,
        );
        spec.shape.material = enum_of(
            sh.material.as_ref(),
            &[
                ("plain", Material::Plain),
                ("paper", Material::Paper),
                ("grid", Material::Grid),
            ],
            "shape.material",
            u,
        );
        spec.shape.corner = enum_of(
            sh.corner.as_ref(),
            &[
                ("round", Corner::Round),
                ("square", Corner::Square),
                ("cut", Corner::Cut),
            ],
            "shape.corner",
            u,
        );
        spec.shape.boundary = enum_of(
            sh.boundary.as_ref(),
            &[
                ("hairline", Boundary::Hairline),
                ("double", Boundary::Double),
                ("inset", Boundary::Inset),
                ("none", Boundary::None),
            ],
            "shape.boundary",
            u,
        );
        spec.shape.emphasis = enum_of(
            sh.emphasis.as_ref(),
            &[
                ("fill", Emphasis::Fill),
                ("underline", Emphasis::Underline),
                ("bracket", Emphasis::Bracket),
                ("bar", Emphasis::Bar),
                ("rail", Emphasis::Rail),
                ("glow", Emphasis::Glow),
            ],
            "shape.emphasis",
            u,
        );
        spec.shape.divider = enum_of(
            sh.divider.as_ref(),
            &[
                ("line", Divider::Line),
                ("double", Divider::Double),
                ("none", Divider::None),
            ],
            "shape.divider",
            u,
        );
        spec.shape.caps = enum_of(
            sh.caps.as_ref(),
            &[
                ("off", Caps::Off),
                ("upper", Caps::Upper),
                ("tracked", Caps::Tracked),
            ],
            "shape.caps",
            u,
        );
        spec.shape.shine = enum_of(
            sh.shine.as_ref(),
            &[("gradient", Shine::Gradient), ("flat", Shine::Flat)],
            "shape.shine",
            u,
        );
        spec.shape.gauge = enum_of(
            sh.gauge.as_ref(),
            &[("rail", Gauge::Rail), ("pips", Gauge::Pips)],
            "shape.gauge",
            u,
        );
        spec.shape.title = enum_of(
            sh.title.as_ref(),
            &[("plain", Title::Plain), ("chevron", Title::Chevron)],
            "shape.title",
            u,
        );
        spec.shape.focus = enum_of(
            sh.focus.as_ref(),
            &[("plain", Focus::Plain), ("hatch", Focus::Hatch)],
            "shape.focus",
            u,
        );
        spec.shape.controls = enum_of(
            sh.controls.as_ref(),
            &[("icons", Controls::Icons), ("text", Controls::Text)],
            "shape.controls",
            u,
        );
    }

    Ok(spec)
}

// ---------------------------------------------------------------------------
// Globals, resolution and hot reload — mirroring theme.rs exactly
// ---------------------------------------------------------------------------

pub struct SkinRegistry {
    pub builtins: Vec<(String, Arc<SkinSpec>)>,
    /// The user's own hot-reloaded file.
    pub custom: Arc<SkinSpec>,
    /// Which builtin `$TD_SKIN`/the user file resolved to, when it named one.
    pub active_id: String,
}
impl Global for SkinRegistry {}

/// `$TD_SKIN` if set, else `~/.config/terminal-delight/skin.toml`.
pub fn skin_path() -> PathBuf {
    if let Ok(p) = std::env::var("TD_SKIN") {
        return PathBuf::from(p);
    }
    crate::instance::config_dir().join("skin.toml")
}

fn mtime(path: &PathBuf) -> Option<SystemTime> {
    fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// What `active_id` means when nothing has been chosen: follow whatever the
/// active THEME asks for, and the default when it asks for nothing. Spelled as a
/// named constant because "theme" is a *state*, not a skin id, and reading it as
/// one is the mistake waiting to be made.
pub const FOLLOW_THEME: &str = "theme";

/// The user's own hot-reloaded file, as an `active_id`.
pub const USER_FILE: &str = "custom";

/// The spec a region should draw with.
///
/// Resolution, most specific first: an explicitly selected skin (`ctl skin deco`)
/// — then the user's own file if that is what is selected — then the builtin the
/// active THEME asked for (`skin = "deco"` at the top of a theme file), then the
/// default.
///
/// A theme naming a skin is how one click changes both axes at once without a
/// picker in the tray. An explicit selection overrides that, and stays overridden
/// until it is set back to [`FOLLOW_THEME`] — because a person who has just said
/// "use deco" did not mean "use deco until I change theme".
pub fn spec(cx: &App) -> Arc<SkinSpec> {
    let reg = cx.global::<SkinRegistry>();
    if reg.active_id == USER_FILE {
        return reg.custom.clone();
    }
    let by_id = |id: &str| {
        reg.builtins
            .iter()
            .find(|(k, _)| k == canonical_id(id))
            .map(|(_, s)| s.clone())
    };
    if reg.active_id != FOLLOW_THEME {
        if let Some(s) = by_id(&reg.active_id) {
            return s;
        }
    }
    theme::theme(cx)
        .skin
        .as_deref()
        .and_then(by_id)
        .or_else(|| by_id("default"))
        .unwrap_or_else(|| Arc::new(SkinSpec::default()))
}

/// The most buttons the SKIN picker can draw: every builtin, plus the `custom`
/// slot, which the picker shows only while the user's skin file is drawing.
///
/// The WORST case on purpose — the tray has to fit when that file exists, and a
/// geometry test that assumed it did not would pass on this machine and fail on
/// the one where somebody wrote a skin.
pub fn picker_max_count() -> usize {
    BUILTIN_SKINS.len() + 1
}

/// Every skin that can be selected, as `(id, icon, whether it is active now)`.
/// `custom` appears only when the user actually has a file.
pub fn all_skins(cx: &App) -> Vec<(String, String, bool)> {
    let reg = cx.global::<SkinRegistry>();
    let active = active_id(cx);
    let mut out: Vec<(String, String, bool)> = reg
        .builtins
        .iter()
        .map(|(id, s)| (id.clone(), s.icon.clone(), *id == active))
        .collect();
    if skin_path().exists() {
        out.push((
            USER_FILE.to_string(),
            reg.custom.icon.clone(),
            active == USER_FILE,
        ));
    }
    out
}

/// Whether a skin was CHOSEN, as opposed to inherited from the theme.
///
/// The distinction is not pedantry: a window that is following its theme and a
/// window pinned to the skin that theme happens to name look identical and behave
/// differently the moment the theme changes. It is what decides whether a "back to
/// the theme" control has anything to undo.
pub fn is_pinned(cx: &App) -> bool {
    cx.global::<SkinRegistry>().active_id != FOLLOW_THEME
}

/// Which skin is actually drawing — resolving `theme` to the id it follows, so a
/// status line never answers a question with the word "theme".
pub fn active_id(cx: &App) -> String {
    let reg = cx.global::<SkinRegistry>();
    if reg.active_id != FOLLOW_THEME {
        return canonical_id(&reg.active_id).to_string();
    }
    theme::theme(cx)
        .skin
        .clone()
        .unwrap_or_else(|| "default".to_string())
}

/// Choose a skin for the running window. `id` is a builtin, `custom` for the
/// user's own file, or [`FOLLOW_THEME`] to go back to whatever the theme wants.
///
/// Returns the error text for an id that names nothing, rather than silently
/// falling back — a switch that appears to work and does not is the single most
/// confusing thing a theming system can do, and the whole layer is built around
/// refusing to do it.
pub fn select(cx: &mut App, id: &str) -> Result<String, String> {
    let id = canonical_id(id);
    let known: Vec<String> = all_skins(cx).into_iter().map(|(k, _, _)| k).collect();
    if id != FOLLOW_THEME && !known.iter().any(|k| k == id) {
        return Err(format!(
            "no skin {id:?} — have: {}, {FOLLOW_THEME}",
            known.join(", ")
        ));
    }
    if id == USER_FILE && !skin_path().exists() {
        return Err(format!(
            "no user skin at {} — copy a builtin there first",
            skin_path().display()
        ));
    }
    cx.global_mut::<SkinRegistry>().active_id = id.to_string();
    cx.refresh_windows();
    Ok(active_id(cx))
}

/// Hold the skin that is drawing now, so a change of DESIGN cannot move it.
///
/// A window that never picked a skin draws the one its design asks for
/// (`skin = "ops"` at the top of a design file), and a skin that follows the
/// design changes when the design does. Parker, 2026-10-25, after Chalk turned
/// into OPS when he picked Hacker: *"These are intended to move independently."*
/// Every path that changes the window's design calls this first, so the two
/// rows stay independent for a window that never touched the SKIN row as well
/// as for one that did. A skin already chosen is left exactly as it is.
pub fn hold(cx: &mut App) {
    if is_pinned(cx) {
        return;
    }
    let drawing = active_id(cx);
    if let Err(e) = select(cx, &drawing) {
        eprintln!("terminal-delight: {e}");
    }
}

/// The skin a chrome region draws with: the active spec, baked against the
/// active palette at the window's scale. Call once per region, not per element.
pub fn skin(cx: &App, scale: f32) -> Skin {
    let th = theme::theme(cx);
    spec(cx).bake(&th, scale)
}

/// Native furniture only. A grid must keep `theme::theme` / its pane's resolved
/// Design; projecting chrome never mutates the registry or a saved palette.
pub fn native_theme(cx: &App, th: Arc<Theme>) -> Arc<Theme> {
    let spec = spec(cx);
    if spec.shape.controls == Some(Controls::Text) {
        Arc::new(spec.bake(&th, 1.).chrome_theme(&th))
    } else {
        th
    }
}

/// The skin for chrome that belongs to a PANE rather than to the window — a pane
/// header, a pane frame. Panes carry their own palette, so baking against the
/// window's would paint a green pane's header in the workspace's brass. Shape is
/// still window-global: only the inks differ.
pub fn for_theme(cx: &App, th: &Theme, scale: f32) -> Skin {
    spec(cx).bake(th, scale)
}

/// On-demand read-back of the live registry, not a second parse of the file.
/// This lets an author distinguish a saved edit from an applied edit.
pub(crate) fn snapshot(cx: &App) -> String {
    let th = theme::theme(cx);
    let choice = theme::outer_choice(cx);
    let sk = for_theme(cx, &th, th.grade.scale);
    let mut value: serde_json::Value =
        serde_json::from_str(&sk.to_json(&choice.id)).expect("resolved skin");
    value["active_skin"] = serde_json::json!(active_id(cx));
    value["scope"] = serde_json::json!("outer");
    value["palette"] = serde_json::json!(choice.palette);
    value["custom_path"] = serde_json::json!(skin_path());
    value.to_string()
}

/// Load the skin, start the hot-reload watcher. No first-run seed: an absent
/// `skin.toml` means "use the theme's skin", which is the right answer for
/// everyone who has never heard of this file.
pub fn init(cx: &mut App) {
    let path = skin_path();
    let builtins: Vec<(String, Arc<SkinSpec>)> = BUILTIN_SKINS
        .iter()
        .map(|(id, src)| {
            (
                (*id).to_string(),
                Arc::new(parse(src).expect("embedded skin parses")),
            )
        })
        .collect();
    let (custom, active_id) = match fs::read_to_string(&path).ok().map(|s| parse(&s)) {
        Some(Ok(spec)) => {
            report_unknown(&path, &spec);
            // The one positive witness this layer has. A skin is data, it is
            // applied silently, and the chrome cannot be screenshotted from a
            // shell — so without this line "the skin I asked for is live" is an
            // inference from the absence of errors, which is not evidence of
            // anything. Printed only when a file was actually read, so the
            // default launch stays quiet.
            eprintln!(
                "terminal-delight: skin \"{}\" from {} ({:?} corners, {:?} boundary, {:?} emphasis)",
                spec.name,
                path.display(),
                spec.shape.corner.unwrap_or_default(),
                spec.shape.boundary.unwrap_or_default(),
                spec.shape.emphasis.unwrap_or_default(),
            );
            (Arc::new(spec), USER_FILE.to_string())
        }
        Some(Err(err)) => {
            eprintln!("skin {}: {err} (using the theme's skin)", path.display());
            (Arc::new(SkinSpec::default()), FOLLOW_THEME.to_string())
        }
        None => (Arc::new(SkinSpec::default()), FOLLOW_THEME.to_string()),
    };
    cx.set_global(SkinRegistry {
        builtins,
        custom,
        active_id,
    });

    let mut last = mtime(&path);
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(300))
            .await;
        let now = mtime(&path);
        if now != last {
            last = now;
            match fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| parse(&s))
            {
                Ok(spec) => {
                    report_unknown(&path, &spec);
                    cx.update(|cx| {
                        let reg = cx.global_mut::<SkinRegistry>();
                        reg.custom = Arc::new(spec);
                        // Adopt the file only when nobody has chosen a skin.
                        // Saving `skin.toml` must not yank the window off a skin
                        // that was selected explicitly — "use deco" does not mean
                        // "use deco until I touch an unrelated file", and a
                        // selection that can be revoked by a background poller is
                        // not a selection.
                        if reg.active_id == FOLLOW_THEME {
                            reg.active_id = USER_FILE.to_string();
                        }
                        cx.refresh_windows();
                    });
                }
                // Keeping the skin we have is deliberate: an editor writing a
                // file in two syscalls would otherwise flash the default look
                // through every save.
                Err(err) => eprintln!("skin reload error (keeping current): {err}"),
            }
        }
    })
    .detach();
}

/// What [`init`] installs on a machine with no `skin.toml` — follow the
/// theme's skin — from the builtins alone, with no file read and no watcher.
/// For a test that builds a real pane.
#[cfg(test)]
pub(crate) fn init_embedded(cx: &mut App) {
    let builtins = BUILTIN_SKINS
        .iter()
        .map(|(id, src)| {
            (
                (*id).to_string(),
                Arc::new(parse(src).expect("embedded skin parses")),
            )
        })
        .collect();
    cx.set_global(SkinRegistry {
        builtins,
        custom: Arc::new(SkinSpec::default()),
        active_id: FOLLOW_THEME.to_string(),
    });
}

// ---------------------------------------------------------------------------
// The `skin` verb — resolve headlessly and print what came out
// ---------------------------------------------------------------------------

/// The embedded TOML of one builtin skin, by id.
/// Skins that were offered once and have since been retired, read back as the
/// skin that replaced them — a saved selection, a `ctl skin` or a new-window
/// template naming one still lands on a skin that exists. Console was retired
/// on 2026-10-25 ("remove the console skin") and reads back as OPS, the 0.5.0
/// default and the other flat, upper-case trim.
fn canonical_id(id: &str) -> &str {
    match id {
        "paper" => "chalk",
        "console" => "ops",
        _ => id,
    }
}

pub fn builtin_toml(id: &str) -> Option<&'static str> {
    BUILTIN_SKINS
        .iter()
        .find(|(k, _)| *k == canonical_id(id))
        .map(|(_, src)| *src)
}

/// A builtin id, or a path to a file. Ids win, which is why the builtins are
/// short words and a file has to be spelled with a separator in it anyway.
fn source_of(arg: &str, builtin: impl Fn(&str) -> Option<&'static str>) -> Result<String, String> {
    if let Some(src) = builtin(arg) {
        return Ok(src.to_string());
    }
    fs::read_to_string(arg).map_err(|e| format!("{arg}: {e}"))
}

const SKIN_USAGE: &str = "usage: terminal-delight skin [--list] [--skin <id|path>] \
     [--theme <id|path>] [--palette <id|path>] [--scale <n>] [--text-size <n>]\n\
\n\
  --list        the skins this build carries, and every way one gets chosen\n\
  --catalogue   versioned native surface inventory, as JSON\n\
  --catalogue-md  the same inventory as a human table\n\
  --atlas-script  print this build's isolated native preview runner\n\
  --template <id>  print a bundled skin file to start a custom look\n\
  --check       refuse unknown tokens and invalid authoring values\n\
  (default)     resolve a skin against a palette and print every token, as JSON\n\
\n\
  --scale       the MENU-BAR dial (0.7..1.6, neutral 1.0) — sizes chrome\n\
  --text-size   the TEXT-SIZE dial (0.6..2.0, neutral 1.0) — sizes the type ramp\n\
                the workbench reads. This is the dial's own factor, NOT the\n\
                percent the tray draws: a tray reading 65% is --text-size 1.51.\n\
\n\
To restyle a RUNNING window — no restart, no file editing:\n\
  terminal-delight ctl skin <name>     switch it now\n\
  terminal-delight ctl skin theme      go back to following the theme\n\
  terminal-delight ctl skin status     what is active, and what is available\n\
  terminal-delight ctl skin snapshot   resolved live UI tokens, as JSON";

/// `--list`. Answers "what skins are there, and how do I pick one" in one screen,
/// because that is the first question anybody asks a theming system and the
/// answer was previously spread across a module doc, an env var and a TOML key.
fn list_skins() -> i32 {
    println!("skins this build carries:\n");
    for (id, src) in BUILTIN_SKINS {
        match parse(src) {
            Ok(s) => {
                let sh = s.shape;
                println!(
                    "  {:<9} {}  {} corners, {} boundary, {} emphasis, {} dividers, {} caps, {}",
                    id,
                    s.icon,
                    word(sh.corner.map(|v| format!("{v:?}"))),
                    word(sh.boundary.map(|v| format!("{v:?}"))),
                    word(sh.emphasis.map(|v| format!("{v:?}"))),
                    word(sh.divider.map(|v| format!("{v:?}"))),
                    word(sh.caps.map(|v| format!("{v:?}"))),
                    word(sh.shine.map(|v| format!("{v:?}"))),
                );
            }
            Err(e) => println!("  {id:<9} !! {e}"),
        }
    }
    let p = skin_path();
    println!(
        "\n  {:<9} ✎  {}",
        USER_FILE,
        if p.exists() {
            format!("your own file, hot-reloaded: {}", p.display())
        } else {
            format!("not present — create {} to get one", p.display())
        }
    );
    // Printed line by line rather than as one continued literal: a `\` at the end
    // of a Rust string literal eats the following newline AND its indentation, so
    // a block written that way silently loses the alignment that makes it a table.
    println!("\nhow one gets chosen, most specific first:\n");
    for line in [
        "  1. terminal-delight ctl skin <name>   an explicit choice, applied to the",
        "                                        running window now, and kept until",
        "                                        you say `ctl skin theme`",
        "  2. $TD_SKIN=<path>                    a file, read at launch",
        "  3. your own skin.toml                 hot-reloaded on save (path above)",
        "  4. skin = \"<name>\" in a theme file    the skin a window starts in; picking",
        "                                        another design later keeps it",
        "  5. default                            the house look — round, lit, ringed",
    ] {
        println!("{line}");
    }
    println!(
        "\nShape and colour are separate axes on purpose: every skin works on every\n\
         palette. `terminal-delight skin --skin deco --theme hacker` shows what that\n\
         means, and `--skin chalk --theme deco` is the same palette wearing a\n\
         different look."
    );
    0
}

/// A strategy the file did not state is shown as its default rather than as a
/// blank — absent is not nothing, it is the default recipe, and a listing that
/// prints an empty column teaches the opposite.
fn word(v: Option<String>) -> String {
    v.unwrap_or_else(|| "default".into()).to_lowercase()
}

/// `terminal-delight skin` — the headless resolver. Exists so a skin can be read
/// back: every token, as the running app would compute it, without a window.
pub fn run_cli(args: &[String]) -> i32 {
    if args.len() == 1 {
        match args[0].as_str() {
            "--catalogue" => {
                println!("{}", catalogue::SOURCE);
                return 0;
            }
            "--catalogue-md" => {
                print!("{}", catalogue::markdown());
                return 0;
            }
            "--atlas-script" => {
                print!("{}", include_str!("../../scripts/skin-atlas.py"));
                return 0;
            }
            _ => {}
        }
    }
    if args.len() == 2 && args[0] == "--template" {
        return match builtin_toml(&args[1]) {
            Some(source) => {
                print!("{source}");
                0
            }
            None => usage_err("--template wants a bundled skin id"),
        };
    }
    let mut strict = false;
    let mut skin_arg = "default".to_string();
    let mut theme_arg = "hacker".to_string();
    let mut palette_arg = None;
    let mut scale = 1.0f32;
    // Neutral, not zero: an unspecified gauge means "the ramp as authored",
    // and a bench resolved at 0pt would print a table of zeroes that looks
    // exactly like a real answer.
    let mut text_size = 1.0f32;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--check" => strict = true,
            "--skin" => match it.next() {
                Some(v) => skin_arg = v.clone(),
                None => return usage_err("--skin wants a value"),
            },
            "--theme" => match it.next() {
                Some(v) => theme_arg = v.clone(),
                None => return usage_err("--theme wants a value"),
            },
            "--palette" => match it.next() {
                Some(v) => palette_arg = Some(v.clone()),
                None => {
                    return usage_err(
                        "--palette wants a bundled id, TD palette or Omarchy colors.toml path",
                    )
                }
            },
            "--scale" => match it.next().and_then(|v| v.parse().ok()) {
                Some(v) => scale = v,
                None => return usage_err("--scale wants a number"),
            },
            // The dial's own units, NOT the tray's percent. `--text-size 1.51`
            // is what a tray reading 65% hands over; passing 0.65 here asks a
            // different and much smaller question, which is the mistake this
            // whole readout exists to make visible.
            "--text-size" => match it.next().and_then(|v| v.parse().ok()) {
                Some(v) => text_size = v,
                None => return usage_err("--text-size wants a number"),
            },
            "--list" | "-l" => return list_skins(),
            "-h" | "--help" => {
                println!("{SKIN_USAGE}");
                return 0;
            }
            other => return usage_err(&format!("unrecognised option `{other}`")),
        }
    }

    if !scale.is_finite() || scale <= 0. || !text_size.is_finite() || text_size <= 0. {
        return usage_err("scale and text-size must be finite and positive");
    }
    let mut th = match source_of(&theme_arg, theme::builtin_toml).and_then(|s| theme::parse(&s)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("terminal-delight skin: theme {e}");
            return 1;
        }
    };
    if let Some(arg) = &palette_arg {
        let source = crate::palette::bundled_toml(arg)
            .map(Ok)
            .unwrap_or_else(|| fs::read_to_string(arg).map_err(|e| e.to_string()));
        th = match source.and_then(|s| {
            theme::overlay_palette(&th, &s).or_else(|error| {
                crate::palette::from_source("skin-preview", &s)
                    .ok_or(error)
                    .and_then(|p| theme::overlay_palette(&th, &p.toml))
            })
        }) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("terminal-delight skin: palette {arg}: {e}");
                return 1;
            }
        };
    }
    let spec = match source_of(&skin_arg, builtin_toml).and_then(|s| parse(&s)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("terminal-delight skin: {e}");
            return 1;
        }
    };
    // Unknown tokens go to stderr, so a caller piping stdout into a file still
    // gets valid JSON and a person still gets told their file has a typo in it.
    for u in &spec.unknown {
        eprintln!("terminal-delight skin: unknown {u}");
    }
    if strict && !spec.unknown.is_empty() {
        return 1;
    }

    let sk = spec.bake(&th, scale).with_type(text_size);
    let mut output: serde_json::Value =
        serde_json::from_str(&sk.to_json(&theme_arg)).expect("resolved skin");
    output["palette"] = serde_json::json!(palette_arg);
    println!(
        "{}",
        serde_json::to_string_pretty(&output).expect("resolved skin")
    );
    0
}

fn usage_err(msg: &str) -> i32 {
    eprintln!("terminal-delight skin: {msg}\n\n{SKIN_USAGE}");
    2
}

impl Skin {
    /// Read back the same resolved values the native renderers use. Strings
    /// are serialized too: an author's quoted name must remain valid JSON.
    fn to_json(&self, theme_id: &str) -> String {
        let inks: serde_json::Map<String, serde_json::Value> = INK_KEYS
            .iter()
            .map(|key| {
                let c = self.ink.by_key(key).expect("registered ink");
                (
                    key.to_string(),
                    serde_json::json!({"hex": crate::hsla_to_hex(c), "a": c.a}),
                )
            })
            .collect();
        let metrics: serde_json::Map<String, serde_json::Value> = METRIC_KEYS
            .iter()
            .map(|key| {
                (
                    key.to_string(),
                    serde_json::json!(self.m.by_key(key).expect("registered metric")),
                )
            })
            .collect();
        let ty: serde_json::Map<String, serde_json::Value> = crate::workbench::Step::ALL
            .iter()
            .map(|step| {
                (
                    format!("{step:?}").to_lowercase(),
                    serde_json::json!({"base": step.base(), "pt": self.pt(*step)}),
                )
            })
            .collect();
        serde_json::to_string_pretty(&serde_json::json!({
            "skin": self.name, "theme": theme_id, "ui_font": self.font_family,
            "heading_font": self.heading_family, "data_font": self.data_family,
            "optical": { "scale": self.type_scale, "min_px": self.type_min_px, "line_height": self.line_height },
            "note": { "decoration": format!("{:?}", self.note_decoration), "stock": self.note_stock },
            "material": format!("{:?}", self.shape.material), "scale": self.scale, "text_size": self.ty.k(),
            "shape": { "controls": format!("{:?}", self.shape.controls), "corner": format!("{:?}", self.shape.corner), "boundary": format!("{:?}", self.shape.boundary), "emphasis": format!("{:?}", self.shape.emphasis), "divider": format!("{:?}", self.shape.divider), "caps": format!("{:?}", self.shape.caps), "shine": format!("{:?}", self.shape.shine), "gauge": format!("{:?}", self.shape.gauge), "title": format!("{:?}", self.shape.title), "focus": format!("{:?}", self.shape.focus), "tube": format!("{:?}", self.shape.tube) },
            "ink": inks, "metric": metrics, "type": ty
        })).expect("resolved skin JSON")
    }
}

fn report_unknown(path: &std::path::Path, spec: &SkinSpec) {
    for u in &spec.unknown {
        eprintln!("skin {}: unknown {u}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminimalist_follows_the_palette_and_only_overrides_the_default_field() {
        let spec = parse(builtin_toml("terminimalist").unwrap()).unwrap();
        for id in ["osaka-jade", "tokyo-night", "flexoki-light"] {
            let source = crate::palette::bundled_toml(id).unwrap();
            let mut th = theme::overlay_palette(&palette(), &source).unwrap();
            // Deliberately non-neutral: the field endpoint must still match.
            th.grade = theme::house_outer().grade;
            let sk = spec.bake(&th, 1.);
            assert_eq!(sk.ink.panel, th.bg, "{id}");
            assert_eq!(sk.ink.ink, th.text, "{id}");
            assert_eq!(sk.ink.mark, th.accent, "{id}");
            let matching = sk.terminal_theme(th.clone());
            assert_eq!(matching.field_background, Some(sk.ink.panel));
            assert_eq!(matching.ansi, th.ansi);
            assert_eq!(matching.grade, th.grade);
            th.grade.set(theme::GradeKey::FieldColour, 0.);
            let black = sk.terminal_theme(th.clone()).field_background.unwrap();
            assert_eq!(black.l, 0.);
            th.grade.set(theme::GradeKey::FieldColour, 0.5);
            let mid = sk.terminal_theme(th.clone()).field_background.unwrap();
            assert!((mid.l - sk.ink.panel.l * 0.5).abs() < 1e-6);
            assert!((mid.s - sk.ink.panel.s * 0.5).abs() < 1e-6);
            assert_eq!(mid.h, sk.ink.panel.h);
            th.grade.set(theme::GradeKey::FieldColour, 1.);
            assert_eq!(
                sk.terminal_theme(th.clone()).field_background,
                Some(sk.ink.panel)
            );
        }
        for (id, source) in BUILTIN_SKINS {
            if *id == "terminimalist" {
                continue;
            }
            let th = palette();
            let sk = parse(source).unwrap().bake(&th, 1.);
            assert_eq!(sk.terminal_theme(th.clone()).field_background, None, "{id}");
            let mut chosen = th;
            chosen.grade.set(theme::GradeKey::FieldColour, 1.);
            assert_eq!(
                sk.terminal_theme(chosen).field_background,
                Some(sk.ink.panel),
                "{id}"
            );
        }
    }

    #[test]
    fn flat_marked_notes_keep_the_theme_ink_and_take_its_complement() {
        let spec = parse(builtin_toml("terminimalist").unwrap()).unwrap();
        for id in ["osaka-jade", "tokyo-night", "flexoki-light"] {
            let th = theme::overlay_palette(&palette(), &crate::palette::bundled_toml(id).unwrap())
                .unwrap();
            let sk = spec.bake(&th, 1.);
            let quiet = sk.marked_note_palette(&th, false);
            let marked = sk.marked_note_palette(&th, true);
            assert_ne!(quiet.base, marked.base, "{id}");
            assert_eq!(marked.ink, sk.ink.ink);
            assert_eq!(marked.pin.h, th.complement.h);
            assert_eq!(marked.base.h, th.complement.h);
        }
    }

    #[test]
    fn optical_type_is_independent_of_the_substrate() {
        let th = palette();
        let font =
            "[font]\nfamily = 'Share Tech Mono'\nscale = 1.2\nmin_px = 0\nline_height = 1.3\n";
        let plain = parse(&format!("{font}[shape]\nmaterial = 'plain'\n"))
            .unwrap()
            .bake(&th, 1.);
        let paper = parse(&format!("{font}[shape]\nmaterial = 'paper'\n"))
            .unwrap()
            .bake(&th, 1.);
        assert_eq!(plain.text_px(10.), paper.text_px(10.));
        assert_eq!(
            plain.pt(crate::workbench::Step::Body),
            paper.pt(crate::workbench::Step::Body)
        );
        assert_eq!(paper.type_min_px, 0.);
        assert_eq!(paper.line_height, 1.3);
        let mono = parse("[font]\nfamily = 'Share Tech Mono'\n[shape]\nmaterial = 'paper'")
            .unwrap()
            .bake(&th, 1.);
        assert_eq!(mono.text_px(10.), px(10.));
    }

    #[test]
    fn nested_typos_and_non_finite_authoring_values_are_reported() {
        let spec = parse("[font]\nscsale = 1.5\n[shape]\nmatreial = 'paper'\n[note]\ndecroation = 'flower'\n[ink]\npanel = { from = 'bg', typo = 1 }\n[typo]\nx = 1").unwrap();
        for key in [
            "font.scsale",
            "shape.matreial",
            "note.decroation",
            "ink.panel.typo",
            "typo",
        ] {
            assert!(spec.unknown.iter().any(|v| v.contains(key)), "lost {key}");
        }
        for bad in [
            "[font]\nscale = nan",
            "[font]\nmin_px = -1",
            "[metric]\ngrain = inf",
            "[metric]\nlift = -0.1",
            "[metric]\nmarker_alpha = 1.1",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad}");
        }
    }

    #[test]
    fn readback_preserves_quoted_names_and_separate_policies() {
        let sk = parse("name = 'My \"studio\"'\n[font]\nscale = 1.2\n[shape]\nmaterial = 'grid'\ntube = 'stock'\n[note]\ndecoration = 'flower'\nstock = false").unwrap().bake(&palette(), 1.);
        let v: serde_json::Value = serde_json::from_str(&sk.to_json("palette\"file")).unwrap();
        assert_eq!(v["skin"], "My \"studio\"");
        assert_eq!(v["theme"], "palette\"file");
        assert_eq!(v["shape"]["tube"], "Stock");
        assert_eq!(v["note"]["stock"], false);
        assert!(v["type"]["body"]["base"].is_number());
        assert!(!sk.glass());
    }

    #[test]
    fn material_rendering_stays_inside_the_skin_boundary() {
        fn visit(path: &std::path::Path) {
            for entry in std::fs::read_dir(path).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(&path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") {
                    continue;
                }
                if path
                    .file_name()
                    .is_some_and(|n| n == "skin.rs" || n == "paper.rs")
                    || path.components().any(|c| c.as_os_str() == "skin")
                {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap();
                assert!(
                    !source.contains(".is_paper("),
                    "material branch escaped to {}",
                    path.display()
                );
                for backend in ["crate::paper::", "crate::deck::"] {
                    assert!(
                        !source.contains(backend),
                        "material backend {backend} escaped to {}",
                        path.display()
                    );
                }
            }
        }
        visit(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"));
    }

    #[test]
    fn text_controls_are_opt_in_and_the_skin_does_not_rewrite_a_design() {
        let th = palette();
        let ordinary = parse(DEFAULT_SKIN_TOML).unwrap().bake(&th, 1.);
        assert!(!ordinary.text_controls());
        assert_eq!(ordinary.control_text("🎨"), "🎨");
        assert_eq!(ordinary.chrome_theme(&th).bg, th.bg);

        let minimal = parse(builtin_toml("terminimalist").unwrap()).unwrap();
        assert!(minimal.unknown.is_empty());
        let sk = minimal.bake(&th, 1.);
        assert!(sk.text_controls());
        assert_eq!(sk.control_text("🎨"), "look");
        assert_eq!(sk.control_text("0%"), "0%");
        assert_eq!(sk.control_text("unavailable"), "unavailable");
        let projected = sk.chrome_theme(&th);
        assert_eq!(projected.bg, sk.ink.panel);
        assert_eq!(sk.region_face(th.surface.alpha(0.05)), sk.ink.panel);
        assert_eq!(projected.text, sk.ink.ink);
        assert_eq!(
            projected.ansi, th.ansi,
            "status colours retain their meaning"
        );
        assert_eq!(projected.font_family, th.font_family);
        assert_eq!(projected.font_size, th.font_size);
        assert_eq!(projected.warp, th.warp);
        assert_eq!(projected.crt, th.crt);
        assert_eq!(
            sk.to_json("reference")
                .parse::<serde_json::Value>()
                .unwrap()["shape"]["controls"],
            "Text"
        );
    }

    #[test]
    fn open_furniture_removes_boxes_but_keeps_selection_and_warning_rules() {
        let sk = parse(builtin_toml("terminimalist").unwrap())
            .unwrap()
            .bake(&palette(), 1.);
        assert!(sk.unboxed());
        let mut forced = sk.clone();
        forced.m.border = 3.;
        let mut frame = forced.frame(div().border_2());
        assert_eq!(
            frame.style().border_widths,
            div().border(px(0.)).style().border_widths
        );
        let border = || div().border_2();
        let zero = div().border(px(0.)).style().border_widths.clone();
        for mut d in [
            sk.frame(border()),
            sk.finish(border()),
            sk.finish_group(border()),
            sk.action(border(), false),
            sk.bezel(false, 1.),
        ] {
            assert_eq!(d.style().border_widths, zero);
        }
        let mut selected = sk.choice(border(), true);
        let mut expected = div().border(px(0.)).border_l(px(1.));
        assert_eq!(
            selected.style().border_widths,
            expected.style().border_widths
        );
        let mut warning = sk.finish_edged(border(), sk.ink.danger);
        assert_eq!(
            warning.style().border_widths,
            expected.style().border_widths
        );
        assert_eq!(warning.style().border_color, Some(sk.ink.danger));
        assert_eq!(sk.box_border(2.), px(0.));
    }

    #[test]
    fn legacy_skin_box_widths_are_unchanged_even_without_panel_boundaries() {
        for (id, src) in BUILTIN_SKINS {
            let mut sk = parse(src).unwrap().bake(&palette(), 1.);
            if sk.text_controls() {
                continue;
            }
            assert!(!sk.unboxed(), "{id}");
            assert_eq!(sk.box_border(1.), px(1.), "{id}");
            assert_eq!(sk.box_border(2.), px(2.), "{id}");
            sk.shape.boundary = Boundary::None;
            assert!(
                !sk.unboxed(),
                "{id}: icon controls do not opt into open furniture"
            );
            assert_eq!(sk.box_border(2.), px(2.), "{id}");
        }
        let mut sk = parse(builtin_toml("terminimalist").unwrap())
            .unwrap()
            .bake(&palette(), 1.);
        sk.shape.boundary = Boundary::Hairline;
        assert!(
            !sk.unboxed(),
            "text controls alone do not remove a requested border"
        );
        assert_eq!(sk.box_border(2.), px(2.));
    }

    #[test]
    fn pinned_note_stock_keeps_readable_ink_on_a_pale_terminal() {
        let mut th = palette();
        th.bg = gpui::white();
        th.text = gpui::black();
        let spec = parse(builtin_toml("terminimalist").unwrap()).unwrap();
        let paper = spec.bake(&th, 1.).note_palette(&th);
        let ratio = crate::notepaper::contrast(
            crate::notepaper::Rgb::of(paper.base),
            crate::notepaper::Rgb::of(paper.ink),
        );
        assert!(
            ratio >= 4.5,
            "skin stock must retain readable handwriting: {ratio}"
        );
    }

    #[test]
    fn chalk_follows_palette_and_keeps_terminal_font_owned_by_the_theme() {
        let chalk = parse(builtin_toml("chalk").unwrap()).unwrap();
        assert_eq!(builtin_toml("paper"), builtin_toml("chalk"));
        for id in theme::builtin_ids() {
            let th = theme::parse(theme::builtin_toml(id).unwrap()).unwrap();
            let before = (th.bg, th.text, th.accent, th.font_family.clone(), th.warp);
            let sk = chalk.bake(&th, 1.);
            assert_eq!(sk.ink.panel, th.surface);
            assert_eq!(sk.ink.mark, th.accent);
            assert!(f32::from(sk.text_px(9.)) >= 16.);
            assert_eq!(
                before,
                (th.bg, th.text, th.accent, th.font_family.clone(), th.warp)
            );
            assert_eq!(SkinSpec::default().bake(&th, 1.).text_px(9.), px(9.));
        }
    }

    #[test]
    fn finishing_stock_keeps_a_floating_panels_position() {
        let sk = parse(builtin_toml("chalk").unwrap())
            .unwrap()
            .bake(&palette(), 1.);
        let mut panel = sk.finish(div().absolute().top(px(20.)));
        assert_eq!(panel.style().position, Some(gpui::Position::Absolute));
    }

    #[test]
    fn a_plain_skin_does_not_add_stock_to_the_old_transparent_bands() {
        let mut band = SkinSpec::default().bake(&palette(), 1.).sheet();
        assert!(band.style().background.is_none());
    }

    #[test]
    fn chalk_gauge_pigment_remains_visible_on_pale_stock() {
        let mut th = palette();
        th.bg = gpui::white();
        th.surface = gpui::rgb(0xe7e2d8).into();
        th.text = gpui::black();
        let sk = parse(builtin_toml("chalk").unwrap()).unwrap().bake(&th, 1.);
        assert!(contrast(sk.ink.select, sk.ink.btn_face) >= 3.);
    }

    #[test]
    fn paper_ui_handwriting_does_not_change_the_terminal_font() {
        let spec = parse(include_str!("../skins/chalk.toml")).unwrap();
        assert!(spec.unknown.is_empty(), "{:?}", spec.unknown);
        for id in ["clean", "deco", "hacker"] {
            let th = theme::parse(theme::builtin_toml(id).unwrap()).unwrap();
            let terminal_font = th.font_family.clone();
            let sk = spec.bake(&th, 1.0);
            assert!(sk.is_paper());
            assert_eq!(sk.font_family, "Caveat");
            assert_eq!(th.font_family, terminal_font);
            assert_ne!(sk.font_family, th.font_family);
        }
        let default = SkinSpec::default();
        assert!(default.font_family.is_none());
        assert!(default.shape.material.is_none());
        let th = palette();
        assert_eq!(default.bake(&th, 1.).font_family, th.font_family);
        assert!(!default.bake(&th, 1.).is_paper());
    }

    #[test]
    fn a_misspelled_material_is_reported() {
        let spec = parse("[shape]\nmaterial = 'papre'\n").unwrap();
        assert!(spec.unknown.iter().any(|k| k.contains("papre")));
        assert!(spec.shape.material.is_none());
    }

    fn palette() -> Theme {
        theme::parse(theme::DEFAULT_THEME_TOML).expect("the embedded theme parses")
    }

    /// The gate on the whole layer. Every ink the shipped default skin resolves
    /// to must equal the literal the chrome computes today — otherwise adopting
    /// a token silently restyles the app, and nobody would know which token lied.
    #[test]
    fn default_skin_reproduces_todays_chrome() {
        let th = palette();
        let sk = parse(DEFAULT_SKIN_TOML).unwrap().bake(&th, 1.0);

        // the left bar's own frame: .bg(th.bg) + border darken(th.surface, 0.3)
        assert_eq!(sk.ink.panel, th.bg);
        assert_eq!(sk.ink.rule_strong, crate::darken(th.surface, 0.3));
        // its divider: th.faint.alpha(0.25)
        assert_eq!(sk.ink.rule, th.faint.alpha(0.25));
        // the scope chip: accent text on accent.alpha(0.14)
        assert_eq!(sk.ink.mark, th.accent);
        assert_eq!(sk.ink.mark_wash, th.accent.alpha(0.14));
        // The meta ink. It was 0.45 to match the 23 hand-written
        // `th.text.alpha(0.45)` expressions this token replaced; it is 0.60
        // because 0.45 does not clear the small-text floor on any palette we
        // ship — see `the_benchs_meta_ink_is_readable_on_every_builtin_palette`,
        // which is the assertion that actually defends the number.
        assert_eq!(sk.ink.ink_faint, th.text.alpha(0.60));
        // the hover lift, spelled hsla(0., 0., 1., 0.12) at every site
        assert_eq!(sk.ink.hover, hsla(0., 0., 1., 0.12));
        // the scale track's well: darken(th.surface, 0.4)
        assert_eq!(sk.ink.panel_sunken, crate::darken(th.surface, 0.4));

        // gpui's rounded_sm is rems(0.25) = 4px; the left bar's frame is px(10.)
        assert_eq!(sk.radius(), px(4.));
        assert_eq!(sk.radius_lg(), px(10.));

        // Every shape strategy still matches the old chrome — EXCEPT emphasis.
        //
        // The phosphor ring replaced the fill in the default skin on 2026-09-12,
        // deliberately and at the owner's request, so this is no longer the
        // identity. Asserting the rest field by field rather than relaxing the
        // whole comparison: the value of this test is that a SECOND unintended
        // departure cannot hide behind the first one, and `!= Shapes::default()`
        // would have let it.
        assert_eq!(sk.shape.emphasis, Emphasis::Glow, "the one intended change");
        let d = Shapes::default();
        assert_eq!(sk.shape.corner, d.corner);
        assert_eq!(sk.shape.boundary, d.boundary);
        assert_eq!(sk.shape.divider, d.divider);
        assert_eq!(sk.shape.caps, d.caps);
        assert_eq!(sk.shape.shine, d.shine);
    }

    /// Absent is not zero. A skin that declares one strategy and nothing else is
    /// complete, and every ink it stayed silent about still tracks the palette.
    #[test]
    fn an_undeclared_token_resolves_to_its_recipe_not_to_a_default_value() {
        let th = palette();
        let sk = parse("name = \"minimal\"\n[shape]\ncorner = \"square\"\n")
            .unwrap()
            .bake(&th, 1.0);

        assert_eq!(sk.shape.corner, Corner::Square);
        // nothing about colour was said, so colour is exactly the default skin's
        assert_eq!(sk.ink, parse(DEFAULT_SKIN_TOML).unwrap().bake(&th, 1.0).ink);
        // and specifically: not black, not transparent
        assert_eq!(sk.ink.mark, th.accent);
        assert_ne!(sk.ink.rule.a, 0.);
    }

    /// The claim that makes the layer worth having: the same skin under a
    /// different palette produces a different, still-correct set of colours.
    #[test]
    fn a_recipe_skin_follows_whatever_palette_it_is_given() {
        let a = palette();
        let mut b = palette();
        b.accent = hsla(0.09, 0.6, 0.55, 1.); // brass
        b.surface = hsla(0.55, 0.2, 0.09, 1.);

        let spec = parse(DEFAULT_SKIN_TOML).unwrap();
        let sa = spec.bake(&a, 1.0);
        let sb = spec.bake(&b, 1.0);

        assert_ne!(sa.ink.mark, sb.ink.mark);
        assert_eq!(sb.ink.mark, b.accent);
        assert_eq!(sb.ink.rule_strong, crate::darken(b.surface, 0.3));
    }

    /// A pinned ink is the other half of the contract: a look that must be gold
    /// stays gold when the palette is green.
    #[test]
    fn a_pinned_ink_ignores_the_palette() {
        let a = palette();
        let mut b = palette();
        b.accent = hsla(0.33, 0.9, 0.5, 1.);

        let spec = parse("[ink]\nmark = \"#c8a44d\"\n").unwrap();
        assert_eq!(spec.bake(&a, 1.).ink.mark, spec.bake(&b, 1.).ink.mark);
        assert_eq!(
            spec.bake(&a, 1.).ink.mark,
            theme::parse_hex("#c8a44d").unwrap()
        );
    }

    /// Console is no longer offered, and a window that saved it opens in OPS
    /// rather than failing to find its skin.
    #[test]
    fn a_retired_console_skin_reads_back_as_ops() {
        assert!(!BUILTIN_SKINS.iter().any(|(id, _)| *id == "console"));
        assert_eq!(canonical_id("console"), "ops");
        assert_eq!(builtin_toml("console"), builtin_toml("ops"));
    }

    /// Only a skin that NAMES a heading or data face changes the chrome's type,
    /// so every skin written before those keys draws exactly what it drew —
    /// the helpers are no-ops, not a default of their own.
    #[test]
    fn only_a_skin_that_names_its_faces_changes_the_type() {
        let th = palette();
        for (id, src) in BUILTIN_SKINS {
            let sk = parse(src).unwrap().bake(&th, 1.0);
            if *id == "ops" {
                assert_eq!(sk.heading_family, "Oxanium");
                assert_eq!(sk.data_family, "Share Tech Mono");
                assert_eq!(sk.font_family, "Chakra Petch");
                assert_eq!(sk.heading_text("Jev"), "JEV");
            } else {
                assert_eq!(
                    sk.heading_family, sk.font_family,
                    "{id} grew a heading face"
                );
                assert_eq!(sk.data_family, sk.font_family, "{id} grew a data face");
                assert_eq!(sk.heading_text("Jev"), "Jev", "{id} recased a name");
            }
        }
    }

    /// OPS is the one skin wearing every new device, and a cut corner never
    /// becomes a radius anywhere — `cut` is square plus a drawn plate.
    #[test]
    fn ops_wears_the_deck_and_a_cut_corner_has_no_radius() {
        let th = palette();
        let src = BUILTIN_SKINS.iter().find(|(i, _)| *i == "ops").unwrap().1;
        let spec = parse(src).unwrap();
        assert!(
            spec.unknown.is_empty(),
            "ops.toml has typos: {:?}",
            spec.unknown
        );
        let sk = spec.bake(&th, 1.0);
        assert_eq!(sk.shape.corner, Corner::Cut);
        assert_eq!(sk.shape.emphasis, Emphasis::Bar);
        assert_eq!(sk.shape.gauge, Gauge::Pips);
        assert_eq!(sk.shape.title, Title::Chevron);
        assert_eq!(sk.shape.focus, Focus::Hatch);
        assert_eq!(sk.shape.material, Material::Grid);
        assert!(sk.is_cut() && !sk.is_paper());
        assert_eq!(sk.radius(), px(0.));
        assert_eq!(sk.radius_pill(), px(0.));
        assert_eq!(sk.rad(6.), px(0.));
        assert!(sk.m.cut > 0.);
        // On the bar the label sits ON the selection and goes dark; elsewhere
        // it is the lit body colour.
        assert_eq!(sk.lit_label(), sk.ink.ink_on_mark);
        let deco = parse(include_str!("../skins/deco.toml"))
            .unwrap()
            .bake(&th, 1.0);
        assert_eq!(deco.lit_label(), deco.ink.ink_lit);
        assert!(deco.chevron("»").is_none() && deco.focus_hatch(true).is_none());
        assert!(sk.chevron("»").is_some() && sk.focus_hatch(true).is_some());
        assert!(
            sk.focus_hatch(false).is_none(),
            "an unfocused pane is hatched"
        );
    }

    /// Every builtin is a distinct LOOK, not a restyle of the same one.
    ///
    /// Two skins whose strategies all agree are one skin with two names, and the
    /// only honest thing to do with the second is delete it. The check is cheap
    /// and it is the one that stops a skin list from becoming a colour list.
    #[test]
    fn no_two_builtin_skins_resolve_to_the_same_shape() {
        let th = palette();
        let baked: Vec<(&str, Shapes)> = BUILTIN_SKINS
            .iter()
            .map(|(id, src)| (*id, parse(src).unwrap().bake(&th, 1.0).shape))
            .collect();
        for (i, (a_id, a)) in baked.iter().enumerate() {
            for (b_id, b) in &baked[i + 1..] {
                assert_ne!(a, b, "{a_id} and {b_id} are the same look twice");
            }
        }
    }

    /// The glow is a border AND a bloom, and it reserves its border when unlit.
    ///
    /// The reservation is the part worth a test: without it, marking a different
    /// tab moves every tab on the strip by a border width, which looks like a
    /// layout bug and is impossible to attribute to a skin.
    #[test]
    fn the_ring_reserves_its_border_so_marking_something_moves_nothing() {
        let th = palette();
        let mut spec = parse(DEFAULT_SKIN_TOML).unwrap();
        spec.shape.emphasis = Some(Emphasis::Glow);
        let sk = spec.bake(&th, 1.0);
        assert_eq!(sk.shape.emphasis, Emphasis::Glow);
        // The bloom has a real radius and a real alpha — a glow of zero is a
        // border, and a skin that quietly shipped one would look like a bug in
        // the renderer rather than a wrong number in a file.
        assert!(sk.m.glow > 0., "a ring with no bloom is just a border");
        // The fill is a SEAT, not a highlight. Above about a tenth it stops
        // reading as a lit edge and starts reading as a selected row in a web
        // list, which is the specific thing this was corrected away from — the
        // focused pane whose recipe the ring copies carries no fill at all.
        assert!(
            sk.ink.row_active.a <= 0.12,
            "row_active is back to being a highlight: {}",
            sk.ink.row_active.a
        );
        assert!(
            sk.ink.mark_wash.a <= 0.16,
            "mark_wash is drowning the ring: {}",
            sk.ink.mark_wash.a
        );
        assert!(
            sk.m.glow_a > 0. && sk.m.glow_a <= 1.,
            "glow_a is an alpha, not a length: {}",
            sk.m.glow_a
        );
        // Both skins that use the ring must give it something to be lit in.
        for src in [DEFAULT_SKIN_TOML, include_str!("../skins/deco.toml")] {
            let s = parse(src).unwrap().bake(&th, 1.0);
            if s.shape.emphasis == Emphasis::Glow {
                assert!(s.ink.mark.a > 0.5, "{} rings in a transparent ink", s.name);
            }
        }
    }

    /// sRGB relative luminance, the WCAG definition.
    fn luminance(c: gpui::Rgba) -> f32 {
        fn lin(v: f32) -> f32 {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    /// `over` composited onto `under` by `over`'s own alpha. Done in RGB rather
    /// than in HSL because HSL does not composite — interpolating a hue between
    /// magenta and near-black walks it through colours neither of them contains.
    fn over(top: Hsla, bottom: Hsla) -> Hsla {
        let f = gpui::Rgba::from(top);
        let b = gpui::Rgba::from(bottom);
        let mix = |x: f32, y: f32| x * f.a + y * (1. - f.a);
        Hsla::from(gpui::Rgba {
            r: mix(f.r, b.r),
            g: mix(f.g, b.g),
            b: mix(f.b, b.b),
            a: 1.,
        })
    }

    /// The WCAG contrast ratio between two OPAQUE colours, 1.0 … 21.0.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let (x, y) = (
            luminance(gpui::Rgba::from(a)),
            luminance(gpui::Rgba::from(b)),
        );
        let (hi, lo) = if x > y { (x, y) } else { (y, x) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// A word on a tab can be READ, on every palette we ship.
    ///
    /// This is the test that would have caught what a person caught by eye. The
    /// bench's shelf tabs drew the lit one's label in `select`, inside a border
    /// in `select`, on a seat mixed from `select`, under a bloom of `select` —
    /// four devices, one hue, and the glyphs were the only one of the four you
    /// had to read. The unlit ones took `ink_off`, which is Faint, a legitimate
    /// colour for a label that is genuinely not in effect and the wrong one for
    /// a tab you are meant to press. Parker: *"The workbench right bar tabs are
    /// unreadable"*, then *"retro is bad... Tide is AWEFUL!"* — a palette-shaped
    /// complaint, which is why this walks every palette rather than the default.
    ///
    /// The floors are the WCAG small-text and UI-component numbers, and they are
    /// not aspirational — both legs were run against the inks this replaced,
    /// which is the only way to know a check is measuring anything. On
    /// `quiet-command` the old lit label came out at **1.38:1** against its own
    /// seat and the old resting label at **1.08:1** against its face. 1.0 is two
    /// identical colours. That is what "unreadable" was, as a number.
    #[test]
    fn a_tab_label_is_legible_on_every_builtin_palette() {
        for id in crate::theme::builtin_ids() {
            let src = crate::theme::builtin_toml(id).expect("a builtin id resolves to its toml");
            let th = theme::parse(src).unwrap_or_else(|e| panic!("{id} does not parse: {e:?}"));
            let sk = parse(DEFAULT_SKIN_TOML).unwrap().bake(&th, 1.0);
            // The two grounds chrome sits on. A tab lands on one or the other.
            for ground in [sk.ink.panel, sk.ink.panel_raised] {
                // LIT: the label over the seat the ring lays down.
                let seat = over(sk.ink.select.alpha(sk.ink.mark_wash.a), ground);
                let lit = contrast(over(sk.ink.ink_lit, seat), seat);
                assert!(
                    lit >= 4.5,
                    "{id}: a lit tab's word is {lit:.2}:1 against its own seat"
                );
                // AT REST: still a control, so still a word somebody reads.
                let face = over(sk.ink.face_rest, ground);
                let rest = contrast(over(sk.ink.ink_dim, face), face);
                assert!(
                    rest >= 3.0,
                    "{id}: a resting tab's word is {rest:.2}:1 against its face"
                );
                // …and its EDGE has to be findable, or the control has no shape.
                let edge = contrast(over(sk.ink.edge_rest, ground), ground);
                assert!(
                    edge >= 1.4,
                    "{id}: a resting tab's edge is {edge:.2}:1 against the ground"
                );
            }
        }
    }

    /// The small print on a card can be READ, on every palette we ship.
    ///
    /// The tab test above fixed the labels on controls. This one is the line
    /// under them, and it was worse: the bench drew every subtitle, cost line,
    /// consequence, provenance line and section tag — thirty-nine sites in
    /// `benchdraw`, four more in the pane — in the PALETTE's `faint` role
    /// rather than in a text ink. `faint` is furniture. It is what a divider is
    /// mixed from, and on the six builtin palettes a word painted in it lands
    /// between **1.22:1 and 1.62:1** against the ground under it. 1.0 is two
    /// identical colours, so the whole range is a rounding error away from
    /// drawing nothing at all. Parker, on the decision card: *"the sub text is
    /// impossible to read"*.
    ///
    /// The floor is WCAG's small-text number, because that is the size this
    /// text actually is — `Step::Tag` is nine points and `Step::Small` eleven.
    /// Both legs were run against the inks they replace and both go red, which
    /// is the only way to know a check is measuring anything: `ink_faint` at
    /// its old 0.45 fails on all six, and `th.faint` fails by a factor of three.
    #[test]
    fn the_benchs_meta_ink_is_readable_on_every_builtin_palette() {
        for id in crate::theme::builtin_ids() {
            let src = crate::theme::builtin_toml(id).expect("a builtin id resolves to its toml");
            let th = theme::parse(src).unwrap_or_else(|e| panic!("{id} does not parse: {e:?}"));
            let sk = parse(DEFAULT_SKIN_TOML).unwrap().bake(&th, 1.0);
            for ground in [sk.ink.panel, sk.ink.panel_raised] {
                let meta = contrast(over(sk.ink.ink_faint, ground), ground);
                assert!(
                    meta >= 4.5,
                    "{id}: a card's small print is {meta:.2}:1 against its ground"
                );
                // …and the ink it replaced is held here as the measurement,
                // not as a memory. If a future palette makes the palette role
                // legible on its own, this is where that stops being a lie.
                let furniture = contrast(over(th.faint, ground), ground);
                assert!(
                    furniture < meta,
                    "{id}: the palette's `faint` role now reads better ({furniture:.2}:1) \
                     than the meta ink ({meta:.2}:1) — the bench may want it back"
                );
            }
        }
    }

    /// A panel holds its contents OFF its own border.
    ///
    /// The rule used to live at the call sites, and seven of the bench's
    /// fourteen missed it — which is not visible in the source, only in a
    /// photograph of a lit border running through the descenders of an
    /// eleven-point line. Asserted on every builtin skin because the inset is
    /// the skin's `pad_x`/`pad_y`, and a skin file is allowed to set those.
    #[test]
    fn a_panel_holds_its_contents_off_its_own_border() {
        for (id, src) in BUILTIN_SKINS {
            let sk = parse(src)
                .unwrap_or_else(|e| panic!("{id} does not parse: {e:?}"))
                .bake(&palette(), 1.0);
            assert!(
                sk.m.pad_x > 0. && sk.m.pad_y > 0.,
                "{id}: a skin with no padding gives every panel a border on its text"
            );
            let mut got = sk.panel();
            let mut want = div().px(sk.px(sk.m.pad_x)).py(sk.px(sk.m.pad_y));
            assert_eq!(
                got.style().padding,
                want.style().padding,
                "{id}: a panel's inset is not the skin's own padding"
            );
        }
    }

    /// One bloom, and a quiet one.
    ///
    /// The crisp spread-ring is gone on purpose — on a box five pixels wider
    /// than its own word it closed over the glyphs from every side — and the
    /// halo that remains is deliberately just past the model/effort strip's
    /// zero. A future edit that reaches for 0.4 again is reaching for the blob.
    #[test]
    fn the_phosphor_is_dialled_to_the_strip_plus_a_little() {
        let sk = parse(DEFAULT_SKIN_TOML).unwrap().bake(&palette(), 1.0);
        assert!(sk.m.glow > 0., "a ring with no bloom is just a border");
        assert!(
            (0.05..=0.18).contains(&sk.m.glow_a),
            "glow_a left the strip's neighbourhood: {}",
            sk.m.glow_a
        );
        // The thumb is the one fill that is allowed to be heavier than a seat,
        // because on a slider there is no lit border doing the marking.
        assert!(
            sk.ink.mark_wash.a * 1.6 > sk.ink.row_active.a,
            "the slider's thumb is lighter than the seat under a row"
        );
    }

    #[test]
    fn the_deco_skin_is_square_ringed_and_doubled() {
        let th = palette();
        let sk = parse(include_str!("../skins/deco.toml"))
            .unwrap()
            .bake(&th, 1.0);
        assert_eq!(sk.shape.corner, Corner::Square);
        assert_eq!(sk.shape.emphasis, Emphasis::Glow);
        assert_eq!(sk.shape.divider, Divider::Double);
        assert_eq!(sk.radius(), px(0.));
        assert_eq!(sk.radius_lg(), px(0.));
        assert_eq!(sk.radius_pill(), px(0.));
    }

    /// Scale is applied once, here, rather than at 688 call sites.
    #[test]
    fn metrics_carry_the_window_scale() {
        let th = palette();
        let sk = parse(DEFAULT_SKIN_TOML).unwrap().bake(&th, 2.0);
        assert_eq!(sk.px(6.), px(12.));
        assert_eq!(sk.at(0.5).px(6.), px(6.));
        assert_eq!(sk.rad(4.), px(8.), "`rad` is the SCALED radius");
    }

    /// Corners do not scale, and this is the test that says so.
    ///
    /// The chrome spells `rounded_sm()` — a fixed 4px — fifty-five times, and the
    /// house scale is 0.80. A `radius()` that multiplied by the scale would have
    /// rounded every corner in the app by 0.8px on the first frame, under a
    /// refactor that claimed to change nothing. It was written that way once.
    #[test]
    fn a_corner_radius_is_a_shape_not_a_length_so_it_ignores_the_scale() {
        let th = palette();
        let spec = parse(DEFAULT_SKIN_TOML).unwrap();
        for scale in [0.7, 0.8, 1.0, 1.6, 2.0] {
            let sk = spec.bake(&th, scale);
            assert_eq!(sk.radius(), px(4.), "radius moved at scale {scale}");
            assert_eq!(sk.radius_lg(), px(10.), "radius_lg moved at scale {scale}");
            assert_eq!(sk.radius_pill(), px(9999.), "pill moved at scale {scale}");
        }
    }

    #[test]
    fn tracking_happens_in_the_string_because_gpui_has_no_letter_spacing() {
        let th = palette();
        let mut spec = parse(DEFAULT_SKIN_TOML).unwrap();
        spec.shape.caps = Some(Caps::Tracked);
        let sk = spec.bake(&th, 1.);
        assert_eq!(sk.caps("tasks"), "T\u{2009}A\u{2009}S\u{2009}K\u{2009}S");

        spec.shape.caps = Some(Caps::Upper);
        assert_eq!(spec.bake(&th, 1.).caps("tasks"), "TASKS");
        spec.shape.caps = Some(Caps::Off);
        assert_eq!(spec.bake(&th, 1.).caps("tasks"), "tasks");
    }

    /// A misspelled token is REPORTED, not dropped. The failure this prevents is
    /// the one every theming system has: a file that looks right, parses fine,
    /// and does nothing.
    #[test]
    fn a_typo_is_collected_rather_than_swallowed() {
        let spec = parse(
            "[ink]\nrule_stong = \"#ffffff\"\n[metric]\nradius_xl = 3.0\n[shape]\ncorner = \"bevel\"\n",
        )
        .unwrap();
        assert!(spec.unknown.iter().any(|u| u.contains("rule_stong")));
        assert!(spec.unknown.iter().any(|u| u.contains("radius_xl")));
        assert!(spec.unknown.iter().any(|u| u.contains("bevel")));
        // and the file still resolves to a usable skin
        assert_eq!(spec.bake(&palette(), 1.).shape.corner, Corner::Round);
    }

    /// `skins/default.toml` is the template every skin author copies from, and a
    /// token missing from it is a token nobody will ever know exists. The file
    /// deliberately carries no live values — the defaults live once, in the macro
    /// tables — so what has to be guarded is that its COMMENTED listing still
    /// names every token, and still quotes the right number where it quotes one.
    #[test]
    fn the_default_skin_template_documents_every_token_and_quotes_it_correctly() {
        for key in INK_KEYS {
            assert!(
                default_ink(key).is_some(),
                "{key} is in INK_KEYS but has no default recipe"
            );
            assert!(
                DEFAULT_SKIN_TOML.contains(&format!("# {key} ")),
                "skins/default.toml documents no `{key}` line"
            );
        }
        for key in METRIC_KEYS {
            let want = default_metric(key).expect("a metric key has a default");
            let line = DEFAULT_SKIN_TOML
                .lines()
                .find(|l| l.trim_start().starts_with(&format!("# {key} ")))
                .unwrap_or_else(|| panic!("skins/default.toml documents no `{key}` line"));
            let quoted: f32 = line
                .split('=')
                .nth(1)
                .and_then(|rhs| rhs.split('#').next())
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or_else(|| panic!("`{key}` line quotes no number: {line}"));
            assert_eq!(quoted, want, "skins/default.toml has drifted on {key}");
        }
    }

    /// The read-back verb prints every strategy there is.
    ///
    /// `shine` was added and the JSON was not, so for one build the probe
    /// answered a question about the skin while silently omitting the field that
    /// decides whether a header is lit. A verb that exists to make data
    /// inspectable is worse than no verb when it under-reports, because it is
    /// trusted. The `{:?}` of each enum is the payload, so the assertion is that
    /// every strategy NAME appears as a key.
    #[test]
    fn the_probe_prints_every_shape_strategy() {
        let th = palette();
        let json = parse(include_str!("../skins/deco.toml"))
            .unwrap()
            .bake(&th, 1.0)
            .to_json("deco");
        for key in [
            "corner",
            "boundary",
            "emphasis",
            "divider",
            "caps",
            "shine",
            "gauge",
            "title",
            "focus",
            "material",
            "ui_font",
            "heading_font",
            "data_font",
        ] {
            assert!(
                json.contains(&format!("\"{key}\"")),
                "the probe omits {key}"
            );
        }
        // …and every ink and metric, so a token added to the table cannot be
        // added to the app without becoming inspectable in the same commit.
        for key in INK_KEYS.iter().chain(METRIC_KEYS.iter()) {
            assert!(
                json.contains(&format!("\"{key}\"")),
                "the probe omits {key}"
            );
        }
    }

    #[test]
    fn a_near_miss_key_is_named_in_the_report() {
        let spec = parse("[ink]\nrule_stong = \"#ffffff\"\n").unwrap();
        assert!(
            spec.unknown[0].contains("did you mean rule_strong"),
            "{:?}",
            spec.unknown
        );
        // …and something that is not a near miss of anything says so plainly
        // rather than pointing at whatever happens to be closest.
        let wild = parse("[metric]\nbanana_split = 3.0\n").unwrap();
        assert!(
            !wild.unknown[0].contains("did you mean"),
            "{:?}",
            wild.unknown
        );
    }

    /// Distance in sRGB, 0..1 — crude, but it is the failure mode this guards
    /// that matters, not the colour science: two inks that land on the SAME byte
    /// triple, which is what happens when a state ink resolves from an ANSI slot
    /// the palette happened to point at its own accent.
    fn apart(a: Hsla, b: Hsla) -> f32 {
        let rgb = |c: Hsla| {
            let hex = crate::hsla_to_hex(c);
            let n = u32::from_str_radix(&hex[1..], 16).unwrap();
            [
                ((n >> 16) & 0xff) as f32,
                ((n >> 8) & 0xff) as f32,
                (n & 0xff) as f32,
            ]
        };
        let (x, y) = (rgb(a), rgb(b));
        ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt() / 441.7
    }

    /// A warning the colour of the furniture is not a warning.
    ///
    /// This is not hypothetical. The deco palette's first draft pointed `ansi3`
    /// at the same brass as its accent, so `warn` resolved to exactly `mark` —
    /// a pane at 90% of its ceiling would have been painted in the colour of the
    /// frame around it. `terminal-delight skin --skin deco --theme deco` showed
    /// it as two identical hex strings; this test is what stops the next palette
    /// from doing it again, across every combination rather than the one a person
    /// happened to look at.
    #[test]
    fn no_state_ink_collapses_onto_the_accent_in_any_builtin_pairing() {
        const FLOOR: f32 = 0.08;
        for (sid, ssrc) in BUILTIN_SKINS {
            let spec = parse(ssrc).unwrap();
            for tid in crate::theme::builtin_ids() {
                let th = theme::parse(theme::builtin_toml(tid).expect("a builtin theme")).unwrap();
                let sk = spec.bake(&th, 1.0);
                let ink = &sk.ink;
                for (name, c) in [
                    ("ok", ink.ok),
                    ("warn", ink.warn),
                    ("danger", ink.danger),
                    ("live", ink.live),
                    // `select` matters most of the six. The accent is the
                    // chrome's furniture — rules, edges, frames — so a selection
                    // that resolves to it paints the one row you are standing on
                    // in the colour of everything around it, and the ring stops
                    // marking anything at all.
                    ("select", ink.select),
                ] {
                    // The open skin uses the palette accent on a thicker,
                    // inset bar beside the selected title. That geometry carries
                    // selection; its explicit accent recipe is intentional.
                    if name == "select" && sk.unboxed() {
                        assert_eq!(c, th.accent);
                        continue;
                    }
                    assert!(
                        apart(c, ink.mark) > FLOOR,
                        "{sid}/{tid}: `{name}` is {} and `mark` is {} — the same colour",
                        crate::hsla_to_hex(c),
                        crate::hsla_to_hex(ink.mark)
                    );
                }
                assert!(
                    apart(ink.ok, ink.danger) > FLOOR,
                    "{sid}/{tid}: healthy and failed are the same colour"
                );
                // The EVENT inks must also differ from each other, not merely
                // from the furniture. "this is running" and "this is the one you
                // are on" are the two things a person tracks on a busy screen,
                // and painting them the same colour is worse than painting
                // either of them badly. Caught exactly this way: `select` was
                // given `human`, and deco had already pinned `live` to `human`.
                assert!(
                    apart(ink.select, ink.live) > FLOOR,
                    "{sid}/{tid}: `select` and `live` are both {}",
                    crate::hsla_to_hex(ink.select)
                );
            }
        }
    }

    #[test]
    fn every_builtin_skin_parses_with_no_unknown_tokens() {
        for (id, src) in BUILTIN_SKINS {
            let spec = parse(src).unwrap_or_else(|e| panic!("{id}: {e}"));
            assert!(spec.unknown.is_empty(), "{id} carries {:?}", spec.unknown);
        }
    }

    #[test]
    fn recipe_alpha_is_set_last_so_a_blend_cannot_restore_opacity() {
        let th = palette();
        let r = Recipe::of(Role::Text).a(0.2).toward(Role::Accent, 0.5);
        assert_eq!(r.bake(&th).a, 0.2);
    }

    /// A control's ring follows the scope's phosphor gauge like every frame
    /// around it: at the house setting it is the skin's own authored bloom, and
    /// all the way down there is no bloom at all — the border stays, drawn by
    /// `ring`, and only the light goes.
    #[test]
    fn a_controls_bloom_follows_the_phosphor_gauge() {
        let mut th = palette();
        let tint = th.accent;
        let house = SkinSpec::default().bake(&th, 1.0);
        let bloom = house.bloom(tint);
        assert_eq!(bloom.len(), 1, "one halo at the house setting");
        assert_eq!(bloom[0].color.a, house.m.glow_a, "the skin's own heat");
        assert_eq!(bloom[0].blur_radius, px(house.m.glow), "and its own reach");

        th.grade.phosphor = 0.0;
        let dark = SkinSpec::default().bake(&th, 1.0);
        assert!(dark.bloom(tint).is_empty(), "all the way down, no bloom");
        assert_eq!(dark.m, house.m, "the authored metrics are untouched");
    }
}
