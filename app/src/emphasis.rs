//! What a thing on a surface is asking of the reader.
//!
//! # Why this module exists
//!
//! Before it, every renderer on the bench decided its own emphasis inline, from
//! whatever boolean happened to be in scope:
//!
//! ```text
//! .text_color(if open { th.accent } else { th.faint })
//! .bg(th.surface.alpha(if open { 0.6 } else { 0.35 }))
//! .border_color(if s.register == Register::Asks { th.complement.alpha(0.6) }
//!               else if open { th.accent.alpha(0.45) }
//!               else { th.faint.alpha(0.3) })
//! ```
//!
//! Three different questions — what ink, what fill, what edge — each answered by
//! its own chain, in its own order, at its own call site. Nothing held them
//! together, so the answers drifted: the accent came to mean *the gist* and *the
//! open section* and *the kind chip*; the complement came to mean *an ask* and
//! *the doubts* and *a confidence that is only a hunch*. Two hues carrying four
//! unrelated meanings each is two hues carrying none, and no single edit could
//! fix it because there was no single place the question was answered.
//!
//! Parker, looking at the result: *"we want to keep the boxes and borders, but
//! have attention guided in an opinionated way ... ALL THE READING will ONLY be
//! phosphor highlighted when ACTIVE — all the reading will be attentionally
//! equal ... the NEED FROM YOU DOES GET ESCALATED!"*
//!
//! # The shape of the fix
//!
//! Taken from `gpui-base`, which hit the same problem across sixty controls and
//! solved it with one resolver and a fixed, documented precedence — *"Every
//! control routes through this function so the ordering cannot drift apart
//! between controls again."* The same idea, narrowed to the one axis the bench
//! actually has: how loudly a thing is allowed to speak.
//!
//! Three seams, and each owns exactly one question:
//!
//! | Layer | Owns | Does not own |
//! |---|---|---|
//! | [`crate::workbench`] | the STATE — what is open, what the keyboard is on, what is unanswered | how any of it looks |
//! | this module | the TIER that state earns, and the tier's appearance | what is on the card |
//! | [`crate::benchdraw`] | composition — what goes where | which thing is loudest |
//!
//! A renderer asks for a tier and is clothed. It never picks an ink again.
//!
//! # The budget used to be a property of the type
//!
//! [`Emphasis::Active`] was handed out by `shelf()`, which tiered a whole row of
//! registers at once so that "exactly one lit thing" could not be overspent by a
//! renderer that forgot. The response card stopped being a row on 2026-09-18 —
//! it shows one body under a strip of tabs — and the function went with it,
//! because a rule about rows kept in a file with no rows is the shape the next
//! reader would have reached for. The budget now holds for the older reason: the
//! only thing on a card that can be Active is the tab being read, and there is
//! one of those.

use gpui::{px, Hsla, Styled};

use crate::skin::Skin;
use crate::theme::Theme;

/// How loudly something on a surface is allowed to speak.
///
/// Ordered by prominence, quietest first — `Quiet < Reading < Active <
/// Escalated` — so the ramp can be asserted rather than trusted, and so a
/// comparison in a renderer (`tier >= Emphasis::Active`) reads the way it
/// sounds.
///
/// Four rungs and deliberately not five. A tier that no reader can tell from
/// its neighbour is a tier that costs a vocabulary and buys nothing, which is
/// the state the surface was already in with five competing emphases and no
/// rank between them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Emphasis {
    /// Present, and making no claim on the reader: weights, origin, counts, the
    /// articles of doubt. Grey, and never a hue — an unknown that arrives in a
    /// colour looks like a claim.
    Quiet,
    /// Reading. Every register is this, and they are PEERS: the brief does not
    /// outrank the technical brief, because a reader who deliberately opened the
    /// technical brief is reading the technical brief. The brief is drawn FIRST,
    /// which is an order and not a rung.
    Reading,
    /// The one register the reader is actually in. The only thing on a card that
    /// gets phosphor.
    Active,
    /// A person is the blocker. The card's single interrupt, and the only user
    /// of the waiting hue.
    Escalated,
}

impl Emphasis {
    /// Every rung, quietest first — the ramp's ordering, written down.
    ///
    /// Used by the tests rather than by the render, which asks for rungs by
    /// name; kept in the shipped build because it is the list the ramp tests
    /// walk, and a ramp that is not strictly increasing is two names for one
    /// tier.
    #[allow(dead_code)]
    pub const ALL: [Emphasis; 4] = [
        Emphasis::Quiet,
        Emphasis::Reading,
        Emphasis::Active,
        Emphasis::Escalated,
    ];
}

