//! The rail's second opinion.
//!
//! [`engstate`](crate::engstate) measures a project with git and writes every
//! fact it found as a [`Frame`]. This module is what happens when a System One
//! model is asked which of those facts a person should read first, and how
//! badly the window wants them.
//!
//! **The model never writes a frame.** It is handed the frames git already
//! produced, verbatim, and it answers with an ordering and one probability.
//! Everything here is therefore a permutation and an annotation: no frame is
//! created, edited, or dropped, and the worst answer the model can give costs
//! one dwell of a worse line.
//!
//! **The absence of a judgement is the fallback, and it is not a second code
//! path.** A disabled plugin, a dead endpoint, a machine with no key, a call
//! that ran past its budget and a reading that moved on all arrive here as the
//! same `None`, and `None` means the bar is exactly what git said it was — in
//! git's own order. There is nothing to keep in step, because there is only
//! one thing.

use crate::engstate::{Frame, Segment, Tone};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Which reading a judgement is about.
///
/// Hashed from the frames themselves rather than from the clock, so two
/// readings that found the same facts share a key and an answer about the
/// first is still true of the second. A scan that changed something gets a new
/// key, and the judgement about the old one stops applying the instant it
/// lands — see [`order`].
pub type Key = u64;

/// What the model said about one reading.
#[derive(Clone, Debug, PartialEq)]
pub struct Judgement {
    /// The reading this answers. Compared before anything here is believed.
    pub reading: Key,
    /// Frame texts, the one that most earns a person's attention first.
    ///
    /// These are echoes of strings the rail wrote. A text that matches no
    /// frame is ignored rather than trusted — the model is choosing from a
    /// list, and a choice outside the list is not a choice.
    pub order: Vec<String>,
    /// The probability that a person arriving at this window must act before
    /// doing anything else. `None` when the question was asked and came back
    /// without an answer, which is a different thing from a low one.
    pub attention: Option<f32>,
}

/// Below this, the model is not asking for anyone.
const SETTLED: f32 = 0.4;
/// At or above this, it is.
const ACT: f32 = 0.6;

/// What the rail draws beside the badge.
///
/// Four states, because a bar whose endpoint died must not look like a calm
/// project. [`Undeclared`](Diamond::Undeclared) is drawn hollow and means
/// nobody has been asked — never "fine".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diamond {
    /// Nothing was asked, or the answer is about a state that has moved.
    Undeclared,
    /// Git found nothing and the model agrees.
    Calm,
    /// The model is genuinely split, or it disagrees with git.
    Split,
    /// Somebody should look before doing anything else.
    Act,
}

/// The frames a person sees, in the order they see them.
///
/// Returns its input untouched — same frames, same order, same allocation
/// order — whenever there is no judgement, the judgement is about a different
/// reading, or it ranked nothing. Those are the ordinary cases and they are
/// the ones that must cost nothing.
pub fn order(frames: Vec<Frame>, judgement: Option<&Judgement>) -> Vec<Frame> {
    let key = key_of(&frames);
    let Some(j) = judgement.filter(|j| j.reading == key) else {
        return frames;
    };
    if j.order.is_empty() {
        return frames;
    }
    let mut out = frames;
    // Stable, so frames the model did not rank keep git's order among
    // themselves and sit behind the ones it did.
    out.sort_by_key(|f| {
        j.order
            .iter()
            .position(|t| *t == f.text)
            .unwrap_or(usize::MAX)
    });
    out
}

/// Which diamond to draw, given what git concluded and what the model said.
///
/// One rule is worth stating out loud: **the model may raise the alarm but it
/// may not lower git's.** When git found something and the model shrugs, that
/// is a disagreement between an instrument and an opinion, and amber is the
/// honest colour for it. Green is reserved for the case where both agree there
/// is nothing.
pub fn diamond(git_calm: bool, judgement: Option<&Judgement>, frames: &[Frame]) -> Diamond {
    let key = key_of(frames);
    let Some(j) = judgement.filter(|j| j.reading == key) else {
        return Diamond::Undeclared;
    };
    let Some(p) = j.attention else {
        return Diamond::Undeclared;
    };
    if p >= ACT {
        Diamond::Act
    } else if p >= SETTLED {
        Diamond::Split
    } else if git_calm {
        Diamond::Calm
    } else {
        Diamond::Split
    }
}

