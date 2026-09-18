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
use gpui::{
    div, point, px, BoxShadow, Div, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled,
};

use crate::skin::{Role, Skin};
use crate::surface::{
    Body, Confidence, Depth, Kind, Register, Response, Shelf, Surface, SurfaceId, Verdict, Weight,
};
use crate::theme::Theme;
use crate::workbench::{Embodiment, Row, Step, Tint};

/// What a response renderer needs to draw folds it cannot decide for itself:
/// whose sections these are, which of them are open, and where to register
/// the headers as click targets. Absent, every section is drawn folded with
/// no target — the summary and compact bodies, and any caller that has no
/// zone list to offer.
pub struct Folds<'a> {
    pub id: &'a SurfaceId,
    pub open: &'a dyn Fn(&crate::surface::Section) -> bool,
    /// The register the reader last opened — the one row the card lights.
    /// `None` before they have opened anything, which draws nothing lit.
    pub lit: Option<&'a str>,
    pub zones: std::rc::Rc<std::cell::RefCell<Vec<crate::workbench::Zone>>>,
}

/// Which palette role each meaning borrows.
///
/// A table, and a table of [`Role`]s rather than of colours, because `Role` is
/// the skin's existing vocabulary for "a name the palette answers to" — so the
/// day a skin file wants to re-cast the bench, this is already the shape it
/// would set, and nothing here has to change to let it.
///
/// **The roles are the theme's SECONDARY inks, not the accent.** The first
/// version resolved three of these five to the accent and one to its
/// complement, and the result is what a bench looked like on a magenta theme:
/// magenta markers on magenta chips beside a magenta border, with the colour
/// carrying no information because there was only ever one of it. Parker,
/// looking at that: *"The colour palette should REALLY leverage the secondary
/// colours of the theme!"*
///
/// The hues are his own, from the house palette he uses in every decision
/// brief — red a decision is waiting, green ready, amber yours to argue with,
/// blue structure, grey unknown — mapped onto the ANSI slots so they arrive in
/// each theme's OWN red and green rather than in a hard-coded one. A terminal
/// palette is built to be mutually distinguishable; borrowing it is how the
/// bench gets four separable hues on every theme for free.
fn role_of(tint: Tint) -> Role {
    match tint {
        // Documents, drawings, tables: structure.
        Tint::Ident => Role::Ansi(12),
        // A person is the blocker.
        Tint::Waiting => Role::Ansi(9),
        // Proposed, and yours to argue with.
        Tint::Pending => Role::Ansi(11),
        // Settled, accepted, done.
        Tint::Settled => Role::Ansi(10),
        // Grey, and deliberately not a hue: an unknown that arrives in a
        // colour looks like a claim, and no claim has been made.
        Tint::Unknown => Role::Faint,
    }
}

/// Resolve a kind's meaning to this theme's ink.
///
/// The one place a [`Tint`] becomes a colour, so a palette change moves every
/// marker on the bench at once.
pub fn ink(tint: Tint, th: &Theme) -> Hsla {
    role_of(tint).of(th)
}

/// (see the two functions below)
///
/// One function, used by every layered thing on the bench — the waiting
/// block, the card, the composer, the rail — so depth is a property of the
/// vocabulary rather than a decision taken four times with four different
/// numbers. Add a fifth layer later and it arrives already looking like the
/// other four.
///
/// The glow rides `th.glow`, the same dial the header's own bloom uses, so a
/// flat theme stays flat and a phosphor theme gets phosphor without this
/// having an opinion of its own. `tint` is the element's meaning-colour,
/// which is what makes a question bloom in the complement and a document in
/// the accent: the depth carries the same information the border does.
/// Depth, and deliberately no phosphor.
///
/// It takes a tint and a theme it does not read, so that it and [`aglow`] are
/// interchangeable — a renderer that glows only while a thing is waiting picks
/// one of the two by name and calls it, rather than branching around two
/// different shapes of call. The unused arguments are the price of that, and
/// they are cheaper than the branch.
pub fn raised<E: Styled>(el: E, _tint: Hsla, _th: &Theme) -> E {
    el.shadow(depth())
}

/// The drop and the inner edge — what makes a surface read as a solid face
/// rather than as a rectangle of a different colour. Every layered thing on
/// the bench gets this; it is depth, and depth is free.
fn depth() -> Vec<BoxShadow> {
    vec![
        // The layer casting onto what it covers.
        BoxShadow {
            color: gpui::black().alpha(0.42),
            offset: point(px(0.), px(2.)),
            blur_radius: px(14.),
            spread_radius: px(0.),
            inset: false,
        },
        // A bright inner top edge — the same reflection the pane header draws.
        BoxShadow {
            color: gpui::white().alpha(0.06),
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        },
    ]
}

/// The attention spine's own frame, applied to something on the bench.
///
/// **Copied, not re-invented.** The right-hand spine is the surface on this
/// machine that already knows how to say *look at this* — a real two-pixel
/// border in the meaning colour, an opaque darkened fill, a crisp outer ring,
/// a soft phosphor bloom and two shadows underneath it. Parker, after the
/// title card got bigger and no more urgent: *"ATTENTION is more than mere
/// size... Look - thicker brighter phosphor... shaded layers... use the right
/// attention spine as the gold standard!"*.
///
/// So it uses the spine's own [`crate::float_shadows`] rather than a second
/// recipe that agrees with it today. A copy would drift the first time either
/// was touched, and the bench would slowly stop looking like the window it
/// lives in.
///
/// `strength` dims the whole thing for a state that is present without being
/// urgent: a frame that shouts at Idle is a frame nobody reads at Waiting.
pub fn spine_frame<E: Styled>(el: E, tint: Hsla, strength: f32, sk: &Skin, th: &Theme) -> E {
    let lit = tint.alpha(0.85 * strength);
    el.rounded(sk.rad_raw(8.))
        .border_2()
        .border_color(lit)
        .bg(crate::darken(th.surface, 0.45))
        .shadow(crate::float_shadows(tint.alpha(strength)))
}

/// Depth AND the tube's phosphor — for the one thing on a surface that is
/// asking to be looked at.
///
/// Split from [`raised`] because it was being spent on everything: the title
/// card, the waiting block, the opened card, the composer, the rail's head row
/// and every option chip could bloom at once, and a screen where six things
/// glow has told the reader nothing. Parker: *"the amount of glow is just way
/// too much ... glow should MEAN something, this is noise"*.
///
/// The budget is one per REGION — the head of the rail, the thing waiting on
/// you in the body, the primary action on a card — and everything else takes
/// depth, which separates surfaces without making a claim about attention.
pub fn aglow<E: Styled>(el: E, tint: Hsla, th: &Theme) -> E {
    aglow_at(el, tint, 1.0, th)
}

/// [`aglow`], at a fraction of its strength.
///
/// The budget is still one bloom per region; this is how loud that one bloom is.
/// A register the reader last opened is *lit* — it marks where they are — and an
/// escalation is *shouting*, and drawing both at the same intensity made the
/// bookmark look like a summons. Parker: *"this light should be on the most
/// recent clicked and be about 1/2 the intensity"*.
///
/// `strength` scales the bloom's alpha and its spread together, so a half-lit
/// thing is smaller as well as dimmer — halving only the alpha leaves a
/// same-sized halo that still draws the eye from across a pane.
pub fn aglow_at<E: Styled>(el: E, tint: Hsla, strength: f32, th: &Theme) -> E {
    let mut shadows = depth();
    let strength = strength.clamp(0., 1.);
    if th.glow > 0.001 && strength > 0.001 {
        shadows.push(BoxShadow {
            color: tint.alpha((th.glow * 0.45 * strength).min(0.5)),
            offset: point(px(0.), px(0.)),
            blur_radius: px(22. * strength.max(0.5)),
            spread_radius: px(strength),
            inset: false,
        });
    }
    el.shadow(shadows)
}

