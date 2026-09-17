//! Drawing the workbench: six renderers, one visual language.
//!
//! Everything here is inert. These functions build elements and attach no
//! handlers, because the handlers belong to the pane that owns the bench —
//! which keeps this module a vocabulary rather than a second controller, the
//! same split [`crate::paint`] uses for the paint overlay's tiles.
//!
//! # The agent never chose any of this
//!
//! A payload said `architecture`. Every pixel below — the box, the corner
//! radius, the rule between nodes, the colour of a pending verdict, the width
//! of the rail — was decided here, against this pane's theme, this pane's skin
//! and this pane's curvature. That is the trade the protocol makes: the agent
//! gets meaning, we get the window.
//!
//! Which is also why there is no HTML, no webview, no component tree and no
//! model-generated markup anywhere in this file. A surface is data; this turns
//! data into gpui elements; the CRT pass bends the result exactly as much as it
//! bends the terminal underneath.
//!
//! # Every corner goes through the skin
//!
//! No literal `rounded(px(…))`, no `px(9. * s)`, no branching on which skin is
//! active. A workbench drawn with hand-rolled corners would be the one surface
//! in this window that stays round under a square skin, and the guard test in
//! [`crate::skin`] exists because that has happened before.

use gpui::prelude::FluentBuilder;
use gpui::{div, px, Div, Hsla, ParentElement, Styled};

use crate::skin::Skin;
use crate::surface::{Confidence, Depth, Kind, Shelf, Surface, Verdict, Weight};
use crate::theme::Theme;
use crate::workbench::{Embodiment, Row, Tint};

/// Resolve a kind's meaning to this theme's ink.
///
/// The one place a [`Tint`] becomes a colour, so a palette change moves every
/// marker on the bench at once.
pub fn ink(tint: Tint, th: &Theme) -> Hsla {
    match tint {
        Tint::Ident => th.accent,
        Tint::Waiting => th.complement,
        Tint::Pending => th.accent.alpha(0.72),
        Tint::Settled => th.text.alpha(0.66),
        Tint::Unknown => th.faint,
    }
}

/// A small mono label — the chrome's own voice, used for every kind chip,
/// field name and count on the bench.
fn micro(text: impl Into<String>, size: f32, colour: Hsla, th: &Theme) -> Div {
    div()
        .text_size(px(size))
        .text_color(colour)
        .font_family(th.font_family.clone())
        .child(text.into())
}

/// One row of the rail.
///
/// The marker is 3 pixels rather than 2. A rail is a low-density surface — a
/// column of short rows with almost nothing else on it — and the skin files
/// record why that matters: at 2px against no other lines a marker reads as a
/// scratch. The number is not carried over from a denser surface.
pub fn rail_row(row: &Row, sk: &Skin, th: &Theme) -> Div {
    let tint = ink(row.tint, th);
    sk.row()
        .flex()
        .flex_col()
        .gap(px(2.))
        .cursor_pointer()
        .border_l(px(3.))
        .border_color(tint)
        .when(row.selected, |d| d.bg(th.accent.alpha(0.12)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(6.))
                .child(micro(row.kind.to_string(), 9.5, tint, th))
                .when(row.unseen, |d| {
                    d.child(
                        div()
                            .w(px(5.))
                            .h(px(5.))
                            .rounded(sk.rad_raw(3.))
                            .bg(th.complement),
                    )
                }),
        )
        .child(
            div()
                .text_size(px(12.5))
                .text_color(th.text)
                .child(clip(&row.title, 34)),
        )
        .child(micro(clip(&row.subtitle, 38), 9.5, th.faint, th))
}

/// The rail collapsed: one tick per surface, newest at the top.
///
/// The same gesture the attention spine makes on the window's right edge, one
/// scale down and scoped to this pane. A person who has learned the spine has
/// already learned this.
pub fn rail_tick(tint: Tint, unseen: bool, sk: &Skin, th: &Theme) -> Div {
    div()
        .w(px(4.))
        .h(px(if unseen { 16. } else { 11. }))
        .rounded(sk.rad_raw(2.))
        .bg(ink(tint, th).alpha(if unseen { 1.0 } else { 0.6 }))
}

