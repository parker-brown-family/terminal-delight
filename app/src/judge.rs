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

use crate::engstate::{Frame, ProjectState, Segment, Tone};
use serde_json::{json, Value};
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
        let rank = j
            .order
            .iter()
            .position(|t| *t == f.text)
            .unwrap_or(usize::MAX);
        // A warning git MEASURED outranks an opinion about it — the same rule
        // [`diamond`] applies to the badge, applied to the ticker. The model
        // may order warnings among themselves and may promote anything it
        // likes; it may not sink a warning beneath a line that is not one.
        //
        // This is not hypothetical. On the first live ranking, a Foreign frame
        // — a pane filed under this project but writing into another
        // repository, which the badge shows as `⚠ 3 FOREIGN` — came back
        // "background" at 1.23 and sorted below a repository count. Tone is a
        // fact about what git found; the level is an opinion about what it is
        // worth, and the fact wins.
        (f.tone != Tone::Warn, rank)
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

/// What the rail hands the plugin, and what it expects back.
///
/// **No question wording lives here, deliberately.** `plugins.rs` promises that
/// a public checkout of this repository carries nothing that knows what Jev is,
/// and while the mechanical gate beside that promise only forbids endpoints,
/// keys and model ids, the promise is worth keeping whole. There is a second,
/// larger reason: wording is where these questions actually fail — a rubric
/// level whose boundary cannot be stated in one sentence goes soft — and the
/// plugin can be edited and re-run in seconds where this crate takes a gpui
/// rebuild. So the rail sends **facts**, and the plugin composes the questions.
pub fn payload(state: &ProjectState, frames: &[Frame]) -> Value {
    let ids: Vec<Value> = frames
        .iter()
        .enumerate()
        .map(|(i, f)| {
            json!({
                "id": format!("f{i}"),
                "kind": format!("{:?}", f.kind),
                "text": f.text,
                // The severity the rail already assigned. Sent because the
                // first live ranking judged a frame the rail had drawn as a
                // warning without ever being told it was one, and scored it
                // background. The model should judge the same object the bar
                // draws, not a copy with the severity stripped out.
                //
                // This does NOT replace the floor in `order`. Telling the
                // model a line is a warning makes it better informed; it does
                // not make it unable to rank one last, and only the floor
                // makes that impossible.
                "tone": match f.tone {
                    Tone::Warn => "warn",
                    Tone::Good => "good",
                    Tone::Muted => "muted",
                    Tone::Plain => "plain",
                },
            })
        })
        .collect();
    json!({
        "reading": key_of(frames).to_string(),
        "project": state.name,
        "frames": ids,
        "facts": {
            "calm": state.is_calm(),
            "checkouts": state.primary_checkouts().len(),
            "shared": state.shared_count(),
            // `dirty` is Option all the way out: unmeasured is not zero, and a
            // null here must never arrive at the model as "nothing uncommitted".
            "dirty": state.dirty_checkouts(),
            "foreign": state.foreign.len(),
            "visitors": state.visitors.len(),
            "no_git": state.no_git.len(),
            "conflicts": state.conflicts().len(),
            "collisions": state.collisions().len(),
            "landing": state.landing().len(),
            "idle_worktrees": state.idle.len(),
            "commits_last_hour": state
                .primary()
                .and_then(|p| p.pulse)
                .map(|p| p.iter().sum::<u32>()),
        }
    })
}