/// A small mono label — the chrome's own voice, used for every kind chip,
/// field name and count on the bench.
fn micro(text: impl Into<String>, step: Step, colour: Hsla, sk: &Skin, th: &Theme) -> Div {
    div()
        .text_size(px(sk.pt(step)))
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
    use crate::workbench::Standing;
    let tint = ink(row.tint, th);
    // The spine's own row, read out of `Workspace::rail_panel` rather than
    // approximated from a screenshot. Parker, twice: *"SERIOUSLY LOOK AND
    // STUDY EXACTLY WHAT OUR RIGHT ATTENTION SPINE IS DOING"*.
    //
    // Its anatomy, and every part of it earns its place there:
    //
    // - a seven-pixel slot holding a filled dot in the lane's colour when the
    //   row is unseen and NOTHING when it is not, rather than a hollow one —
    //   two glyphs would make a reader learn a vocabulary to read four things
    // - the LANE word, uppercased, small, in the lane's colour
    // - what it belongs to, dimmer and larger, beside it
    // - a spacer, then the AGE at the right edge
    // - the headline underneath at full strength
    // - a provenance line under that: where the fact came from and when
    // - a fill and a left edge that both get heavier on the cursor's row, in
    //   the lane's own colour and never a second accent hue
    //
    // The bench's lane is its STANDING and its origin is its format, so the
    // same skeleton carries different bones.
    // Heavier for the row the eye should land on: the one the keyboard is on,
    // OR the head of the queue. The spine's cursor edge marks where the
    // keyboard is; on this rail the head of a shelf earns the same weight,
    // because a person arriving has not moved a cursor yet and still needs to
    // be told where to start.
    //
    // `Standing::lit` is the single place that decides, and a test walks seven
    // shelf shapes demanding at most one row claims it — so this can never be
    // the emphasis on two rows at once.
    let on_cursor = row.selected || row.standing.lit();
    let lane = match row.standing {
        Standing::Waiting => Some("WAITING ON YOU"),
        Standing::Queued => Some("ALSO WAITING"),
        Standing::Current => Some("STANDS NOW"),
        Standing::Past => match (row.tint, row.kind) {
            (Tint::Settled, "question") | (Tint::Settled, "decision") => Some("ANSWERED"),
            (Tint::Settled, _) => Some("DONE"),
            _ => None,
        },
    };
    let head = div()
        .flex()
        .flex_row()
        .gap(px(6.))
        .items_center()
        .child(
            div()
                .w(px(7.))
                .flex_none()
                .text_size(px(sk.pt(Step::Tag)))
                .text_color(tint)
                .child(if row.unseen { "\u{25cf}" } else { "" }),
        )
        .children(lane.map(|l| {
            div()
                .flex_none()
                .text_size(px(sk.pt(Step::Tag)))
                .text_color(tint)
                .child(l)
        }))
        .children(row.badge.clone().map(|b| {
            div()
                .flex_none()
                .text_size(px(sk.pt(Step::Fine)))
                .text_color(th.faint)
                .child(b)
        }))
        // No age on the row. The spine puts one on every queue row because each
        // row there is a different pane; every row HERE belongs to one agent,
        // so a per-row age said the same fact as many times as there were rows
        // — Parker: *"those are fine, but repeating them is not... they should
        // be in the agent state as a collected SINGLE counter"*. It is on the
        // agent bar now, once, as how long the agent has been in its state.
        .child(div().flex_1());
    sk.row()
        .flex()
        .flex_col()
        .gap(px(1.))
        .pl(px(7.))
        .pr(px(6.))
        .py(px(5.))
        .border_l(px(if on_cursor { 4. } else { 2. }))
        .border_color(tint)
        .rounded(sk.radius())
        .bg(if on_cursor {
            th.text.alpha(0.16)
        } else {
            th.text.alpha(0.05)
        })
        .hover(move |st| st.bg(th.text.alpha(0.12)))
        .child(head)
        .child(
            div()
                .text_size(px(sk.pt(Step::Small)))
                .text_color(th.text)
                .child(row.title.clone()),
        )
        // Where this fact came from, and when. A surface that shows a state
        // without its provenance is asking to be trusted on nothing — the
        // spine's words, and the reason its rows read as evidence rather than
        // as assertions.
        .when(!row.subtitle.trim().is_empty(), |d| {
            d.child(
                div()
                    .text_size(px(sk.pt(Step::Tag)))
                    .text_color(th.faint)
                    .child(clip(&row.subtitle, 44)),
            )
        })
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
/// A shelf tab: its NAME, and nothing else.
///
/// It read `artifacts 1·1`, a total beside an unseen count, and the
/// pair answered a question nobody asks of a tab. Parker: *"the 1-1 and 3-3
/// enumerations needs to die... just titles"*. The rows beneath ARE the
/// count, and they are already on screen.
///
/// The unseen signal survives, because "something arrived while you were
/// elsewhere" is worth knowing and is what the second number was really for
/// — but it is carried by the word's own COLOUR rather than by a digit. One
/// glance, no arithmetic, and the tab stays a tab.
pub fn shelf_tab(
    shelf: Shelf,
    active: bool,
    _count: usize,
    unseen: usize,
    sk: &Skin,
    th: &Theme,
) -> Div {
    sk.chip(active)
        .text_size(px(sk.pt(Step::Note)))
        .font_family(th.font_family.clone())
        .when(unseen > 0 && !active, |d| {
            d.text_color(ink(crate::workbench::Tint::Waiting, th))
        })
        .child(shelf.label().to_string())
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
        return row.child(micro("unweighed", Step::Note, th.faint, sk, th));
    }
    let pill = |text: String, colour: Hsla| {
        sk.chip(false)
            .text_size(px(sk.pt(Step::Fine)))
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
            // Amber — yours to argue with. The same ink the doubts use, because
            // it is the same fact said about a different thing.
            Confidence::Inferred | Confidence::Hunch => ink(crate::workbench::Tint::Pending, th),
            Confidence::Unknown => th.faint,
        };
        d.child(pill(c.label().to_string(), colour))
    })
}

/// The body of whatever is selected, at the size this pane can honestly show.
pub fn body(
    surface: &Surface,
    how: Embodiment,
    folds: Option<&Folds>,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let frame = div().flex().flex_col().gap(px(10.)).w_full();
    // A QUESTION gets neither the subtitle nor the weights strip.
    //
    // `2 options · waiting on you` counts something the reader can see and
    // repeats what the label already said, and `unweighed` is the agent
    // declining to estimate a picker it did not declare. Both are true and
    // neither is worth a line in front of somebody who has been asked a
    // question. Parker: *"2 options (we can see it is 2 options, no need to
    // show this... if the machine needs it fine, but don't show user)"*.
    let asking = matches!(surface.kind, Kind::Question(_));
    // ABOVE THE TITLE, and above everything.
    //
    // The position is the point, not the colour: an escalation sorted among six
    // identical register panels can be scrolled past, and one pinned over the
    // card's own name cannot. If the agent is blocked, the reply is the
    // secondary thing on the card.
    //
    // A summary is one line by definition and never carries it. Answered, or
    // declared `none`, this is `None` and the card opens on its title as before
    // — the loudest thing on the surface has to be able to go away.
    let call = match &surface.kind {
        Kind::Response(r) => escalation_call(r).map(|call| (r, call)),
        _ => None,
    };
    let frame = match (how, call) {
        (Embodiment::Summary, _) | (_, None) => frame,
        (_, Some((r, call))) => match &r.escalation {
            Some(e) => frame.child(escalation(e, call, sk, th)),
            None => frame,
        },
    };
    match how {
        Embodiment::Summary => frame.child(summary_line(surface, sk, th)),
        Embodiment::Compact => frame
            .child(heading(surface, sk, th))
            .child(compact(surface, folds, sk, th)),
        Embodiment::Full if asking => frame
            .child(heading(surface, sk, th))
            .child(full(surface, folds, sk, th)),
        Embodiment::Full => frame
            .child(heading(surface, sk, th))
            .child(weights(&surface.weight, sk, th))
            .child(full(surface, folds, sk, th)),
    }
}

fn summary_line(surface: &Surface, sk: &Skin, th: &Theme) -> Div {
    div()
        .flex()
        .flex_row()
        .gap(px(8.))
        .items_baseline()
        .child(micro(
            surface.kind.id().to_string(),
            Step::Note,
            th.faint,
            sk,
            th,
        ))
        .child(
            div()
                .text_size(px(sk.pt(Step::Lead)))
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
                        .text_size(px(sk.pt(Step::Fine)))
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
                        .text_size(px(sk.pt(Step::Title)))
                        .text_color(th.text)
                        .child(surface.title.clone()),
                ),
        )
        // The subtitle, for the kinds whose subtitle says something a reader
        // cannot already see. A question's is a count of its own visible
        // options beside a state its own colour already carries, so it is
        // omitted rather than dimmed: a line nobody needs is noise at any
        // opacity. A response's subtitle is its gist, which the body draws
        // large directly underneath.
        .when(
            !matches!(surface.kind, Kind::Question(_) | Kind::Response(_)),
            |d| {
                d.child(micro(
                    surface.subtitle(),
                    Step::Note,
                    crate::emphasis::meta(th),
                    sk,
                    th,
                ))
            },
        )
        // WHO PUT IT HERE, on every card, and at full strength when nobody
        // can say: a surface that arrived from nowhere is the one to look at
        // twice, so the unknown is the loud one and the attributed is quiet.
        //
        // Both strengths are READABLE now. They were `th.faint` and
        // `th.faint.alpha(0.7)` — grey on grey, and then a fainter grey on the
        // same grey, so both cases were illegible and the distinction between
        // them carried nothing to anyone who could not read either.
        .child(micro(
            surface.origin.label(),
            Step::Fine,
            if surface.origin.is_unattributed() {
                crate::emphasis::meta(th)
            } else {
                crate::emphasis::meta(th).alpha(0.42)
            },
            sk,
            th,
        ))
}

/// The shape of the thing, for a pane too small to hold the thing.
fn compact(surface: &Surface, folds: Option<&Folds>, sk: &Skin, th: &Theme) -> Div {
    // A panel rather than bare rows: at this size the body and the rail sit
    // close enough together that an unframed list reads as part of the rail.
    let list = sk.panel().flex().flex_col().gap(px(3.));
    match &surface.kind {
        Kind::Markdown(m) => list.child(paragraph(first_lines(&m.body, 6), sk, th)),
        Kind::Table(t) => list.children(
            t.rows
                .iter()
                .take(5)
                .map(|r| micro(join_cells(r, " · "), Step::Small, th.text, sk, th)),
        ),
        Kind::Architecture(a) => list.children(
            a.nodes
                .iter()
                .take(8)
                .map(|n| micro(format!("▪ {}", n.label), Step::Small, th.text, sk, th)),
        ),
        Kind::Changeset(c) => list.children(c.hunks.iter().take(8).map(|h| {
            micro(
                format!("{}  +{} −{}", clip(&h.file, 28), h.added, h.removed),
                Step::Small,
                verdict_ink(h.verdict, th),
                sk,
                th,
            )
        })),
        Kind::Decision(d) => list.children(d.options.iter().map(|o| {
            micro(
                format!("{} {}", if o.recommended { "◉" } else { "○" }, o.name),
                Step::Small,
                if o.recommended { th.text } else { th.faint },
                sk,
                th,
            )
        })),
        // A question's options are the BUTTONS underneath, and listing
        // them here printed every one of them twice — six labels as text
        // directly above the same six as chips. Parker, on the second
        // time this shipped: *"Again -- repeating ourselves ... just
        // ummm... just the buttons"*.
        //
        // The full renderer was trimmed for exactly this and this one was
        // missed, so it now DELEGATES: one renderer for a question at
        // either size, and no second list of kinds to keep in step.
        Kind::Question(q) => question(q, sk, th),
        Kind::Artifact(a) => list.child(micro(a.href.clone(), Step::Small, th.faint, sk, th)),
        // DELEGATES, for the same reason the question does and then some.
        //
        // The compact form was a list of one-line section summaries: the right
        // information and no way to act on it, because a summary carries no
        // press target. That made the fold — the whole interaction this kind
        // exists for — silently unavailable at a width a tiled pane reaches
        // constantly. Measured on a half-monitor pane with the left bar
        // showing: about 394 points of content against a 460 threshold, so the
        // common case was the one with no affordance.
        //
        // A compact card may legitimately show LESS. It may not show a control
        // that is missing, which is what the reader reads as a broken feature
        // rather than as a small screen.
        Kind::Response(r) => response(r, folds, sk, th),
        Kind::Unclassified(u) => list.child(micro(u.reason.clone(), Step::Small, th.faint, sk, th)),
    }
}

/// The whole thing.
fn full(surface: &Surface, folds: Option<&Folds>, sk: &Skin, th: &Theme) -> Div {
    match &surface.kind {
        Kind::Artifact(a) => artifact(a, sk, th),
        Kind::Markdown(m) => paragraph(m.body.clone(), sk, th),
        Kind::Table(t) => table(t, sk, th),
        Kind::Architecture(a) => architecture(a, sk, th),
        Kind::Changeset(c) => changeset(c, sk, th),
        Kind::Decision(d) => decision(d, sk, th),
        Kind::Question(q) => question(q, sk, th),
        Kind::Response(r) => response(r, folds, sk, th),
        Kind::Unclassified(u) => unclassified(u, sk, th),
    }
}

