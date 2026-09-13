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

/// Today's chrome, stated as tokens. Must stay pixel-faithful to what main.rs
/// draws — `default_skin_reproduces_todays_chrome` is the gate.
pub const DEFAULT_SKIN_TOML: &str = include_str!("../skins/default.toml");

/// (id, embedded toml) for every built-in skin, mirroring `theme::BUILTIN_THEMES`.
const BUILTIN_SKINS: &[(&str, &str)] = &[
    ("default", DEFAULT_SKIN_TOML),
    ("deco", include_str!("../skins/deco.toml")),
    ("console", include_str!("../skins/console.toml")),
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
            other => {
                let n: u8 = other.strip_prefix("ansi")?.parse().ok()?;
                if n > 15 {
                    return None;
                }
                Role::Ansi(n)
            }
        })
    }

    fn of(self, th: &Theme) -> Hsla {
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
    ink_faint    => "ink_faint",    Recipe::of(Role::Text).a(0.45),                "meta text: counts, ages, paths";
    ink_ghost    => "ink_ghost",    Recipe::of(Role::Text).a(0.20),                "text that is present but not for reading yet";
    ink_on_mark  => "ink_on_mark",  Recipe::of(Role::White).a(0.95),               "text drawn ON the accent";

    // ---- the accent, by job ----
    mark         => "mark",         Recipe::of(Role::Accent),                      "the accent at full strength: what is selected";
    mark_soft    => "mark_soft",    Recipe::of(Role::Accent).a(0.85),              "the accent carrying text";
    mark_dim     => "mark_dim",     Recipe::of(Role::Accent).a(0.40),              "the accent at rest";
    mark_wash    => "mark_wash",    Recipe::of(Role::Accent).a(0.14),              "the accent as a background tint";
    row_active   => "row_active",   Recipe::of(Role::Accent).a(0.22),              "the ground under the row you are standing on";
    hover        => "hover",        Recipe::of(Role::White).a(0.12),               "the lift under the pointer";

    // ---- states ----
    // The BRIGHT ansi slots, not the normal ones. A terminal palette very often
    // points its accent at one of its own normal colours — field-command's accent
    // and its `ansi2` are both `#8fa85f` — so `ok = ansi2` would have painted
    // "healthy" in exactly the colour of the furniture on a theme we already
    // ship. The brights are both further from the accent and better signals
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
    glow        => "glow",        7.0,   "how far the phosphor ring blooms past its border";
    glow_a      => "glow_a",      0.55,  "how hot the bloom is, 0..1 — NOT a length";
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Shapes {
    pub corner: Corner,
    pub boundary: Boundary,
    pub emphasis: Emphasis,
    pub divider: Divider,
    pub caps: Caps,
    pub shine: Shine,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShapeSpecs {
    pub corner: Option<Corner>,
    pub boundary: Option<Boundary>,
    pub emphasis: Option<Emphasis>,
    pub divider: Option<Divider>,
    pub caps: Option<Caps>,
    pub shine: Option<Shine>,
}

impl ShapeSpecs {
    fn bake(&self) -> Shapes {
        Shapes {
            corner: self.corner.unwrap_or_default(),
            boundary: self.boundary.unwrap_or_default(),
            emphasis: self.emphasis.unwrap_or_default(),
            divider: self.divider.unwrap_or_default(),
            caps: self.caps.unwrap_or_default(),
            shine: self.shine.unwrap_or_default(),
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
    pub name: String,
    pub icon: String,
    pub ink: Inks,
    pub m: Metrics,
    pub shape: Shapes,
    /// The window's UI scale, folded in here so no call site multiplies by `s`.
    pub scale: f32,
}

impl SkinSpec {
    pub fn bake(&self, th: &Theme, scale: f32) -> Skin {
        Skin {
            name: self.name.clone(),
            icon: self.icon.clone(),
            ink: self.ink.bake(th),
            m: self.metric.bake(),
            shape: self.shape.bake(),
            scale,
        }
    }
}

// ---------------------------------------------------------------------------
// The element vocabulary
// ---------------------------------------------------------------------------

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
            Corner::Round => px(9999.),
            Corner::Square => px(0.),
        }
    }

    /// An arbitrary radius the chrome already spells as `rounded(px(v * scale))`.
    /// Kept rather than folded into [`Skin::radius`] because the chrome genuinely
    /// has several — a chip is not a card is not a window — and flattening them
    /// all to one token would be a look change smuggled in under a refactor.
    pub fn rad(&self, v: f32) -> Pixels {
        match self.shape.corner {
            Corner::Round => self.px(v),
            Corner::Square => px(0.),
        }
    }

    /// The same, for the sites that spell it `rounded(px(v))` with no scale.
    pub fn rad_raw(&self, v: f32) -> Pixels {
        match self.shape.corner {
            Corner::Round => px(v),
            Corner::Square => px(0.),
        }
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
        let clear = hsla(0., 0., 0., 0.);
        let lit = |on: bool| if on { tint } else { clear };
        match self.shape.emphasis {
            Emphasis::Fill | Emphasis::Underline => b_b(d, self.m.rail).border_color(lit(active)),
            Emphasis::Rail => b_l(d, self.m.rail).border_color(lit(active)),
            // The ring takes the whole edge, so the strip's reserved bottom rule
            // is not drawn as well — two devices on one tab is one too many, and
            // `ring` reserves its own border in every state regardless.
            Emphasis::Glow => self.ring(d, active, tint),
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

    /// A whole chrome region: its ground, its corner, its edge. `Boundary::Double`
    /// hangs an inset ring inside it, which is why the returned div is already
    /// `relative()` — a caller's own absolute children still position against it.
    pub fn panel(&self) -> Div {
        let d = div()
            .relative()
            .bg(self.ink.panel)
            .rounded(self.radius_lg());
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
        div()
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
        div()
            .id(id)
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
        // The ring reserves its border in every state, so an unlit chip under a
        // glow skin is the same size as a lit one.
        if matches!(self.shape.emphasis, Emphasis::Glow) {
            let ink = if active {
                self.ink.mark
            } else {
                self.ink.ink_off
            };
            let r = self.ring(base.text_color(ink), active, self.ink.mark);
            return if active { r.bg(self.ink.mark_wash) } else { r };
        }
        if !active {
            return base.text_color(self.ink.ink_off);
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
            Emphasis::Glow => unreachable!("handled above, before the inactive early-return"),
        }
    }

    /// The phosphor ring: a lit border that blooms outward.
    ///
    /// `tint` is what the marked thing is lit in — the accent, or the thing's own
    /// colour where it has one. The border is drawn at EVERY state and merely
    /// goes transparent when unlit, so switching which row or tab is active never
    /// moves anything by a border width. That reservation is the whole reason
    /// this lives here rather than at a call site.
    ///
    /// One shadow, no inset. An inset companion was tried and removed: it reads
    /// as a bevel, and a bevel is the claim that the thing is raised, which is
    /// the opposite of what a glow says.
    pub fn ring<E: Styled>(&self, d: E, lit: bool, tint: Hsla) -> E {
        let clear = hsla(0., 0., 0., 0.);
        let d = b_all(d, self.m.border).border_color(if lit { tint } else { clear });
        if !lit {
            return d;
        }
        d.shadow(vec![gpui::BoxShadow {
            color: tint.alpha(self.m.glow_a.clamp(0., 1.)),
            offset: gpui::point(px(0.), px(0.)),
            blur_radius: px(self.m.glow),
            spread_radius: px(0.),
            inset: false,
        }])
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
        // Glow keeps the wash. A ring alone is enough to FIND the row and not
        // enough to read it against its neighbours in a list of twenty; the
        // HumanLayer panels this is taken from tint the active row as well as
        // ring it, and they are right to.
        if matches!(self.shape.emphasis, Emphasis::Glow) {
            let r = self.ring(d, active, self.ink.mark);
            return if active { r.bg(self.ink.row_active) } else { r };
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
            .cursor_pointer();
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
}

#[derive(Deserialize)]
struct FileShape {
    corner: Option<String>,
    boundary: Option<String>,
    emphasis: Option<String>,
    divider: Option<String>,
    caps: Option<String>,
    shine: Option<String>,
}

#[derive(Deserialize)]
struct SkinFile {
    name: Option<String>,
    icon: Option<String>,
    #[serde(default)]
    ink: BTreeMap<String, FileInk>,
    #[serde(default)]
    metric: BTreeMap<String, f32>,
    shape: Option<FileShape>,
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
    let mut spec = SkinSpec {
        name: f.name.unwrap_or_else(|| "unnamed".into()),
        icon: f.icon.unwrap_or_else(|| "▢".into()),
        ..SkinSpec::default()
    };

    for (key, value) in &f.ink {
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
        if !spec.metric.set(key, *value) {
            spec.unknown.push(no_such("metric", key, METRIC_KEYS));
        }
    }

    if let Some(sh) = &f.shape {
        let u = &mut spec.unknown;
        spec.shape.corner = enum_of(
            sh.corner.as_ref(),
            &[("round", Corner::Round), ("square", Corner::Square)],
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
            .find(|(k, _)| k == id)
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
        return reg.active_id.clone();
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

/// The skin a chrome region draws with: the active spec, baked against the
/// active palette at the window's scale. Call once per region, not per element.
pub fn skin(cx: &App, scale: f32) -> Skin {
    let th = theme::theme(cx);
    spec(cx).bake(&th, scale)
}

/// The skin for chrome that belongs to a PANE rather than to the window — a pane
/// header, a pane frame. Panes carry their own palette, so baking against the
/// window's would paint a green pane's header in the workspace's brass. Shape is
/// still window-global: only the inks differ.
pub fn for_theme(cx: &App, th: &Theme, scale: f32) -> Skin {
    spec(cx).bake(th, scale)
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

// ---------------------------------------------------------------------------
// The `skin` verb — resolve headlessly and print what came out
// ---------------------------------------------------------------------------

/// The embedded TOML of one builtin skin, by id.
pub fn builtin_toml(id: &str) -> Option<&'static str> {
    BUILTIN_SKINS
        .iter()
        .find(|(k, _)| *k == id)
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

const SKIN_USAGE: &str =
    "usage: terminal-delight skin [--list] [--skin <id|path>] [--theme <id|path>] [--scale <n>]\n\
\n\
  --list     the skins this build carries, and every way one gets chosen\n\
  (default)  resolve a skin against a palette and print every token, as JSON\n\
\n\
To restyle a RUNNING window — no restart, no file editing:\n\
  terminal-delight ctl skin <name>     switch it now\n\
  terminal-delight ctl skin theme      go back to following the theme\n\
  terminal-delight ctl skin status     what is active, and what is available";

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
        "  4. skin = \"<name>\" in a theme file    so picking a theme moves both axes",
        "  5. default                            the house look — round, lit, ringed",
    ] {
        println!("{line}");
    }
    println!(
        "\nShape and colour are separate axes on purpose: every skin works on every\n\
         palette. `terminal-delight skin --skin deco --theme hacker` shows what that\n\
         means, and `--skin console --theme deco` is the same palette wearing a\n\
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
    let mut skin_arg = "default".to_string();
    let mut theme_arg = "hacker".to_string();
    let mut scale = 1.0f32;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--skin" => match it.next() {
                Some(v) => skin_arg = v.clone(),
                None => return usage_err("--skin wants a value"),
            },
            "--theme" => match it.next() {
                Some(v) => theme_arg = v.clone(),
                None => return usage_err("--theme wants a value"),
            },
            "--scale" => match it.next().and_then(|v| v.parse().ok()) {
                Some(v) => scale = v,
                None => return usage_err("--scale wants a number"),
            },
            "--list" | "-l" => return list_skins(),
            "-h" | "--help" => {
                println!("{SKIN_USAGE}");
                return 0;
            }
            other => return usage_err(&format!("unrecognised option `{other}`")),
        }
    }

    let th = match source_of(&theme_arg, theme::builtin_toml).and_then(|s| theme::parse(&s)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("terminal-delight skin: theme {e}");
            return 1;
        }
    };
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

    let sk = spec.bake(&th, scale);
    println!("{}", sk.to_json(&theme_arg));
    0
}

fn usage_err(msg: &str) -> i32 {
    eprintln!("terminal-delight skin: {msg}\n\n{SKIN_USAGE}");
    2
}

impl Skin {
    /// Every resolved token, as JSON. Hand-rolled rather than derived because a
    /// colour has to come out as BOTH a hex string and its alpha — a reader that
    /// only sees `#c2a34f` cannot tell a rule from a wash, and those are the two
    /// most common things to get wrong in a skin file.
    fn to_json(&self, theme_id: &str) -> String {
        let ink = |name: &str, c: Hsla| {
            format!(
                "    \"{name}\": {{ \"hex\": \"{}\", \"a\": {:.3} }}",
                crate::hsla_to_hex(c),
                c.a
            )
        };
        let inks = INK_KEYS
            .iter()
            .map(|k| ink(k, self.ink.by_key(k).expect("every key resolves")))
            .collect::<Vec<_>>()
            .join(",\n");
        let metrics = METRIC_KEYS
            .iter()
            .map(|k| {
                format!(
                    "    \"{k}\": {}",
                    self.m.by_key(k).expect("every key resolves")
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!(
            "{{\n  \"skin\": \"{}\",\n  \"theme\": \"{theme_id}\",\n  \"scale\": {},\n  \"shape\": {{\n    \"corner\": \"{:?}\",\n    \"boundary\": \"{:?}\",\n    \"emphasis\": \"{:?}\",\n    \"divider\": \"{:?}\",\n    \"caps\": \"{:?}\",\n    \"shine\": \"{:?}\"\n  }},\n  \"ink\": {{\n{inks}\n  }},\n  \"metric\": {{\n{metrics}\n  }}\n}}",
            self.name,
            self.scale,
            self.shape.corner,
            self.shape.boundary,
            self.shape.emphasis,
            self.shape.divider,
            self.shape.caps,
            self.shape.shine,
        )
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
        // the most-used meta ink in the chrome, 23 call sites
        assert_eq!(sk.ink.ink_faint, th.text.alpha(0.45));
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

    /// The third skin is a FILE. No Rust was added for `console`, and this test
    /// is the assertion of that: it checks the look it produces is genuinely a
    /// different look — not merely a different set of colours — using only
    /// strategies that already existed.
    #[test]
    fn the_console_skin_is_a_third_look_made_of_nothing_but_a_file() {
        let th = palette();
        let sk = parse(include_str!("../skins/console.toml"))
            .unwrap()
            .bake(&th, 1.0);
        assert_eq!(sk.shape.boundary, Boundary::None, "regions carry no edge");
        assert_eq!(sk.shape.emphasis, Emphasis::Rail);
        assert_eq!(sk.shape.caps, Caps::Upper, "upper, but NOT tracked");
        assert_eq!(sk.shape.shine, Shine::Flat);
        assert_eq!(sk.radius(), px(0.));

        // Separating by space rather than by line only works if a raised surface
        // is actually lighter than the ground. With `boundary = none` this is the
        // ONLY thing distinguishing a bar from what it sits on.
        assert!(
            sk.ink.panel_raised.l > sk.ink.panel.l,
            "a console's bar must lift off its ground: {:?} vs {:?}",
            sk.ink.panel_raised,
            sk.ink.panel
        );
        // And its rail has to be thicker than deco's, because it is the only mark
        // on a screen with no other lines on it.
        let deco = parse(include_str!("../skins/deco.toml"))
            .unwrap()
            .bake(&th, 1.0);
        assert!(sk.m.rail > deco.m.rail);
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
        for key in ["corner", "boundary", "emphasis", "divider", "caps", "shine"] {
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
            for tid in [
                "quiet-command",
                "field-command",
                "tactical-overdrive",
                "gamba",
                "deco",
                "hacker",
            ] {
                let th = theme::parse(theme::builtin_toml(tid).expect("a builtin theme")).unwrap();
                let ink = spec.bake(&th, 1.0).ink;
                for (name, c) in [
                    ("ok", ink.ok),
                    ("warn", ink.warn),
                    ("danger", ink.danger),
                    ("live", ink.live),
                ] {
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
}