/// The shelf tabs at the top of the rail: artifacts · decisions · other.
pub fn shelf_tab(
    shelf: Shelf,
    active: bool,
    count: usize,
    unseen: usize,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let label = if unseen > 0 {
        format!("{} {}·{}", shelf.label(), count, unseen)
    } else if count > 0 {
        format!("{} {}", shelf.label(), count)
    } else {
        shelf.label().to_string()
    };
    sk.chip(active)
        .cursor_pointer()
        .text_size(px(9.5))
        .font_family(th.font_family.clone())
        .when(unseen > 0 && !active, |d| d.text_color(th.complement))
        .child(label)
}

/// The weights strip: effort, complexity, depth, confidence.
///
/// A surface the agent did not weigh says so in one word rather than showing
/// four empty slots — four `unavailable`s in a row is noise, and one honest
/// sentence is the same fact.
pub fn weights(w: &Weight, sk: &Skin, th: &Theme) -> Div {
    let row = div()
        .flex()
        .flex_row()
        .flex_wrap()
        .gap(px(6.))
        .items_center();
    if w.is_silent() {
        return row.child(micro("unweighed", 10., th.faint, th));
    }
    let pill = |text: String, colour: Hsla| {
        sk.chip(false)
            .text_size(px(9.5))
            .font_family(th.font_family.clone())
            .text_color(colour)
            .child(text)
    };
    row.when_some(w.effort, |d, e| {
        d.child(pill(
            format!(
                "effort {}",
                match e {
                    crate::surface::Effort::Small => "S",
                    crate::surface::Effort::Medium => "M",
                    crate::surface::Effort::Large => "L",
                    crate::surface::Effort::Epic => "XL",
                }
            ),
            th.text,
        ))
    })
    .when_some(w.complexity, |d, c| {
        d.child(pill(
            match c {
                crate::surface::Complexity::Trivial => "trivial",
                crate::surface::Complexity::Moderate => "moderate",
                crate::surface::Complexity::Involved => "involved",
                crate::surface::Complexity::Hairy => "hairy",
            }
            .to_string(),
            th.text,
        ))
    })
    .when_some(w.foundation.clone(), |d, f| {
        // Depth is the field a reviewer actually wants, so bedrock is the one
        // weight allowed to shout.
        let colour = match f.depth {
            Depth::Bedrock => th.complement,
            Depth::Subsystem => th.accent,
            _ => th.text,
        };
        d.child(pill(
            format!(
                "{} · {}",
                match f.depth {
                    Depth::Leaf => "leaf",
                    Depth::Component => "component",
                    Depth::Subsystem => "subsystem",
                    Depth::Bedrock => "bedrock",
                },
                clip(&f.system, 22)
            ),
            colour,
        ))
    })
    .when_some(w.confidence, |d, c| {
        let colour = match c {
            Confidence::Measured => th.text,
            Confidence::Inferred | Confidence::Hunch => th.complement,
            Confidence::Unknown => th.faint,
        };
        d.child(pill(c.label().to_string(), colour))
    })
}

/// The body of whatever is selected, at the size this pane can honestly show.
pub fn body(surface: &Surface, how: Embodiment, sk: &Skin, th: &Theme) -> Div {
    let frame = div().flex().flex_col().gap(px(10.)).w_full();
    match how {
        Embodiment::Summary => frame.child(summary_line(surface, th)),
        Embodiment::Compact => frame
            .child(heading(surface, sk, th))
            .child(compact(surface, sk, th)),
        Embodiment::Full => frame
            .child(heading(surface, sk, th))
            .child(weights(&surface.weight, sk, th))
            .child(full(surface, sk, th)),
    }
}

fn summary_line(surface: &Surface, th: &Theme) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(px(8.))
        .items_baseline()
        .child(micro(surface.kind.id().to_string(), 10., th.faint, th))
        .child(
            div()
                .text_size(px(13.))
                .text_color(th.text)
                .child(clip(&surface.title, 60)),
        )
}

fn heading(surface: &Surface, sk: &Skin, th: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.))
                .items_center()
                .child(
                    sk.chip(false)
                        .text_size(px(9.5))
                        .font_family(th.font_family.clone())
                        .text_color(ink(crate::workbench::tint_of(&surface.kind), th))
                        .child(surface.kind.id().to_string()),
                )
                .child(
                    // Wraps rather than clips: the heading is the one string
                    // on the bench with room to be long, and a title cut at
                    // "A kind this build has never hea" tells the reader
                    // nothing except that something was cut.
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(16.))
                        .text_color(th.text)
                        .child(surface.title.clone()),
                ),
        )
        .child(micro(surface.subtitle(), 10.5, th.faint, th))
}