/// The gist, as the first row of the reading shelf.
///
/// It used to be a banner — 15-point type on its own raised floor with the
/// accent down its edge. That made it outrank a technical brief the reader had
/// deliberately opened, and it spent the accent, which now means one thing only.
/// It is a [`crate::surface::Register`] like the others, and it earns its place
/// by being first and open rather than by being loud.
fn gist_section(tldr: &str) -> crate::surface::Section {
    crate::surface::Section {
        key: "tldr".to_string(),
        label: "tl;dr".to_string(),
        register: Register::Tldr,
        body: crate::surface::Body::Prose(tldr.to_string()),
    }
}

/// What this response's escalation earns, or `None` for one that draws nothing.
///
/// One function, so the shelf and the card cannot disagree about whether the
/// asks were promoted — the duplicate-content bug this file has shipped before
/// is exactly a disagreement between two places that each decided for
/// themselves.
fn escalation_call(r: &Response) -> Option<crate::emphasis::Call> {
    let e = r.escalation.as_ref()?;
    crate::emphasis::call_of(e.level, e.unanswered())
}

/// The one thing on a card allowed to interrupt you.
///
/// It takes the attention spine's own frame rather than a second recipe: the
/// right-hand spine is the surface on this machine that already knows how to say
/// *look at this*, and [`spine_frame`] is that frame. Parker: *"ATTENTION is
/// more than mere size... use the right attention spine as the gold standard!"*
///
/// It is not a fold. Unanswered asks are always drawn open, because a summons
/// behind a click is a summons nobody sees.
fn escalation(
    e: &crate::surface::Escalation,
    call: crate::emphasis::Call,
    sk: &Skin,
    th: &Theme,
) -> Div {
    use crate::surface::EscalationLevel as L;
    let tint = call.tint(th);
    let open = e.unanswered();
    let legend = format!(
        "\u{25c6} {} \u{b7} {} unanswered{}",
        match e.level {
            L::Blocking => "NEEDS YOU",
            L::Wanted => "WANTED",
            // Unreachable while `call_of` returns None for it; written out
            // rather than unwrapped so a future level cannot panic a card.
            L::None => "CLEAR",
        },
        open,
        match e.level {
            L::Blocking => " \u{b7} BLOCKING",
            _ => " \u{b7} WORK CONTINUES",
        }
    );
    // The frame comes from the Call, not from a `spine_frame` call written out
    // here: one place decides what a summons looks like.
    let frame = call.clothe(
        div().flex().flex_col().gap(px(5.)).px(px(11.)).py(px(9.)),
        sk,
        th,
    );
    frame
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(8.))
                .items_baseline()
                .child(micro(
                    legend,
                    Step::Tag,
                    tint.alpha(0.85 * call.strength.max(0.6)),
                    sk,
                    th,
                ))
                // An inferred summons says so on its face. The bench
                // reconstructed this from an `asks` register; the agent never
                // declared a level, and a red frame it did not ask for is a
                // claim the bench is making on its own behalf.
                .when(e.inferred, |d| {
                    d.child(micro("inferred", Step::Tag, th.faint, sk, th))
                }),
        )
        .when_some(e.why.clone(), |d, why| {
            d.child(
                div()
                    .text_size(px(sk.pt(Step::Small)))
                    .text_color(th.text.alpha(0.8))
                    .child(why),
            )
        })
        .children(e.items.iter().filter(|a| !a.answered).map(|a| {
            div()
                .flex()
                .flex_row()
                .gap(px(8.))
                .items_start()
                .child(
                    div()
                        .flex_none()
                        .mt(px(sk.tpx(3.)))
                        .w(px(sk.tpx(9.)))
                        .h(px(sk.tpx(9.)))
                        .rounded(sk.rad_raw(2.))
                        .border_1()
                        .border_color(tint.alpha(0.8)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_size(px(sk.pt(Step::Body)))
                        .text_color(th.text)
                        .child(a.ask.clone()),
                )
        }))
}

/// `3 doubts · 1 hunch`: how much the agent is unsure of, and how unsure.
fn doubts_measure(r: &Response) -> String {
    let n = r.doubts.len();
    let mut s = if n == 1 {
        "1 doubt".to_string()
    } else {
        format!("{n} doubts")
    };
    let hunches = r
        .doubts
        .iter()
        .filter(|d| {
            matches!(
                d.confidence,
                Some(Confidence::Hunch) | Some(Confidence::Unknown)
            )
        })
        .count();
    if hunches == 1 {
        s.push_str(" \u{b7} 1 hunch");
    } else if hunches != 0 {
        s.push_str(&format!(" \u{b7} {hunches} hunches"));
    }
    s
}

/// A reply, as registers a person unfolds.
///
/// The gist first and always open. Then one panel per section: a header that
/// is the click target, carrying a chevron for the fold state, the label,
/// and how much is behind it, so a folded `Technical brief · 340 words` is a
/// promise the reader can weigh before spending it. The body draws under the
/// header when the section is open. The doubts come last, in the complement
/// colour and never folded — they are the part of a reply prose buries and
/// the part a person most needs, and hiding them behind a click would be
/// burying them again with a nicer typeface.
///
/// Which sections are open is not decided here: `folds.open` answers it, from
/// the bench's toggles and `workbench::section_default_open`. Without folds
/// everything is drawn closed and nothing is pressable, which is what a
/// summary is.
fn response(r: &Response, folds: Option<&Folds>, sk: &Skin, th: &Theme) -> Div {
    let promoted = escalation_call(r).is_some();
    // The gist first, then every register — except an `asks` the escalation has
    // already promoted. Drawing both would print the same questions twice, which
    // this file has shipped twice before and been told off for twice: *"Again —
    // repeating ourselves ... just ummm... just the buttons"*.
    let tldr = gist_section(&r.tldr);
    let rows: Vec<&crate::surface::Section> = std::iter::once(&tldr)
        .chain(
            r.sections
                .iter()
                .filter(|s| !(promoted && s.register == Register::Asks)),
        )
        .collect();
    let open: Vec<bool> = rows
        .iter()
        .map(|s| folds.is_some_and(|f| (f.open)(s)))
        .collect();
    // The tiers, allocated for the whole shelf at once so exactly one row can be
    // lit. A renderer cannot overspend the budget because it never holds it.
    //
    // The lit row is the one the reader last OPENED, not the first one that
    // happens to be open — so a card nobody has touched arrives with nothing
    // lit, and the light moves as they read rather than sitting on the tl;dr
    // forever.
    let lit = folds
        .and_then(|f| f.lit)
        .and_then(|key| rows.iter().position(|s| s.key == key));
    let tiers = crate::emphasis::shelf(&open, lit);

    let frame = div().flex().flex_col().gap(px(8.));
    let frame = frame.children(rows.iter().zip(tiers).zip(&open).map(|((s, tier), &open)| {
        let facet = crate::emphasis::facet(tier, th);
        let header = div()
            .flex()
            .flex_row()
            .gap(px(8.))
            .items_baseline()
            .child(
                div()
                    .w(px(sk.tpx(12.)))
                    .flex_none()
                    .text_size(px(sk.pt(Step::Note)))
                    .text_color(if open { facet.tint } else { th.faint })
                    .child(if open { "\u{25be}" } else { "\u{25b8}" }),
            )
            .child(
                div()
                    .text_size(px(sk.pt(Step::Body)))
                    .text_color(facet.ink)
                    .child(s.label.clone()),
            );
        // NO COUNT. `32 words`, `2 items`, `4 facts` used to sit beside every
        // label, and the justification written here was that a folded section
        // is "a promise the reader can weigh before spending it". That is not
        // how anyone reads. Nobody has ever declined to open a technical brief
        // because it was thirty-two words rather than forty, and the number is
        // wrong for the only question a reader actually has, which is whether
        // the thing is worth reading. Parker: *"the number of words or facts —
        // all those counters are AI trash anti-patterns and die in a fire"*.
        // The header is the target, and only the header: a click in a long
        // open body should place nothing and fold nothing.
        let header = match folds {
            Some(f) => header.relative().child(zone(
                f.zones.clone(),
                crate::workbench::Hit::ToggleSection {
                    id: f.id.clone(),
                    key: s.key.clone(),
                },
            )),
            None => header,
        };
        // Shape from the Skin, then the tier, then nothing else. Every register
        // is the same panel; what separates them is which tier they were handed.
        let panel = facet.clothe(
            sk.panel()
                .flex()
                .flex_col()
                .gap(px(6.))
                .px(px(11.))
                .py(px(8.)),
            sk,
            th,
        );
        let panel = panel.child(header);
        if open {
            panel.child(section_body(&s.body, s.register, sk, th))
        } else {
            panel
        }
    }));
    frame.when(!r.doubts.is_empty(), |d| {
        // The doubts are neither reading nor a summons: they are present, and
        // they make no claim on the reader's attention. The only colour in the
        // block is each claim's own confidence, which is the information in it.
        let quiet = crate::emphasis::facet(crate::emphasis::Emphasis::Quiet, th);
        d.child(
            quiet
                .clothe(
                    sk.panel()
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .px(px(11.))
                        .py(px(8.)),
                    sk,
                    th,
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.))
                        .items_baseline()
                        .child(micro("ARTICLES OF DOUBT", Step::Tag, th.faint, sk, th))
                        .child(micro(doubts_measure(r), Step::Note, th.faint, sk, th)),
                )
                .children(r.doubts.iter().map(|doubt| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .gap(px(7.))
                                .items_baseline()
                                .child(
                                    div()
                                        .text_size(px(sk.pt(Step::Body)))
                                        .text_color(th.text)
                                        .child(format!("\u{b7} {}", doubt.claim)),
                                )
                                .child(micro(
                                    // Undeclared and unknown are different
                                    // facts, and the card says which.
                                    doubt
                                        .confidence
                                        .map(|c| c.label().to_string())
                                        .unwrap_or_else(|| "confidence undeclared".into()),
                                    Step::Tag,
                                    // Amber is the house colour for a thing you
                                    // are meant to argue with, and that is
                                    // exactly what an inference or a hunch is.
                                    // Unknown stays grey and never takes a hue:
                                    // an unknown that arrives in a colour looks
                                    // like a claim, and no claim has been made.
                                    match doubt.confidence {
                                        Some(Confidence::Measured) => th.text,
                                        Some(Confidence::Unknown) | None => th.faint,
                                        Some(_) => ink(crate::workbench::Tint::Pending, th),
                                    },
                                    sk,
                                    th,
                                )),
                        )
                        .when_some(doubt.why.clone(), |x, why| {
                            x.child(div().pl(px(12.)).child(micro(
                                why,
                                Step::Small,
                                th.text.alpha(0.75),
                                sk,
                                th,
                            )))
                        })
                })),
        )
    })
}