/// Read what the plugin said, or decide it said nothing usable.
///
/// Every route out of here that is not a complete answer is `None`, which is
/// the git-only bar. Three absences the plugin keeps distinct — not installed,
/// no client or key, and abstained — all collapse to `None` *here*, at the
/// edge, having stayed separate for the whole journey. That is the right place
/// for a collapse: a renderer may decide unknown draws as nothing, a store may
/// not decide unknown is zero.
pub fn parse(frames: &[Frame], body: &str) -> Option<Judgement> {
    let v: Value = serde_json::from_str(body).ok()?;
    if v.get("available") == Some(&Value::Bool(false)) {
        return None;
    }
    // A reading the plugin echoes back that is not the one we asked about is a
    // wrong answer, not a stale one — refuse it rather than key it.
    let asked = key_of(frames);
    if let Some(echo) = v.get("reading").and_then(|r| r.as_str()) {
        if echo != asked.to_string() {
            return None;
        }
    }
    // `attention` absent or null stays None. It must not become 0.0, which
    // would read as "nobody is needed" — a claim nothing made.
    let attention = v
        .get("attention")
        .and_then(|a| a.as_f64())
        .map(|a| a as f32)
        .filter(|a| (0.0..=1.0).contains(a));
    let order = v
        .get("ranked")
        .and_then(|r| r.as_array())
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str())
                .filter_map(|id| id.strip_prefix('f'))
                .filter_map(|n| n.parse::<usize>().ok())
                .filter_map(|i| frames.get(i))
                .map(|f| f.text.clone())
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    if order.is_empty() && attention.is_none() {
        return None; // nothing usable came back
    }
    Some(Judgement {
        reading: asked,
        order,
        attention,
    })
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

    // ---- reading what the plugin said ------------------------------------

    /// A reading with no repository in it — enough to exercise `payload`
    /// without a git tree, and it keeps every count honestly at its own zero.
    fn bare_state() -> ProjectState {
        ProjectState {
            project: None,
            name: Some("TD".into()),
            scanned_at: std::time::Instant::now(),
            took: std::time::Duration::ZERO,
            repos: vec![],
            checkouts: vec![],
            idle: vec![],
            no_git: vec![],
            foreign: vec![],
            visitors: vec![],
        }
    }

    fn body(frames: &[Frame], extra: &str) -> String {
        format!(
            r#"{{"available":true,"reading":"{}",{extra}}}"#,
            key_of(frames)
        )
    }

    #[test]
    fn a_ranking_comes_back_as_the_rails_own_texts() {
        let f = three();
        let j = parse(&f, &body(&f, r#""ranked":["f2","f0"],"attention":0.7"#)).expect("parsed");
        assert_eq!(j.order, vec!["quiet for an hour", "two dirty"]);
        assert_eq!(j.attention, Some(0.7));
        assert_eq!(j.reading, key_of(&f));
    }

    #[test]
    fn an_unavailable_plugin_is_no_judgement() {
        let f = three();
        let r = parse(&f, r#"{"available":false,"reason":"no key"}"#);
        assert_eq!(r, None);
    }

    #[test]
    fn an_answer_about_another_reading_is_refused_not_kept() {
        // a wrong answer, not a stale one — keying it to what we asked would
        // launder somebody else's judgement into this bar.
        let f = three();
        let r = parse(&f, r#"{"available":true,"reading":"99","ranked":["f0"]}"#);
        assert_eq!(r, None);
    }

    #[test]
    fn a_missing_attention_never_becomes_zero() {
        // 0.0 would read as "nobody is needed" — a claim nothing made.
        let f = three();
        let j = parse(&f, &body(&f, r#""ranked":["f0"],"attention":null"#)).expect("parsed");
        assert_eq!(j.attention, None);
        assert_eq!(diamond(true, Some(&j), &f), Diamond::Undeclared);
    }

    #[test]
    fn an_attention_outside_zero_to_one_is_dropped() {
        let f = three();
        for bad in ["1.4", "-0.2"] {
            let j = parse(
                &f,
                &body(&f, &format!(r#""ranked":["f0"],"attention":{bad}"#)),
            )
            .expect("parsed");
            assert_eq!(j.attention, None, "accepted {bad}");
        }
    }

    #[test]
    fn an_id_for_a_frame_that_does_not_exist_is_dropped() {
        let f = three();
        let j = parse(&f, &body(&f, r#""ranked":["f9","f0","nonsense"]"#)).expect("parsed");
        assert_eq!(j.order, vec!["two dirty"]);
    }

    #[test]
    fn nothing_usable_is_no_judgement() {
        let f = three();
        assert_eq!(parse(&f, &body(&f, r#""ranked":[]"#)), None);
        assert_eq!(parse(&f, "not json at all"), None);
        assert_eq!(parse(&f, "{}"), None);
    }

    #[test]
    fn the_payload_carries_no_question_wording() {
        // the promise plugins.rs makes: nothing in this crate knows what the
        // model is being asked. The rail sends facts and its own frame texts.
        let f = three();
        let p = payload(&bare_state(), &f).to_string();
        for leak in ["How ", "Rate ", "probability", "criteria", "instructions"] {
            assert!(!p.contains(leak), "payload carries {leak:?}");
        }
        assert!(p.contains("two dirty"));
        assert!(p.contains("\"f0\""));
    }

    #[test]
    fn a_frame_is_sent_with_the_severity_the_rail_gave_it() {
        let mut f = three();
        f[1].tone = Tone::Warn;
        let p = payload(&bare_state(), &f);
        let sent = p["frames"].as_array().expect("frames");
        assert_eq!(sent[0]["tone"], "plain");
        assert_eq!(sent[1]["tone"], "warn");
        // and every frame carries one — a missing tone would read as no
        // opinion when the rail always has one.
        assert!(sent.iter().all(|x| x["tone"].is_string()));
    }

    /// A body the real plugin really returned, kept verbatim.
    ///
    /// Recorded 2026-09-23 from `jev-mcp` on the JEV project's live reading,
    /// 346 ms through OpenRouter. Every other body in this file is one I wrote,
    /// which only ever proves my own spelling — this one proves theirs, and it
    /// is the only fixture here that would notice the plugin changing shape.
    ///
    /// **One field is edited, and only one.** The real body ends with a
    /// `backend` naming the model, and pasting it here tripped
    /// `plugins::source_says_nothing_about_jev_but_its_name` — the gate that
    /// keeps a model id out of this crate — within minutes of the guard being
    /// written. It was right to. `parse` never reads that field, so the value
    /// is elided and every field the parser actually touches is verbatim.
    const LIVE_BODY: &str = r#"{"available": true, "reason": null, "reading": "17250913310000000001", "attention": 0.66, "ranked": ["f1", "f2", "f3", "f0", "f4", "f5", "f6"], "band": 1.5, "unjudged": [], "frames": [{"id": "f0", "kind": "Repos", "level": "background", "position": 0.97, "reason": null}, {"id": "f1", "kind": "Branches", "level": "worth_noticing", "position": 1.62, "reason": null}, {"id": "f2", "kind": "Shared", "level": "worth_noticing", "position": 2.24, "reason": null}, {"id": "f3", "kind": "Shared", "level": "act_soon", "position": 2.73, "reason": null}, {"id": "f4", "kind": "Foreign", "level": "background", "position": 1.23, "reason": null}, {"id": "f5", "kind": "Pulse", "level": "background", "position": 1.33, "reason": null}, {"id": "f6", "kind": "Sentence", "level": "background", "position": 1.24, "reason": null}], "backend": "<elided: a model id, which this crate may not carry>"}"#;

    /// The same reading's frames, in git's order, with git's tones.
    fn live_frames() -> Vec<Frame> {
        let mut f = vec![
            frame(FrameKind::Repos, "2 repositories"),
            frame(FrameKind::Branches, "main \u{2190} master"),
            frame(FrameKind::Shared, "master is shared by 3 writers"),
            frame(FrameKind::Shared, "main is shared by 3 writers"),
            frame(
                FrameKind::Foreign,
                "\u{26a0} ideas spitball is operating elsewhere",
            ),
            frame(FrameKind::Pulse, "5 commits in the last hour"),
            frame(FrameKind::Sentence, "1 lines of work, one shared"),
        ];
        // as engstate writes them: shared and foreign are warnings
        f[2].tone = Tone::Warn;
        f[3].tone = Tone::Warn;
        f[4].tone = Tone::Warn;
        f
    }

    #[test]
    fn the_real_plugins_real_answer_parses() {
        let f = live_frames();
        // the recorded body names a reading of its own; re-key it to these
        // frames so the echo check passes and the ranking is what is tested.
        let body = LIVE_BODY.replace("17250913310000000001", &key_of(&f).to_string());
        let j = parse(&f, &body).expect("the live body parses");
        assert_eq!(j.attention, Some(0.66));
        assert_eq!(j.order.len(), 7, "every frame was ranked");
        assert_eq!(j.order[0], "main \u{2190} master");
    }

    #[test]
    fn a_warning_is_never_sunk_beneath_a_line_that_is_not_one() {
        // THE LIVE CASE: the Foreign frame came back "background" at 1.23 and
        // the plugin ranked it below a repository count. Tone is a fact git
        // measured; the level is an opinion about it, and the fact wins.
        let f = live_frames();
        let body = LIVE_BODY.replace("17250913310000000001", &key_of(&f).to_string());
        let j = parse(&f, &body).expect("parsed");
        let out = order(f, Some(&j));
        let warn_last = out
            .iter()
            .rposition(|x| x.tone == Tone::Warn)
            .expect("warns");
        let plain_first = out
            .iter()
            .position(|x| x.tone != Tone::Warn)
            .expect("plains");
        assert!(
            warn_last < plain_first,
            "a warning sorted below a non-warning: {:?}",
            out.iter().map(|x| (&x.text, x.tone)).collect::<Vec<_>>()
        );
        // and the model still orders the warnings among themselves
        assert_eq!(out[0].text, "master is shared by 3 writers");
    }

    #[test]
    fn the_model_may_still_promote_within_a_tone() {
        // the floor must not freeze git's order outright — that would make the
        // whole ranking a no-op for every ordinary reading.
        let f = three(); // all Tone::Plain
        let j = judging(&f, &["quiet for an hour", "two converging"], None);
        let out = order(f, Some(&j));
        assert_eq!(out[0].text, "quiet for an hour");
        assert_eq!(out[1].text, "two converging");
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