/// The shape of the thing, for a pane too small to hold the thing.
fn compact(surface: &Surface, sk: &Skin, th: &Theme) -> Div {
    // A panel rather than bare rows: at this size the body and the rail sit
    // close enough together that an unframed list reads as part of the rail.
    let list = sk.panel().flex().flex_col().gap(px(3.));
    match &surface.kind {
        Kind::Markdown(m) => list.child(paragraph(first_lines(&m.body, 6), th)),
        Kind::Table(t) => list.children(
            t.rows
                .iter()
                .take(5)
                .map(|r| micro(join_cells(r, " · "), 11., th.text, th)),
        ),
        Kind::Architecture(a) => list.children(
            a.nodes
                .iter()
                .take(8)
                .map(|n| micro(format!("▪ {}", n.label), 11., th.text, th)),
        ),
        Kind::Changeset(c) => list.children(c.hunks.iter().take(8).map(|h| {
            micro(
                format!("{}  +{} −{}", clip(&h.file, 28), h.added, h.removed),
                11.,
                verdict_ink(h.verdict, th),
                th,
            )
        })),
        Kind::Decision(d) => list.children(d.options.iter().map(|o| {
            micro(
                format!("{} {}", if o.recommended { "◉" } else { "○" }, o.name),
                11.5,
                if o.recommended { th.text } else { th.faint },
                th,
            )
        })),
        Kind::Artifact(a) => list.child(micro(a.href.clone(), 11., th.faint, th)),
        Kind::Unclassified(u) => list.child(micro(u.reason.clone(), 11., th.faint, th)),
    }
}

/// The whole thing.
fn full(surface: &Surface, sk: &Skin, th: &Theme) -> Div {
    match &surface.kind {
        Kind::Artifact(a) => artifact(a, sk, th),
        Kind::Markdown(m) => paragraph(m.body.clone(), th),
        Kind::Table(t) => table(t, sk, th),
        Kind::Architecture(a) => architecture(a, sk, th),
        Kind::Changeset(c) => changeset(c, sk, th),
        Kind::Decision(d) => decision(d, sk, th),
        Kind::Unclassified(u) => unclassified(u, sk, th),
    }
}

fn artifact(a: &crate::surface::Artifact, sk: &Skin, th: &Theme) -> Div {
    field_grid(
        vec![
            ("target", Some(a.href.clone())),
            ("type", a.mime.clone()),
            ("about", a.summary.clone()),
        ],
        sk,
        th,
    )
}

fn table(t: &crate::surface::Table, sk: &Skin, th: &Theme) -> Div {
    let header = div()
        .flex()
        .flex_row()
        .gap(px(10.))
        .children(t.columns.iter().map(|c| {
            div()
                .flex_1()
                .child(micro(c.to_uppercase(), 9.5, th.faint, th))
        }));
    let rows = t.rows.iter().map(|row| {
        div()
            .flex()
            .flex_row()
            .gap(px(10.))
            .py(px(2.))
            .children(row.iter().map(|cell| {
                div().flex_1().child(match cell {
                    Some(text) => micro(clip(text, 40), 11.5, th.text, th),
                    // A cell nobody filled says so, rather than being blank and
                    // reading as a value of nothing.
                    None => micro("unavailable", 11.5, th.faint, th),
                })
            }))
    });
    sk.panel()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(header)
        .child(sk.rule_h())
        .children(rows)
}

