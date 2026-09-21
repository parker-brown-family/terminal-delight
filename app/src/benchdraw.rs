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
//! in this window that stays round under a square skin.
//!
//! This paragraph claimed a guard test in `crate::skin` for weeks and there was
//! none — and two literal corners went in underneath the claim, in the
//! composer, where they stayed round while the rest of the window squared. The
//! gate is real now and it is `every_corner_on_the_bench_goes_through_the_skin`,
//! at the bottom of THIS file beside the three other source scans. A promise
//! pointing somewhere else is how the first one went unnoticed, so it points
//! here.

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

/// What a response renderer needs to draw a card it cannot decide for itself:
/// whose reply this is, what the reader picked, and where to register the tabs
/// and chips as click targets.
///
/// Absent — the summary and compact bodies, and any caller with no zone list to
/// offer — the card draws its first tab's first register and nothing is
/// pressable.
///
/// **Both picks are [`Option`] and both mean "the reader has not chosen".** The
/// renderer collapses that to the first group and the first register in it, at
/// draw time; the state map behind this may not store the collapse. See
/// [`crate::workbench::resolve_tab`].
pub struct Picks<'a> {
    pub id: &'a SurfaceId,
    pub tab: Option<crate::surface::Group>,
    /// Which register inside the picked tab, keyed by the group so a reader
    /// returning to a tab lands where they left it. Resolved by the caller,
    /// which is the half that holds the map.
    pub reg: &'a dyn Fn(crate::surface::Group) -> Option<String>,
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
        // Yours. The one role in the palette that already means "you".
        Tint::Mine => Role::Human,
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
/// you in the body, the card that is asking — and everything else takes depth,
/// which separates surfaces without making a claim about attention.
///
/// **A REGION is the unit, and a control is not one.** This is 22 pixels of
/// blur at `th.glow × 0.45`, sized for a card; [`crate::skin::Skin::halo`] is
/// the same idea at a control's scale, five and a quarter at 0.11. Handing this
/// to a button the size of one word closes the bloom over the glyphs from every
/// side — the exact failure the crisp spread-ring was removed from `Skin::ring`
/// for — and the tube's own bloom pass then multiplies it again. APPROVE and
/// the strip's launch verb were both drawn this way until 2026-09-18.
///
/// [`launch_button`] keeps it on purpose and is the only control that does: it
/// is alone on an empty workbench, so there is nothing for the bloom to close
/// over and nothing else competing for the one budget.
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
        // Through `sel`, which is what makes eighty call sites selectable in
        // one edit. A `StyledText` inherits the size, colour and family set
        // above exactly as the plain string it replaced did.
        .child(sel(text.into()))
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
    let lane = match (row.standing, row.kind) {
        // A note does not STAND. The standing vocabulary is about work a person
        // has to resolve — what is waiting, what the answer currently is, how it
        // got there — and none of those questions apply to something you wrote
        // to yourself. `STANDS NOW` on a comment would be the rail claiming an
        // opinion the comment never held.
        //
        // The head of the board still earns a word, because the head of a shelf
        // is where a reader who has not moved the cursor is meant to start, and
        // `Standing::lit` already gives it the weight. LATEST is what that word
        // is on a chronological board. Parker: *"LATEST is nice."*
        (Standing::Current, "comment") => Some("LATEST"),
        (_, "comment") => None,
        (Standing::Waiting, _) => Some("WAITING ON YOU"),
        (Standing::Queued, _) => Some("ALSO WAITING"),
        (Standing::Current, _) => Some("STANDS NOW"),
        (Standing::Past, _) => match (row.tint, row.kind) {
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
                .text_color(sk.ink.ink_faint)
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
                .child(sel(row.title.clone())),
        )
        // Where this fact came from, and when. A surface that shows a state
        // without its provenance is asking to be trusted on nothing — the
        // spine's words, and the reason its rows read as evidence rather than
        // as assertions.
        .when(!row.subtitle.trim().is_empty(), |d| {
            d.child(
                div()
                    .text_size(px(sk.pt(Step::Tag)))
                    .text_color(sk.ink.ink_faint)
                    .child(sel(clip(&row.subtitle, 44))),
            )
        })
}

/// The `+ write a note` row, at the head of the comments board.
///
/// # Why it exists at all
///
/// A note is opened on purpose or not at all — typing on the board used to
/// open one under the first character and that took the keystroke away from the
/// agent, which is where typing goes on every other shelf. Taking that back out
/// left `alt+m` as the only door, and a feature reachable by one undiscoverable
/// chord is a feature most people never find. This is the chord's visible twin.
///
/// # Why it is outlined rather than filled
///
/// Every real row on this rail is a filled box with a solid colour edge. This
/// one is a dashed outline over nothing, which is the oldest honest signal in
/// the vocabulary: a filled box is a THING, an outlined box is a SLOT where a
/// thing would go. It needs no icon to explain it and no label saying "button",
/// and it cannot be misread as the newest note — which a filled row at the top
/// of a newest-first list absolutely would be.
///
/// # Why it wears its own shortcut
///
/// `ALT+M` sits on the right of the row, quiet, permanently. The affordance
/// teaches the faster way to use it every time somebody reaches for the slower
/// one, so the mouse path trains the keyboard path out of existence instead of
/// competing with it.
///
/// The geometry is [`rail_row`]'s — the same 7-point left inset, the same
/// vertical padding, the same corner from the skin — so it sits IN the list
/// rather than on top of it, and a restyle moves both.
pub fn add_note_row(sk: &Skin, th: &Theme) -> Div {
    let mine = ink(Tint::Mine, th);
    sk.row()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(7.))
        .pl(px(7.))
        .pr(px(6.))
        .py(px(6.))
        .border_l(px(2.))
        .border_color(mine.alpha(0.45))
        .border_t(px(1.))
        .border_r(px(1.))
        .border_b(px(1.))
        .border_dashed()
        .rounded(sk.radius())
        .bg(mine.alpha(0.05))
        .hover(move |st| st.bg(mine.alpha(0.14)))
        .cursor_pointer()
        // The plus sits in the same 7-point column the unseen dot occupies on a
        // real row, so the two line up down the list instead of the affordance
        // hanging off the side of it.
        .child(
            div()
                .w(px(7.))
                .flex_none()
                .text_size(px(sk.pt(Step::Small)))
                .text_color(mine)
                .child("\u{2b}"),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(sk.pt(Step::Small)))
                .text_color(mine.alpha(0.92))
                .child("write a note"),
        )
        .child(micro("ALT+M", Step::Tag, sk.ink.ink_faint, sk, th))
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

/// One part of the weight sentence that carries an ink of its own.
///
/// Byte ranges into [`WeightLine::text`] rather than separate elements,
/// because the sentence is ONE run of text: a highlight follows a wrap and a
/// row of coloured boxes does not, and this line has to survive a pane narrow
/// enough to break it across three rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    /// The system's own name, in the agent's own words. Full strength — it is
    /// the proper noun in the sentence, and it is the string that used to be
    /// the one thing cut off.
    Named,
    /// How far down the change reaches.
    Deep(Depth),
    /// How much the agent will stand behind any of it.
    Sure(Confidence),
}

/// The weights written out as a sentence, and the ranges inside it that are
/// not plain prose.
pub struct WeightLine {
    pub text: String,
    pub marks: Vec<std::ops::Range<usize>>,
    pub inks: Vec<Mark>,
}

/// `A` or `An`, agreeing with what follows it.
///
/// Eight adjectives reach this and exactly one of them starts with a vowel,
/// which is precisely how `A involved job` shipped in the first draft of this
/// sentence: a case that occurs once is a case nobody writes a branch for.
fn article(word: &str) -> &'static str {
    match word.chars().next() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "An",
        _ => "A",
    }
}

/// The four weights, said the way a person would say them.
///
/// **This is the whole point of the readout and it was the thing it did not
/// do.** The strip used to print `effort L · complexity moderate · depth
/// subsystem · terminal-delight — wo… · confidence measured` — four
/// name-and-token pairs, one of them cut off mid-word with nothing to hover
/// and no way to see the rest. Parker: *"That depth summary is … with no hover
/// to reveal — it is also a bit machine legible and not for people"*.
///
/// Two defects, one repair. A sentence cannot be truncated without becoming
/// obviously broken, so the pressure that produced `terminal-delight — wo…` is
/// gone rather than patched with a tooltip; and the enum tokens are replaced
/// by what each one MEANS, which the types already say in their own doc
/// comments and which no reader of the card could otherwise know. `subsystem`
/// is a word you have to have been told; *several components depend on it* is
/// the sentence being told.
///
/// Every clause drops on its own, because every weight is optional and an
/// absent one must read as absent rather than as a default. The property test
/// walks all 625 combinations and demands each one is still a sentence.
pub fn weight_line(w: &Weight) -> WeightLine {
    let mut text = String::new();
    let mut marks: Vec<std::ops::Range<usize>> = Vec::new();
    let mut inks: Vec<Mark> = Vec::new();

    // HOW BIG and HOW TANGLED, in one noun phrase: they are two adjectives
    // about the same job and a person says them in one breath. Kept apart in
    // the type for the reason the type gives — a thousand-line rename is large
    // and trivial — and that distinction survives here as two adjectives, not
    // as two rows.
    let size = w.effort.map(|e| match e {
        crate::surface::Effort::Small => "small",
        crate::surface::Effort::Medium => "middling",
        crate::surface::Effort::Large => "big",
        crate::surface::Effort::Epic => "huge",
    });
    let tangle = w.complexity.map(|c| match c {
        crate::surface::Complexity::Trivial => "straightforward",
        crate::surface::Complexity::Moderate => "fiddly",
        crate::surface::Complexity::Involved => "involved",
        crate::surface::Complexity::Hairy => "hairy",
    });
    match (size, tangle) {
        (Some(s), Some(t)) => text.push_str(&format!("{} {s}, {t} job", article(s))),
        (Some(s), None) => text.push_str(&format!("{} {s} job", article(s))),
        (None, Some(t)) => text.push_str(&format!("{} {t} job", article(t))),
        (None, None) => {}
    }

    // WHERE IT LANDS, and what it costs to be wrong there — the field a
    // reviewer actually wants, per `Foundation`'s own doc: not "how long" but
    // "if this is wrong, how much else is wrong with it".
    if let Some(f) = &w.foundation {
        text.push_str(if text.is_empty() { "In " } else { " in " });
        // The name is written WHOLE. `clip(&f.system, 22)` is what Parker was
        // pointing at, and a sentence has nowhere to put an ellipsis: the line
        // wraps instead, which is what the heading directly above it has done
        // since a title was cut at "A kind this build has never hea".
        //
        // An empty system and the parser's own `unnamed` sentinel both say the
        // same thing and neither of them says it in English, so they say it
        // here. Unknown is not zero, and "In , where" is how a renderer admits
        // it forgot that.
        let system = match f.system.trim() {
            "" | "unnamed" => "a system it did not name",
            named => named,
        };
        let at = text.len();
        text.push_str(system);
        marks.push(at..text.len());
        inks.push(Mark::Named);
        text.push_str(", where ");
        let at = text.len();
        text.push_str(match f.depth {
            Depth::Leaf => "nothing else depends on it",
            Depth::Component => "other components call it",
            Depth::Subsystem => "several components depend on it",
            Depth::Bedrock => "a mistake here is paid for by everything above it",
        });
        marks.push(at..text.len());
        inks.push(Mark::Deep(f.depth));
    }
    if !text.is_empty() {
        text.push('.');
    }

    // HOW SURE, as its own sentence. It is a statement ABOUT the three above
    // rather than a fourth thing standing beside them, and a reader who has
    // just been told how deep something reaches is owed the next sentence
    // saying whether anybody checked.
    if let Some(c) = w.confidence {
        if !text.is_empty() {
            text.push(' ');
        }
        let at = text.len();
        text.push_str(match c {
            Confidence::Measured => "Measured",
            Confidence::Inferred => "Inferred, not measured",
            Confidence::Hunch => "A hunch",
            Confidence::Unknown => "The agent looked and could not tell",
        });
        marks.push(at..text.len());
        inks.push(Mark::Sure(c));
        text.push('.');
    }

    WeightLine { text, marks, inks }
}