/// A section's contents by its shape: prose as lines, a list as bullets —
/// numbered where the register is a sequence — and facts as a name beside a
/// value on a row of its own.
fn section_body(body: &Body, register: Register, sk: &Skin, th: &Theme) -> Div {
    match body {
        Body::Prose(text) => paragraph(text.clone(), sk, th),
        Body::Items(items) => {
            div()
                .flex()
                .flex_col()
                .gap(px(3.))
                .children(items.iter().enumerate().map(|(i, item)| {
                    let mark = if register == Register::Next {
                        format!("{}.", i + 1)
                    } else {
                        "\u{b7}".to_string()
                    };
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(7.))
                        .items_baseline()
                        .child(div().w(px(sk.tpx(18.))).flex_none().child(micro(
                            mark,
                            Step::Small,
                            th.faint,
                            sk,
                            th,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(sk.pt(Step::Body)))
                                .text_color(th.text.alpha(0.9))
                                .child(item.clone()),
                        )
                }))
        }
        Body::Facts(facts) => div()
            .flex()
            .flex_col()
            .gap(px(3.))
            .children(facts.iter().map(|(name, value)| {
                div()
                    .flex()
                    .flex_row()
                    .gap(px(10.))
                    .items_baseline()
                    .child(div().w(px(sk.tpx(110.))).flex_none().child(micro(
                        name.clone(),
                        Step::Note,
                        th.faint,
                        sk,
                        th,
                    )))
                    .child(div().flex_1().min_w(px(0.)).child(micro(
                        value.clone(),
                        Step::Body,
                        th.text,
                        sk,
                        th,
                    )))
            })),
    }
}

/// A question the agent is waiting on, with its options numbered.
///
/// Numbered because the numbers are real: they are the option's position in
/// the agent's own menu, and the bench answers by walking that menu. A reader
/// who prefers the terminal can flip to TERM and press the same number.
/// One decision node's progress through its own questions.
///
/// A segment per question, filled for the ones already answered, with the
/// count said in words beside it. Drawn UNDER the options because that is
/// where it was asked for and where it belongs: the question is the thing to
/// read, and the progress is the context you check afterwards.
///
/// It exists at all because a round of questions was arriving as a pile of
/// separate waiting rows, which is true of the data and wrong about the work —
/// Parker: *"it should feel more like progress along a workflow, but be a
/// SINGLE DECISION NODE even if we are making multiple decisions"*.
pub fn round_progress(round: &crate::surface::Round, sk: &Skin, th: &Theme) -> Div {
    let done = round.answered();
    let total = round.total();
    let tint = if round.submitting {
        ink(crate::workbench::Tint::Settled, th)
    } else {
        ink(crate::workbench::Tint::Waiting, th)
    };
    div()
        .flex()
        .flex_col()
        .gap(px(5.))
        .child(
            div()
                .flex()
                .flex_row()
                .gap(px(3.))
                .children(round.steps.iter().map(|step| {
                    div()
                        .h(px(5.))
                        .flex_1()
                        .rounded(sk.rad_raw(2.))
                        // An unanswered segment is DRAWN, dim, rather than
                        // left out: an empty slot is how a person sees there
                        // is more to come.
                        .bg(if step.done { tint } else { tint.alpha(0.22) })
                })),
        )
        .child(micro(
            if round.submitting {
                format!("{done} of {total} answered \u{b7} ready to submit")
            } else {
                format!("{done} of {total} answered")
            },
            Step::Fine,
            th.faint,
            sk,
            th,
        ))
}

/// The review flyout: one answered question at a time, with arrows.
///
/// A round of five leaves five answers scattered down a rail, and checking
/// what you said means opening each one and losing the question you are in the
/// middle of. Parker: *"a button to REVIEW answers -> clicking this would open
/// a flyout overlay (so that we don't navigate around) which has a left arrow
/// right arrow gallery type of a feel for questions asked and answered"*.
///
/// An OVERLAY rather than a card, deliberately: the thing underneath is a
/// question somebody is part-way through answering, and replacing it with a
/// history is exactly the navigation this exists to avoid. It draws over, and
/// closing it puts the person back where they were with nothing to restore.
///
/// The pane attaches the arrows and the close, because pressing one is a
/// change of state rather than a change of picture.
pub fn review_flyout(
    at: usize,
    total: usize,
    title: &str,
    answer: &str,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let tint = ink(crate::workbench::Tint::Settled, th);
    aglow(
        sk.panel()
            // CENTRED on what it covers, not parked at the bottom.
            //
            // A flyout opens because somebody pressed a button in the middle
            // of the card, and their eye is already there; putting the answer
            // at the foot of the pane asks them to go and find it. Parker:
            // *"the POSITION should be CENTERED on the question element and
            // overlapping it ... if the person CLICKED review, the popup will
            // be where they JUST clicked"*. The centring is done by the
            // wrapper the pane puts this in, so this only has to say how wide
            // it is willing to be.
            .max_w(px(620.))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(16.))
            .bg(th.surface)
            .border_l(px(3.))
            .border_color(tint)
            // Over everything, and taking its own clicks: an overlay that
            // let a press through to the card underneath would answer a
            // question while somebody was reading an old one.
            .occlude(),
        tint,
        th,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.))
            .child(micro("REVIEW", Step::Fine, tint, sk, th))
            .child(div().flex_1())
            // Where you are in the gallery, said plainly. This is a count a
            // person cannot see — unlike the option count, which was printed
            // over the top of the options themselves.
            .child(micro(
                format!("{} of {}", at + 1, total.max(1)),
                Step::Fine,
                th.faint,
                sk,
                th,
            )),
    )
    .child(
        div()
            .text_size(px(sk.pt(Step::Head)))
            .text_color(th.text)
            .child(title.to_string()),
    )
    .child(
        div()
            .text_size(px(sk.pt(Step::Lead)))
            .text_color(tint)
            .child(answer.to_string()),
    )
}

/// A question, opened from the rail.
///
/// Deliberately the same object as [`waiting_block`] — the same tint, the same
/// left edge, the same numbered lines — because it is the same question, and
/// the only difference between the two is whether the person went looking for
/// it or it arrived in front of them. Two designs for one thing taught the
/// reader that the bench has two kinds of question, which it does not.
///
/// The heading above already asks the question, so this does not ask it again:
/// what it adds is what each option COSTS, which is the part a person is
/// actually weighing. The pane attaches the pressable chips underneath.
/// A question, opened from the rail — the QUESTION, and what you can do
/// about it.
///
/// It used to ask three times. The heading asked it, then a nested block
/// carrying its own WAITING ON YOU asked it again, then the options appeared
/// twice: once as a list of labelled panels and once as the chips the pane
/// attaches underneath. Parker: *"declaring question then asking a question is
/// an anti-pattern in UX (can i ask you a question? question ... just ASK THE
/// QUESTION!"*.
///
/// So this renders only what the heading and the chips cannot: what each
/// option COSTS, where the agent said so, and how a settled question was
/// settled. On a question with no descriptions and no answer — which is what
/// `Ready to submit your answers?` is — it renders nothing at all, and the
/// card is a question and two buttons.
fn question(q: &crate::surface::Question, sk: &Skin, th: &Theme) -> Div {
    use crate::surface::Answered;
    let chosen = match q.answer {
        Answered::Chose(i) => Some(i),
        _ => None,
    };
    let _ = sk;
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        // What an option costs, for the options that say. Never the label —
        // the chip underneath is the label, and printing it here is the
        // duplication that made this card unreadable.
        .children(q.options.iter().enumerate().filter_map(|(i, o)| {
            let what = o.what_happens.clone()?;
            let dim = chosen.is_some() && chosen != Some(i);
            Some(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.))
                    .items_baseline()
                    .child(micro(
                        format!("{}", i + 1),
                        Step::Note,
                        th.faint.alpha(if dim { 0.5 } else { 1.0 }),
                        sk,
                        th,
                    ))
                    .child(micro(
                        what,
                        Step::Small,
                        th.text.alpha(if dim { 0.4 } else { 0.75 }),
                        sk,
                        th,
                    )),
            )
        }))
        // How it was settled, when it was. Three states drawn as three,
        // because "answered, and the transcript does not say how" is a real
        // reading — somebody typed prose instead of picking — and showing it
        // as the first option would invent a decision nobody made.
        //
        // Nothing at all while it is waiting: the chips are right there, and
        // a line of prose explaining that a button is a button is the noise
        // this card was drowning in.
        .children(match &q.answer {
            Answered::Waiting => None,
            Answered::Chose(_) => Some(micro("answered".to_string(), Step::Note, th.faint, sk, th)),
            Answered::Typed(said) => Some(micro(
                format!("answered in the terminal \u{b7} \u{201c}{said}\u{201d}"),
                Step::Note,
                th.text.alpha(0.8),
                sk,
                th,
            )),
            Answered::ChoseUnknown => Some(micro(
                "answered in the terminal \u{b7} how is unavailable".to_string(),
                Step::Note,
                th.faint,
                sk,
                th,
            )),
        })
        .when_some(q.round.as_ref(), |d, round| {
            d.child(round_progress(round, sk, th))
        })
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
                .child(micro(c.to_uppercase(), Step::Fine, th.faint, sk, th))
        }));
    let rows = t.rows.iter().map(|row| {
        div()
            .flex()
            .flex_row()
            .gap(px(10.))
            .py(px(2.))
            .children(row.iter().map(|cell| {
                div().flex_1().child(match cell {
                    Some(text) => micro(clip(text, 40), Step::Small, th.text, sk, th),
                    // A cell nobody filled says so, rather than being blank and
                    // reading as a value of nothing.
                    None => micro("unavailable", Step::Small, th.faint, sk, th),
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
                                .text_size(px(sk.pt(Step::Small)))
                                .text_color(th.text)
                                .child(n.label.clone()),
                        )
                        .when_some(n.state.clone(), |d, s| {
                            d.child(micro(s, Step::Tag, th.accent, sk, th))
                        })
                }));
        if name.is_empty() {
            inner
        } else {
            sk.panel()
                .flex()
                .flex_col()
                .gap(px(5.))
                .child(micro(name.to_uppercase(), Step::Tag, th.faint, sk, th))
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
            Step::Note,
            th.text.alpha(0.8),
            sk,
            th,
        )
    });
    let dangling = a.dangling.iter().map(|e| {
        micro(
            format!("{} → {} · no such node", e.from, e.to),
            Step::Note,
            th.complement,
            sk,
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
            d.child(micro(r, Step::Note, th.faint, sk, th))
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
                        .child(micro(h.file.clone(), Step::Small, th.text, sk, th))
                        .child(micro(
                            format!("+{} −{}", h.added, h.removed),
                            Step::Note,
                            th.faint,
                            sk,
                            th,
                        ))
                        .child(micro(
                            verdict_word(h.verdict).to_string(),
                            Step::Fine,
                            verdict_ink(h.verdict, th),
                            sk,
                            th,
                        )),
                )
                .child(patch(&h.patch, sk, th))
        }))
}