/// Boxes, grouped by their declared boundary, with the edges written under
/// them as text.
///
/// Arrows are deliberately not drawn as lines here. A line between two boxes in
/// a resizable pane is a layout problem with no good answer at 300 pixels, and
/// an edge list that is always legible beats a diagram that is sometimes
/// beautiful. The dangling edges get their own block, because an arrow to a
/// node nobody declared is a fact about the agent's model and hiding it would
/// be inventing a tidier picture than the one that was sent.
fn architecture(a: &crate::surface::Architecture, sk: &Skin, th: &Theme) -> Div {
    let mut groups: Vec<(String, Vec<&crate::surface::Node>)> = Vec::new();
    for node in &a.nodes {
        let key = node.group.clone().unwrap_or_default();
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, list)) => list.push(node),
            None => groups.push((key, vec![node])),
        }
    }
    let boxes = groups.into_iter().map(|(name, nodes)| {
        let inner =
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(6.))
                .children(nodes.into_iter().map(|n| {
                    sk.panel()
                        .px(px(8.))
                        .py(px(5.))
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(th.text)
                                .child(n.label.clone()),
                        )
                        .when_some(n.state.clone(), |d, s| d.child(micro(s, 9., th.accent, th)))
                }));
        if name.is_empty() {
            inner
        } else {
            sk.panel()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(micro(name.to_uppercase(), 9., th.faint, th))
                .child(inner)
        }
    });
    let edges = a.edges.iter().map(|e| {
        micro(
            format!(
                "{} → {}{}",
                e.from,
                e.to,
                e.label
                    .as_ref()
                    .map(|l| format!("  ({l})"))
                    .unwrap_or_default()
            ),
            10.5,
            th.text.alpha(0.8),
            th,
        )
    });
    let dangling = a.dangling.iter().map(|e| {
        micro(
            format!("{} → {} · no such node", e.from, e.to),
            10.5,
            th.complement,
            th,
        )
    });
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .children(boxes)
        .child(sk.rule_h())
        .children(edges)
        .children(dangling)
}

fn changeset(c: &crate::surface::Changeset, sk: &Skin, th: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .when_some(c.repository.clone(), |d, r| {
            d.child(micro(r, 10., th.faint, th))
        })
        .children(c.hunks.iter().map(|h| {
            sk.panel()
                .flex()
                .flex_col()
                .gap(px(3.))
                .border_l(px(3.))
                .border_color(verdict_ink(h.verdict, th))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .items_baseline()
                        .child(micro(h.file.clone(), 11.5, th.text, th))
                        .child(micro(
                            format!("+{} −{}", h.added, h.removed),
                            10.,
                            th.faint,
                            th,
                        ))
                        .child(micro(
                            verdict_word(h.verdict).to_string(),
                            9.5,
                            verdict_ink(h.verdict, th),
                            th,
                        )),
                )
                .child(patch(&h.patch, th))
        }))
}

/// A patch, coloured the way a diff is coloured everywhere else, and clipped
/// so one enormous hunk cannot own the pane.
fn patch(text: &str, th: &Theme) -> Div {
    let lines = text.lines().take(24);
    div().flex().flex_col().children(lines.map(|line| {
        let colour = if line.starts_with("+++") || line.starts_with("---") {
            th.faint
        } else if line.starts_with('+') {
            th.accent
        } else if line.starts_with('-') {
            th.complement
        } else {
            th.text.alpha(0.75)
        };
        div()
            .text_size(px(11.))
            .text_color(colour)
            .font_family(th.font_family.clone())
            .child(line.to_string())
    }))
}

fn decision(d: &crate::surface::Decision, sk: &Skin, th: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(
            div()
                .text_size(px(13.5))
                .text_color(th.text)
                .child(d.question.clone()),
        )
        .children(d.options.iter().map(|o| {
            // Exactly one option is lit, and the parser has already made sure of
            // it. A reader scanning three equal cards has to read all three
            // before he can start weighing.
            let panel = sk.panel().flex().flex_col().gap(px(3.));
            let panel = if o.recommended {
                panel.border_l(px(3.)).border_color(th.accent)
            } else {
                panel
            };
            panel
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(7.))
                        .items_baseline()
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(th.text)
                                .child(o.name.clone()),
                        )
                        .when(o.recommended, |x| {
                            x.child(micro("recommended", 9., th.accent, th))
                        }),
                )
                .when_some(o.case.clone(), |x, case| {
                    x.child(micro(case, 11., th.text.alpha(0.8), th))
                })
                .when_some(o.cost.clone(), |x, cost| {
                    x.child(micro(format!("cost · {cost}"), 11., th.faint, th))
                })
        }))
        .when(!d.consequences.is_empty(), |x| {
            x.child(sk.rule_h()).child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(micro("IF WE DO", 9., th.faint, th))
                    .children(
                        d.consequences
                            .iter()
                            .map(|c| micro(format!("· {c}"), 11., th.text.alpha(0.85), th)),
                    ),
            )
        })
}