/// The weights: how big, how tangled, how deep, and how sure.
///
/// A surface nobody weighed says so in a sentence rather than showing four
/// empty slots — four `unavailable`s in a row is noise, and one honest line is
/// the same fact.
///
/// **NOT CHIPS.** These were four `sk.chip(false)` pills sitting directly above
/// the tab strip, which is a row of chips you press — so the card offered eight
/// identical-looking controls of which four did nothing at all. Parker: *"the
/// TASK STATS row … cards need to be bordered and grouped together and made
/// obvious they are NOT CLICKABLE BUTTONS … these are read only"*.
///
/// One border around the group survives that, and is the whole of what is left
/// of the old layout. The four labelled values inside it are gone: see
/// [`weight_line`] for why a tuple of enum tokens with one of them truncated
/// was the wrong object, and what replaced it.
///
/// Colour survives too. Depth and confidence are the two weights that carry an
/// argument, and they keep their inks — now as highlights on their own clause
/// of the sentence, which is the same mechanism the composer's caret uses and
/// the only one that stays correct through a wrap.
pub fn weights(w: &Weight, sk: &Skin, th: &Theme) -> Div {
    if w.is_silent() {
        return div().child(micro(
            "Nobody said how big this is or how deep it goes.",
            Step::Note,
            sk.ink.ink_faint,
            sk,
            th,
        ));
    }
    let line = weight_line(w);
    let spans: Vec<(std::ops::Range<usize>, gpui::HighlightStyle)> = line
        .marks
        .iter()
        .zip(line.inks.iter())
        .map(|(range, mark)| {
            let colour = match *mark {
                // Depth is the field a reviewer actually wants, so bedrock is
                // the one weight allowed to shout.
                Mark::Deep(Depth::Bedrock) => th.complement,
                Mark::Deep(Depth::Subsystem) => th.accent,
                Mark::Named | Mark::Deep(_) | Mark::Sure(Confidence::Measured) => th.text,
                // Amber — yours to argue with. The same ink the doubts use,
                // because it is the same fact said about a different thing.
                Mark::Sure(Confidence::Inferred | Confidence::Hunch) => ink(Tint::Pending, th),
                Mark::Sure(Confidence::Unknown) => sk.ink.ink_faint,
            };
            (
                range.clone(),
                gpui::HighlightStyle {
                    color: Some(colour),
                    ..Default::default()
                },
            )
        })
        .collect();
    // A BLOCK, not a flex row. gpui's `Style::default` is `Display::Block`, so
    // the sentence is a single run of text laid out against the card's width
    // and it WRAPS — which is the whole reason the four values became one
    // string. A `flex_row` here would make the sentence a flex item with
    // `min-width: auto` and it would run off the right edge instead.
    div()
        .w_full()
        .px(px(10.))
        .py(px(6.))
        .rounded(sk.radius())
        .border_1()
        .border_color(sk.ink.rule)
        .text_size(px(sk.pt(Step::Fine)))
        .font_family(th.font_family.clone())
        .text_color(crate::emphasis::meta(th))
        .child(sel(line.text).with_highlights(spans))
}