/// A patch, coloured the way a diff is coloured everywhere else, and clipped
/// so one enormous hunk cannot own the pane.
fn patch(text: &str, sk: &Skin, th: &Theme) -> Div {
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
            .text_size(px(sk.pt(Step::Small)))
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
                .text_size(px(sk.pt(Step::Lead)))
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
                                .text_size(px(sk.pt(Step::Body)))
                                .text_color(th.text)
                                .child(o.name.clone()),
                        )
                        .when(o.recommended, |x| {
                            x.child(micro("recommended", Step::Tag, th.accent, sk, th))
                        }),
                )
                .when_some(o.case.clone(), |x, case| {
                    x.child(micro(case, Step::Small, th.text.alpha(0.8), sk, th))
                })
                .when_some(o.cost.clone(), |x, cost| {
                    x.child(micro(
                        format!("cost · {cost}"),
                        Step::Small,
                        th.faint,
                        sk,
                        th,
                    ))
                })
        }))
        .when(!d.consequences.is_empty(), |x| {
            x.child(sk.rule_h()).child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(micro("IF WE DO", Step::Tag, th.faint, sk, th))
                    .children(d.consequences.iter().map(|c| {
                        micro(format!("· {c}"), Step::Small, th.text.alpha(0.85), sk, th)
                    })),
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
        .child(micro(
            "NOTHING IS CLAIMED ABOUT THIS",
            Step::Tag,
            th.faint,
            sk,
            th,
        ))
        .child(paragraph(first_lines(&u.raw, 20), sk, th))
}

/// One fact per row, and each row its OWN surface.
///
/// These were four lines of text stacked four pixels apart on the pane's bare
/// background, and at that spacing a name, its value and the next name read as
/// one paragraph — Parker, on the opened card: *"there needs to be cards /
/// visual hierarchy // borders / depth for each element!"*. A row is a fact,
/// facts are separate things, and the cheapest way to say so is to give each
/// one a floor of its own.
///
/// An unavailable value keeps its row and dims BOTH halves, so a fact nobody
/// has is visibly a fact nobody has rather than a row that looks broken.
fn field_grid(fields: Vec<(&str, Option<String>)>, sk: &Skin, th: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .children(fields.into_iter().map(|(name, value)| {
            let known = value.is_some();
            sk.panel()
                .flex()
                .flex_row()
                .gap(px(12.))
                .items_baseline()
                .px(px(11.))
                .py(px(8.))
                .bg(th.surface.alpha(if known { 0.55 } else { 0.3 }))
                .border_l(px(2.))
                .border_color(if known {
                    th.accent.alpha(0.35)
                } else {
                    th.faint.alpha(0.35)
                })
                .child(div().w(px(sk.tpx(76.))).flex_none().child(micro(
                    name.to_uppercase(),
                    Step::Fine,
                    th.faint,
                    sk,
                    th,
                )))
                .child(match value {
                    Some(v) => {
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .child(micro(v, Step::Body, th.text, sk, th))
                    }
                    // Shown missing rather than omitted: an omitted row leaves a
                    // hole a reader fills in themselves.
                    None => div().flex_1().min_w(px(0.)).child(micro(
                        "unavailable",
                        Step::Body,
                        th.faint,
                        sk,
                        th,
                    )),
                })
        }))
}

fn paragraph(text: String, sk: &Skin, th: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .children(text.lines().map(|line| {
            div()
                .text_size(px(sk.pt(Step::Body)))
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

/// The bench's title card: what this agent is doing, in words.
///
/// The first version was a status line — a glyph, a lowercase phrase and a
/// tool name at eleven points — sitting above a surface of raised cards and
/// large type, and it read as debug output that had wandered in. It also had
/// to work out its own colour by looking for the substring `"your turn"` in
/// its own label, which is how a word and a colour end up disagreeing.
///
/// It is now the one thing at the top of the bench that answers *what is
/// happening here*: a dot in the state's colour, the state in sentence case,
/// and whatever tool is running beside it. Urgent states earn the phosphor,
/// the same way the head of the rail does and for the same reason.
pub fn title_card(
    state: crate::workbench::AgentState,
    in_state_ms: u64,
    vitals: Option<&crate::workbench::TurnVitals>,
    tool: Option<&str>,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let tint = ink(state.tint(), th);
    let urgent = state.urgent();
    // 65% OF THE SPINE'S BLOOM, and that is arithmetic rather than an eye.
    //
    // The spine glows with `float_shadows(accent)` at full alpha, and
    // `spine_frame` multiplies exactly that by its strength — so "as strong as
    // the right spine glow, times 0.65" is the number, not an approximation of
    // one. Parker gave two ways to say it: *"65% as strong as the right spine
    // glow! or perhaps 80% as strong as the working focus pane glow!"*. The
    // spine is the one that can be computed; the focus tube's glow is a shader
    // term with no alpha to take a percentage of, so it would have been an eye
    // pretending to be a number.
    const SPINE_SHARE: f32 = 0.65;
    spine_frame(
        div()
            .flex()
            // ONE LINE. It was a column — a header row above a lamp-and-state
            // row — which is two lines of chrome to say one short thing, and
            // on a wide pane it left a band of empty the height of a
            // paragraph. Everything it carries fits across.
            .flex_row()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .py(px(8.)),
        tint,
        SPINE_SHARE,
        sk,
        th,
    )
    .child(micro("AGENT", Step::Fine, th.faint, sk, th))
    .child(
        // A LAMP, not a bullet. Ringed rather than merely bigger: a filled
        // circle reads as punctuation at any size, and a ring around it reads
        // as an indicator — the difference between a full stop and something
        // that is on.
        div()
            .w(px(14.))
            .h(px(14.))
            .flex_none()
            .rounded(sk.rad_raw(7.))
            .border_2()
            .border_color(tint.alpha(0.9))
            .flex()
            .items_center()
            .justify_center()
            .child(div().w(px(6.)).h(px(6.)).rounded(sk.rad_raw(3.)).bg(tint)),
    )
    .child(
        // The state, still the largest thing on the bar. The SIZE is constant
        // and the COLOUR says which state it is: a calm state that shrinks is
        // a calm state nobody can find, and finding it is the whole job.
        div()
            .flex_none()
            .whitespace_nowrap()
            .text_size(px(sk.pt(Step::Title)))
            .text_color(if urgent { tint } else { th.text.alpha(0.9) })
            .child(state.word()),
    )
    .child(
        // THE ONE COUNTER: how long the agent has been like this. It replaces
        // an age on every rail row, which repeated one fact per row.
        micro(
            crate::attention::age_label(Some(std::time::Duration::from_millis(in_state_ms))),
            Step::Note,
            th.faint,
            sk,
            th,
        ),
    )
    // THE TURN'S OWN NUMBERS, while there is a turn: the agent's clock, its
    // token count and the call it is in the middle of, each read off its own
    // status line rather than counted here, and each drawn only when the
    // screen carried it. A working agent whose screen carried none of them —
    // a narrow pane truncates the line — says `turn · unread` rather than
    // showing a zero it never measured. The bar had room for all of this
    // and was spending it on nothing.
    .when_some(vitals, |d, v| {
        if v.is_unread() {
            return d.child(micro(
                "turn \u{b7} unread",
                Step::Note,
                th.faint.alpha(0.7),
                sk,
                th,
            ));
        }
        d.child(
            div()
                .flex()
                .flex_row()
                .items_baseline()
                .gap(px(8.))
                .min_w(px(0.))
                .overflow_hidden()
                .when_some(v.elapsed.clone(), |d, e| {
                    d.child(micro(
                        format!("turn {e}"),
                        Step::Note,
                        th.text.alpha(0.75),
                        sk,
                        th,
                    ))
                })
                .when_some(v.tokens, |d, n| {
                    d.child(micro(
                        format!("\u{2193} {} tokens", crate::hud::fmt_tokens(n)),
                        Step::Note,
                        th.text.alpha(0.75),
                        sk,
                        th,
                    ))
                })
                .when_some(v.doing.clone(), |d, doing| {
                    d.child(micro(
                        clip(&doing, 56),
                        Step::Note,
                        th.accent.alpha(0.85),
                        sk,
                        th,
                    ))
                }),
        )
    })
    .child(div().flex_1())
    .when_some(tool.map(str::to_string), |d, t| {
        d.child(micro(t, Step::Fine, th.faint, sk, th))
    })
}

/// Dress an answer as a button: always a button, coloured by what it is.
///
/// Three states and they are genuinely three. `primary` is the row the agent's
/// own cursor is on — what pressing return in the terminal would do — and it
/// gets the accent and the weight. `chosen` is what a person already picked,
/// and it keeps its colour without the emphasis, because a decision already
/// taken is a record. Everything else is a plain bordered control: pressable,
/// legible, quiet.
///
/// The border and the padding never vary. An option that looks like a word
/// instead of a button is one nobody presses, whatever colour it is.
pub fn option_button<E: Styled>(el: E, primary: bool, chosen: bool, sk: &Skin, th: &Theme) -> E {
    let el = el
        .px(px(if primary { 16. } else { 12. }))
        .py(px(if primary { 9. } else { 7. }))
        .text_size(px(if primary {
            sk.pt(Step::Lead)
        } else {
            sk.pt(Step::Body)
        }))
        .border_1();
    if primary {
        el.border_color(th.accent.alpha(0.9))
    } else if chosen {
        el.border_color(ink(crate::workbench::Tint::Settled, th).alpha(0.7))
    } else {
        el.border_color(th.text.alpha(0.28))
    }
}

/// Dress a verb as a button: big enough to hit, lit if it is the main one.
///
/// The size IS the affordance. A row of identical small words says every verb
/// is equally likely and none of them is a button; one chunky lit control and
/// a few quiet ones says what the card is FOR, and the quiet ones still work.
/// Kept here rather than at the call site so that every future verb row —
/// changesets, decisions, whatever arrives next — gets the same shape by
/// asking for it.
pub fn verb_button<E: Styled>(el: E, primary: bool, sk: &Skin, th: &Theme) -> E {
    let el = el
        .px(px(if primary { 18. } else { 12. }))
        .py(px(if primary { 10. } else { 6. }))
        .text_size(px(if primary {
            sk.pt(Step::Lead)
        } else {
            sk.pt(Step::Small)
        }));
    if primary {
        // The primary action on a card — one per card.
        aglow(el.border_color(th.accent.alpha(0.75)), th.accent, th)
    } else {
        el
    }
}

/// The places the composer writes down where it ended up.
///
/// Two handles that are one idea — *what did layout actually do with this box*
/// — and they travel together because both are read by something that has to
/// agree with the other: a click resolves through the text layout, and the
/// wheel through the scroll handle.
///
/// A third lived here briefly, holding the whole box so a sticky note could
/// stop above it. The note left the bench and the slot went with it rather
/// than staying as a measurement nobody reads.
#[derive(Clone, Default)]
pub struct Slots {
    /// The text's own layout, for turning a click into a character.
    pub layout: std::rc::Rc<std::cell::RefCell<Option<gpui::TextLayout>>>,
    /// Where a long draft has been scrolled to.
    pub scroll: gpui::ScrollHandle,
}

/// Make the element this is a child of a click target under the warp.
///
/// A canvas that covers its parent and, at paint, records the parent's FLAT
/// bounds and what a press there means into the pane's zone list. The pane's
/// root mouse handler then un-bends the pointer and looks the point up — see
/// [`crate::workbench::hit_at`] — instead of letting gpui hit-test a flat tree
/// against a bent picture.
///
/// The parent must be `relative()` so `inset_0` measures it and not some
/// ancestor; the call sites add that alongside this.
pub fn zone(
    into: std::rc::Rc<std::cell::RefCell<Vec<crate::workbench::Zone>>>,
    hit: crate::workbench::Hit,
) -> impl gpui::IntoElement {
    gpui::canvas(
        move |bounds, _window, _cx| {
            into.borrow_mut().push(crate::workbench::Zone {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                w: f32::from(bounds.size.width),
                h: f32::from(bounds.size.height),
                hit: hit.clone(),
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

/// The line into the agent's own terminal.
///
/// Not a text box. While it is armed, every keystroke is encoded by the same
/// function the terminal face uses and written straight to the
/// pseudoterminal, so the agent's own line editor does the work: slash
/// commands complete, history recalls, ctrl+c interrupts, and a paste is a
/// paste. What shows here is a shadow of what has already been sent, kept
/// only so there is something to look at before the agent's echo arrives.
///
/// It is deliberately the biggest thing on the bench. The first version was a
/// one-line strip carrying three hints, and it read as a status bar rather
/// than as somewhere to type — so it got explained instead of used.
///
/// The second version was a large quiet rectangle, on the theory that it
/// needed no caption, and that was wrong in the other direction: a big empty
/// panel with a grey sentence in the corner reads as a DISABLED panel. Parker,
/// looking at it: *"type to the agent has to be about 6x more obvious that
/// there is something here.. TOO subtle!"*
///
/// So the affordance is a caret — a fat accent block sitting where the first
/// character will land, which is the one shape on a screen that means "your
/// cursor is here" to everybody, and which no amount of grey caption
/// substitutes for. The prompt beside it is large enough to read across a
/// room, and one line underneath says what the keys do, because the three
/// things it names (type without clicking first, enter sends, paste takes an
/// image) are each invisible otherwise.
pub fn composer(
    line: Option<&crate::workbench::Line>,
    focused: bool,
    shows: &crate::workbench::Shows,
    slots: &Slots,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let Slots { layout, scroll } = slots;
    let (layout, scroll) = (layout.clone(), scroll.clone());
    let open = line.is_some();
    let live = open && focused;
    // The two size decisions arrive as one value rather than as a pair of
    // booleans, because they are one decision — what a pane this size shows —
    // and they are made and asserted in `workbench::shows`.
    let (tight, hint) = (shows.tight, shows.hint);
    let tall = if tight { 46. } else { 84. };
    // The type shrinks with the draft, two steps and then a floor. See
    // [`crate::workbench::composer_pt`] for why it stops rather than going on
    // shrinking: the research is unanimous that composers scroll, and 12.5pt
    // is where "small but readable" ends.
    let chars = line.map_or(0, |l| l.chars());
    let pt = if tight {
        sk.pt(Step::Lead)
    } else {
        crate::workbench::composer_pt(chars, shows.composer_w, shows.composer_max, sk.ty.k())
    };
    let hidden =
        crate::workbench::composer_hidden(chars, shows.composer_w, shows.composer_max, sk.ty.k());

    // The caret rides IN the text, as a highlight on the character it is on.
    //
    // Three earlier versions placed it by arithmetic — column times a cell,
    // column times a measured advance, then an invisible copy of the prefix —
    // and each was more nearly right than the last while sharing one fatal
    // assumption: that the line is one line. It is not. A long message wraps,
    // and a caret positioned along a single axis lands at the end of the first
    // row while the text continues on the second.
    //
    // A highlight has no such assumption. The text system puts the background
    // behind that character wherever it ends up, which is the same mechanism
    // that makes a selection follow a wrap, and it is exact for a proportional
    // font as a side effect of not measuring anything.
    let body: gpui::AnyElement = match line {
        Some(l) if !l.is_empty() || open => {
            // A space to hold the caret when it sits past the last character.
            // Without it there is nothing at that index to put a background
            // behind, and the caret at the end of a line — where it is most of
            // the time — would simply not draw.
            let text = format!("{} ", l.text());
            let at = text
                .char_indices()
                .nth(l.caret())
                .map(|(i, _)| i)
                .unwrap_or(l.text().len());
            let next = text[at..]
                .chars()
                .next()
                .map(|c| at + c.len_utf8())
                .unwrap_or(text.len());
            let caret = gpui::HighlightStyle {
                background_color: Some(th.human.alpha(if live { 0.85 } else { 0.35 })),
                color: Some(if live { th.bg } else { th.text }),
                ..Default::default()
            };
            let styled = gpui::StyledText::new(text).with_highlights([(at..next, caret)]);
            // The layout handle is filled in during prepaint and shared by
            // reference, so taking it here is taking the real thing. It is how
            // a click becomes a column — see `TerminalView::bench_click` — and
            // it replaces both the measured advance and the origin probe,
            // neither of which could survive a wrapped line.
            *layout.borrow_mut() = Some(styled.layout().clone());
            styled.into_any_element()
        }
        _ => div()
            .text_color(th.text.alpha(0.72))
            .child("type to the agent")
            .into_any_element(),
    };

    raised(
        sk.panel()
            .flex()
            .flex_col()
            .gap(px(if tight { 5. } else { 10. }))
            .justify_center()
            .min_h(px(tall))
            .flex_none()
            .px(px(if tight { 10. } else { 18. }))
            .py(px(if tight { 8. } else { 16. }))
            .bg(th.surface)
            // Lit whether or not it is armed. The border was the only thing
            // saying "this is an input" and it only said so AFTER the first
            // click, which is the wrong way round: the invitation has to be
            // legible before anyone has accepted it.
            .border_color(th.human.alpha(if live { 0.9 } else { 0.5 })),
        th.human,
        th,
    )
    .child(
        div()
            .flex()
            .flex_row()
            .items_start()
            .gap(px(if tight { 7. } else { 12. }))
            .child(
                // No fixed height: the box GROWS with what is in it.
                //
                // It was pinned at one line and clipped the second, which on a
                // surface whose whole job is a long message to an agent is the
                // one thing it must not do. Parker, with a three-line prompt
                // cut off mid-sentence: *"LOTS of text which overloads the
                // interaction text entry needs to auto grow the area"*. It
                // grows to a third of the pane and then scrolls, because a
                // composer that can eat the conversation above it has traded
                // one clipping problem for another.
                div()
                    .id("bench-composer-text")
                    .flex_1()
                    .min_w(px(0.))
                    .max_h(px(shows.composer_max))
                    // A SCROLL CONTAINER, so the wheel over this box moves
                    // this box and not the terminal behind it.
                    //
                    // The pane's own wheel handler sits on its root element
                    // and scrolls the terminal, so a scroll anywhere inside a
                    // pane scrolled the agent's transcript — including a
                    // scroll aimed squarely at a 1,500-word draft. Parker:
                    // *"a scroll action OVER the text entry area should scroll
                    // up and down there... we must still be adhering to good
                    // programming principles!!!!"*
                    //
                    // gpui does the clipping, the offset and the clamping.
                    // It does NOT decide which box the wheel is over: under
                    // the tube its hit-test is flat and the picture is bent,
                    // so the pane's pointer hook un-bends the wheel the way
                    // it un-bends clicks and drives this container's
                    // `ScrollHandle` itself — `TerminalView::bench_wheel`.
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .flex()
                    .flex_col()
                    // The END stays visible, not the beginning — by following
                    // the caret, not by pinning the layout.
                    //
                    // This box was `justify_end`, which puts a long draft's
                    // tail at the bottom by overflowing the TOP, and a gpui
                    // scroll container cannot scroll into that: its offset is
                    // held between zero and the content's overhang, and an
                    // overhang at the top is on the wrong side of zero. So a
                    // long draft showed its last lines and the wheel could
                    // never reach its first. Now the box lays out from the
                    // top like any scroll container, and the view asks for
                    // its bottom after every edit made at the end of the line
                    // (`TerminalView::composer_follows`) — which is what every
                    // chat composer does: the caret stays on screen while you
                    // type, and the wheel reads back over what you wrote.
                    .text_size(px(pt))
                    .font_family(th.font_family.clone())
                    .text_color(th.text)
                    .child(body),
            )
            // What went with the line but is not in it. Drawn where an
            // attachment is drawn in every messaging surface — beside the
            // text, not inside it — because that is exactly what it is.
            .when_some(line.map(|l| l.pasted()).filter(|n| *n > 0), |d, n| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(px(3.))
                        .bg(th.accent.alpha(0.18))
                        .child(micro(
                            if n == 1 {
                                "\u{1f5ce} 1 IMAGE".to_string()
                            } else {
                                format!("\u{1f5ce} {n} IMAGES")
                            },
                            Step::Tag,
                            th.accent,
                            sk,
                            th,
                        )),
                )
            })
            // Only while it is armed, and then unmissable. This is the answer
            // to the question the surface kept failing: *am I typing to the
            // agent right now, or do I have to click something first?*
            .when(live, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(px(3.))
                        .bg(th.human.alpha(0.16))
                        .child(micro("LIVE \u{2192} AGENT", Step::Tag, th.human, sk, th)),
                )
            }),
    )
    // A draft past what the box can show says so, rather than leaving a person
    // to wonder whether the top of their paragraph survived. The agent's own
    // prompt does the same thing with a big paste (`[Pasted text #1 +N
    // lines]`), which is the strongest available evidence for what a person
    // working here already expects.
    .when_some(hidden.filter(|_| !tight), |d, n| {
        d.child(micro(
            format!("\u{2191} {n} more characters above"),
            Step::Fine,
            th.faint,
            sk,
            th,
        ))
    })
    .when(hint, |d| {
        d.child(micro(
            "TYPE ANYWHERE \u{b7} ENTER SENDS \u{b7} PASTE TEXT, FILES OR AN IMAGE",
            Step::Fine,
            th.human.alpha(0.72),
            sk,
            th,
        ))
    })
}

/// The agent, talking. The main area's ordinary state.
///
/// Its own recent output, in its own font, with nothing drawn around it. This
/// is the bench's answer to "what is it doing" and it is the default because
/// that is the question a person arriving at a pane actually has — the rail
/// is for what it MADE, which is a different and rarer question.
pub fn conversation(tail: &[String], sk: &Skin, th: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(1.))
        .children(tail.iter().map(|line| {
            // The person's own turns are the landmarks in a scroll, so they
            // keep full strength and everything else recedes. Claude marks
            // them with a leading chevron; a shell has none, and then nothing
            // is emphasised, which is correct rather than a fallback.
            let mine =
                line.trim_start().starts_with('\u{203a}') || line.trim_start().starts_with('>');
            div()
                .text_size(px(sk.pt(Step::Body)))
                .font_family(th.font_family.clone())
                .text_color(if mine { th.human } else { th.text.alpha(0.62) })
                .child(line.clone())
        }))
}

/// The question the agent has stopped on, drawn where it is talking.
///
/// In the conversation rather than behind a click: an agent that cannot
/// continue without a person is the one thing on this surface nobody should
/// have to go looking for. The chips are attached by the pane, because
/// pressing one reaches a pseudoterminal.
pub fn waiting_block(q: &crate::surface::Question, sk: &Skin, th: &Theme) -> Div {
    aglow(
        sk.panel()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(16.))
            .border_l(px(3.))
            .border_color(ink(Tint::Waiting, th))
            .bg(th.surface),
        ink(Tint::Waiting, th),
        th,
    )
    .child(micro(
        "WAITING ON YOU",
        Step::Fine,
        ink(Tint::Waiting, th),
        sk,
        th,
    ))
    .child(
        div()
            .text_size(px(sk.pt(Step::Head)))
            .text_color(th.text)
            .child(q.question.clone()),
    )
    .child(
        div()
            .flex()
            .flex_col()
            .gap(px(5.))
            .children(q.options.iter().enumerate().filter_map(|(i, o)| {
                o.what_happens.as_ref().map(|what| {
                    micro(
                        format!("{} \u{b7} {}", i + 1, what),
                        Step::Note,
                        th.text.alpha(0.55),
                        sk,
                        th,
                    )
                })
            })),
    )
    .when_some(q.round.as_ref(), |d, round| {
        d.child(round_progress(round, sk, th))
    })
}

/// The collapse handle on the rail's inner edge.
///
/// A tray's affordance: one chevron, vertically centred on the border it
/// moves, pointing the way it will go. It is drawn at the edge rather than in
/// the rail's header because the thing it closes is the whole column, and a
/// control that lives inside what it hides is a control you cannot find again.
pub fn rail_handle(open: bool, sk: &Skin, th: &Theme) -> Div {
    div()
        .w(px(20.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            // BIGGER and BOLD. It was a thirteen-point chevron in the faint
            // ink — the dimmest mark on the surface, holding the only gesture
            // that gives the bench its width back, on a target fourteen pixels
            // wide. A control nobody can see is a control nobody uses.
            div()
                .text_size(px(sk.pt(Step::Display)))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(th.text.alpha(0.75))
                .child(if open { "\u{203a}" } else { "\u{2039}" }),
        )
}

/// An empty bench says what would fill it.
///
/// The most common state on day one, and the one a person will judge the
/// feature by. A blank rectangle reads as broken; a sentence naming the verb
/// reads as waiting.
///
/// This is the EXPLANATION half only. On a shell pane the verb that fixes the
/// emptiness is a button of its own — see [`launch_button`] — because a chip
/// appended to the end of a paragraph is a footnote, and the thing a person is
/// meant to press cannot be a footnote.
pub fn empty(is_agent: bool, dir: &str, action: Option<Div>, sk: &Skin, th: &Theme) -> Div {
    // **One container, not two orphans.**
    //
    // This was a full-width panel of text with the button as its SIBLING in the
    // body's column. Two children of a tall flex column do not read as one
    // thing, and they did not look like one: the button centred itself, the
    // heading stayed hard against the left edge a third of a pane away, and the
    // panel's own top border ran between them like a rule separating two
    // unrelated blocks. Parker, shown it on a 1870-pixel pane: *"that just looks
    // absolutely terrible... I am basically imagining a standard dialogue
    // window... super simple stuff"*.
    //
    // So it is a dialogue: one bounded card, its own width rather than the
    // pane's, centred, everything inside it centred with it, and the action
    // INSIDE the card it belongs to. Nothing here is novel — it is the shape
    // every desktop has used for an empty state for thirty years, which is the
    // point. A surface with nothing on it is the wrong place to invent.
    //
    // **The complement, at full strength, one rung up the ramp.** An empty
    // surface is the one place the bench can afford to be legible rather than
    // quiet: there is nothing for the text to compete with, and a faint
    // 10-point label in a field of nothing reads as a disabled control rather
    // than as an answer. Parker: *"use the bright other text colour and bigger
    // font by 30%"*. The 30% is spent on the RAMP rather than on a multiplier —
    // `Note` 10 to `Lead` 13 is exactly it, and `Body` 12 to `Head` 15 is the
    // nearest rung. A literal `* 1.3` would be the sixteenth font size the ramp
    // exists to have abolished.
    let card = sk
        .panel()
        .flex()
        .flex_col()
        .items_center()
        .w_full()
        // A dialogue is a fixed object, not a column that grows with the
        // window: past about forty characters a centred line stops being a
        // caption and starts being a paragraph nobody reads.
        .max_w(px(sk.tpx(380.)))
        .gap(px(sk.tpx(14.)))
        .px(px(sk.tpx(26.)))
        .py(px(sk.tpx(22.)))
        .child(
            micro(
                "NOTHING ON THE WORKBENCH",
                Step::Lead,
                th.complement,
                sk,
                th,
            )
            .text_center(),
        )
        // A SHELL gets the heading and the button.
        //
        // The sentence that used to sit here — *"A shell has no agent to present
        // anything. Start one and its work appears here."* — explained the
        // button directly above it, which the button's own words already
        // explain. Parker: *"the little flavour text about the shell can go
        // away"*.
        .when(is_agent, |d| {
            d.child(
                micro(
                    "This agent has presented no work objects yet.",
                    Step::Head,
                    th.complement.alpha(0.85),
                    sk,
                    th,
                )
                .text_center(),
            )
            .child(
                micro(
                    format!("drop a .json here: {dir}"),
                    Step::Note,
                    th.faint,
                    sk,
                    th,
                )
                .text_center(),
            )
        })
        .children(action);
    // The card centres itself in whatever box it is handed, so no caller has to
    // remember to do it — the last arrangement failed exactly because one of the
    // two pieces centred and the other did not.
    div().flex().flex_col().items_center().w_full().child(card)
}

/// The one verb an empty shell bench offers, as a button and nothing else.
///
/// It was a chip, appended as the last child of the sentence panel above, and
/// it inherited that panel's place at the bottom of a body that reads upward
/// like a transcript. On a tall pane the only thing to press on the whole
/// surface sat on the floor, under an acre of nothing, at the size of a label
/// — Parker: *"SPINNING up a new agent in workbench — the ACTIon for this is
/// WAAAAAAY at the bottome of the screen... it should be a FULLY standalone
/// button, then the explanation is in a bit of a separate element"*.
///
/// So it is three separate claims, and each is drawn rather than argued:
///
/// 1. **Standalone.** Its own element, above the explanation rather than
///    inside it — a button, not a word in a paragraph. It takes its own width
///    rather than the column's: a control stretched edge to edge across a pane
///    stops reading as a thing to press and starts reading as a banner, and it
///    was the only one on the surface doing it. Parker: *"not full width"*.
/// 2. **At eye level.** [`crate::workbench::body_anchor`] stops the body
///    reading from the floor while an offer is the thing on it.
/// 3. **The loudest thing on an empty surface**, which it can afford to be
///    precisely because the surface is empty: the spine's `aglow`, the tint the
///    bench already spends on "this is yours to act on", and [`Step::Head`].
pub fn launch_button(sk: &Skin, th: &Theme) -> Div {
    aglow(
        sk.chip(true)
            .flex()
            .flex_row()
            .items_center()
            .justify_center()
            // Its own width, centred in the column — a flex child stretches on
            // the cross axis unless it says otherwise, which is where the full
            // width came from.
            .self_center()
            .flex_none()
            .px(px(sk.tpx(22.)))
            .gap(px(sk.tpx(8.)))
            .py(px(sk.tpx(9.)))
            .cursor_pointer()
            .text_size(px(sk.pt(Step::Head)))
            .font_weight(gpui::FontWeight::BOLD)
            .child("\u{2301}")
            .child(sk.caps("LAUNCH AGENT")),
        th.human,
        th,
    )
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

    /// Every size on the bench goes through the pane's gauge.
    ///
    /// The companion to `a_renderer_contains_no_decisions`, and the same shape
    /// of guard for the same reason. Before the ramp existed this file carried
    /// fifteen distinct type sizes as literals, and not one of them could
    /// answer to the slider the person had already set. The fix was mechanical
    /// — seventy call sites — so the way it comes UNDONE is mechanical too: one
    /// new element, written the way every neighbouring element used to be
    /// written, drawing at a fixed size on a surface that scales around it.
    /// Nobody would notice until a pane at 2.0x had one label still at eleven
    /// points.
    ///
    /// **A box that holds text counts as a size.** The second half of the rule,
    /// and the half that is easy to forget while doing the first: a `micro` at
    /// a gauged rung inside a `w(px(110.))` column is type growing inside
    /// something that does not, which is how an exact fit starts wrapping down
    /// a row. Four such columns were missed on the first pass here and caught
    /// by re-reading, so the scan now holds them: a fixed `w`/`h` on a `div`
    /// whose own line also names `micro(` or `text_size(` has to go through
    /// `sk.tpx`.
    ///
    /// Scanned in the CODE half only (the tests are split off first), with
    /// comments stripped, so this test's own prose may name a size.
    ///
    /// **Mutation-tested when written**, one plant at a time and each reverted
    /// before the next — seven, one per shape the regression takes: a
    /// `text_size(px(20.))`; an inline `micro(…, 10., …)`; the `if primary`
    /// ternary reverted to `13.` / `12.`; a rustfmt-split `micro` whose size
    /// argument sits alone on its own line, at two different indents; and two
    /// label columns reverted from `sk.tpx(n)` to a bare `n`. All seven failed
    /// this test and were named by line number. A scan nobody has watched fail
    /// is a scan that might match nothing.
    ///
    /// An eighth was planted to check it does NOT cry wolf: `.gap(px(8.))` left
    /// as a literal, which is a renderer choosing its own spacing and is none
    /// of this rule's business. It passed, as it must — a check that fires on
    /// innocent lines gets switched off, and then nothing is enforced.
    #[test]
    fn every_size_on_the_bench_goes_through_the_gauge() {
        let src = include_str!("benchdraw.rs");
        let (code, _tests) = src.split_once("\n#[cfg(test)]").expect("a test module");
        let mut found = Vec::new();
        for (n, raw) in code.lines().enumerate() {
            let line = raw.split("//").next().unwrap_or("");
            // A type size written as a number rather than taken from the ramp.
            if let Some(at) = line.find("text_size(px(") {
                let rest = &line[at + "text_size(px(".len()..];
                let head = rest.trim_start();
                // `if x { sk.pt(..) } else { .. }` is fine; a digit is not.
                let lit = head
                    .strip_prefix("if ")
                    .map_or(head, |t| t.split('{').nth(1).unwrap_or("").trim_start());
                if lit.starts_with(|c: char| c.is_ascii_digit()) {
                    found.push(format!("{}: a fixed type size: {}", n + 1, raw.trim()));
                }
            }
            // `micro` takes a rung, never a number. Catching this as a separate
            // rule matters: the helper's second parameter is the one place a
            // size can be passed without the words `text_size` appearing at all.
            if let Some(at) = line.find("micro(") {
                let rest = line[at + "micro(".len()..].trim_start();
                if rest.starts_with(|c: char| c.is_ascii_digit()) {
                    found.push(format!("{}: micro() took a number: {}", n + 1, raw.trim()));
                }
            }
            // The same size, passed to a `micro(` that rustfmt split across
            // lines — the argument then sits alone on its own line, where
            // neither rule above can see the call it belongs to. Every
            // multi-line call in this file spells that slot `Step::…`, so a
            // line holding nothing but a number and a comma is the regression
            // and nothing else. (Checked against the file as it stands: no
            // other argument is ever written alone as a bare float.)
            let t = line.trim();
            if let Some(num) = t.strip_suffix(',') {
                if !num.is_empty()
                    && num.starts_with(|c: char| c.is_ascii_digit())
                    && num.chars().all(|c| c.is_ascii_digit() || c == '.')
                {
                    found.push(format!("{}: a bare size argument: {t}", n + 1));
                }
            }
            // A BOX that holds text, sized without the gauge. Only lines that
            // also carry the text on them are scanned, which is what keeps this
            // from firing on the gaps, radii and paddings a renderer is entitled
            // to choose — those are the skin's business and do not grow with a
            // person's reading size.
            if line.contains("micro(") || line.contains("text_size(") {
                for dim in [".w(px(", ".h(px(", ".min_w(px(", ".min_h(px("] {
                    if let Some(at) = line.find(dim) {
                        let arg = line[at + dim.len()..].trim_start();
                        if arg.starts_with(|c: char| c.is_ascii_digit()) && !arg.starts_with('0') {
                            found.push(format!(
                                "{}: a text box that does not scale with its text: {}",
                                n + 1,
                                raw.trim()
                            ));
                        }
                    }
                }
            }
        }
        assert!(
            found.is_empty(),
            "sizes that ignore the pane's text-size gauge:\n{}",
            found.join("\n")
        );
    }

    /// A renderer contains no decisions — rule three of the architecture
    /// pass, made mechanical.
    ///
    /// Two shapes a decision takes when it hides in a renderer, both scanned
    /// for in the CODE half of this file (the tests are split off first, so
    /// this test's own text is never read):
    ///
    /// 1. a comparison against a number other than zero — `if pane_w < 400.`
    ///    is a threshold, and a threshold is a rule that belongs in
    ///    `workbench.rs` as a named constant with a table test;
    /// 2. a read of the environment or the clock — a mode or a timing, which
    ///    belongs in `workbench.rs` as an input the view resolves and hands
    ///    in.
    ///
    /// Zero is allowed on either side of a comparison because "is there any"
    /// is presence, not policy (`unseen > 0`, and `> 0.001` for a theme
    /// float's zero). Equality is not scanned: `n == 1` picks a plural, and
    /// that is grammar. Counts like `.take(5)` are not scanned: how many rows
    /// a compact card shows is typography, and typography is what a renderer
    /// is for. Comments are stripped first: prose may say "more than 3".
    ///
    /// Mutation-tested 2026-09-17 against the file as it stood — see the
    /// commit that added it for the three plants and the lines they were
    /// caught at. The unmutated file passes, so this is a guard and not an
    /// alarm; a scan that cries wolf gets switched off, and then nothing is
    /// enforced.
    #[test]
    fn a_renderer_contains_no_decisions() {
        let src = include_str!("benchdraw.rs");
        let (code, _tests) = src.split_once("\n#[cfg(test)]").expect("a test module");
        let mut found = Vec::new();
        for (n, raw) in code.lines().enumerate() {
            let line = raw.split("//").next().unwrap_or("");
            for needle in [
                "std::env",
                "env::var",
                "Instant",
                "SystemTime",
                ".elapsed(",
                "::now(",
            ] {
                if line.contains(needle) {
                    found.push(format!("{}: reads {needle}: {}", n + 1, raw.trim()));
                }
            }
            if let Some(lit) = threshold(line) {
                found.push(format!("{}: compares against {lit}: {}", n + 1, raw.trim()));
            }
        }
        assert!(
            found.is_empty(),
            "decisions in the renderer:\n{}",
            found.join("\n")
        );
    }

    /// A numeric literal other than zero on either side of `<`, `>`, `<=` or
    /// `>=` in one line of code, if there is one. `->`, `=>` and the `<` of a
    /// generic are not comparisons and are skipped.
    fn threshold(line: &str) -> Option<String> {
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            if c != b'<' && c != b'>' {
                i += 1;
                continue;
            }
            let prev = if i > 0 { b[i - 1] } else { b' ' };
            let mut end = i + 1;
            if end < b.len() && b[end] == b'=' {
                end += 1;
            }
            if prev != b'-' && prev != b'=' {
                if let Some(lit) = leading_number(line[end..].trim_start()) {
                    if !is_zero(&lit) {
                        return Some(lit);
                    }
                }
                if let Some(lit) = trailing_number(line[..i].trim_end()) {
                    if !is_zero(&lit) {
                        return Some(lit);
                    }
                }
            }
            i = end;
        }
        None
    }

    fn leading_number(s: &str) -> Option<String> {
        let n: String = s
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '_')
            .collect();
        n.starts_with(|c: char| c.is_ascii_digit()).then_some(n)
    }

    fn trailing_number(s: &str) -> Option<String> {
        let tail: String = s
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '_')
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        if !tail.ends_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        // `k1 >` is a name ending in a digit, not a literal.
        let head = &s[..s.len() - tail.len()];
        if head.ends_with(|c: char| c.is_alphanumeric() || c == '_') {
            return None;
        }
        Some(tail)
    }

    /// Zero, or the theme's zero.
    fn is_zero(lit: &str) -> bool {
        lit.replace('_', "")
            .parse::<f64>()
            .map(|v| v == 0.0 || v == 0.001)
            .unwrap_or(true)
    }

    /// Both sizes of a kind that carries an INTERACTION must route to one
    /// renderer, or the small one grows a copy that quietly drops the control.
    ///
    /// This has now happened twice in this file. The question's compact arm
    /// listed its options as text above the same options as chips, and was
    /// fixed by delegating. The response's compact arm listed its sections as
    /// one-line summaries with no press target, so the fold was unavailable at
    /// a width a tiled pane reaches constantly — the same defect wearing the
    /// other failure mode: not a duplicated control, a missing one.
    ///
    /// Comments are stripped before matching, because a scan that can be
    /// satisfied by the prose explaining the line it guards is not a gate. The
    /// two kinds named here are the ones whose compact form would otherwise
    /// lose a thing a person presses; a table or a diagram may legitimately
    /// show less, and neither is listed.
    #[test]
    fn an_interactive_kind_has_one_renderer_for_both_sizes() {
        let src = include_str!("benchdraw.rs");
        let (code, _tests) = src.split_once("\n#[cfg(test)]").expect("a test module");
        let stripped: String = code
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let (compact_body, rest) = stripped
            .split_once("fn compact(")
            .expect("a compact renderer")
            .1
            .split_once("fn full(")
            .expect("a full renderer");
        let full_body = rest;
        for (kind, renderer) in [
            ("Kind::Question(q)", "question("),
            ("Kind::Response(r)", "response("),
        ] {
            for (which, body) in [("compact", compact_body), ("full", full_body)] {
                let arm = body
                    .split_once(kind)
                    .unwrap_or_else(|| panic!("{which} has no arm for {kind}"))
                    .1;
                let arm = arm.split_once('\n').map(|(a, _)| a).unwrap_or(arm);
                assert!(
                    arm.contains(renderer),
                    "{which}'s {kind} arm does not delegate to {renderer}: {arm}"
                );
            }
        }
    }

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
    fn the_five_meanings_are_five_different_colours() {
        // The bug this guards is not "the wrong colour", it is "one colour":
        // three of these five resolved to the accent, so a magenta theme drew
        // a magenta bench and the ink said nothing. Any theme, four hues plus
        // a grey — checked on every builtin, since a palette that separates on
        // one theme and collapses on another is the same failure a week later.
        for id in [
            "quiet-command",
            "field-command",
            "tactical-overdrive",
            "gamba",
            "deco",
            "hacker",
        ] {
            let toml = crate::theme::builtin_toml(id).expect("a builtin theme");
            let th = crate::theme::parse(toml).expect("a builtin theme parses");
            let inks: Vec<Hsla> = [
                Tint::Ident,
                Tint::Waiting,
                Tint::Pending,
                Tint::Settled,
                Tint::Unknown,
            ]
            .iter()
            .map(|t| ink(*t, &th))
            .collect();
            for (i, a) in inks.iter().enumerate() {
                for (j, b) in inks.iter().enumerate().skip(i + 1) {
                    assert!(
                        a != b,
                        "{id}: meanings {i} and {j} resolve to the same ink {a:?}"
                    );
                }
            }
        }
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