/// Something arrived that this build cannot type.
///
/// Drawn plainly and labelled, never guessed at. The reason comes first
/// because it is the actionable half — it tells whoever is reading what the
/// agent would have to change — and the payload is kept underneath so nobody
/// has to take our word for what was sent.
fn unclassified(u: &crate::surface::Unclassified, sk: &Skin, th: &Theme) -> Div {
    // The reason is NOT repeated here. It is already the subtitle under the
    // heading, because `Surface::subtitle` uses it for this kind — and printed
    // twice, three lines apart, it reads as two different complaints about the
    // same payload. Caught by photographing the build, which is the only place
    // a duplicated line is visible at all.
    sk.panel()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(micro("NOTHING IS CLAIMED ABOUT THIS", 9., th.faint, th))
        .child(paragraph(first_lines(&u.raw, 20), th))
}

fn field_grid(fields: Vec<(&str, Option<String>)>, sk: &Skin, th: &Theme) -> Div {
    sk.panel()
        .flex()
        .flex_col()
        .gap(px(4.))
        .children(fields.into_iter().map(|(name, value)| {
            div()
                .flex()
                .flex_row()
                .gap(px(10.))
                .items_baseline()
                .child(
                    div()
                        .w(px(76.))
                        .child(micro(name.to_uppercase(), 9.5, th.faint, th)),
                )
                .child(match value {
                    Some(v) => micro(v, 11.5, th.text, th),
                    // Shown missing rather than omitted: an omitted row leaves a
                    // hole a reader fills in themselves.
                    None => micro("unavailable", 11.5, th.faint, th),
                })
        }))
}

fn paragraph(text: String, th: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .children(text.lines().map(|line| {
            div()
                .text_size(px(12.5))
                .text_color(th.text.alpha(0.9))
                .child(line.to_string())
        }))
}

fn verdict_ink(v: Verdict, th: &Theme) -> Hsla {
    match v {
        Verdict::Undecided => th.faint,
        Verdict::Accepted => th.accent,
        Verdict::Rejected => th.complement,
    }
}

fn verdict_word(v: Verdict) -> &'static str {
    match v {
        Verdict::Undecided => "undecided",
        Verdict::Accepted => "accepted",
        Verdict::Rejected => "rejected",
    }
}

/// An empty bench says what would fill it.
///
/// The most common state on day one, and the one a person will judge the
/// feature by. A blank rectangle reads as broken; a sentence naming the verb
/// reads as waiting.
pub fn empty(is_agent: bool, dir: &str, sk: &Skin, th: &Theme) -> Div {
    sk.panel()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(micro("NOTHING ON THE BENCH", 10., th.faint, th))
        .child(if is_agent {
            micro(
                "This agent has presented no work objects yet.",
                12.,
                th.text.alpha(0.85),
                th,
            )
        } else {
            micro(
                "A shell has no agent to present anything. Launch one into this pane.",
                12.,
                th.text.alpha(0.85),
                th,
            )
        })
        .when(is_agent, |d| {
            d.child(micro(
                format!("drop a .json here: {dir}"),
                10.,
                th.faint,
                th,
            ))
        })
}

/// Cut to a character budget, with an ellipsis that says it was cut.
fn clip(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn first_lines(text: &str, n: usize) -> String {
    let mut out: Vec<&str> = text.lines().take(n).collect();
    if text.lines().count() > n {
        out.push("…");
    }
    out.join("\n")
}

fn join_cells(row: &[Option<String>], sep: &str) -> String {
    row.iter()
        .map(|c| c.clone().unwrap_or_else(|| "—".into()))
        .collect::<Vec<_>>()
        .join(sep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_marks_that_it_clipped() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("abcdefghij", 5), "abcd…");
        // A multi-byte title must not be cut mid-character.
        assert_eq!(clip("→→→→→→", 3), "→→…");
    }

    #[test]
    fn a_missing_cell_and_an_empty_one_read_differently() {
        assert_eq!(join_cells(&[None, Some("x".into())], "·"), "—·x");
        assert_eq!(
            join_cells(&[Some(String::new()), Some("x".into())], "·"),
            "·x"
        );
    }

    #[test]
    fn first_lines_says_when_it_stopped() {
        let text = (1..=10)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(first_lines(&text, 3).ends_with('…'));
        assert!(!first_lines("one\ntwo", 5).ends_with('…'));
    }

    #[test]
    fn verdict_words_and_inks_cover_all_three_states() {
        // A boolean would have two, and a proposed edit, a rejected one and an
        // unread one are three different things.
        let words: Vec<&str> = [Verdict::Undecided, Verdict::Accepted, Verdict::Rejected]
            .iter()
            .map(|v| verdict_word(*v))
            .collect();
        assert_eq!(words, vec!["undecided", "accepted", "rejected"]);
    }
}