// `shelf()` lived here: one tier per row of an open-or-shut reading shelf, with
// the one-lit-thing budget enforced by construction because no call site held
// it. It is gone, and the deletion is the point rather than a tidy-up.
//
// It solved a problem the card no longer has. A shelf needs tiering when several
// rows are on screen at once and exactly one of them may be lit; the tabbed card
// shows ONE body, so the register being read is the only register there is, and
// "which one is Active" stops being a question anything has to answer. Keeping
// the function would have left a rule about a row of things in a file whose only
// row is now a strip of tabs — and the next person to want tiers would have
// reached for it and got the old shape back.
//
// The rules it carried are not lost. *A closed row is never Active* has no
// closed rows to speak of; *with no focus nothing is Active* survives as the
// renderer drawing the first register with no accent until the reader picks one,
// which is the same promise made where it now applies. Parker's own reason for
// rule 2 — *"kill the bright light on tl;dr as a persistent default... this
// light should be on the most recent clicked"* — is why the strip lights the
// open TAB and not the first one on a card nobody has touched.

/// What an escalation earns on the card, or `None` for one that draws nothing.
///
/// `unanswered` is the count of asks still open. An escalation whose questions
/// have all been answered draws NOTHING rather than a spent red frame: the
/// loudest thing on the surface has to be able to go away, or a card you have
/// already dealt with keeps shouting and the frame stops meaning anything.
pub fn call_of(level: crate::surface::EscalationLevel, unanswered: usize) -> Option<Call> {
    use crate::surface::EscalationLevel as L;
    match (level, unanswered) {
        (_, 0) => None,
        (L::None, _) => None,
        (L::Blocking, _) => Some(Call {
            tier: Emphasis::Escalated,
            strength: 1.0,
        }),
        // Present without being urgent. The SAME frame at a lower strength
        // rather than a second recipe that agrees with the first today: the dial
        // `benchdraw::spine_frame` already takes exists for exactly this, and a
        // copy would drift the first time either was touched.
        (L::Wanted, _) => Some(Call {
            tier: Emphasis::Escalated,
            strength: 0.55,
        }),
    }
}

/// An escalation's tier and how hard to push it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Call {
    pub tier: Emphasis,
    /// Dims the whole frame for a state that is present without being urgent.
    /// Feeds `benchdraw::spine_frame`'s own `strength` argument unchanged.
    pub strength: f32,
}

impl Call {
    /// The summons' meaning-colour, for the label and the checkboxes inside it.
    pub fn tint(self, th: &Theme) -> Hsla {
        facet(self.tier, th).tint
    }

    /// Clothe an element as this summons.
    ///
    /// The escalation wears the attention spine's frame rather than the panel
    /// edge every other tier gets, because it is the one element on the surface
    /// that has to say *look at this* — and [`crate::benchdraw::spine_frame`] is
    /// already that frame, borders and bloom and shadows together.
    ///
    /// It lives HERE, beside [`Facet::clothe`], for the reason the module
    /// exists: a renderer reaching for `spine_frame` itself would be a second
    /// place deciding what an escalation looks like, and two places that agree
    /// today are two places that disagree after the first edit.
    pub fn clothe<E: Styled>(self, el: E, sk: &Skin, th: &Theme) -> E {
        crate::benchdraw::spine_frame(el, self.tint(th), self.strength, sk, th)
    }
}

/// Ink for secondary text that must still be READ.
///
/// `th.faint` is the palette's dimmest ink and it is drawn on `th.surface`,
/// which in a dark theme is a few points of lightness away from it — so a line
/// in it is grey on grey and is not read so much as detected. Parker, on a card
/// whose origin line, verbs and mirrored transcript were all drawn that way:
/// *"the grey text on grey background is aweful!"*
///
/// The repair is not "brighten faint", which would move every hairline, rule
/// and tick that legitimately wants to disappear. It is a second ink, derived
/// from the FOREGROUND rather than from the palette's dimmest slot, so it
/// tracks the text colour on every theme and cannot collapse into the surface.
/// Faint keeps its job: marks, not words.
///
/// Anything with WORDS in it that a person may need to read takes this. A rule,
/// a tick, a chevron's inactive state takes `th.faint`.
pub fn meta(th: &Theme) -> Hsla {
    th.text.alpha(0.62)
}

/// The resolved appearance of one tier, against one theme.
///
/// The single place a tier becomes colour, so a palette change moves every
/// emphasis on the bench at once — the same contract [`crate::benchdraw::ink`]
/// already holds for a surface's KIND.
#[derive(Clone, Copy, Debug)]
pub struct Facet {
    /// The tier's meaning-colour: what its edge and its label are drawn in.
    pub tint: Hsla,
    /// Ink for the tier's own text.
    pub ink: Hsla,
    /// The left edge, which is what actually separates the tiers at a glance.
    pub edge: Hsla,
    pub edge_px: f32,
    /// Fill behind the panel.
    pub fill: Hsla,
    /// How much of the card's one bloom this tier spends. `0.0` is none.
    ///
    /// A fraction rather than a flag because *lit* and *shouting* are different
    /// amounts: the active register marks where the reader is, which wants half
    /// the bloom an escalation gets — one is a bookmark, the other is a summons.
    /// Parker: *"this light should be on the most recent clicked and be about
    /// 1/2 the intensity"*.
    pub glow: f32,
}