/// The body of whatever is selected, at the size this pane can honestly show.
pub fn body(
    surface: &Surface,
    how: Embodiment,
    picks: Option<&Picks>,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let frame = div().flex().flex_col().gap(px(10.)).w_full();
    // A QUESTION gets neither the subtitle nor the weights strip.
    //
    // `2 options · waiting on you` counts something the reader can see and
    // repeats what the label already said, and the unweighed line is the agent
    // declining to estimate a picker it did not declare. Both are true and
    // neither is worth a line in front of somebody who has been asked a
    // question. Parker: *"2 options (we can see it is 2 options, no need to
    // show this... if the machine needs it fine, but don't show user)"*.
    //
    // A COMMENT skips the weights for a stronger reason: there is nothing that
    // could ever fill them. Effort, complexity, depth and confidence are an
    // AGENT's estimate of work it did, and a note is a person writing a
    // sentence to themselves. *Nobody said how big this is* on every comment
    // card would be a permanent report of an absence nobody could ever fill —
    // the same line on
    // every row of the shelf, which is a line that has stopped carrying
    // anything. Its subtitle stays, because the stamp under a note is the one
    // fact a chronological board is sorted by.
    let unweighable = matches!(surface.kind, Kind::Question(_) | Kind::Comment(_));
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
            .child(compact(surface, picks, sk, th)),
        Embodiment::Full if unweighable => frame
            .child(heading(surface, sk, th))
            .child(full(surface, picks, sk, th)),
        Embodiment::Full => frame
            .child(heading(surface, sk, th))
            .child(weights(&surface.weight, sk, th))
            .child(full(surface, picks, sk, th)),
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
            sk.ink.ink_faint,
            sk,
            th,
        ))
        .child(
            div()
                .text_size(px(sk.pt(Step::Lead)))
                .text_color(th.text)
                .child(sel(clip(&surface.title, 60))),
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
                        .child(sel(surface.title.clone())),
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
fn compact(surface: &Surface, picks: Option<&Picks>, sk: &Skin, th: &Theme) -> Div {
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
                verdict_ink(h.verdict, sk, th),
                sk,
                th,
            )
        })),
        Kind::Decision(d) => list.children(d.options.iter().map(|o| {
            micro(
                format!("{} {}", if o.recommended { "◉" } else { "○" }, o.name),
                Step::Small,
                if o.recommended {
                    th.text
                } else {
                    sk.ink.ink_faint
                },
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
        Kind::Artifact(a) => {
            list.child(micro(a.href.clone(), Step::Small, sk.ink.ink_faint, sk, th))
        }
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
        Kind::Response(r) => response(r, picks, sk, th),
        // DELEGATES, and belongs on the list the delegation test walks: a
        // comment is short by nature, so there is nothing a narrow pane could
        // usefully show LESS of. A second renderer here would exist only to
        // drift from the first one.
        Kind::Comment(_) => comment(surface, sk, th),
        Kind::Unclassified(u) => list.child(micro(
            u.reason.clone(),
            Step::Small,
            sk.ink.ink_faint,
            sk,
            th,
        )),
    }
}

/// The whole thing.
fn full(surface: &Surface, picks: Option<&Picks>, sk: &Skin, th: &Theme) -> Div {
    match &surface.kind {
        Kind::Artifact(a) => artifact(a, sk, th),
        Kind::Markdown(m) => paragraph(m.body.clone(), sk, th),
        Kind::Table(t) => table(t, sk, th),
        Kind::Architecture(a) => architecture(a, sk, th),
        Kind::Changeset(c) => changeset(c, sk, th),
        Kind::Decision(d) => decision(d, sk, th),
        Kind::Question(q) => question(q, sk, th),
        Kind::Response(r) => response(r, picks, sk, th),
        Kind::Comment(_) => comment(surface, sk, th),
        Kind::Unclassified(u) => unclassified(u, sk, th),
    }
}

// `gist_section()` used to synthesize a `Section` for the always-shown register
// so the accordion could treat it as a row like any other. The tabbed card has
// `workbench::Leaf::Brief` instead — a variant rather than a fabricated struct,
// because the plain brief has no section on the wire and inventing one made it
// possible for a real section to collide with it.

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
                    d.child(micro("inferred", Step::Tag, sk.ink.ink_faint, sk, th))
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
                        .child(sel(a.ask.clone())),
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

/// A reply, as a strip of group tabs over one body.
///
/// A tab per group, a quieter chip row picking the register inside the open
/// one, and exactly one body. Both rows disappear when they would carry a
/// single thing, so the reply most agents send — a gist and nothing else — is
/// two sentences with no chrome at all.
///
/// **It was an accordion until 2026-09-18**, one framed panel per register open
/// or shut, and the panels were the complaint: six registers cost six borders,
/// six pairs of paddings and five gaps before a word of body, and four of those
/// six are alternative lengths of the same reply that nobody reads twice.
/// Parker, on a card that had taken four fifths of a pane: *"these occupy too
/// much space… we want similar to the right bar, folder tabs that go across the
/// top dividing them into groups"*. The three candidate shapes, the case table
/// and the four decisions are in the brief —
/// `~/Work/reports/2026-09-18-response-registers-as-tabs.html`.
///
/// What the reader picked is not decided here: [`Picks`] carries it, and absent
/// means they have not chosen. Without picks the first tab's first register is
/// drawn and nothing is pressable, which is what a summary is.
fn response(r: &Response, picks: Option<&Picks>, sk: &Skin, th: &Theme) -> Div {
    use crate::workbench::Leaf;
    let promoted = escalation_call(r).is_some();
    // Every readable thing, bucketed into its tabs, by the ONE function that
    // decides it. The strip, the chip row and the body all read this list, so a
    // register cannot be drawn under a tab the strip never offered.
    //
    // An `asks` the escalation has already promoted leaves: drawing both prints
    // the same questions twice, which this file has shipped twice before and
    // been told off for twice — *"Again — repeating ourselves ... just ummm...
    // just the buttons"*.
    let tabs = crate::workbench::tabbed(r, promoted);
    let open_group = crate::workbench::resolve_tab(picks.and_then(|p| p.tab), &tabs);
    let leaves: &[Leaf] = open_group
        .and_then(|g| tabs.iter().find(|(t, _)| *t == g))
        .map(|(_, l)| l.as_slice())
        .unwrap_or(&[]);
    let picked_key = open_group.and_then(|g| picks.and_then(|p| (p.reg)(g)));
    let shown = crate::workbench::resolve_leaf(picked_key.as_deref(), leaves);

    let frame = div().flex().flex_col().gap(px(4.));
    // Whether the tabs are drawn at all decides whether there is a PANEL to
    // draw under them — a bare gist keeps its no-chrome shape.
    let stripped = crate::workbench::draws_strip(&tabs);

    // THE STRIP — one tab per group, and never a strip of one. A single tab
    // says nothing a reader did not already know and costs a row on the card
    // that most replies are: a gist and nothing else.
    let frame = frame.when(stripped, |d| {
        d.child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(3.))
                .children(tabs.iter().map(|(g, _leaves)| {
                    let active = open_group == Some(*g);
                    // The tier vocabulary, not a hand-picked colour: the open
                    // tab IS the Active thing on this card now, which is what
                    // `emphasis::shelf()` used to decide for a row of panels.
                    let facet = crate::emphasis::facet(
                        if active {
                            crate::emphasis::Emphasis::Active
                        } else {
                            crate::emphasis::Emphasis::Reading
                        },
                        th,
                    );
                    let tab = sk
                        .chip(active)
                        .flex()
                        .flex_row()
                        .items_baseline()
                        .gap(px(4.))
                        .text_size(px(sk.pt(Step::Note)))
                        .font_family(th.font_family.clone())
                        .text_color(if active {
                            facet.ink
                        } else {
                            crate::emphasis::meta(th)
                        })
                        .when(active, |x| x.border_b_1().border_color(facet.tint))
                        .child(sk.caps(g.label()));
                    match picks {
                        Some(p) => tab.cursor_pointer().relative().child(zone(
                            p.zones.clone(),
                            crate::workbench::Hit::PickTab {
                                id: p.id.clone(),
                                group: *g,
                            },
                        )),
                        None => tab,
                    }
                })),
        )
    });

    // THE PANEL — a border around WHAT THE OPEN TAB IS SHOWING, in the tab's
    // own lit colour, so the strip reads as tabs on a folder rather than as
    // three words floating above some prose. Parker, on the chip row and the
    // body under it: *"which should be BORDERED around their associated
    // section … that changes per what user clicks"*.
    //
    // It is the ACTIVE facet's tint because that is the ink the lit tab
    // already underlines itself with — the border and the tab are the same
    // colour because they are the same thing. Alpha, not a second token: a
    // full-strength box around body text competes with the body.
    //
    // Only when there IS a strip. A reply that is a gist and nothing else
    // draws no tabs, and boxing two sentences that nobody chose to see would
    // put the chrome back that removing the accordion took away.
    let lit = crate::emphasis::facet(crate::emphasis::Emphasis::Active, th);
    let panel = div().flex().flex_col().gap(px(6.)).when(stripped, |d| {
        d.px(px(11.))
            .py(px(9.))
            .rounded(sk.radius_lg())
            .border_1()
            .border_color(lit.tint.alpha(0.45))
    });

    // THE CHIP ROW — quieter than the strip, and absent when the open tab holds
    // one thing. Two rows of chrome over a single register is the chrome this
    // change exists to remove.
    let panel = panel.when(crate::workbench::draws_chips(leaves), |d| {
        d.child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(4.))
                .children(leaves.iter().map(|leaf| {
                    let active = shown.is_some_and(|s| s.key() == leaf.key());
                    let facet = crate::emphasis::facet(
                        if active {
                            crate::emphasis::Emphasis::Active
                        } else {
                            crate::emphasis::Emphasis::Reading
                        },
                        th,
                    );
                    let chip = div()
                        .px(px(sk.tpx(5.)))
                        .text_size(px(sk.pt(Step::Note)))
                        .font_family(th.font_family.clone())
                        .text_color(if active {
                            facet.ink
                        } else {
                            crate::emphasis::meta(th)
                        })
                        .when(active, |x| {
                            x.border_b_1().border_color(facet.tint.alpha(0.8))
                        })
                        .child(sel(leaf.label().to_string()));
                    match picks {
                        Some(p) => chip.cursor_pointer().relative().child(zone(
                            p.zones.clone(),
                            crate::workbench::Hit::PickRegister {
                                id: p.id.clone(),
                                key: leaf.key().to_string(),
                            },
                        )),
                        None => chip,
                    }
                })),
        )
    });

    // ONE BODY. The shown register IS the lit one now — which is what took
    // `emphasis::shelf()`'s only caller away: there is no row of things to tier
    // when only one of them is on screen.
    let panel = panel.when_some(shown, |d, leaf| match leaf {
        Leaf::Brief => d.child(section_body(
            &crate::surface::Body::Prose(r.brief.clone()),
            Register::Layman,
            sk,
            th,
        )),
        Leaf::Section(s) => d.child(section_body(&s.body, s.register, sk, th)),
        Leaf::Doubts => d,
    });

    // The doubts block, drawn when the doubts are what is being read.
    let showing_doubts = matches!(shown, Some(Leaf::Doubts));
    let panel = panel.when(showing_doubts && !r.doubts.is_empty(), |d| {
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
                        .child(micro(
                            "ARTICLES OF DOUBT",
                            Step::Tag,
                            sk.ink.ink_faint,
                            sk,
                            th,
                        ))
                        .child(micro(
                            doubts_measure(r),
                            Step::Note,
                            sk.ink.ink_faint,
                            sk,
                            th,
                        )),
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
                                        .child(sel(format!("\u{b7} {}", doubt.claim))),
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
                                        Some(Confidence::Unknown) | None => sk.ink.ink_faint,
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
    });

    // The strip, then everything the strip is about, inside one boundary.
    frame.child(panel)
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
                            sk.ink.ink_faint,
                            sk,
                            th,
                        )))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(sk.pt(Step::Body)))
                                .text_color(th.text.alpha(0.9))
                                .child(sel(item.clone())),
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
                        sk.ink.ink_faint,
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
/// NAMED, because the names are the whole point and we already have them.
///
/// The first version of this drew one blank segment per question and the words
/// "1 of 2 answered". Both facts were true and neither said *which* question —
/// so a person looking at the bench mid-round could not tell what the other
/// questions were, or that the one on screen was the second of them. Parker,
/// with the picker's own strip beside the bench: *"at the top here has the
/// navigator for WHAT question is being answered: ended state - orphans -
/// submit"*.
///
/// The labels were being parsed and thrown away one step before drawing:
/// [`crate::surface::Step::label`] has been populated the whole time, by the
/// screen reader from the picker's tab bar and now by the channel from the
/// tool's own `header`.
///
/// `zones` makes the steps pressable. Pass [`None`] where there is nothing to
/// press into — the strip still names every step, because knowing a question
/// exists is worth more than being able to jump to it.
pub fn round_progress(
    round: &crate::surface::Round,
    zones: Option<&std::rc::Rc<std::cell::RefCell<Vec<crate::workbench::Zone>>>>,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let done = round.answered();
    let total = round.total();
    let settled = ink(crate::workbench::Tint::Settled, th);
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(div().flex().flex_row().flex_wrap().gap(px(4.)).children(
            round.steps.iter().enumerate().map(|(i, step)| {
                let here = round.current == Some(i);
                // THE SAME TAB A REGISTER IS, and deliberately not a chip of
                // this function's own invention. The registers a reader unfolds
                // on a response card — reading, evidence, next — are underlined
                // text with a muted rest, and a round's questions are the same
                // gesture over the same kind of thing: several readings of one
                // card, one of which you are in. Parker, seeing the first cut:
                // *"we should have tabs along the top of the questions for
                // multiple questions — similar to the response: reading -
                // evidence - next"*. Two vocabularies for one gesture is how a
                // surface stops feeling like one surface.
                let facet = crate::emphasis::facet(
                    if here {
                        crate::emphasis::Emphasis::Active
                    } else {
                        crate::emphasis::Emphasis::Reading
                    },
                    th,
                );
                // A tick is the one thing a register tab has no use for and a
                // question tab needs: a register is never *finished*, and a
                // step that has been answered is. Drawn in the settled hue so
                // done reads as done even on the tab you are standing on —
                // being here does not un-answer it.
                let label = if step.done {
                    format!("\u{2713} {}", step.label)
                } else {
                    step.label.clone()
                };
                let tab = div()
                    .px(px(sk.tpx(5.)))
                    .text_size(px(sk.pt(Step::Note)))
                    .font_family(th.font_family.clone())
                    .text_color(match (here, step.done) {
                        (true, _) => facet.ink,
                        (false, true) => settled.alpha(0.85),
                        (false, false) => crate::emphasis::meta(th),
                    })
                    .when(here, |x| x.border_b_1().border_color(facet.tint.alpha(0.8)))
                    .child(sel(label));
                // Pressable only where BOTH are true: we have somewhere to
                // send the press, and this step has a card of its own. A
                // screen-read step has no surface and must not look like a
                // button that does nothing.
                match (zones, step.id.as_ref()) {
                    (Some(z), Some(id)) if !here => tab
                        .cursor_pointer()
                        .relative()
                        .child(zone(z.clone(), crate::workbench::Hit::OpenRow(id.clone()))),
                    _ => tab,
                }
            }),
        ))
        .child(micro(
            if round.submitting {
                format!("{done} of {total} answered \u{b7} ready to submit")
            } else {
                format!("{done} of {total} answered")
            },
            Step::Fine,
            sk.ink.ink_faint,
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
                sk.ink.ink_faint,
                sk,
                th,
            )),
    )
    .child(
        div()
            .text_size(px(sk.pt(Step::Head)))
            .text_color(th.text)
            .child(sel(title.to_string())),
    )
    .child(
        div()
            .text_size(px(sk.pt(Step::Lead)))
            .text_color(tint)
            .child(sel(answer.to_string())),
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
                        if dim {
                            sk.ink.ink_ghost
                        } else {
                            sk.ink.ink_faint
                        },
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
            Answered::Chose(_) => Some(micro(
                "answered".to_string(),
                Step::Note,
                sk.ink.ink_faint,
                sk,
                th,
            )),
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
                sk.ink.ink_faint,
                sk,
                th,
            )),
        })
        .when_some(q.round.as_ref(), |d, round| {
            // No zones: the card body is drawn in several places, not all of
            // them a pane collecting presses. It names every step; the block
            // the pane assembles is where they can be pressed.
            d.child(round_progress(round, None, sk, th))
        })
}

fn artifact(a: &crate::surface::Artifact, sk: &Skin, th: &Theme) -> Div {
    // The three that make it an artifact, then everything else the agent sent
    // about it. The extra keys used to be lost at the parse and are now kept
    // (see [`crate::surface::Artifact::notes`]) — a `finding`, a `measured`, a
    // `decide` is the reason the document is worth opening, and a card that
    // showed only its path made every artifact look identical.
    let mut fields: Vec<(&str, Option<String>)> = vec![
        ("target", Some(a.href.clone())),
        ("type", a.mime.clone()),
        ("about", a.summary.clone()),
    ];
    fields.extend(a.notes.iter().map(|(k, v)| (k.as_str(), Some(v.clone()))));
    field_grid(fields, sk, th)
}

/// How wide each column wants to be, as a share of the row.
///
/// A table of four columns drawn as four equal columns is four columns of the
/// wrong width: an issue number needs nine characters and the verdict beside
/// it needs sixty, and splitting the row evenly gives the short one an acre
/// and clips the long one. Parker, on a four-column follow-up table: *"not
/// readable due to overflow... should be formatted smartly"*.
///
/// The demand of a column is its widest cell, header included, CLAMPED at both
/// ends before anything is divided: without the ceiling one essay-length cell
/// takes the whole row and leaves its neighbours a sliver, and without the
/// floor a column of one-character cells becomes unreadable at any width.
/// Shares are what a caller gets, not pixels — the row does not know how wide
/// it is, and a fraction survives the pane being resized.
fn column_shares(t: &crate::surface::Table) -> Vec<f32> {
    /// Below this a column cannot hold a word, whatever its content.
    const FLOOR: f32 = 10.0;
    /// Above this a column is wrapping anyway, so more demand buys nothing.
    const CEILING: f32 = 48.0;
    let demand: Vec<f32> = t
        .columns
        .iter()
        .enumerate()
        .map(|(i, head)| {
            let widest = t
                .rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(|cell| match cell {
                    Some(text) => text.chars().count(),
                    // `unavailable` is what the cell will DRAW, so it is what
                    // the column has to be wide enough for.
                    None => "unavailable".len(),
                })
                .max()
                .unwrap_or(0)
                .max(head.chars().count());
            (widest as f32).clamp(FLOOR, CEILING)
        })
        .collect();
    let total: f32 = demand.iter().sum();
    if total <= 0.0 {
        return vec![1.0; t.columns.len().max(1)];
    }
    demand.iter().map(|d| d / total).collect()
}