impl Diamond {
    /// What the badge wears for this verdict, or nothing when there is nothing
    /// honest to say.
    ///
    /// [`Undeclared`](Diamond::Undeclared) draws **nothing**, not a hollow
    /// mark, and the reason is narrow: until a judgement can actually exist,
    /// every reading is undeclared, and a mark on every badge would be a
    /// permanent decoration rather than a fact. The hollow mark arrives with
    /// the call that can fail — it means *this was asked and did not answer*,
    /// which cannot be true before anything is asked.
    ///
    /// The glyph is `◈` and not `◆`: the solid diamond is already the
    /// afterglow's dot, and two marks meaning different things must not look
    /// the same.
    pub fn segment(self) -> Option<Segment> {
        let (text, tone) = match self {
            Diamond::Undeclared => return None,
            Diamond::Calm => ("\u{25c8}", Tone::Good),
            Diamond::Split => ("\u{25c8}?", Tone::Plain),
            Diamond::Act => ("\u{25c8} LOOK", Tone::Warn),
        };
        Some(Segment {
            text: text.into(),
            tone,
        })
    }
}

/// The key for a set of frames — what a judgement must match to be believed.
pub fn key_of(frames: &[Frame]) -> Key {
    let mut h = DefaultHasher::new();
    for f in frames {
        f.kind.hash(&mut h);
        f.text.hash(&mut h);
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engstate::{FrameKind, Tone};

    fn frame(kind: FrameKind, text: &str) -> Frame {
        Frame {
            kind,
            text: text.into(),
            tone: Tone::Plain,
        }
    }

    fn three() -> Vec<Frame> {
        vec![
            frame(FrameKind::Dirty, "two dirty"),
            frame(FrameKind::Collision, "two converging"),
            frame(FrameKind::Pulse, "quiet for an hour"),
        ]
    }

    fn judging(frames: &[Frame], order: &[&str], attention: Option<f32>) -> Judgement {
        Judgement {
            reading: key_of(frames),
            order: order.iter().map(|s| (*s).to_string()).collect(),
            attention,
        }
    }

    // ---- the fallback is the default ------------------------------------

    #[test]
    fn no_judgement_is_gits_own_order() {
        let f = three();
        assert_eq!(order(f.clone(), None), f);
    }

    #[test]
    fn a_judgement_about_another_reading_is_ignored() {
        let f = three();
        let mut stale = judging(&f, &["quiet for an hour"], Some(0.9));
        stale.reading ^= 1; // any other reading
        assert_eq!(order(f.clone(), Some(&stale)), f);
    }

    #[test]
    fn an_empty_ranking_is_gits_own_order() {
        let f = three();
        let j = judging(&f, &[], Some(0.9));
        assert_eq!(order(f.clone(), Some(&j)), f);
    }

    // ---- the model may only permute --------------------------------------

    #[test]
    fn a_ranking_reorders_and_nothing_else() {
        let f = three();
        let j = judging(&f, &["quiet for an hour", "two dirty"], None);
        let out = order(f.clone(), Some(&j));
        assert_eq!(out[0].text, "quiet for an hour");
        assert_eq!(out[1].text, "two dirty");
        // same frames, still, whatever order they are in
        let mut before: Vec<&str> = f.iter().map(|x| x.text.as_str()).collect();
        let mut after: Vec<&str> = out.iter().map(|x| x.text.as_str()).collect();
        before.sort_unstable();
        after.sort_unstable();
        assert_eq!(before, after);
        assert_eq!(f.len(), out.len());
    }

    #[test]
    fn a_text_the_rail_never_wrote_is_ignored() {
        let f = three();
        // the failure this guards: a model answering with prose of its own and
        // the rail putting it on the glass.
        let j = judging(&f, &["everything is fine, relax"], Some(0.1));
        let out = order(f.clone(), Some(&j));
        assert_eq!(out, f);
        assert!(!out.iter().any(|x| x.text.contains("relax")));
    }

    #[test]
    fn unranked_frames_keep_gits_order_behind_the_ranked_ones() {
        let f = three();
        let j = judging(&f, &["quiet for an hour"], None);
        let out = order(f, Some(&j));
        assert_eq!(out[0].text, "quiet for an hour");
        // the two it said nothing about are still in the order git chose
        assert_eq!(out[1].text, "two dirty");
        assert_eq!(out[2].text, "two converging");
    }

    // ---- the diamond -----------------------------------------------------

    #[test]
    fn nothing_asked_is_hollow_not_green() {
        // the bug this exists to prevent: a dead endpoint looking exactly like
        // a healthy project.
        assert_eq!(diamond(true, None, &three()), Diamond::Undeclared);
    }

    #[test]
    fn asked_but_unanswered_is_hollow_not_green() {
        let f = three();
        let j = judging(&f, &[], None);
        assert_eq!(diamond(true, Some(&j), &f), Diamond::Undeclared);
    }

    #[test]
    fn a_stale_answer_is_hollow() {
        let f = three();
        let mut j = judging(&f, &[], Some(0.05));
        j.reading ^= 1;
        assert_eq!(diamond(true, Some(&j), &f), Diamond::Undeclared);
    }

    #[test]
    fn the_model_may_raise_the_alarm() {
        let f = three();
        let j = judging(&f, &[], Some(0.91));
        assert_eq!(diamond(true, Some(&j), &f), Diamond::Act);
    }

    #[test]
    fn the_model_may_not_talk_git_into_green() {
        let f = three();
        let j = judging(&f, &[], Some(0.02));
        assert_eq!(diamond(false, Some(&j), &f), Diamond::Split);
        assert_eq!(diamond(true, Some(&j), &f), Diamond::Calm);
    }

    #[test]
    fn the_middle_is_its_own_state() {
        let f = three();
        for p in [0.4, 0.5, 0.59] {
            let j = judging(&f, &[], Some(p));
            assert_eq!(diamond(true, Some(&j), &f), Diamond::Split, "at {p}");
        }
    }

    // ---- the key ---------------------------------------------------------

    #[test]
    fn an_undeclared_badge_is_todays_badge() {
        // the whole fallback, in one assertion: before anything can be asked,
        // the rail wears nothing new.
        assert_eq!(Diamond::Undeclared.segment(), None);
    }

    #[test]
    fn every_other_verdict_is_worn() {
        for d in [Diamond::Calm, Diamond::Split, Diamond::Act] {
            assert!(d.segment().is_some(), "{d:?} must be visible");
        }
    }

    #[test]
    fn the_judgement_mark_is_not_the_afterglows_dot() {
        // ◆ is already taken, and two marks that mean different things must
        // not look the same.
        for d in [Diamond::Calm, Diamond::Split, Diamond::Act] {
            let text = d.segment().expect("visible").text;
            assert!(
                !text.contains('\u{25c6}'),
                "{d:?} wears the afterglow's dot"
            );
        }
    }

    #[test]
    fn the_key_moves_when_the_facts_do() {
        let f = three();
        let mut g = f.clone();
        g[0].text = "three dirty".into();
        assert_ne!(key_of(&f), key_of(&g));
    }

    #[test]
    fn the_key_holds_when_the_facts_do() {
        assert_eq!(key_of(&three()), key_of(&three()));
    }

    #[test]
    fn reordering_the_same_facts_is_a_different_reading() {
        // a judgement is an answer about a list; the list is what was asked.
        let f = three();
        let mut g = f.clone();
        g.swap(0, 2);
        assert_ne!(key_of(&f), key_of(&g));
    }
}