/// Resolve a tier against a theme.
///
/// Every hue here has exactly ONE job, which is the whole point:
///
/// | Tier | Hue | and nothing else uses it |
/// |---|---|---|
/// | [`Emphasis::Escalated`] | the waiting red, `Role::Ansi(9)` | ✓ |
/// | [`Emphasis::Active`] | the accent | ✓ (the kind chip aside, which is an identity and not an emphasis) |
/// | [`Emphasis::Reading`] | none — faint edge, surface fill | ✓ |
/// | [`Emphasis::Quiet`] | none, ever | ✓ |
///
/// The reds and ambers come through [`crate::benchdraw::ink`]'s role table
/// rather than being invented here, so they arrive in each theme's OWN red
/// instead of a hard-coded one.
pub fn facet(e: Emphasis, th: &Theme) -> Facet {
    use crate::workbench::Tint;
    match e {
        Emphasis::Escalated => {
            let tint = crate::benchdraw::ink(Tint::Waiting, th);
            Facet {
                tint,
                ink: th.text,
                edge: tint,
                edge_px: 2.,
                fill: crate::darken(th.surface, 0.45),
                glow: 1.0,
            }
        }
        Emphasis::Active => Facet {
            tint: th.accent,
            ink: th.text,
            edge: th.accent,
            edge_px: 2.,
            fill: th.accent.alpha(0.05),
            // Half. A bookmark, not a summons.
            glow: 0.5,
        },
        Emphasis::Reading => Facet {
            tint: th.faint,
            ink: th.text,
            edge: th.faint.alpha(0.3),
            edge_px: 2.,
            fill: th.surface.alpha(0.35),
            glow: 0.0,
        },
        Emphasis::Quiet => Facet {
            tint: th.faint,
            ink: th.text.alpha(0.75),
            edge: th.faint.alpha(0.3),
            edge_px: 2.,
            fill: th.surface.alpha(0.25),
            glow: 0.0,
        },
    }
}

impl Facet {
    /// Clothe an element in this tier.
    ///
    /// The precedence, fixed here and nowhere else — the same layering
    /// `gpui-base` fixes for its controls, for the same reason:
    ///
    /// ```text
    /// the Skin's shape  →  this tier's facet  →  GPUI's runtime hover/active
    /// ```
    ///
    /// Later layers override only what they set. A caller that needs its own
    /// style to survive a tier replays it after this call, which is explicit and
    /// visible, rather than this guessing which fields were deliberate.
    pub fn clothe<E: Styled>(self, el: E, sk: &Skin, th: &Theme) -> E {
        let el = el
            .bg(self.fill)
            .border_l(px(sk.tpx(self.edge_px)))
            .border_color(self.edge);
        if self.glow > 0.001 {
            crate::benchdraw::aglow_at(el, self.tint, self.glow, th)
        } else {
            el
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped default palette — the one every ink in this module resolves
    /// against, so a test asserts against real colours rather than invented ones.
    fn palette() -> Theme {
        crate::theme::parse(crate::theme::DEFAULT_THEME_TOML).expect("the embedded theme parses")
    }

    #[test]
    fn the_ramp_is_strictly_increasing() {
        for pair in Emphasis::ALL.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{:?} must be quieter than {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn a_bookmark_is_dimmer_than_a_summons() {
        let th = palette();
        let th_active = facet(Emphasis::Active, &th);
        let th_esc = facet(Emphasis::Escalated, &th);
        assert!(
            th_active.glow > 0.0 && th_active.glow <= th_esc.glow / 2.0 + f32::EPSILON,
            "active {} must be about half of escalated {}",
            th_active.glow,
            th_esc.glow
        );
        for quiet in [Emphasis::Reading, Emphasis::Quiet] {
            assert_eq!(facet(quiet, &th).glow, 0.0, "{quiet:?} never blooms");
        }
    }

    #[test]
    fn an_answered_escalation_draws_nothing() {
        use crate::surface::EscalationLevel as L;
        assert_eq!(call_of(L::Blocking, 0), None);
        assert_eq!(call_of(L::Wanted, 0), None);
    }

    #[test]
    fn a_declared_none_draws_nothing_even_with_items() {
        // `none` is the agent saying it looked and needs nothing. Its items are
        // whatever it listed for the record; they are not a summons.
        assert_eq!(call_of(crate::surface::EscalationLevel::None, 3), None);
    }

    #[test]
    fn blocking_outranks_wanted_without_changing_tier() {
        use crate::surface::EscalationLevel as L;
        let b = call_of(L::Blocking, 1).expect("blocking with an open ask calls");
        let w = call_of(L::Wanted, 1).expect("wanted with an open ask calls");
        assert_eq!(b.tier, w.tier, "one frame, not two recipes");
        assert!(
            w.strength < b.strength,
            "wanted is present without being urgent: {} vs {}",
            w.strength,
            b.strength
        );
    }
}