fn table(t: &crate::surface::Table, sk: &Skin, th: &Theme) -> Div {
    let shares = column_shares(t);
    // `min_w_0` on every cell and nothing anywhere allowed to grow past its
    // share. A flex child's floor is its CONTENT by default, so a long cell
    // pushed the row wider than the card and the last column was drawn off the
    // right edge of the pane — visible in a photograph and in nothing else.
    // With the floor removed the share is binding and the text wraps inside
    // it, which is why no cell is clipped to a character count any more.
    let cell = |share: f32| div().w(gpui::relative(share)).min_w_0();
    let header =
        div()
            .flex()
            .flex_row()
            .w_full()
            .gap(px(10.))
            .children(t.columns.iter().enumerate().map(|(i, c)| {
                cell(shares.get(i).copied().unwrap_or(0.0)).child(micro(
                    c.to_uppercase(),
                    Step::Fine,
                    sk.ink.ink_faint,
                    sk,
                    th,
                ))
            }));
    let rows = t.rows.iter().map(|row| {
        div()
            .flex()
            .flex_row()
            .w_full()
            .items_start()
            .gap(px(10.))
            .py(px(2.))
            .children(row.iter().enumerate().map(|(i, c)| {
                cell(shares.get(i).copied().unwrap_or(0.0)).child(match c {
                    // Clipped at a budget no pane can show rather than at a
                    // width: the wrap decides what fits, and the cap is only
                    // here so one pathological cell cannot make a row taller
                    // than the window.
                    Some(text) => micro(clip(text, 600), Step::Small, th.text, sk, th),
                    // A cell nobody filled says so, rather than being blank and
                    // reading as a value of nothing.
                    None => micro("unavailable", Step::Small, sk.ink.ink_faint, sk, th),
                })
            }))
    });
    sk.panel()
        .flex()
        .flex_col()
        .w_full()
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
                                .child(sel(n.label.clone())),
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
                .child(micro(
                    name.to_uppercase(),
                    Step::Tag,
                    sk.ink.ink_faint,
                    sk,
                    th,
                ))
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
            d.child(micro(r, Step::Note, sk.ink.ink_faint, sk, th))
        })
        .children(c.hunks.iter().map(|h| {
            sk.panel()
                .flex()
                .flex_col()
                .gap(px(3.))
                .border_l(px(3.))
                .border_color(verdict_ink(h.verdict, sk, th))
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
                            sk.ink.ink_faint,
                            sk,
                            th,
                        ))
                        .child(micro(
                            verdict_word(h.verdict).to_string(),
                            Step::Fine,
                            verdict_ink(h.verdict, sk, th),
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
            sk.ink.ink_faint
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
            .child(sel(line.to_string()))
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
                .child(sel(d.question.clone())),
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
                                .child(sel(o.name.clone())),
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
                        sk.ink.ink_faint,
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
                    .child(micro("IF WE DO", Step::Tag, sk.ink.ink_faint, sk, th))
                    .children(d.consequences.iter().map(|c| {
                        micro(format!("· {c}"), Step::Small, th.text.alpha(0.85), sk, th)
                    })),
            )
        })
}

/// A note the person left, drawn as the note and nothing else.
///
/// It takes the whole [`Surface`] rather than the payload, which none of its
/// neighbours do, and the reason is the TITLE. A comment's title is derived
/// from its own first line, so drawing both would print that line twice — once
/// large in the heading and once again as the opening of the body, three lines
/// apart. Comparing the two is the only way to know whether that has happened,
/// and the payload alone cannot: a dropped file may carry a title that is
/// nothing to do with its body, and there the first line is real content that
/// must not be swallowed.
///
/// So: show what the heading has not already said. A one-line note draws as a
/// heading with its provenance and an empty body, which is the whole note; a
/// note with more draws the rest underneath.
fn comment(surface: &Surface, sk: &Skin, th: &Theme) -> Div {
    let body = match &surface.kind {
        Kind::Comment(c) => c.body.as_str(),
        // Unreachable through `compact`/`full`, which match the kind before
        // calling. Drawing nothing beats a panic on a surface.
        _ => "",
    };
    let (first, rest) = match body.split_once('\n') {
        Some((head, tail)) => (head, tail),
        None => (body, ""),
    };
    // The heading already carries the first line IF it is the title. Where the
    // two differ the first line is the payload's own, and it stays.
    let shown = if first.trim() == surface.title.trim() {
        rest
    } else {
        body
    };
    let panel = sk.panel().flex().flex_col().gap(px(6.));
    if shown.trim().is_empty() {
        // Not an error, and not empty in a way worth apologising for: a
        // one-line note IS the heading above. The card says what the shelf is
        // for instead of leaving a blank panel that reads as a failure.
        return panel.child(micro(
            "A NOTE TO YOURSELF \u{b7} THE AGENT WAS NOT TOLD",
            Step::Tag,
            // `ink_faint`, not `th.faint` — the chrome legibility pass moved
            // every quiet line onto the foreground at low alpha because the
            // palette's grey on a grey panel was not readable at all. A line
            // added after that pass has no business reintroducing it.
            sk.ink.ink_faint,
            sk,
            th,
        ));
    }
    panel.child(paragraph(shown.trim().to_string(), sk, th))
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
            sk.ink.ink_faint,
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
                    sk.ink.ink_faint,
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
                        sk.ink.ink_faint,
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
                .child(sel(line.to_string()))
        }))
}

/// The colour of a hunk's verdict — drawn as a WORD beside it and as the edge
/// down its left side, which is why this takes the skin as well as the palette.
/// Undecided used to answer the palette's `faint` role, and that role is
/// furniture: as a word it was unreadable and as an edge it was a line you had
/// to hunt for. The meta ink is both readable and findable.
fn verdict_ink(v: Verdict, sk: &Skin, th: &Theme) -> Hsla {
    match v {
        Verdict::Undecided => sk.ink.ink_faint,
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
/// `trailing` is the strip's right-hand run — the dials and the verb — built by
/// the caller because each of them carries a click zone, and a zone is a
/// decision about what a press MEANS. This renderer takes them already made and
/// puts them where they go; see the test that asserts this file contains no
/// decisions.
pub fn title_card(
    state: crate::workbench::AgentState,
    in_state_ms: u64,
    vitals: Option<&crate::workbench::TurnVitals>,
    tool: Option<&str>,
    trailing: Vec<Div>,
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
    .child(micro("AGENT", Step::Fine, sk.ink.ink_faint, sk, th))
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
            sk.ink.ink_faint,
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
                sk.ink.ink_faint,
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
        d.child(micro(t, Step::Fine, sk.ink.ink_faint, sk, th))
    })
    // THE SLOT. It has been here since the bar became one line — a spacer
    // above whose only job is pushing a trailing element to the right edge —
    // and nothing had ever been put in it, on a bar that is empty across most
    // of its width at any ordinary pane size.
    .children(trailing)
}

/// Every control on the strip wears this: a bordered, padded, pressable box.
///
/// One face for the dials and the verb, because they sit in a row and a row of
/// controls that do not match reads as a row of loose words. Parker, on the
/// strip as it was — three different shapes, one of them an emoji: *"EACH of
/// these should look like a nice button"*.
///
/// The three tones are the three things a control on this strip can be. `open`
/// is a dial with its list down and takes the accent, because something is
/// happening. `primary` is the launch, the only control here that offers
/// rather than takes. Everything else is the quiet bordered default — which is
/// what END is on purpose: a destructive control that shouts is one people
/// press by accident.
fn strip_face(open: bool, primary: bool, sk: &Skin, th: &Theme) -> Div {
    let (edge, fill) = match (open, primary) {
        (true, _) => (th.accent.alpha(0.85), th.accent.alpha(0.14)),
        (_, true) => (th.human.alpha(0.70), th.human.alpha(0.12)),
        _ => (th.text.alpha(0.30), th.text.alpha(0.06)),
    };
    div()
        .flex()
        .flex_row()
        .items_center()
        .flex_none()
        .gap(px(sk.tpx(5.)))
        .px(px(sk.tpx(9.)))
        .py(px(sk.tpx(4.)))
        .rounded(sk.rad(4.))
        .border_1()
        .border_color(edge)
        .bg(fill)
}

/// One of the strip's dials: what the agent was told, and a way to change it.
///
/// `value` is what to draw — the model or the effort, already resolved by the
/// caller from the dial, the launch command, or the harness itself. `known`
/// is whether anybody actually SAID it: an inferred value is drawn faint, so
/// the button reads as a value without ever claiming somebody chose it. The
/// old behaviour drew the dial's own `model ?` / `effort ?` word instead, which
/// was honest and useless — Parker: *"instead of model should say CLAUDE,
/// instead of effort should say xhigh"*.
///
/// `live` is whether a press would be read now; a dial that cannot be pressed
/// says so by going quiet rather than by disappearing, because a control that
/// vanishes and returns is one nobody learns the position of. **Quiet is the
/// CARET's job, never the value's** — see [`dial_ink`].
pub fn dial(value: &str, known: bool, open: bool, live: bool, sk: &Skin, th: &Theme) -> Div {
    strip_face(open, false, sk, th)
        .when(live, |d| d.cursor_pointer())
        .text_size(px(sk.pt(Step::Note)))
        .text_color(th.text.alpha(dial_ink(known)))
        .child(sk.caps(&value.to_uppercase()))
        .child(
            div()
                .text_size(px(sk.pt(Step::Tag)))
                // The affordance, and the only part of the chip that is allowed
                // to answer to `live`: this arrow says a press would open a
                // list, and while the agent is working it would not.
                .text_color(if live {
                    sk.ink.ink_faint
                } else {
                    sk.ink.ink_ghost
                })
                .child("\u{25be}"),
        )
}

/// How strongly a dial draws its VALUE, as an alpha on the theme's text ink.
///
/// **It takes `known` and nothing else, and that is the whole point.** Which
/// model and which effort a pane's agent is running is a fact read off the
/// launch command; it does not stop being true while that agent is busy, and it
/// is most worth reading exactly then — a turn in flight is when a person asks
/// *which model is burning my tokens on this*. The chip used to switch to
/// `faint` at 0.45 alpha whenever the dials were not pressable, which is every
/// working turn, and on the lit header of a working pane that reads as two empty
/// boxes. Parker, with a screenshot of each: *"when idle we can see the model
/// and effort, but when in flight it is hard to read — should ALWAYS be
/// visible!"*.
///
/// So pressability is drawn by the caret and the cursor, and the ink carries one
/// claim only: did somebody CHOOSE this value, or is it inherited from the
/// harness. Two states, two inks, neither of them a disappearing act.
pub fn dial_ink(known: bool) -> f32 {
    if known {
        0.92
    } else {
        0.62
    }
}

/// The list a dial opens: the harness's own values, the current one lit.
///
/// Drawn as a column of rows rather than a native menu because the bench is
/// under the tube and every press on it resolves through the un-bent hit test —
/// a platform menu would be hit-tested flat and land on the wrong row under any
/// real curvature.
pub fn dial_menu(rows: Vec<Div>, sk: &Skin, th: &Theme) -> Div {
    raised(
        sk.panel()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(px(5.))
            .bg(th.surface),
        th.accent,
        th,
    )
    .children(rows)
}

/// One value in an open dial's list.
///
/// `lit` is the value the dial is showing. `chosen` is whether anybody SAID so
/// — the same claim the button's ink makes, carried down into the list so the
/// two cannot disagree: a level a person picked is the accent, a level read off
/// the launch command is the accent at half strength, and a row that is neither
/// is plain text. The list used to light only what a press on the dial had set,
/// so an agent launched with `--model opus` showed OPUS above a list with
/// nothing marked in it at all.
pub fn dial_row(label: &str, lit: bool, chosen: bool, sk: &Skin, th: &Theme) -> Div {
    sk.chip(lit)
        .relative()
        .flex()
        .flex_row()
        .items_center()
        .cursor_pointer()
        .whitespace_nowrap()
        .text_size(px(sk.pt(Step::Note)))
        .text_color(match (lit, chosen) {
            (true, true) => th.accent,
            (true, false) => th.accent.alpha(0.62),
            _ => th.text.alpha(0.85),
        })
        .child(label.to_string())
}

/// The strip's trailing verb: end the agent that is here, or start the next.
///
/// `primary` is the launch — the one that offers something rather than taking
/// something away, and the only one on an ended pane, so it can afford the
/// glow. END is deliberately quiet: a destructive control that shouts is one
/// people press by accident, and this one is beside a dial.
///
/// `glyph` is optional and END no longer carries one. `\u{23f9}` rendered as a
/// colour emoji on this desk — an orange box beside two grey words, which is
/// the loudest thing on the strip attached to the one control nobody should
/// press by accident. Parker: *"instead of end with whatever trash emoji —
/// should say end session"*.
pub fn strip_button(label: &str, glyph: &str, primary: bool, sk: &Skin, th: &Theme) -> Div {
    let el = strip_face(false, primary, sk, th)
        .cursor_pointer()
        .text_size(px(sk.pt(Step::Note)))
        .text_color(if primary {
            th.human
        } else {
            th.text.alpha(0.78)
        })
        .when(!glyph.is_empty(), |d| d.child(glyph.to_string()))
        .child(sk.caps(&label.to_uppercase()));
    if primary {
        // A CONTROL's halo, not a region's. This is a two-word button; it took
        // [`aglow`], which is sized for a whole card, and came out as a cloud
        // with something written in it. See [`Skin::halo`].
        sk.halo(el, th.human)
    } else {
        el
    }
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
///
/// **The size is ALL it adds.** The lit border, the seat and the halo already
/// arrived with [`Skin::chip`], which both call sites hand in, and this used to
/// paint a second border in a second hue over the first and then REPLACE the
/// chip's halo with [`aglow`] — the bloom meant for a whole region. A region's
/// bloom is 22 pixels of blur at `glow × 0.45`; on the hacker palette that is
/// 0.38, against the ring's five and a quarter at 0.11. Four times the spread
/// and three and a half times the heat, on a box the size of one word, and
/// then multiplied again by the tube's own bloom pass. Parker, on APPROVE:
/// *"about 3x or 4 to much extra!!!! dial it WAY back"* — and the ratio he
/// eyeballed is the ratio that was in the file.
pub fn verb_button<E: Styled>(el: E, primary: bool, sk: &Skin) -> E {
    el.px(px(if primary { 18. } else { 12. }))
        .py(px(if primary { 10. } else { 6. }))
        .text_size(px(if primary {
            sk.pt(Step::Lead)
        } else {
            sk.pt(Step::Small)
        }))
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

/// One run of text as it was BUILT, before anything has been laid out.
///
/// The layout handle is filled in during prepaint and shared by reference, so
/// holding it here is holding the real thing — the same trick the composer
/// already uses to turn a click into a column. [`resolve`] reads the geometry
/// out of it once the frame has painted.
pub struct Drawn {
    layout: gpui::TextLayout,
    text: gpui::SharedString,
}

thread_local! {
    /// Where [`sel`] puts the runs it makes, while a bench is being built.
    ///
    /// **A thread-local, and the honest reasons rather than convenience.**
    /// Twenty-five functions in this file draw text and every one of them
    /// takes `(sk: &Skin, th: &Theme)` and nothing else. Threading a collector
    /// through all of them would touch sixty signatures and would make this
    /// module *less* of what its own header claims it is — "a vocabulary
    /// rather than a second controller" — by giving every drawing function a
    /// mutable output parameter it does not otherwise need.
    ///
    /// What makes it safe rather than merely cheap:
    ///
    /// - The element tree is built in ONE synchronous pass on the main thread,
    ///   inside `render`. That is a property of gpui, not an assumption about
    ///   our code.
    /// - It is armed by [`collecting`], an RAII guard that restores whatever
    ///   was there before on the way out — so an early return, a `?`, or a
    ///   panic cannot leave it armed, and a nested build cannot steal the
    ///   outer one's list.
    /// - Disarmed, [`sel`] pushes nothing at all. A `StyledText` built outside
    ///   a bench build is an ordinary `StyledText`.
    ///
    /// The other property this buys for free is the one the selection actually
    /// needs: **build order is reading order.** A list filled at paint would be
    /// in paint order, which is nearly the same and not exactly, and "nearly"
    /// is how a selection ends up copying a heading into the middle of a
    /// paragraph.
    static SINK: std::cell::RefCell<Option<std::rc::Rc<std::cell::RefCell<Vec<Drawn>>>>> =
        const { std::cell::RefCell::new(None) };
}

/// Arm the run collector for one bench build. Disarms on drop.
///
/// The returned guard borrows nothing and does nothing but restore; hold it
/// for exactly as long as the tree is being built.
#[must_use = "the collector disarms the moment this is dropped"]
pub struct Collecting(Option<std::rc::Rc<std::cell::RefCell<Vec<Drawn>>>>);

pub fn collecting(into: std::rc::Rc<std::cell::RefCell<Vec<Drawn>>>) -> Collecting {
    into.borrow_mut().clear();
    Collecting(SINK.with(|s| s.borrow_mut().replace(into)))
}

impl Drop for Collecting {
    fn drop(&mut self) {
        let prev = self.0.take();
        SINK.with(|s| *s.borrow_mut() = prev);
    }
}

/// Text a reader is allowed to drag over.
///
/// A `StyledText` rather than a bare string, because a `StyledText` keeps a
/// [`gpui::TextLayout`] — the one thing that can turn a point inside a run
/// into a character index and back, exactly, through a wrap and a proportional
/// font. It inherits its size, colour and family from the parent `div` the way
/// a plain string child does, so swapping one for the other changes no pixels.
///
/// Outside a [`collecting`] scope this is a plain `StyledText` and registers
/// nothing, which is what makes it safe to use anywhere in this file.
pub fn sel(text: impl Into<gpui::SharedString>) -> gpui::StyledText {
    let text = text.into();
    let styled = gpui::StyledText::new(text.clone());
    SINK.with(|s| {
        if let Some(into) = s.borrow().as_ref() {
            into.borrow_mut().push(Drawn {
                layout: styled.layout().clone(),
                text,
            });
        }
    });
    styled
}

/// Read the laid-out geometry of every collected run.
///
/// **Only ever called from a paint-phase closure at the very bottom of the
/// bench's tree**, and that placement is load-bearing rather than tidy.
/// `gpui::TextLayout` panics when asked for bounds it has not measured — the
/// inner state is an `Option` behind a private field, so there is no way to
/// ask politely — and gpui runs every child's prepaint before any child's
/// paint. Reading here is therefore the one position in the frame where every
/// run in the list is guaranteed to have been measured. See
/// [`atom_probe`], which is the element that does it.
///
/// `region` decides which of the measured regions each run fell in, which is
/// how scope is enforced: by where a thing was drawn, not by which function
/// drew it.
///
/// The two vectors come out in lockstep and are built in one pass for that
/// reason — `crate::workbench::Atom` cannot hold a gpui type, and a selection
/// needs both the geometry and the layout at the same index.
pub fn resolve(
    drawn: &[Drawn],
    region: impl Fn(f32, f32, f32, f32) -> crate::workbench::Region,
) -> (Vec<crate::workbench::Atom>, Vec<gpui::TextLayout>) {
    let mut atoms = Vec::with_capacity(drawn.len());
    let mut layouts = Vec::with_capacity(drawn.len());
    for d in drawn {
        let b = d.layout.bounds();
        let (x, y) = (f32::from(b.origin.x), f32::from(b.origin.y));
        let (w, h) = (f32::from(b.size.width), f32::from(b.size.height));
        atoms.push(crate::workbench::Atom {
            x,
            y,
            w,
            h,
            text: d.text.to_string(),
            region: region(x, y, w, h),
        });
        layouts.push(d.layout.clone());
    }
    debug_assert_eq!(atoms.len(), layouts.len());
    (atoms, layouts)
}

/// The element that resolves the run list, once the frame has painted.
///
/// `debug` arrives as a parameter rather than being read from the environment
/// here, because `a_renderer_contains_no_decisions` forbids this module from
/// reading `std::env` at all — and it is right to: the decision to log is the
/// pane's, and a renderer that consults the environment is one whose output
/// cannot be reproduced from its arguments.
///
/// Goes LAST in the bench's tree, beside the pointer hook and for a related
/// reason: both need every sibling to have been through the frame already.
/// See [`resolve`] for why reading a `TextLayout` any earlier is a panic
/// waiting for a narrow pane.
pub fn atom_probe(
    debug: bool,
    drawn: std::rc::Rc<std::cell::RefCell<Vec<Drawn>>>,
    into: std::rc::Rc<std::cell::RefCell<(Vec<crate::workbench::Atom>, Vec<gpui::TextLayout>)>>,
    regions: std::rc::Rc<
        std::cell::RefCell<Vec<(crate::workbench::Rect, crate::workbench::Region)>>,
    >,
) -> impl gpui::IntoElement {
    gpui::canvas(
        |_, _, _| {},
        move |_, _, _window, _cx| {
            let rs = regions.borrow();
            let resolved = resolve(&drawn.borrow(), |x, y, w, h| {
                crate::workbench::region_of(&rs, x, y, w, h)
            });
            // THE TRACER BULLET, and the reason it counts per region rather
            // than in total. A run that never registered is absent from every
            // copy afterwards and has no symptom on screen — the text is
            // drawn, it simply cannot be dragged over. A total would go up
            // when the strip gained a label and say nothing about whether the
            // card did. Printed only when the shape changes, so a moving
            // pointer does not fill the log.
            if debug {
                use crate::workbench::Region;
                let n = |r: Region| resolved.0.iter().filter(|a| a.region == r).count();
                let shape = (
                    n(Region::Body),
                    n(Region::Rail),
                    n(Region::Composer),
                    n(Region::Chrome),
                );
                thread_local! {
                    static LAST: std::cell::Cell<(usize, usize, usize, usize)> =
                        const { std::cell::Cell::new((usize::MAX, 0, 0, 0)) };
                }
                LAST.with(|l| {
                    if l.get() != shape {
                        l.set(shape);
                        eprintln!(
                            "[bench-sel] runs: body={} rail={} composer={} chrome={} regions={}",
                            shape.0,
                            shape.1,
                            shape.2,
                            shape.3,
                            rs.len()
                        );
                        // And WHAT they say, because a count alone cannot
                        // tell "the bench is drawing almost nothing" from
                        // "the collector is dropping almost everything", and
                        // those two want opposite repairs.
                        for a in resolved.0.iter().take(12) {
                            eprintln!(
                                "[bench-sel]   {:?} {:?}",
                                a.region,
                                a.text.chars().take(56).collect::<String>()
                            );
                        }
                    }
                });
            }
            *into.borrow_mut() = resolved;
        },
    )
    .absolute()
    .inset_0()
}

/// Record a region's flat rectangle, so [`resolve`] can say what fell inside it.
///
/// [`probe`]'s sibling: same canvas, same flat bounds, but appending to a list
/// rather than overwriting one value, because there are several regions and
/// they are recorded by different parts of the tree.
pub fn region_probe(
    into: std::rc::Rc<std::cell::RefCell<Vec<(crate::workbench::Rect, crate::workbench::Region)>>>,
    region: crate::workbench::Region,
) -> impl gpui::IntoElement {
    gpui::canvas(
        move |bounds, _window, _cx| {
            into.borrow_mut().push((
                crate::workbench::Rect {
                    x: f32::from(bounds.origin.x),
                    y: f32::from(bounds.origin.y),
                    w: f32::from(bounds.size.width),
                    h: f32::from(bounds.size.height),
                },
                region,
            ));
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
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

/// Record where the element this is a child of ended up, without making it a
/// click target.
///
/// [`zone`]'s other half: same canvas, same flat bounds, no [`crate::workbench::Hit`].
/// It exists because one element on the bench — the root — has to be MEASURED
/// rather than pressed: an absolutely-positioned overlay inside it is placed in
/// coordinates relative to its padding box, and the only thing that knows where
/// a window-space rectangle lands in that space is the root's own origin.
///
/// Giving the root a zone instead would have been cheaper and wrong: a
/// root-sized rectangle at the bottom of the list answers every click that hit
/// nothing, and "on the bench but on nothing" is an answer the wheel reads.
///
/// The parent must be `relative()`, exactly as for [`zone`].
pub fn probe(
    into: std::rc::Rc<std::cell::RefCell<Option<crate::workbench::Rect>>>,
) -> impl gpui::IntoElement {
    gpui::canvas(
        move |bounds, _window, _cx| {
            *into.borrow_mut() = Some(crate::workbench::Rect {
                x: f32::from(bounds.origin.x),
                y: f32::from(bounds.origin.y),
                w: f32::from(bounds.size.width),
                h: f32::from(bounds.size.height),
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
/// `hot` is a dragged file hovering over this box, and it is drawn in the
/// person's own colour at full strength: a drop is the same act as typing,
/// aimed at the same line, so it would be strange for it to arrive in a
/// colour that means anything else.
pub fn composer(
    line: Option<&crate::workbench::Line>,
    focused: bool,
    hot: bool,
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
            // SELECT-ALL, drawn by the same mechanism as the caret: one
            // highlight over the whole run, which follows a wrap because the
            // text system puts the background behind the characters wherever
            // they land. The caret is dropped while it is up — a block cursor
            // inside a selection reads as two carets, and there is only one
            // place the next keystroke can go.
            let selection = gpui::HighlightStyle {
                background_color: Some(th.human.alpha(if live { 0.34 } else { 0.18 })),
                ..Default::default()
            };
            // The SELECTED RUN, not the whole string. It was
            // `0..l.text().len()` while select-all was the only selection
            // there was; with a range that would paint the entire draft the
            // moment one character was highlighted.
            let spans = match l.sel_bytes() {
                Some(r) => vec![(r, selection)],
                None => vec![(at..next, caret)],
            };
            let styled = gpui::StyledText::new(text).with_highlights(spans);
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
            // legible before anyone has accepted it. A file hovering over it
            // takes the border to full strength and tints the box, because
            // "let go here" has to beat "you may type here" while a person is
            // holding something.
            .border_color(th.human.alpha(if hot {
                1.0
            } else if live {
                0.9
            } else {
                0.5
            }))
            // Blended rather than laid over: `bg` replaces, so a translucent
            // wash here would drop the panel's own surface and let the pane
            // behind it through.
            .when(hot, |d| d.bg(th.surface.blend(th.human.alpha(0.14)))),
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
                        .rounded(sk.rad_raw(3.))
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
            // agent right now, or do I have to click something first?* While
            // a file is over the box it gives up its place: what happens when
            // you let go is the more urgent question, and two chips side by
            // side would be competing for the same corner.
            .when(live && !hot, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(sk.rad_raw(3.))
                        .bg(th.human.alpha(0.16))
                        .child(micro("LIVE \u{2192} AGENT", Step::Tag, th.human, sk, th)),
                )
            })
            // No count of files. The drag's value is parked on the app until
            // the drop and the hover only knows that SOMETHING is being
            // carried, so saying "1 file" here would be inventing a number.
            .when(hot, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(px(7.))
                        .py(px(2.))
                        .rounded(sk.rad_raw(3.))
                        .bg(th.human.alpha(0.24))
                        .child(micro(
                            "\u{2913} DROP TO INSERT THE PATH",
                            Step::Tag,
                            th.human,
                            sk,
                            th,
                        )),
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
            sk.ink.ink_faint,
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

/// The box a note is typed into, and a box with no wire to the agent.
///
/// A separate renderer rather than a mode on [`composer`], for the same reason
/// the note is a separate field on the pane: the two boxes do opposite things
/// and look alike, and the cost of confusing them is a private sentence
/// arriving in somebody's prompt. Nothing here reads `Shows`, `Slots` or the
/// scroll handle, because none of them apply — a note is not mirroring a remote
/// editor, so there is no caret to chase across a wrap that somebody else owns.
///
/// It SAYS what it is, in a line above the text. That label is the only thing
/// standing between the two boxes for a person who has just pressed `alt+m` out
/// of muscle memory, so it names the destination rather than the feature: not
/// `NOTE`, but where the words are going and who will not see them.
pub fn note_box(
    line: &crate::workbench::Line,
    focused: bool,
    durable: bool,
    sk: &Skin,
    th: &Theme,
) -> Div {
    let mine = ink(Tint::Mine, th);
    // The caret rides IN the text as a highlight on the character it is on —
    // the composer's technique, and the reason is the same one written out at
    // length there: a line wraps, and anything that positions a caret by
    // arithmetic is computing a column on a single axis that does not exist.
    let text = format!("{} ", line.text());
    let at = text
        .char_indices()
        .nth(line.caret())
        .map(|(i, _)| i)
        .unwrap_or(line.text().len());
    let next = text[at..]
        .chars()
        .next()
        .map(|c| at + c.len_utf8())
        .unwrap_or(text.len());
    let caret = gpui::HighlightStyle {
        background_color: Some(mine.alpha(if focused { 0.85 } else { 0.35 })),
        color: Some(if focused { th.bg } else { th.text }),
        ..Default::default()
    };
    let selection = gpui::HighlightStyle {
        background_color: Some(mine.alpha(if focused { 0.34 } else { 0.18 })),
        ..Default::default()
    };
    // The selected run only — see the composer above for why this is not
    // the whole string any more.
    let spans = match line.sel_bytes() {
        Some(r) => vec![(r, selection)],
        None => vec![(at..next, caret)],
    };
    raised(
        sk.panel()
            .flex()
            .flex_col()
            .gap(px(7.))
            .flex_none()
            .px(px(14.))
            .py(px(12.))
            .bg(th.surface)
            .border_color(mine.alpha(if focused { 0.9 } else { 0.5 })),
        mine,
        th,
    )
    .child(micro(
        "A NOTE ON THIS PANE \u{b7} THE AGENT IS NOT TOLD",
        Step::Tag,
        mine,
        sk,
        th,
    ))
    .child(
        div()
            .text_size(px(sk.pt(Step::Body)))
            .text_color(th.text)
            .child(gpui::StyledText::new(text).with_highlights(spans)),
    )
    .child(if durable {
        micro(
            "return posts \u{b7} shift+return a new line \u{b7} esc discards",
            Step::Tag,
            sk.ink.ink_faint,
            sk,
            th,
        )
    } else {
        // SAID BEFORE THE NOTE IS WRITTEN, not after return does nothing.
        //
        // A pane with no surfaces directory — a scratch window, which has no
        // pane id to name one with — has nowhere durable to put a note. The
        // first version of this refused on `return` and reported it to stderr,
        // where nobody is looking, so the key just appeared to be broken. An
        // absence a person can act on has to be on the face of the thing while
        // they still have the choice not to type into it.
        micro(
            "THIS PANE CANNOT SAVE NOTES \u{b7} NOTHING HERE WILL SURVIVE",
            Step::Tag,
            ink(Tint::Waiting, th),
            sk,
            th,
        )
    })
}

/// WHAT YOU SAID, over the reply that answers it.
///
/// Its own block, in the human ink, above the card and outside its scroll —
/// three separations, because the complaint was that the person's own words
/// were nowhere on the surface that shows the answer to them. In the ink the
/// terminal already paints a person's turns in, so the two faces of a pane
/// agree about whose voice this is.
///
/// An EMPTY list is drawn as a sentence rather than as nothing. The message
/// may simply have scrolled out of the pane's history, and a block that
/// vanished in that case would say "you asked nothing", which is a different
/// fact and never the true one.
///
/// It is now RARE, and that is the point of the sentence being this specific.
/// The pane latches every human turn it sees ([`crate::pane::TerminalView`]'s
/// `wb_asked`), so the only way to reach this line is for the message to have
/// left the scrollback before the window ever read it — a pane adopted
/// mid-conversation, or a turn that scrolled past between two sweeps. Parker,
/// on the old behaviour, which hit it every long turn: *"the user message
/// prompt... not available because of scrollback limitation... TOTALLY
/// unacceptable, this is EXACTLY important"*.
pub fn asked(lines: &[String], sk: &Skin, th: &Theme) -> Div {
    sk.panel()
        .flex()
        .flex_col()
        .gap(px(4.))
        // Inset from the reply beneath it, the way a quoted turn is: the
        // indent is what says these two blocks are one exchange and not two
        // unrelated panels stacked. Parker, on the first build of it:
        // *"indent a little bit! very nice!"*
        .ml(px(18.))
        .px(px(12.))
        .py(px(9.))
        .border_l(px(3.))
        .border_color(th.human)
        .bg(th.human.alpha(0.07))
        .child(micro("YOU", Step::Fine, th.human, sk, th))
        .when(lines.is_empty(), |d| {
            d.child(micro(
                "your message left this pane\u{2019}s history before the bench read it",
                Step::Note,
                // `ink_faint`, not `th.faint`. The palette's `faint` is the
                // colour a DIVIDER is mixed from — measured at 1.22:1 to
                // 1.62:1 against the surfaces this block sits on, where 1.0 is
                // two identical colours — so a sentence painted in it is a
                // sentence nobody can read. Parker, of this exact line: *"The
                // text here is very hard to read... get it readable"*.
                sk.ink.ink_faint,
                sk,
                th,
            ))
        })
        .children(lines.iter().map(|line| {
            div()
                .text_size(px(sk.pt(Step::Body)))
                .font_family(th.font_family.clone())
                .text_color(th.human)
                .child(sel(line.clone()))
        }))
}

/// WHAT WOKE IT, where their own words would otherwise go.
///
/// The same block as [`asked`] and pointedly not the same voice. A turn the
/// harness opened — a background task reporting in, a peer session talking —
/// arrives at the hook indistinguishably from typing, and was drawn under the
/// word YOU in the person's own ink: a tool-use id, a `/tmp` path and an XML
/// tag, attributed to them. Parker, reading one: *"BUG! I can be certain that
/// I did not type any of this crazy machine talk!"*
///
/// Drawn INSTEAD of their last message rather than alongside it, because the
/// reply underneath answers this and not that. Falling back to the older
/// human turn would caption a machine's answer with a person's question,
/// which is the one thing a caption must never do — the same defect as the
/// bug, pointing the other way.
///
/// `Ident` rather than `Mine`: the tint that means structure and identity, so
/// the block reads as machinery before a word of it is read.
/// What a wake-up block SAYS: its label, and the sentence underneath it.
///
/// Split out of [`woken`] so the words can be read by a test. A `Div` cannot
/// be asked what text is inside it, and the words are the whole point of this
/// block — the defect it exists to fix was a caption saying the wrong thing,
/// not a caption drawn in the wrong box. The other guards in this file scan
/// the SOURCE for a draw call; a rule about what a person ends up reading
/// wants the string itself.
///
/// The harness's own sentence is used where it gave one. Where it did not,
/// the block says THAT rather than drawing an empty frame a reader would have
/// to guess the meaning of — a wake-up nobody described is still a wake-up,
/// and an absent summary is not an absent turn.
pub fn woken_says(w: &crate::channel::Woken) -> (String, String) {
    use crate::channel::Woken;
    match w {
        Woken::Task { summary } => (
            "WOKEN \u{b7} A BACKGROUND TASK FINISHED".to_string(),
            summary
                .clone()
                .unwrap_or_else(|| "the harness named no task".into()),
        ),
        Woken::Peer { from } => (
            match from {
                Some(name) => format!("WOKEN \u{b7} A MESSAGE FROM {}", name.to_uppercase()),
                None => "WOKEN \u{b7} A MESSAGE FROM ANOTHER SESSION".to_string(),
            },
            "another agent session sent this one a message".to_string(),
        ),
        // A shape this build has not met. It says WHICH one, because "some
        // envelope arrived" and "a `scheduled-wake` arrived" are different
        // facts and only the second one can be chased.
        Woken::Other { tag } => (
            "WOKEN \u{b7} BY THE HARNESS".to_string(),
            format!("a \u{2039}{tag}\u{203a} this build has no name for"),
        ),
    }
}

pub fn woken(w: &crate::channel::Woken, sk: &Skin, th: &Theme) -> Div {
    let hue = ink(Tint::Ident, th);
    let (label, body) = woken_says(w);
    sk.panel()
        .flex()
        .flex_col()
        .gap(px(4.))
        .ml(px(18.))
        .px(px(12.))
        .py(px(9.))
        .border_l(px(3.))
        .border_color(hue)
        .bg(hue.alpha(0.07))
        .child(micro(label, Step::Fine, hue, sk, th))
        .child(
            div()
                .text_size(px(sk.pt(Step::Body)))
                .font_family(th.font_family.clone())
                .text_color(th.text.alpha(0.78))
                .child(sel(body)),
        )
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
                .child(sel(line.clone()))
        }))
}

/// The question the agent has stopped on, drawn where it is talking.
///
/// In the conversation rather than behind a click: an agent that cannot
/// continue without a person is the one thing on this surface nobody should
/// have to go looking for. The chips are attached by the pane, because
/// pressing one reaches a pseudoterminal.
pub fn waiting_block(
    q: &crate::surface::Question,
    zones: Option<&std::rc::Rc<std::cell::RefCell<Vec<crate::workbench::Zone>>>>,
    sk: &Skin,
    th: &Theme,
) -> Div {
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
    // THE NAVIGATOR SITS ABOVE THE QUESTION, not under the options, because it
    // answers "which of these am I on" and that is the first thing a person
    // needs — Parker: *"at the top here has the navigator for WHAT question is
    // being answered"*. Under the options it was a progress bar you checked
    // afterwards; a round of two had already reached the bench with the wrong
    // question on it by then.
    .when_some(q.round.as_ref(), |d, round| {
        d.child(round_progress(round, zones, sk, th))
    })
    .child(
        div()
            .text_size(px(sk.pt(Step::Head)))
            .text_color(th.text)
            .child(sel(q.question.clone())),
    )
    .child(div().flex().flex_col().gap(px(5.)).children(
        q.options.iter().enumerate().filter_map(|(i, o)| {
            o.what_happens.as_ref().map(|what| {
                micro(
                    format!("{} \u{b7} {}", i + 1, what),
                    Step::Note,
                    th.text.alpha(0.55),
                    sk,
                    th,
                )
            })
        }),
    ))
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
                    sk.ink.ink_faint,
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
    /// The wake-up block says what woke it, and never says nothing.
    ///
    /// Reading the STRINGS rather than scanning for the draw call, because
    /// the bug this block exists for was a caption whose words were wrong —
    /// a `<task-notification>`, a tool-use id and a `/tmp` path under the
    /// word YOU. A guard that proved a box was drawn would have passed
    /// happily on every day that defect shipped.
    #[test]
    fn a_wake_up_block_names_what_woke_it_and_is_never_silent() {
        use crate::channel::Woken;
        let says = crate::benchdraw::woken_says;

        let (label, body) = says(&Woken::Task {
            summary: Some("Agent \"/code-review high 627\" finished".into()),
        });
        assert!(label.contains("BACKGROUND TASK"), "{label}");
        assert_eq!(
            body, "Agent \"/code-review high 627\" finished",
            "the harness's own sentence is what a person can act on"
        );

        // The peer's NAME reaches the label, so a pane says who is talking to
        // it rather than that somebody is.
        let (label, _) = says(&Woken::Peer {
            from: Some("terminal-delight-05".into()),
        });
        assert!(label.contains("TERMINAL-DELIGHT-05"), "{label}");

        // An unmet envelope is drawn BY NAME. "Something woke it" and "a
        // `scheduled-wake` woke it" are different facts, and only the second
        // can be chased to whatever is sending them.
        let (_, body) = says(&Woken::Other {
            tag: "scheduled-wake".into(),
        });
        assert!(body.contains("scheduled-wake"), "{body}");

        // Every variant the harness can hand over with nothing in it still
        // produces a sentence. An empty block would read as a bug in the
        // bench rather than as a wake-up nobody described — and silence is
        // exactly what an `unwrap_or_default` would have shipped here.
        for w in [
            Woken::Task { summary: None },
            Woken::Peer { from: None },
            Woken::Other { tag: String::new() },
        ] {
            let (label, body) = says(&w);
            assert!(!label.trim().is_empty(), "a block with no label: {w:?}");
            assert!(!body.trim().is_empty(), "a block with no sentence: {w:?}");
        }
    }

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
    /// Every corner on the bench is the skin's to decide.
    ///
    /// This file's own header has promised this since it was written — *"the
    /// guard test in `crate::skin` exists because that has happened before"* —
    /// and there was no such test in `skin.rs` or anywhere else. Two literal
    /// corners went in underneath that sentence and sat there: the paste chip
    /// and the `LIVE -> AGENT` chip in the composer, both `rounded(px(3.))`,
    /// both of which would have stayed round under a square skin while every
    /// other corner in the window squared. A comment promising a gate is worse
    /// than no comment, because it stops the next reader looking.
    ///
    /// Two shapes are caught. A `rounded()` whose argument is not a skin verb
    /// is a radius decided here; and gpui's own `rounded_sm`/`_md`/`_lg`/
    /// `_full` are fixed numbers wearing names, which is the same bypass with
    /// better manners.
    ///
    /// Comments are stripped first, or this file's header would trip its own
    /// scan on the word it uses to describe the rule.
    ///
    /// Mutation-tested: it FAILED on the two real literals before they were
    /// fixed, which is the only way to know a scan matches anything; and
    /// `.gap(px(8.))`, `.w(px(6.))` and `sk.rad_raw(3.)` were checked to pass
    /// untouched, because a check that fires on innocent lines gets switched
    /// off and then nothing is enforced.
    #[test]
    fn every_corner_on_the_bench_goes_through_the_skin() {
        let src = include_str!("benchdraw.rs");
        let (code, _tests) = src.split_once("\n#[cfg(test)]").expect("a test module");
        let lines: Vec<&str> = code.lines().collect();
        let mut found = Vec::new();
        for (n, raw) in lines.iter().enumerate() {
            let line = raw.split("//").next().unwrap_or("");
            for fixed in ["rounded_sm(", "rounded_md(", "rounded_lg(", "rounded_full("] {
                if line.contains(fixed) {
                    found.push(format!(
                        "{}: {fixed} is a fixed radius, not the skin's: {}",
                        n + 1,
                        raw.trim()
                    ));
                }
            }
            let Some(at) = line.find(".rounded(") else {
                continue;
            };
            let mut arg = line[at + ".rounded(".len()..].trim_start();
            // rustfmt may put the argument on the next line. Follow it rather
            // than flagging a wrap, which is not a decision anybody made.
            if arg.is_empty() {
                arg = lines
                    .get(n + 1)
                    .map(|l| l.split("//").next().unwrap_or("").trim_start())
                    .unwrap_or("");
            }
            if !arg.starts_with("sk.") {
                found.push(format!("{}: a hand-rolled corner: {}", n + 1, raw.trim()));
            }
        }
        assert!(
            found.is_empty(),
            "corners decided in the renderer instead of by the skin. A square \
             skin cannot square these, so they stay round while everything \
             around them changes shape:\n{}",
            found.join("\n")
        );
    }

    /// A round's steps are drawn in the REGISTER vocabulary, not one of their own.
    ///
    /// The first cut of this navigator invented bordered, rounded, padded chips.
    /// Nothing was wrong with them in isolation, which is exactly why this guard
    /// exists: a person flipping between a response card and a question card met
    /// two treatments of one gesture, and **every behavioural test passed** —
    /// they assert over `Round` and `Step` and `waiting_question`, and none of
    /// them can see a draw call. Parker had to catch it by eye. Reverting to
    /// chips would be silent a second time.
    ///
    /// **Mutation-tested when written**, and the mutation is the real regression
    /// rather than a strawman: `round_progress`'s tab reverted to
    /// `.px(px(7.)).py(px(2.)).rounded(sk.radius()).border_1()`. The
    /// behavioural tests stayed green and this one failed, naming the border.
    ///
    /// **The first attempt at that mutation proved nothing and looked like it
    /// had.** It replaced the first `.px(px(sk.tpx(5.)))` in the file — there
    /// are two, the other in the register row this tab was copied from — so the
    /// box landed in a different function, the guard passed, and the only
    /// evidence that the mutation had "applied" was a non-empty `git diff`. A
    /// non-empty diff says the FILE changed and never that the function under
    /// test did. The landing check is now the cut body itself: plant it, re-cut,
    /// count the needle inside the slice, and only then believe what the test
    /// says.
    ///
    /// It checks the two halves separately on purpose. A single disjunction
    /// would pass on a tab that asked `emphasis` for its ink and then drew a box
    /// around it anyway, which is the half-migration a hurried edit produces.
    #[test]
    fn a_round_step_is_drawn_the_way_a_register_tab_is() {
        let body = body_of(include_str!("benchdraw.rs"), "pub fn round_progress(");
        assert!(
            body.contains("emphasis::facet("),
            "the round's steps must take their ink from the same ramp the \
             registers do, so one gesture has one look:\n{body}"
        );
        for boxy in [".border_1()", ".rounded("] {
            assert!(
                !body.contains(boxy),
                "a round's step is a TAB, not a chip — `{boxy}` puts a box round \
                 it and the registers it sits beside have none. The underline is \
                 the whole affordance."
            );
        }
    }

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
            // A comment joins the list for the same reason, one step earlier:
            // it is short enough that a compact form could only be the same
            // thing, so a second renderer would exist purely to drift.
            ("Kind::Comment(_)", "comment("),
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

    /// A weight fixture with everything filled in, so each test can knock one
    /// field out rather than build the struct again.
    fn weighed() -> Weight {
        Weight {
            effort: Some(crate::surface::Effort::Large),
            complexity: Some(crate::surface::Complexity::Moderate),
            foundation: Some(crate::surface::Foundation {
                system: "terminal-delight — workbench".into(),
                depth: Depth::Subsystem,
            }),
            confidence: Some(Confidence::Measured),
        }
    }

    /// The range a mark was put on, read back out of the sentence.
    fn marked(line: &WeightLine, m: Mark) -> Option<String> {
        let at = line.inks.iter().position(|i| *i == m)?;
        Some(line.text[line.marks[at].clone()].to_string())
    }

    /// The strip is a SENTENCE, and the name in it is written out whole.
    ///
    /// The line it replaces read `effort L · complexity moderate · depth
    /// subsystem · terminal-delight — wo… · confidence measured`. Parker:
    /// *"That depth summary is … with no hover to reveal — it is also a bit
    /// machine legible and not for people"*.
    ///
    /// Two assertions for two halves of that. The exact string is the
    /// machine-legibility half — there is no way to write `depth subsystem`
    /// and still satisfy it. The system name is the truncation half.
    #[test]
    fn the_weights_read_as_a_sentence() {
        let line = weight_line(&weighed());
        assert_eq!(
            line.text,
            "A big, fiddly job in terminal-delight — workbench, \
             where several components depend on it. Measured."
        );
        assert_eq!(
            marked(&line, Mark::Named).as_deref(),
            Some("terminal-delight — workbench"),
            "the system's name is not the thing wearing its own ink"
        );
        assert_eq!(
            marked(&line, Mark::Deep(Depth::Subsystem)).as_deref(),
            Some("several components depend on it"),
            "depth's ink is not on depth's clause"
        );
        assert_eq!(
            marked(&line, Mark::Sure(Confidence::Measured)).as_deref(),
            Some("Measured")
        );
    }

    /// No budget, no ellipsis, no hover needed.
    ///
    /// Fifty-one characters, against the twenty-two the old strip allowed —
    /// and the shape of the real string, which is a project and a surface and
    /// a part of it. A sentence has nowhere to put an ellipsis, which is the
    /// point: the pressure that produced `terminal-delight — wo…` is gone
    /// rather than covered with a tooltip.
    #[test]
    fn a_long_system_name_is_written_out_whole() {
        let system = "terminal-delight — workbench — the response card";
        let line = weight_line(&Weight {
            foundation: Some(crate::surface::Foundation {
                system: system.into(),
                depth: Depth::Bedrock,
            }),
            ..weighed()
        });
        assert!(
            line.text.contains(system),
            "the system's name was cut: {}",
            line.text
        );
        assert!(
            !line.text.contains('…'),
            "something in the weight line was elided: {}",
            line.text
        );
    }

    /// Absent is absent — it is never a default, and it is never a hole.
    ///
    /// Every weight is an `Option` on purpose, so the renderer has to read as
    /// English with any of the sixteen subsets missing. This walks all 625
    /// shapes and demands each one is a sentence: no doubled space where a
    /// clause dropped out, no orphaned comma, no `In , where`, a capital at
    /// the front and a full stop at the back.
    ///
    /// It also walks the mark ranges, which gpui debug-asserts are char
    /// boundaries and which `with_highlights` needs sorted and
    /// non-overlapping. A multi-byte system name is in the fixture precisely
    /// so a byte-offset slip fails here rather than in a debug build.
    #[test]
    fn every_shape_of_weight_is_still_a_sentence() {
        use crate::surface::{Complexity, Effort, Foundation};
        let efforts = [
            None,
            Some(Effort::Small),
            Some(Effort::Medium),
            Some(Effort::Large),
            Some(Effort::Epic),
        ];
        let tangles = [
            None,
            Some(Complexity::Trivial),
            Some(Complexity::Moderate),
            Some(Complexity::Involved),
            Some(Complexity::Hairy),
        ];
        let depths = [
            None,
            Some(Depth::Leaf),
            Some(Depth::Component),
            Some(Depth::Subsystem),
            Some(Depth::Bedrock),
        ];
        let sures = [
            None,
            Some(Confidence::Measured),
            Some(Confidence::Inferred),
            Some(Confidence::Hunch),
            Some(Confidence::Unknown),
        ];
        let mut seen = 0;
        for effort in efforts {
            for complexity in tangles {
                for depth in depths {
                    for confidence in sures {
                        let w = Weight {
                            effort,
                            complexity,
                            foundation: depth.map(|depth| Foundation {
                                system: "terminal-delight — workbench".into(),
                                depth,
                            }),
                            confidence,
                        };
                        let line = weight_line(&w);
                        seen += 1;
                        if w.is_silent() {
                            assert!(
                                line.text.is_empty(),
                                "an unweighed surface invented a sentence: {}",
                                line.text
                            );
                            continue;
                        }
                        let t = &line.text;
                        for bad in ["  ", " ,", " .", "..", ", where.", "In ,"] {
                            assert!(!t.contains(bad), "{w:?} reads {t:?} — found {bad:?}");
                        }
                        assert!(
                            t.chars().next().is_some_and(char::is_uppercase),
                            "{w:?} opens lowercase: {t:?}"
                        );
                        assert!(t.ends_with('.'), "{w:?} has no full stop: {t:?}");
                        assert!(
                            !t.starts_with("A involved") && !t.contains(" a involved"),
                            "the article does not agree: {t:?}"
                        );
                        assert_eq!(
                            line.marks.len(),
                            line.inks.len(),
                            "a mark with no ink, or an ink with no mark: {t:?}"
                        );
                        let mut end = 0;
                        for range in &line.marks {
                            assert!(
                                range.start >= end && range.end <= t.len(),
                                "marks are not sorted and inside the text: {t:?}"
                            );
                            assert!(
                                t.is_char_boundary(range.start) && t.is_char_boundary(range.end),
                                "a mark cuts a character in half: {t:?}"
                            );
                            end = range.end;
                        }
                    }
                }
            }
        }
        assert_eq!(seen, 625);
    }

    /// The parser fills a missing `system` with `unnamed`, and an agent can
    /// send an empty one. Both are the same fact and neither is English.
    #[test]
    fn a_system_with_no_name_says_so_in_words() {
        for raw in ["", "   ", "unnamed"] {
            let line = weight_line(&Weight {
                effort: None,
                complexity: None,
                foundation: Some(crate::surface::Foundation {
                    system: raw.into(),
                    depth: Depth::Leaf,
                }),
                confidence: None,
            });
            assert_eq!(
                line.text,
                "In a system it did not name, where nothing else depends on it."
            );
        }
    }

    #[test]
    fn clipping_marks_that_it_clipped() {
        assert_eq!(clip("short", 10), "short");
        assert_eq!(clip("abcdefghij", 5), "abcd…");
        // A multi-byte title must not be cut mid-character.
        assert_eq!(clip("→→→→→→", 3), "→→…");
    }

    /// The function body, with its comments stripped and its own tests cut off.
    ///
    /// Comments go first so neither the explanation above a line nor the prose
    /// in this test can satisfy a gate that is looking for the line itself —
    /// a source grep has passed on a comment in this repository before.
    fn body_of<'a>(src: &'a str, signature: &str) -> String {
        let code: String = src
            .split_once("\n#[cfg(test)]")
            .map_or(src, |(before, _)| before)
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let at = code
            .find(signature)
            .unwrap_or_else(|| panic!("{signature} is gone"));
        let rest = &code[at..];
        // A top-level fn ends at the first brace in column zero; a method, at
        // the first one at the impl's own indent.
        let end = rest
            .find("\n}")
            .or_else(|| rest.find("\n    }"))
            .map_or(rest.len(), |i| i + 2);
        rest[..end].to_string()
    }

    /// Two pieces of chrome a person asked to be rid of, kept gone.
    ///
    /// Both are drawing, so nothing in a headless suite can see them. What CAN
    /// be seen is the call that would bring each one back, and in both cases
    /// the regression is a single familiar line somebody reaches for without
    /// knowing it was removed on purpose.
    #[test]
    fn the_read_only_weights_are_not_chips_and_the_routing_lines_stay_off() {
        // A weight is a READOUT. Drawn with `sk.chip` it is the same object as
        // the tab strip immediately under it, so the card offered eight
        // identical controls and four of them did nothing. Parker: *"made
        // obvious they are NOT CLICKABLE BUTTONS … these are read only"*.
        let weights = body_of(include_str!("benchdraw.rs"), "pub fn weights(");
        assert!(
            !weights.contains("sk.chip("),
            "the weights are wearing the chip again:\n{weights}"
        );
        assert!(
            weights.contains("border_color("),
            "the weights lost the one border that groups them:\n{weights}"
        );

        // And the verb previews — the routing line and the tagged prompt text
        // under every card — are a diagnostic rather than something a reader
        // is shown. Parker: *"that machine stuff at the bottom … human does
        // not need to see that"*.
        let verbs = body_of(include_str!("pane/bench.rs"), "fn bench_verbs(");
        assert!(
            verbs.contains("TD_VERB_PREVIEW"),
            "the verb previews are on for everybody again:\n{verbs}"
        );
        assert!(
            verbs.contains("when(auditing"),
            "the preview block is built but no longer gated on the flag:\n{verbs}"
        );
    }

    #[test]
    fn a_tables_columns_are_as_wide_as_what_is_in_them() {
        // The real one, from a follow-up rollup on Parker's bench: a short
        // issue number, a sentence, a size and a verdict. Drawn in four equal
        // columns it gave the number an acre, clipped the verdict at forty
        // characters and still ran off the right edge of the card.
        let t = crate::surface::Table {
            columns: vec![
                "issue".into(),
                "what it is".into(),
                "size".into(),
                "verdict just now".into(),
            ],
            rows: vec![vec![
                Some("#410".into()),
                Some("Rust turns a broken-pipe write into a panic, so the CLI dies".into()),
                Some("One line".into()),
                Some("STILL REAL — piping it exited 101 on today's build".into()),
            ]],
        };
        let shares = column_shares(&t);
        assert_eq!(shares.len(), 4);
        let total: f32 = shares.iter().sum();
        assert!(
            (total - 1.0).abs() < 0.001,
            "shares are a whole row: {total}"
        );
        assert!(
            shares[1] > shares[0] * 2.0,
            "the sentence gets more of the row than the number: {shares:?}"
        );
        assert!(
            shares[0] >= 0.08,
            "and the number still gets enough to be read: {shares:?}"
        );
        // A column of essays does not take the whole row. Without the
        // ceiling one long cell leaves its neighbours a sliver each, which is
        // the same unreadable table wearing different proportions.
        let long = "x".repeat(4000);
        let t = crate::surface::Table {
            columns: vec!["a".into(), "b".into()],
            rows: vec![vec![Some("short".into()), Some(long)]],
        };
        let shares = column_shares(&t);
        assert!(
            shares[0] > 0.15,
            "a four-thousand-character neighbour crushed the short column: {shares:?}"
        );
        // And a table with no rows at all is still drawable.
        let empty = crate::surface::Table {
            columns: vec!["a".into(), "b".into()],
            rows: Vec::new(),
        };
        assert_eq!(column_shares(&empty), vec![0.5, 0.5]);
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

    /// A dial's value is readable whatever the agent is doing.
    ///
    /// Two halves, because the value half cannot be checked by looking at a
    /// colour. [`dial_ink`] takes `known` and no liveness at all, so the
    /// regression is unreachable through it — and the scan below is what stops
    /// it coming back through the renderer instead, which is exactly how it
    /// arrived: `match (live, known)` with a `faint` arm at 0.45 for every
    /// state where a press would not be read.
    ///
    /// The scan slices the `dial` renderer out of the CODE half (tests split
    /// off first) and reads only the line that inks the value — the one before
    /// the `sk.caps` child. `live` is legitimately consulted elsewhere in that
    /// function, by the cursor and by the caret, and a scan of the whole body
    /// would fire on both.
    ///
    /// Mutation-tested when written: the old `match (live, known)` arm was put
    /// back and this test named the line; putting the `faint` ink on the caret
    /// alone left it passing, which is the innocent case it must not cry wolf
    /// on.
    #[test]
    fn a_dial_value_is_legible_whatever_the_agent_is_doing() {
        assert!(
            dial_ink(true) > dial_ink(false),
            "a chosen value still has to read louder than an inherited one"
        );
        assert!(
            dial_ink(false) >= 0.55,
            "an inherited value is still a value somebody has to read: {}",
            dial_ink(false)
        );

        let src = include_str!("benchdraw.rs");
        let (code, _tests) = src.split_once("\n#[cfg(test)]").expect("a test module");
        let at = code.find("pub fn dial(").expect("the dial renderer");
        let body = &code[at..];
        let ends = body
            .find(".child(sk.caps(")
            .expect("the dial draws its value");
        let head: String = body[..ends]
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let inked = head
            .rfind(".text_color(")
            .map(|i| &head[i..])
            .expect("the value is inked");
        assert!(
            !inked.contains("live"),
            "the value's ink reads the agent's state: {}",
            inked.trim()
        );
        assert!(
            !inked.contains("faint"),
            "the value is drawn in the faint ink: {}",
            inked.trim()
        );
        assert!(
            inked.contains("dial_ink("),
            "the value's ink goes through dial_ink: {}",
            inked.trim()
        );
    }
}
