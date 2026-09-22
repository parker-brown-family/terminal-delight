//! THE WORKBENCH — a pane's second face.
//!
//! Every pane in this window has always been one thing: a view of a terminal.
//! This gives it a second face, toggled from its own header, showing the same
//! agent's *work* instead of the same agent's *typing*.
//!
//! ```text
//!   ┌── pane ────────────────────────────────────────┐
//!   │ ▸ claude   [ TERMINAL │ WORKBENCH ]      ⋯  ×  │  ← the toggle, per pane
//!   ├────────────────────────────────────┬───────────┤
//!   │                                    │ artifacts │  ← the rail's shelves
//!   │   the selected work object,        │ decisions │
//!   │   rendered natively                │ other     │
//!   │                                    ├───────────┤
//!   │                                    │ ▸ Spine   │  ← colour-coded history
//!   │                                    │ ▸ Pipeline│
//!   └────────────────────────────────────┴───────────┘
//! ```
//!
//! Both halves are in the pane, which is the decision worth defending. The
//! attention spine on the window's right edge answers *which pane wants me*;
//! this answers *what did this agent make*, and those are different questions
//! asked at different moments. Putting the second one in the window's chrome
//! would make a fleet of twenty agents share one rail, and a rail that mixes
//! twenty conversations is a feed. Inside the pane it also inherits the pane's
//! curvature, its skin and its palette for free — the tube bends the workbench
//! exactly as much as it bends the terminal, because the pixel pass does not
//! know or care which face is underneath it.
//!
//! # Nothing here knows about gpui
//!
//! State transitions, shelf membership, selection, what fits at a given width
//! and which button does what are all decided by pure functions over plain
//! data, in the shape [`crate::tree`] and [`crate::slot`] established: a rule
//! that lives in a render closure can only be tested by photographing it.
//!
//! # Embodiment is ours, not the agent's
//!
//! The agent says what a thing *is*. How much of it can be shown at this size,
//! in this pane, right now, is this window's decision — see [`Embodiment`].
//! The same `architecture` surface is a full interactive drawing in a wide
//! pane, a node list in a narrow one, and one line in a pane nobody is looking
//! at. That is the whole reason the protocol carries meaning rather than
//! layout.

use std::collections::HashSet;

use crate::surface::{Action, ActionReport, Kind, Op, Post, Shelf, Surface, SurfaceId, Verdict};

/// The rail's width when open, matching `LEFT_BAR_W` on the other side of the
/// window. Two docks of different widths on one screen read as a mistake
/// before they read as a choice.
pub const RAIL_W: f32 = 208.0;

/// Collapsed, the rail is a strip of colour-coded ticks — the same gesture the
/// attention spine makes on the window's edge, one scale down.
pub const RAIL_TICK_W: f32 = 18.0;

/// Under this, a pane cannot host a rail AND a readable surface, so the rail
/// collapses to ticks. Measured against the narrowest pane this window will
/// make rather than picked: two panes in a 968-pixel tiled window.
pub const RAIL_COLLAPSE_BELOW: f32 = 420.0;

/// Under this, even the ticks go. A pane this narrow is a strip of text and
/// anything else in it is a bug the person cannot even see to report.
pub const RAIL_HIDE_BELOW: f32 = 240.0;

// ---------------------------------------------------------------------------
// which face is showing
// ---------------------------------------------------------------------------

/// A pane shows one of two faces. Both exist at all times; the terminal keeps
/// running while the workbench is up, because an agent whose output stopped
/// being read is not an agent whose work stopped.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Face {
    #[default]
    Terminal,
    Workbench,
}

/// How much of the tube's vignette a face gets.
///
/// The bench gets none. The vignette is an inset shadow on the pane's content
/// box, darkest at the edges, and the composer is the bench's bottom-most
/// element — so on the bench the line a person was typing was the darkest
/// text on the pane, by construction. The composer diagnostic measured it
/// from photographs (*text dims toward the newest line*); the plan's §3-03
/// confirmed it from `crt.rs`. The terminal keeps its grade untouched: the
/// vignette is the tube's, and the bench is not in the tube. It keeps the
/// scanlines, the bloom and the bend, which are.
///
/// Decided 2026-09-17, the recommendation taken.
pub fn vignette_on(face: Face, vignette: f32) -> f32 {
    match face {
        Face::Terminal => vignette,
        Face::Workbench => 0.0,
    }
}

/// Which pane a scripted bench verb reaches — `ctl bench choose|say|type`.
///
/// The FOCUSED pane, when it qualifies: it is the pane a person is looking
/// at, and it is what the verb's documentation promised from the day it was
/// written. Otherwise the first qualifying pane in the order given, which the
/// caller builds with the active tab's panes ahead of the rest. `eligible`
/// carries one flag per pane in that order; `focused` is the focused pane's
/// index in the same order, if any.
///
/// The rule used to be "the first qualifying pane in tab order" and nothing
/// more, and against a restored window of seventeen tabs `ctl bench type`
/// typed into a pane that was not on screen while the one in front of the
/// person stayed empty — terminal-delight#489.
pub fn bench_target(eligible: &[bool], focused: Option<usize>) -> Option<usize> {
    if let Some(i) = focused {
        if eligible.get(i).copied().unwrap_or(false) {
            return Some(i);
        }
    }
    eligible.iter().position(|e| *e)
}

impl Face {
    pub fn other(self) -> Face {
        match self {
            Face::Terminal => Face::Workbench,
            Face::Workbench => Face::Terminal,
        }
    }

    /// What the header chip says. Four characters, because the toggle sits in
    /// a control cluster that already loses glyphs to the ⋯ overflow at 470
    /// pixels and this one never collapses.
    pub fn chip(self) -> &'static str {
        match self {
            Face::Terminal => "TERM",
            Face::Workbench => "BENCH",
        }
    }
}

// ---------------------------------------------------------------------------
// colour, as a role rather than a value
// ---------------------------------------------------------------------------

/// What a kind means, for whoever is choosing the ink.
///
/// Never a colour. The theme owns colour and a module that hard-coded one
/// would be the single place in this window that ignores the active skin —
/// which is exactly the class of bug the skin layer was built to end.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tint {
    /// Structure and identity: documents, notes, drawings.
    Ident,
    /// Something is waiting on a person.
    Waiting,
    /// A change with a verdict still to give.
    Pending,
    /// Settled, accepted, done.
    Settled,
    /// Nothing is claimed about it.
    Unknown,
    /// YOURS. Not a state of the work at all — a statement about whose voice
    /// this is.
    ///
    /// Every other tint here answers *what should happen to this thing*, and a
    /// comment has no answer to that question: it is not waiting, not pending,
    /// not settled, and `Unknown` would be a claim that nobody has looked. What
    /// distinguishes it is authorship, so that is what it says.
    ///
    /// It resolves to the palette's `human` role — the colour your own typing
    /// is already drawn in inside an agent session, and dialable from the
    /// wheel's own pip. A comments board is then legible as yours before a word
    /// of it is read, and it moves with the palette like everything else.
    Mine,
}

/// Sort a shelf into waiting, then what stands, then the record.
///
/// In place, on rows that arrive newest-first, because both facts this needs —
/// which rows are unanswered, and which settled row is newest — are properties
/// of the list rather than of any row in it.
///
/// Waiting rows keep their own newest-first order among themselves and move
/// above everything settled. Exactly one row is [`Standing::Current`], and only
/// when nothing is waiting: while a question is open, what stands is *nothing
/// yet*, and promoting a superseded answer to "current" underneath an
/// unanswered one would be a lie told in bold.
pub fn stand(rows: &mut [Row]) {
    rows.sort_by_key(|r| r.tint != Tint::Waiting);
    let mut head_seen = false;
    for (i, row) in rows.iter_mut().enumerate() {
        row.standing = if row.tint == Tint::Waiting {
            if head_seen {
                Standing::Queued
            } else {
                head_seen = true;
                Standing::Waiting
            }
        } else if i == 0 {
            // Reachable only when nothing is waiting, since waiting rows sort
            // first: while a question is open, what stands is *nothing yet*.
            Standing::Current
        } else {
            Standing::Past
        };
    }
}

impl Standing {
    /// Does this row get the phosphor?
    ///
    /// The glow is a claim about attention, so exactly one row in a shelf may
    /// make it — and [`stand`] guarantees exactly that, since `Waiting` is the
    /// head of the queue and `Current` only exists when the queue is empty. A
    /// surface where three things glow has told the reader nothing.
    pub fn lit(self) -> bool {
        matches!(self, Standing::Waiting | Standing::Current)
    }
}

/// The one place a kind becomes a colour role.
pub fn tint_of(kind: &Kind) -> Tint {
    match kind {
        Kind::Decision(_) => Tint::Waiting,
        Kind::Question(q) => match q.answer {
            crate::surface::Answered::Waiting => Tint::Waiting,
            _ => Tint::Settled,
        },
        Kind::Changeset(c) => {
            if c.hunks.iter().all(|h| h.verdict != Verdict::Undecided) && !c.hunks.is_empty() {
                Tint::Settled
            } else {
                Tint::Pending
            }
        }
        Kind::Artifact(_) | Kind::Markdown(_) | Kind::Table(_) | Kind::Architecture(_) => {
            Tint::Ident
        }
        // A reply is information, whatever it contains; its doubts get their
        // own colour inside the card rather than tinting the whole row.
        Kind::Response(_) => Tint::Ident,
        Kind::Unclassified(_) => Tint::Unknown,
        // The person's own voice, in the person's own colour.
        Kind::Comment(_) => Tint::Mine,
    }
}

/// One readable thing on a response card: a register, or the doubts.
///
/// The doubts are not a [`Section`](crate::surface::Section) — they are their
/// own shape on the wire and their own block on the card — but on a tabbed card
/// they are one more thing you can be looking at, so they are a leaf like any
/// other and the renderer switches on which kind it drew.
#[derive(Clone, Copy, Debug)]
pub enum Leaf<'a> {
    /// The bare minimum, in two sentences. Present only when the agent wrote
    /// one — and where it is, it is what the card opens on.
    Brief,
    /// The whole reply in plain English. Always present: a response without one
    /// does not parse.
    Layman,
    Section(&'a crate::surface::Section),
    /// Present only when the reply carries doubts.
    Doubts,
}

impl<'a> Leaf<'a> {
    /// The key a press names it by — the same key the wire used, where there
    /// was one.
    ///
    /// Takes `self` rather than `&self` (the type is [`Copy`]) so the borrow it
    /// returns belongs to the SECTION rather than to whichever local the leaf
    /// was sitting in, which is what lets a caller map over a list of them.
    pub fn key(self) -> &'a str {
        match self {
            Leaf::Brief => "brief",
            Leaf::Layman => "layman",
            Leaf::Section(s) => &s.key,
            Leaf::Doubts => "doubts",
        }
    }

    /// The word on its chip.
    pub fn label(self) -> &'a str {
        match self {
            // `Brief`, `Plain brief`, `Technical brief` — the row reads as one
            // ladder of depth, shortest on the left, which is the only reason
            // the bare word is allowed to sit beside two that qualify it.
            Leaf::Brief => "Brief",
            Leaf::Layman => "Plain brief",
            Leaf::Section(s) => &s.label,
            Leaf::Doubts => "doubts",
        }
    }

    pub fn group(self) -> crate::surface::Group {
        match self {
            Leaf::Brief | Leaf::Layman => crate::surface::Group::Reading,
            Leaf::Section(s) => crate::surface::Group::of(s.register),
            // The doubts are evidence: they are what the agent could not
            // establish, filed beside what it did.
            Leaf::Doubts => crate::surface::Group::Evidence,
        }
    }
}

/// Everything a response card can show, bucketed into its tabs.
///
/// **One definition, because two would drift.** The strip, the chip row, the
/// body and the default all read this, so a register cannot appear under a tab
/// that the strip does not draw — which is the failure mode of computing the
/// tabs in one place and the contents in another.
///
/// Groups come back in [`Group::ALL`](crate::surface::Group::ALL) order and an
/// empty group is left out, which is what makes "no strip for one group" a
/// property of the data rather than a special case in the renderer.
///
/// `promoted_asks` is the escalation having already drawn the questions above
/// the card: the asks register then leaves, because printing it twice is a bug
/// this file has shipped twice and been told off for twice.
pub fn tabbed(
    r: &crate::surface::Response,
    promoted_asks: bool,
) -> Vec<(crate::surface::Group, Vec<Leaf<'_>>)> {
    // The plain reply LEADS, and the brief comes last in its group.
    //
    // The order is the default: `resolve_leaf` takes the first leaf when the
    // reader has picked nothing, so where a leaf sits and what the card opens
    // on are one decision rather than two that can disagree. Parker, on a
    // first cut that put the brief in front: *"it should be ordered last!
    // Plain Brief is still the default."* A reader who has opened a card has
    // already decided to read; the fifty-word version is the rung they drop
    // to. Pushed after the sections so it lands behind the technical brief —
    // its group filters it back out of this vector, so its position relative
    // to the doubts below does not matter.
    let mut leaves: Vec<Leaf<'_>> = vec![Leaf::Layman];
    leaves.extend(
        r.sections
            .iter()
            .filter(|s| !(promoted_asks && s.register == crate::surface::Register::Asks))
            .map(Leaf::Section),
    );
    if r.brief.is_some() {
        leaves.push(Leaf::Brief);
    }
    if !r.doubts.is_empty() {
        leaves.push(Leaf::Doubts);
    }
    crate::surface::Group::ALL
        .into_iter()
        .filter_map(|g| {
            let mine: Vec<Leaf<'_>> = leaves.iter().copied().filter(|l| l.group() == g).collect();
            (!mine.is_empty()).then_some((g, mine))
        })
        .collect()
}

/// Does this card draw a tab strip at all?
///
/// **Not when there is one group.** A strip of one tab says nothing the reader
/// did not already know and costs a row on the card that most replies are — a
/// gist and nothing else. The rule lives here rather than in the renderer
/// because it is a rule: `benchdraw`'s own guard test refuses a threshold in
/// that file, on the argument that a number a renderer compares against is
/// policy wearing a renderer's clothes.
pub fn draws_strip(tabs: &[(crate::surface::Group, Vec<Leaf<'_>>)]) -> bool {
    tabs.len() > 1
}

/// Does the open tab draw a chip row under the strip?
///
/// Same rule one level down: a chip row naming the single thing already on
/// screen is chrome charging for a choice nobody has.
pub fn draws_chips(leaves: &[Leaf<'_>]) -> bool {
    leaves.len() > 1
}

/// Which tab is open: the one the reader picked, or the first one there is.
///
/// A pick for a tab that is not present any more — the reply was updated and
/// the group went away — falls back rather than drawing an empty card.
pub fn resolve_tab(
    picked: Option<crate::surface::Group>,
    tabs: &[(crate::surface::Group, Vec<Leaf<'_>>)],
) -> Option<crate::surface::Group> {
    picked
        .filter(|g| tabs.iter().any(|(t, _)| t == g))
        .or_else(|| tabs.first().map(|(g, _)| *g))
}

/// Which leaf is shown inside the open tab: the one the reader picked, or the
/// first one in it.
pub fn resolve_leaf<'a>(picked: Option<&str>, leaves: &'a [Leaf<'a>]) -> Option<Leaf<'a>> {
    picked
        .and_then(|k| leaves.iter().find(|l| l.key() == k).copied())
        .or_else(|| leaves.first().copied())
}

/// What the title card says about the turn in flight: the agent's own clock,
/// its own token count, and the tool call it is in the middle of.
///
/// Each is an [`Option`] and stays one all the way to the card: a clock the
/// screen did not carry is drawn as unread, never as zero. Parker, looking at
/// a title bar that said only `Working 10s` across an acre of room: *"we have
/// a lot of space here... show tokens, time elapsed on turn, + the current
/// tool or w/e being used."*
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct TurnVitals {
    /// The turn's elapsed time as the agent printed it — `3m 27s`.
    pub elapsed: Option<String>,
    /// The turn's tokens so far, as a count.
    pub tokens: Option<u64>,
    /// The in-flight tool line — `Calling lean-ctx, terminal-delight 3 times`.
    pub doing: Option<String>,
}

impl TurnVitals {
    /// Did the screen carry any of the three?
    pub fn is_unread(&self) -> bool {
        self.elapsed.is_none() && self.tokens.is_none() && self.doing.is_none()
    }
}

/// The vitals for the card, or `None` when there is no turn to have them.
///
/// Only a WORKING agent has a turn in flight. Numbers off the screen of an
/// idle agent belong to the last turn, which the card is not about, and the
/// session total already lives on the header's token badge.
pub fn turn_vitals(status: &crate::hud::AgentStatus) -> Option<TurnVitals> {
    status.working().then(|| TurnVitals {
        elapsed: status.elapsed.clone(),
        tokens: status.turn_tokens,
        doing: status.doing.clone(),
    })
}

// ---------------------------------------------------------------------------
// how much of a thing fits
// ---------------------------------------------------------------------------

/// How much of a surface this pane can honestly show right now.
///
/// The agent provides semantic truth; the work surface decides embodiment.
/// Same object, three bodies.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Embodiment {
    /// The whole thing, interactive.
    Full,
    /// The shape of it: headings, counts, the first rows, the recommendation.
    Compact,
    /// One line. A pane nobody is looking at does not get to spend pixels
    /// drawing a diagram at a size where the labels are unreadable anyway.
    Summary,
}

/// Decide the body from the room available.
///
/// Room only. An earlier version also asked whether the pane was FOCUSED and
/// dropped an unfocused one to a summary — which meant the bench changed
/// shape depending on which pane held the keyboard, so a person glancing at
/// the pane beside the one they were typing in saw a smaller surface with no
/// way to type into it. In a tiled window every visible pane is being read,
/// so "nobody is looking" was never true of any of them.
///
/// The thresholds are this machine's real widths rather than round numbers:
/// 968 is a tiled pane, and a two-column tab inside it is about 470 each.
pub fn embodiment(content_w: f32, content_h: f32) -> Embodiment {
    if content_w < 300.0 || content_h < 160.0 {
        return Embodiment::Summary;
    }
    if content_w < 460.0 || content_h < 260.0 {
        return Embodiment::Compact;
    }
    Embodiment::Full
}

/// How the rail can be drawn at this pane width.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RailFit {
    /// Rows, titles, shelves.
    Open(u32),
    /// A strip of ticks, one per surface, in kind colours.
    Ticks,
    /// Nothing. The pane is too narrow to spend a pixel on.
    Hidden,
}

/// What fits, given the pane's whole width and whether the person opened it.
///
/// `wanted` is the person's own preference, which loses to physics and wins
/// over everything else: a rail closed by hand stays closed in a pane wide
/// enough to hold it.
pub fn rail_fit(pane_w: f32, wanted: bool) -> RailFit {
    if pane_w < RAIL_HIDE_BELOW {
        return RailFit::Hidden;
    }
    if !wanted || pane_w < RAIL_COLLAPSE_BELOW {
        return RailFit::Ticks;
    }
    // A SHARE of the pane, not a fixed 208. Matching the left bar's width
    // sounded right and was measured against the wrong thing: the left bar
    // divides a WINDOW and this divides a PANE, so the number that is a tenth
    // of one is more than a third of the other — and the body it left behind
    // was too narrow to hold the conversation the bench exists for.
    let want = (pane_w * RAIL_SHARE).clamp(RAIL_MIN_W, RAIL_W);
    RailFit::Open(want as u32)
}

/// The composer's local shadow of the agent's own input line, WITH a caret.
///
/// The bench types straight into a pseudoterminal, so the real line editor is
/// the agent's: Claude's own history, completion and kill-ring do the work,
/// and nothing here should try to replace them. What this holds is a mirror,
/// and it exists for one reason — a person cannot put their cursor in the
/// middle of a line they cannot see the cursor in. Parker: *"I cannot CURSOR
/// around inside the CHATBOX, I need to be able to click here to add
/// something, click there to add something."*
///
/// **Mirroring, not owning.** Every edit here has already been sent as bytes;
/// this applies the same edit to the copy so the box can draw where the agent's
/// caret now is. Both start empty and in sync, and every operation a person can
/// perform moves both by the same amount, so they stay in sync. When they do
/// drift — a history recall, a completion, anything the agent's editor does on
/// its own — the drift is visible in the agent's echo directly above, and
/// pressing enter resets both to empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Line {
    text: String,
    /// In CHARACTERS, not bytes. A byte index would put the caret inside a
    /// multi-byte glyph the first time somebody pastes an em dash.
    caret: usize,
    /// How many images have been pasted into this line.
    ///
    /// A COUNT, not the text of them, and that distinction is the whole
    /// design. On an agent pane the paste chord goes straight to the agent,
    /// which reads the clipboard itself and writes `[Image #7]` into its own
    /// prompt — a number this side cannot know and must not invent. Writing
    /// `[Image]` into the mirror instead would put it three characters out of
    /// step with the line it mirrors, and every caret and click after that
    /// would land in the wrong place.
    ///
    /// So the mirror stays exactly what was typed, and the fact that an image
    /// went with it is carried beside the text rather than inside it. Parker,
    /// on a paste that reached the agent and left the box looking empty: *"The
    /// image is showing up in the terminal mirror, but not in the text area."*
    pasted: usize,
    /// Where a selection STARTED, if one is running. The other end is the
    /// caret.
    ///
    /// This was a `bool` until 2026-09-21, because select-all was the only
    /// selection the box had and one bit held it. The reason recorded at the
    /// time was that "there is no mouse drag over the draft and no shift+arrow
    /// here" — true, and the thing being fixed: a bit cannot say WHERE a
    /// selection began, so the fifth `shift+left` had nowhere to grow from.
    ///
    /// [`None`] is no selection, which is not the same as an empty one. An
    /// anchor equal to the caret is collapsed and normalised back to [`None`]
    /// by [`Line::apply`], so no caller has to decide whether a zero-width
    /// range means *nothing is selected* or *a selection of nothing*.
    ///
    /// Stored UNORDERED — `anchor > caret` is a backwards selection and is
    /// ordinary, because the anchor stays put while the caret walks back over
    /// it and out the other side. [`Line::sel_range`] is the ordered reading.
    anchor: Option<usize>,
    /// What the draft was before each change, newest last, so `ctrl+z` can
    /// walk back through them. A document has an undo; a mirror could not,
    /// because the far end had already taken the keystroke.
    undo: Vec<(String, usize)>,
}

/// How many changes a draft can take back.
pub const UNDO_KEPT: usize = 200;

impl Line {
    pub fn new() -> Line {
        Line::default()
    }

    /// A line already holding text, caret at the end — what a paste or a
    /// scripted `ctl bench say` produces.
    pub fn holding(text: impl Into<String>) -> Line {
        let text = text.into();
        let caret = text.chars().count();
        Line {
            text,
            caret,
            pasted: 0,
            anchor: None,
            undo: Vec::new(),
        }
    }

    /// Remember the draft as it is, ahead of a change.
    fn snapshot(&mut self) {
        let last = self.undo.last();
        if last.is_some_and(|(t, c)| *t == self.text && *c == self.caret) {
            return;
        }
        self.undo.push((self.text.clone(), self.caret));
        if self.undo.len() > UNDO_KEPT {
            self.undo.remove(0);
        }
    }

    /// Take the last change back. `false` when there is nothing to take.
    pub fn undo(&mut self) -> bool {
        let Some((text, caret)) = self.undo.pop() else {
            return false;
        };
        self.text = text;
        self.caret = caret.min(self.chars());
        self.anchor = None;
        true
    }

    /// The caret as a row and a column, over the draft's own line breaks.
    pub fn line_col(&self) -> (usize, usize) {
        let mut row = 0;
        let mut col = 0;
        for (i, c) in self.text.chars().enumerate() {
            if i == self.caret {
                break;
            }
            if c == '\n' {
                row += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (row, col)
    }

    /// Up one line, keeping the column where the line allows. On the first
    /// line the caret goes to the start — the same thing every text box on
    /// this desk does with an up it cannot honour.
    pub fn up(&mut self) {
        let (row, col) = self.line_col();
        if row == 0 {
            self.caret = 0;
            return;
        }
        self.caret = self.at_row_col(row - 1, col);
    }

    /// Down one line; on the last line the caret goes to the end.
    pub fn down(&mut self) {
        let (row, col) = self.line_col();
        let rows = self.text.chars().filter(|c| *c == '\n').count() + 1;
        if row + 1 >= rows {
            self.caret = self.chars();
            return;
        }
        self.caret = self.at_row_col(row + 1, col);
    }

    /// The character index of a row and column, clamped to the row's length.
    fn at_row_col(&self, row: usize, col: usize) -> usize {
        let mut r = 0;
        let mut start = 0;
        for (i, c) in self.text.chars().enumerate() {
            if r == row {
                break;
            }
            if c == '\n' {
                r += 1;
                start = i + 1;
            }
        }
        let len = self
            .text
            .chars()
            .skip(start)
            .take_while(|c| *c != '\n')
            .count();
        start + col.min(len)
    }

    /// Select the whole draft — `ctrl+a`, the convention every text box on this
    /// desk answers to.
    ///
    /// The caret ends at the END of the draft, which is where every text box
    /// on this desk leaves it and where a person expects to carry on typing.
    ///
    /// It used to go to the START, because the mirror had to agree with a
    /// readline-shaped editor on the far end that the same keystroke had
    /// already moved to column zero. There is no far end any more — the
    /// composer owns the draft — so the convention wins. Parker, on the box
    /// not answering the chord at all: *"Ctrl+a does not highlight all in the
    /// workbench text area"*.
    fn mark_all(&mut self) {
        self.anchor = (!self.text.is_empty()).then_some(0);
        self.caret = self.chars();
    }

    /// The selection as an ordered half-open range of CHARACTER indices.
    ///
    /// [`None`] when nothing is selected, and also when the anchor has
    /// collapsed onto the caret — a range of nothing is not a selection, and
    /// making the caller distinguish them would be the same mistake the
    /// boolean made in the other direction.
    ///
    /// **This is the only place that decides it.** `Extend` deliberately does
    /// not clear an anchor the caret walked back onto, because two rules for
    /// what a zero-width range means is one rule too many, and a highlight of
    /// zero characters that also swallows the caret is what it would cost.
    pub fn sel_range(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        let (lo, hi) = if a <= self.caret {
            (a, self.caret)
        } else {
            (self.caret, a)
        };
        (lo < hi).then_some((lo, hi))
    }

    /// The same range in BYTES, for a renderer that highlights a span of the
    /// string it was handed.
    pub fn sel_bytes(&self) -> Option<std::ops::Range<usize>> {
        let (lo, hi) = self.sel_range()?;
        Some(self.byte_at(lo)..self.byte_at(hi))
    }

    /// What is selected, for the clipboard.
    pub fn selected_text(&self) -> Option<&str> {
        let r = self.sel_bytes()?;
        Some(&self.text[r])
    }

    /// Start a selection here if one is not already running, then let the
    /// caller move the caret. The anchor is never moved by a second call —
    /// that is the whole reason it exists.
    fn anchor_here(&mut self) {
        if self.anchor.is_none() {
            self.anchor = Some(self.caret);
        }
    }

    /// Drop the selection without touching the text — any unshifted motion
    /// does this.
    fn clear_mark(&mut self) {
        self.anchor = None;
    }

    /// Delete what is selected and report that something went. The caret
    /// lands where the selection started, which is where the replacement for
    /// it belongs.
    fn take_marked(&mut self) -> bool {
        let Some((lo, hi)) = self.sel_range() else {
            return false;
        };
        self.cut(lo, hi);
        self.caret = lo;
        self.anchor = None;
        true
    }

    /// Record that an image went to the agent with this line.
    pub fn note_paste(&mut self) {
        self.pasted += 1;
    }

    /// The images are no longer on the agent's line, so stop saying they are.
    ///
    /// No live caller since the composer became a document: the dial used to
    /// erase the far end's line (taking the agent's `[Image #7]` with it) and
    /// this kept the count honest. The far end's line is empty now, so nothing
    /// is erased. Kept for the test that pins the count's meaning.
    #[cfg(test)]
    pub fn forget_pastes(&mut self) {
        self.pasted = 0;
    }

    /// How many, so the composer can say so without claiming to know what the
    /// agent called them.
    pub fn pasted(&self) -> usize {
        self.pasted
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn chars(&self) -> usize {
        self.text.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Insert at the caret and step over it. A SELECTION is replaced — only
    /// the selected run, which is the difference a range buys over the bit
    /// that came before it: typing over `beta` in `alpha beta gamma` used to
    /// take the whole draft with it.
    pub fn insert(&mut self, s: &str) {
        self.snapshot();
        self.take_marked();
        let at = self.byte_at(self.caret);
        self.text.insert_str(at, s);
        self.caret += s.chars().count();
    }

    /// Delete the character BEFORE the caret. `false` when there is none,
    /// which is how the caller knows the agent's own editor did nothing
    /// either.
    pub fn backspace(&mut self) -> bool {
        if self.caret == 0 {
            return false;
        }
        let from = self.byte_at(self.caret - 1);
        let to = self.byte_at(self.caret);
        self.text.replace_range(from..to, "");
        self.caret -= 1;
        true
    }

    /// Delete the character AT the caret.
    pub fn delete(&mut self) -> bool {
        if self.caret >= self.chars() {
            return false;
        }
        let from = self.byte_at(self.caret);
        let to = self.byte_at(self.caret + 1);
        self.text.replace_range(from..to, "");
        true
    }

    /// Put the caret somewhere by index — a click — and drop the selection.
    ///
    /// **The only way in from outside this type.** The four movers below are
    /// private because each moves the caret and leaves the anchor alone,
    /// which is right inside [`Line::apply`] — where the rule about what a
    /// selection survives is applied once, for all of them — and is a trap
    /// anywhere else.
    ///
    /// It was a trap. `seek` was public, `bench_click` called it, and a click
    /// into a select-all left the highlight standing while the caret moved
    /// (#615), found by reading rather than by a test. The first fix put a
    /// `clear_mark()` beside the call, which repairs the instance and leaves
    /// the next caller to remember. This is the shape that needs no
    /// remembering: there is one door, and it does both halves.
    pub fn place(&mut self, at: usize) {
        self.caret = at.min(self.chars());
        self.anchor = None;
    }

    fn left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    fn right(&mut self) {
        self.caret = (self.caret + 1).min(self.chars());
    }

    fn home(&mut self) {
        self.caret = 0;
    }

    fn end(&mut self) {
        self.caret = self.chars();
    }

    /// Move the caret, and nothing else.
    ///
    /// One mover for both the bare arrows and the shift-held ones, so a
    /// selection can never extend somewhere a plain arrow would not have
    /// gone. Two movers would be two word rules and two wrap rules, which is
    /// exactly how `ctrl+w` nearly ended up meaning different things on the
    /// two sides of the old mirror.
    fn move_to(&mut self, m: Motion) {
        match m {
            Motion::Left => self.left(),
            Motion::Right => self.right(),
            Motion::WordLeft => self.caret = self.word_start(),
            Motion::WordRight => self.caret = self.word_end(),
            Motion::Home => self.home(),
            Motion::End => self.end(),
            Motion::Up => self.up(),
            Motion::Down => self.down(),
        }
    }

    /// Apply an [`Edit`]. The one place a key becomes a change to this line,
    /// so the mirror and the agent's own editor cannot drift by having two
    /// slightly different ideas of what `ctrl+w` does.
    pub fn apply(&mut self, edit: Edit) {
        // WHAT A SELECTION SURVIVES, in one place.
        //
        // `Extend` grows it, `SelectAll` makes it, and backspace/delete
        // consume it. Everything else drops it — decided here rather than in
        // each arm, which is what stops a new arm silently inheriting a stale
        // highlight. The plain Left and Right arms below read the range
        // BEFORE this runs, because collapsing onto an edge needs to know
        // which edge.
        let sel = self.sel_range();
        if !matches!(
            edit,
            Edit::SelectAll | Edit::Backspace | Edit::Delete | Edit::Extend(_)
        ) {
            self.clear_mark();
        }
        // A change is remembered before it is made; a motion is not a change.
        if matches!(
            edit,
            Edit::Backspace
                | Edit::Delete
                | Edit::KillWordLeft
                | Edit::KillWordRight
                | Edit::KillToStart
                | Edit::KillToEnd
                | Edit::Newline
        ) {
            self.snapshot();
        }
        match edit {
            Edit::Undo => {
                self.undo();
            }
            Edit::Newline => {
                self.take_marked();
                let at = self.byte_at(self.caret);
                self.text.insert(at, '\n');
                self.caret += 1;
            }
            // Grow the selection. The anchor is set on the FIRST extend and
            // never again, so the fifth shift+left grows from where the first
            // one started — the one thing the boolean could not do.
            // The anchor is NOT cleared when the caret walks back onto it.
            // Whether that is a selection is [`Line::sel_range`]'s question
            // and it answers it in one place — a second normalisation here
            // would be a second rule, and the one that ran first would decide
            // what a zero-width range means. Keeping the anchor is also more
            // correct: shift+right, shift+left, shift+right re-selects the
            // same character rather than re-anchoring at the caret.
            Edit::Extend(m) => {
                self.anchor_here();
                self.move_to(m);
            }
            Edit::SelectAll => self.mark_all(),
            // A plain LEFT or RIGHT with a selection up COLLAPSES to the near
            // edge rather than stepping one character off the caret. Pressing
            // left after selecting a word puts you at its start, which is
            // what every text box does and what the hand expects. The other
            // six motions just move, which is also the convention.
            Edit::Move(Motion::Left) => match sel {
                Some((lo, _)) => self.caret = lo,
                None => self.left(),
            },
            Edit::Move(Motion::Right) => match sel {
                Some((_, hi)) => self.caret = hi,
                None => self.right(),
            },
            Edit::Move(m) => self.move_to(m),
            Edit::Backspace => {
                if !self.take_marked() {
                    self.backspace();
                }
            }
            Edit::Delete => {
                if !self.take_marked() {
                    self.delete();
                }
            }
            Edit::KillWordLeft => {
                let to = self.word_start();
                self.cut(to, self.caret);
                self.caret = to;
            }
            Edit::KillWordRight => {
                let to = self.word_end();
                self.cut(self.caret, to);
            }
            Edit::KillToStart => {
                self.cut(0, self.caret);
                self.caret = 0;
            }
            Edit::KillToEnd => {
                let end = self.chars();
                self.cut(self.caret, end);
            }
            Edit::Submit => self.clear(),
        }
    }

    /// The start of the word behind the caret.
    ///
    /// Skip the whitespace immediately behind, then the run of word characters
    /// before that — readline's rule, and the one every editor on this desk
    /// agrees on. A caret already at the start answers zero rather than
    /// wrapping.
    fn word_start(&self) -> usize {
        let ch: Vec<char> = self.text.chars().collect();
        let mut i = self.caret.min(ch.len());
        while i > 0 && !ch[i - 1].is_alphanumeric() {
            i -= 1;
        }
        while i > 0 && ch[i - 1].is_alphanumeric() {
            i -= 1;
        }
        i
    }

    /// The end of the word in front of the caret, by the mirror rule.
    fn word_end(&self) -> usize {
        let ch: Vec<char> = self.text.chars().collect();
        let mut i = self.caret.min(ch.len());
        while i < ch.len() && !ch[i].is_alphanumeric() {
            i += 1;
        }
        while i < ch.len() && ch[i].is_alphanumeric() {
            i += 1;
        }
        i
    }

    /// Remove a character range. Nothing happens on an empty or inverted one,
    /// which is what a kill at either end of the line asks for.
    fn cut(&mut self, from: usize, to: usize) {
        if from >= to {
            return;
        }
        let a = self.byte_at(from);
        let b = self.byte_at(to);
        self.text.replace_range(a..b, "");
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.pasted = 0;
        self.anchor = None;
        self.undo.clear();
    }

    fn byte_at(&self, chars: usize) -> usize {
        self.text
            .char_indices()
            .nth(chars)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }
}

/// One answered question, as the review gallery shows it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Reviewed {
    pub title: String,
    /// What was chosen, in the words it was chosen by. Never a number — the
    /// point of reviewing is to read the answer, not to decode it.
    pub answer: String,
}

/// What escape takes off, innermost first.
///
/// **It never takes off a question somebody is being waited on.** Escape on
/// this surface has always meant "close the outermost thing", and the ladder
/// ran gallery, then typing, then the open card, then the face — which is
/// correct right up until the open card is the question an agent has stopped
/// on. Then a key that means *give me less* removes the one thing that cannot
/// be got back without going to the terminal, and it did: Parker, tracing his
/// vanishing question, *"i may have pressed ESC while the review pane was up
/// ... and that is what killed the question interaction ... that interaction
/// surface for answering questions must be MORE persistent and the target for
/// hitting esc should always land on the overlay"*.
///
/// So a waiting question is a FLOOR. Escape peels everything above it and
/// stops there, and the way out of a pane that is waiting on you is the TERM
/// chip — a deliberate move rather than the same key you have been dismissing
/// things with.
///
/// **THE BENCH ITSELF IS THE SAME KIND OF FLOOR, for every card and none.** The
/// ladder used to have one more rung under all of this: with nothing left to
/// dismiss, escape flipped the pane to the terminal. That made the workbench a
/// thing you were *inside* rather than a surface you were *on*, and it is the
/// one behaviour no other base surface on this desk has — escape does not take
/// a text editor out of its buffer or a browser out of its page. Parker: *"the
/// workbench is considered a base work surface, so Escape should not escape me
/// out of it."*
///
/// [`Peel::Face`] is therefore deleted rather than made unreachable. An enum
/// variant nothing returns is a trap for the next reader, who has to run the
/// function to find out it is dead; deleting it makes the compiler say so.
/// The ways out of the bench are alt+k and the TERM chip, both deliberate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Peel {
    /// An open dial menu, which is drawn over everything and is the newest
    /// thing on the screen.
    Dial,
    /// The review gallery, drawn over everything.
    Gallery,
    /// A half-typed line in the composer.
    Typing,
    /// The opened card — but only when it is not holding a live question.
    Card,
    /// Nothing: either a question is waiting on a person, or the bench itself
    /// is all that is left and the bench is not something escape leaves.
    Nothing,
}

pub fn peel(dial: bool, gallery: bool, typing: bool, card_open: bool, card_waits: bool) -> Peel {
    // ABOVE THE GALLERY, because it is above everything: a dial menu is the
    // last thing opened and the smallest thing to lose. Escape reaching past
    // it to empty the composer would take the person's sentence to close a
    // list of five words — which is what it did before this rung existed.
    if dial {
        return Peel::Dial;
    }
    if gallery {
        return Peel::Gallery;
    }
    if typing {
        return Peel::Typing;
    }
    if card_open {
        // The floor.
        return if card_waits {
            Peel::Nothing
        } else {
            Peel::Card
        };
    }
    // The other floor, and the reason this function no longer has a fourth
    // rung: a bench with nothing on it is still the surface you are working on.
    Peel::Nothing
}

/// A gesture the bench can take from a click.
///
/// One enum, so that hit-testing is one table and dispatch is one `match`,
/// and so that a new control is a new variant the compiler makes you handle
/// rather than a new closure nobody audits.
#[derive(Clone, PartialEq, Debug)]
pub enum Hit {
    /// Press an option of the open question.
    Choose(usize),
    /// Press a row of the agent's own menu by its navigation index — the
    /// picker's Submit or Next button.
    PressNav(usize),
    /// One of the card's verbs.
    Verb {
        action: crate::surface::Action,
        target: Option<String>,
    },
    /// Open the review gallery.
    Review,
    /// The card's close.
    CloseCard,
    /// The launcher, on a shell pane's empty bench.
    Launch,
    /// End the agent running in this pane, from the strip.
    EndAgent,
    /// Stop the running turn without ending the session, from the strip.
    PauseTurn,
    /// Tell a paused turn to carry on, from the strip.
    ResumeTurn,
    /// Open one of the strip's dials, or close it if it is the open one.
    Dial(Dial),
    /// Take the nth value from the open dial's list — indexed as the harness's
    /// own list orders it, so the press and the flag cannot disagree.
    DialPick(Dial, usize),
    /// The composer's text box: arm it, or move the caret.
    Composer,
    /// The body beneath the composer: arm the line.
    Arm,
    /// The rail's collapse handle, and its collapsed ticks.
    ToggleRail,
    /// A response card's group tab: read that group.
    PickTab {
        id: SurfaceId,
        group: crate::surface::Group,
    },
    /// A register chip inside the open tab: show that one.
    PickRegister {
        id: SurfaceId,
        key: String,
    },
    Shelf(crate::surface::Shelf),
    /// The `+ write a note` row at the head of the comments board.
    ///
    /// A note is now only ever opened on purpose — `alt+m`, or this. Typing on
    /// the board used to open one under the first character and that took the
    /// keystroke away from the agent, which is where typing goes everywhere
    /// else on the bench. Removing it left the feature reachable by one chord
    /// and nothing else, so the chord needed a visible twin: the row teaches
    /// `alt+m` by wearing it, and is the thing a hand reaches for meanwhile.
    AddNote,
    OpenRow(crate::surface::SurfaceId),
    GalleryBack,
    GalleryForward,
    GalleryClose,
    /// The dim field around the gallery: a click there does nothing, and
    /// must not fall through to the card underneath.
    Nothing,
}

impl Hit {
    /// The pointer a control asks for.
    ///
    /// The composer is text. The field around it — which arms the line — and
    /// the dim field around the gallery are nothing to point at. Everything
    /// else is pressed, and says so with a hand.
    pub fn pointer(&self) -> Pointer {
        match self {
            Hit::Composer => Pointer::Text,
            Hit::Arm | Hit::Nothing => Pointer::Arrow,
            _ => Pointer::Hand,
        }
    }
}

/// Does this click close an open dial menu instead of doing what it says?
///
/// `None` is a click that reached no control — the bench's own background,
/// which is most of it — and that is a dismissal like any other. The menu's
/// two controls are the exception: the dial itself toggles, and a value on
/// the list is the press the menu was opened for.
pub fn dial_dismisses(open: bool, hit: Option<&Hit>) -> bool {
    open && !matches!(hit, Some(Hit::Dial(_)) | Some(Hit::DialPick(..)))
}

/// Which of the AGENT strip's two dials.
///
/// The strip is the one place a running agent's model and effort can be
/// changed without leaving the bench for the terminal. Two dials rather than
/// one because they are independent: `claude --help` at 2.1.274 offers all five
/// effort levels for every model, and the one refusal the bundle carries —
/// *"is session-scoped and won't reach the remote process"* — is about cloud
/// sessions, which this window does not launch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dial {
    Model,
    Effort,
}

// A dial used to carry an `unknown()` word — `model ?` and `effort ?` — drawn
// whenever nobody had told this pane anything, on the rule that a default is
// not a reading. The rule survives; the word did not. Nothing can ask a running
// process what model it is on, but the command that STARTED it can be read, and
// where even that says nothing the harness itself is still a fact about the
// pane. So the button now carries a value and the INK carries the claim: chosen
// reads as text, inferred reads faint. See `benchdraw::dial` and the resolution
// in `TerminalView::strip_trailing`.
impl Dial {
    /// The harness's own command for setting this dial, which a press types.
    ///
    /// Both exist in the installed Claude Code (2.1.274) and both take an
    /// inline argument — the bundle carries `"/model, /effort"`,
    /// `argumentHint:"[model]"`, and its own help line *"`/effort` controls how
    /// long Claude thinks before answering"*.
    pub fn command(self) -> &'static str {
        match self {
            Dial::Model => "/model",
            Dial::Effort => "/effort",
        }
    }

    /// The word that has to appear in the harness's own confirmation for it to
    /// be THIS dial's.
    ///
    /// Claude Code 2.1.274 heads the two pickers `Switch model?` and `Change
    /// effort level?` (both read out of the shipped binary, not guessed), so
    /// one word each separates them — and separates either from every other
    /// question a terminal might be showing. The window presses Yes on its own
    /// question and on nothing else: a permission gate is also a two-option
    /// picker with a Yes in it, and auto-answering one of those would approve
    /// a tool call nobody looked at.
    ///
    /// A harness that words it differently is not matched, the window never
    /// presses anything, and the person answers the picker themselves — which
    /// is exactly today's behaviour and is why this can be a plain word match.
    pub fn confirm_word(self) -> &'static str {
        match self {
            Dial::Model => "model",
            Dial::Effort => "effort",
        }
    }
}

/// What the pointer looks like over the bench.
///
/// Decided from the UN-BENT position, like a click, so the hand appears over
/// what the tube shows as a button rather than over where gpui laid it — a
/// child's own `cursor_pointer()` is hit-tested flat and would put the hand
/// beside the button under any real curvature.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Pointer {
    #[default]
    Arrow,
    Text,
    Hand,
}

/// What a wheel turn over the bench moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Wheel {
    /// The composer's draft, by the delta.
    Composer,
    /// The card in the body, when one is on it.
    Card,
    /// The agent's scrollback, when the mirror is showing.
    Mirror,
    /// Nothing — and the turn is consumed, so nothing under the bench moves
    /// flat either.
    Nothing,
}

/// Where a wheel turn goes, given what the UN-BENT pointer is over.
///
/// The composer takes the wheel when the pointer is on it. Otherwise the CARD
/// takes it when there is one, because a card taller than the pane is the
/// commonest thing on this surface and until this arm existed **nothing could
/// reach what it clipped** — the body was `overflow_hidden` and every turn
/// over it landed here as `Nothing`. Parker, on a response card cut off by the
/// composer: *"the bottom of this element is cut off. It should be, if not
/// scrollable, then broken into separate elements that are then collapsible."*
/// It was already collapsible, and that is exactly why collapsing is not the
/// answer: the folds were in the screenshot and one unfoldable section can be
/// taller than the pane on its own.
///
/// Otherwise the mirror, if it is showing — the agent's own scrollback, which
/// is what the wheel did on the bench before there was a composer. Otherwise
/// nothing, and the turn is consumed rather than handed back to gpui, because
/// gpui would hit-test it FLAT and scroll the composer while the eye is on the
/// card above it — the same displacement clicks already un-bend. See
/// [`unwarp`].
///
/// `card` and `mirror` cannot both be true today (the mirror is drawn in the
/// arm that has no card), and the order here says which would win if that ever
/// changed: the thing a person is reading beats the debug flag.
pub fn wheel_target(over: Option<&Hit>, card: bool, mirror: bool) -> Wheel {
    match over {
        Some(Hit::Composer) => Wheel::Composer,
        _ if card => Wheel::Card,
        _ if mirror => Wheel::Mirror,
        _ => Wheel::Nothing,
    }
}

/// A scroll offset after a wheel delta, in gpui's convention.
///
/// The offset is the distance from the container's top to the content's top:
/// zero with the first line showing, `-max` with the last, and a turn is
/// added and then held inside that range. A container with nothing hidden
/// (`max <= 0`) stays at zero whatever the wheel does.
pub fn wheel_offset(offset: f32, delta: f32, max: f32) -> f32 {
    (offset + delta).clamp(-max.max(0.0), 0.0)
}

/// Whether the composer's view follows an edit — asks for its bottom before
/// the next frame — or stays where the person scrolled it.
///
/// It follows while the caret is at the end of the line, which is where the
/// caret is for almost every keystroke, and where "keep the end on screen"
/// means keeping the caret on screen. A caret parked earlier in a long draft
/// was put there on purpose, and a view that jumps to the bottom under a
/// hand is worse than one that has to be scrolled back.
pub fn follows(caret: usize, len: usize) -> bool {
    caret >= len
}

/// A flat rectangle and what pressing it means.
///
/// Recorded by the element itself at paint, in the same flat coordinates gpui
/// lays out in. The barrel warp is a pixel post-pass; layout never moves.
#[derive(Clone, PartialEq, Debug)]
pub struct Zone {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub hit: Hit,
}

/// A flat rectangle that is only a measurement — no hit, nothing pressable.
///
/// Recorded by [`crate::benchdraw::probe`] in the same coordinates a [`Zone`]
/// is, because an overlay has to be POSITIONED against things whose position
/// nobody chose: where the strip's dials ended up after a flex row laid them
/// out, and where the bench's own root is in the window.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// The air between a dial and the list it drops.
pub const DIAL_DROP_GAP: f32 = 6.0;

/// Where an open dial's list hangs, in the bench root's own coordinates.
///
/// `dial` and `root` are flat window-space rectangles — the pressed dial and
/// the bench's outermost box — and the answer is the `(right, top)` an
/// absolutely-positioned child of that root takes to sit directly under the
/// dial. The list used to take a constant `right` measured from the rail, which
/// put it under the END SESSION button at the far end of the strip whichever
/// dial had been pressed; Parker, on the model list: *"are misaligned on the
/// drop down :("*.
///
/// **Right-aligned, not left.** The dials live at the right-hand end of the
/// strip, the list is as wide as its longest word, and nothing here knows that
/// width — so hanging it from the dial's LEFT edge is the one choice that can
/// push it off the pane on a narrow bench. Sharing the dial's right edge cannot,
/// by construction.
///
/// `None` while either rectangle is unmeasured, which is a real state and not a
/// zero: on the very first frame of a window nothing has painted yet, and the
/// caller falls back to the old fixed corner rather than stacking the list in
/// the top-left.
pub fn dial_drop(dial: Option<Rect>, root: Option<Rect>) -> Option<(f32, f32)> {
    let (d, r) = (dial?, root?);
    Some(((r.x + r.w) - (d.x + d.w), (d.y + d.h + DIAL_DROP_GAP) - r.y))
}

/// Which zone a FLAT point lands in.
///
/// The LAST zone that contains the point wins, because zones are recorded in
/// paint order and a later element paints over an earlier one: the gallery,
/// registered last, takes a click that a card underneath it would otherwise
/// have claimed. Left and top edges are inclusive, right and bottom exclusive,
/// so two zones sharing an edge do not both claim it.
pub fn hit_at(zones: &[Zone], x: f32, y: f32) -> Option<&Hit> {
    zones
        .iter()
        .rev()
        .find(|z| x >= z.x && x < z.x + z.w && y >= z.y && y < z.y + z.h)
        .map(|z| &z.hit)
}

// ---------------------------------------------------------------------------
// selecting the bench's text
// ---------------------------------------------------------------------------

/// Which part of the bench a run of text was drawn in.
///
/// Scope is decided by GEOMETRY rather than by call site, and this is the
/// answer that test produces. Every run on the bench registers itself as it is
/// built — the strip's dials as much as a card's paragraph — and then a
/// rectangle test against the two or three measured regions decides which of
/// them a reader is allowed to drag over. Parker's line was *"I don't care
/// about the menus, being able to highlight end session or overview or
/// artifacts is not important"*, and a region test draws that line on the
/// surface instead of maintaining it by hand in a list somebody has to
/// remember to update when a new control is added.
///
/// `Chrome` is the explicit NOT-selectable answer rather than the absence of
/// one: a run outside every region has been looked at and rejected, which is a
/// different fact from a run whose region nobody has computed yet.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Region {
    /// The card — the surface the agent produced, and everything on it.
    Body,
    /// The rail down the right: shelf rows, comment summaries, provenance.
    Rail,
    /// The composer's mirror of the line being typed.
    Composer,
    /// The strip, the shelf tabs, the gallery, the pane header. Drawn, never
    /// selectable.
    Chrome,
}

impl Region {
    /// Can a reader drag over text drawn here?
    pub fn selectable(self) -> bool {
        !matches!(self, Region::Chrome)
    }
}

/// One run of text, as it was actually laid out.
///
/// The twin of [`Zone`], and deliberately the same shape: a flat rectangle
/// recorded by the element itself, in the coordinates gpui lays out in, read
/// back through the same [`unwarp`]. What differs is the payload — a `Zone`
/// carries what pressing it MEANS, an `Atom` carries what it SAYS.
///
/// Nothing here knows about gpui, on the same terms as the rest of this
/// module. The `TextLayout` that turns a point inside this rectangle into a
/// character index lives beside it on the pane's side of the boundary, at the
/// same index; `crate::benchdraw::resolve` builds the two together in one pass
/// so they cannot drift.
#[derive(Clone, PartialEq, Debug)]
pub struct Atom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// What it says. Owned, because the selection has to survive the frame
    /// that drew it — see [`Sel::still_valid`].
    pub text: String,
    pub region: Region,
}

impl Atom {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    /// The vertical middle, which is what the band comparisons use. A run's
    /// TOP is not enough on its own: a chip and the sentence beside it are one
    /// line to the eye and are often a pixel or two apart at the top because
    /// their font sizes differ.
    fn mid(&self) -> f32 {
        self.y + self.h / 2.0
    }
}

/// One end of a selection: which run, and how far into it.
///
/// `byte` is a BYTE offset into that run's text and is always on a character
/// boundary — every constructor here goes through a clamp that walks to one.
/// Bytes rather than characters because that is what gpui's text layout speaks
/// on both sides (`index_for_position` returns one, `with_highlights` takes a
/// range of them), and converting at the edges would put two conversions
/// between the pointer and the highlight instead of none.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Caret {
    pub atom: usize,
    pub byte: usize,
}

/// A selection in progress, or a settled one.
///
/// Anchor is where the press landed and never moves; head follows the pointer.
/// They are stored unordered — `anchor > head` is a backwards drag and is
/// normal — because the anchor has to stay put for a drag that crosses back
/// over its own start, and [`Sel::ends`] is the only thing that cares which is
/// first.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Sel {
    pub anchor: Caret,
    pub head: Caret,
}

impl Sel {
    /// A selection of nothing, at one point. What a press with no drag leaves.
    pub fn at(caret: Caret) -> Sel {
        Sel {
            anchor: caret,
            head: caret,
        }
    }

    /// Anchor and head in document order.
    pub fn ends(&self) -> (Caret, Caret) {
        let back = (self.head.atom, self.head.byte) < (self.anchor.atom, self.anchor.byte);
        if back {
            (self.head, self.anchor)
        } else {
            (self.anchor, self.head)
        }
    }

    /// Does this select nothing? A bare click, or a drag that came back to
    /// where it started.
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// Is this selection still describing the text it was made against?
    ///
    /// **The atom list is rebuilt every frame, and its indices are positions in
    /// a tree that can change under a live drag** — a surface arrives, a
    /// register is switched, a fold opens. An index that pointed at a
    /// paragraph can then point at a heading, and the selection would silently
    /// be of something the reader never dragged over.
    ///
    /// So the anchor's text is checked, not just its index. Cheap (one string
    /// compare against a run that is usually short), and it turns a wrong
    /// selection into no selection, which is the failure a person can see.
    pub fn still_valid(&self, atoms: &[Atom], anchored_text: &str) -> bool {
        let Some(a) = atoms.get(self.anchor.atom) else {
            return false;
        };
        if a.text != anchored_text {
            return false;
        }
        atoms
            .get(self.head.atom)
            .is_some_and(|h| self.head.byte <= h.text.len())
            && self.anchor.byte <= a.text.len()
    }
}

/// Which region a laid-out run fell in.
///
/// Decided on the run's CENTRE rather than its origin. A run whose box starts
/// a pixel above the body's scroll container — a heading sitting flush against
/// the top edge — belongs to the body by any reading a person would give it,
/// and an origin test puts it in `Chrome` and makes it unselectable for
/// reasons nobody can see. The centre is the point the reader is aiming at.
///
/// Later regions win, because they are recorded in tree order and a region
/// drawn later sits on top — the same rule [`hit_at`] uses. No region contains
/// the point at all is [`Region::Chrome`]: looked at, and not selectable.
pub fn region_of(regions: &[(Rect, Region)], x: f32, y: f32, w: f32, h: f32) -> Region {
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    regions
        .iter()
        .rev()
        .find(|(r, _)| cx >= r.x && cx < r.x + r.w && cy >= r.y && cy < r.y + r.h)
        .map(|(_, region)| *region)
        .unwrap_or(Region::Chrome)
}

/// Walk a byte offset back to the nearest character boundary at or before it.
///
/// `index_for_position` answers in bytes against the string it laid out, and a
/// pointer in the middle of a multi-byte character is an ordinary thing for it
/// to be handed. Slicing on that offset panics, so every offset this module
/// produces goes through here first.
pub fn on_boundary(text: &str, mut byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Which run a flat point is in, or the nearest one a reader could have meant.
///
/// A browser does not stop selecting when the pointer leaves the text — it
/// keeps extending to whatever is nearest, which is what makes dragging down
/// the margin work. So a point inside a run answers that run; a point in the
/// gutter answers by distance.
///
/// **The distance is not Euclidean.** A point level with a paragraph but far
/// to its right should take that paragraph, not the short heading three lines
/// up that happens to be nearer as the crow flies. So vertical distance to the
/// run's band dominates, and horizontal distance only settles ties within a
/// band. Getting this wrong is not a crash; it is a selection that jumps a
/// line when the reader drags into the margin, which reads as the feature
/// being flaky.
///
/// `None` when nothing is selectable at all — an empty bench, or a frame that
/// has not painted yet.
pub fn atom_at(atoms: &[Atom], x: f32, y: f32) -> Option<usize> {
    let usable = |a: &&Atom| a.region.selectable() && !a.text.is_empty();
    // Inside one, last wins — later runs paint over earlier ones, the same
    // rule `hit_at` uses and for the same reason.
    if let Some(i) = atoms
        .iter()
        .enumerate()
        .rev()
        .find(|(_, a)| usable(a) && a.contains(x, y))
        .map(|(i, _)| i)
    {
        return Some(i);
    }
    atoms
        .iter()
        .enumerate()
        .filter(|(_, a)| usable(a))
        .min_by(|(_, a), (_, b)| {
            let key = |m: &Atom| {
                // Zero while the point is level with the run, so every run on
                // the reader's line ties here and the horizontal term decides.
                let dy = (y - m.mid()).abs() - m.h / 2.0;
                let dy = dy.max(0.0);
                let dx = if x < m.x {
                    m.x - x
                } else if x > m.x + m.w {
                    x - (m.x + m.w)
                } else {
                    0.0
                };
                (dy, dx)
            };
            let (ay, ax) = key(a);
            let (by, bx) = key(b);
            (ay, ax)
                .partial_cmp(&(by, bx))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(i, _)| i)
}

/// The byte ranges a selection covers, one per run it touches.
///
/// Runs strictly between the two ends are whole; the two ends are partial.
/// Runs that are not selectable are skipped rather than truncating the walk —
/// a chrome run cannot be between two body runs in practice, but a rule that
/// depends on that is a rule waiting to be broken by a layout change.
///
/// Empty ranges are dropped, so a caller can treat a non-empty answer as "there
/// is something to copy".
pub fn spans(atoms: &[Atom], sel: &Sel) -> Vec<(usize, std::ops::Range<usize>)> {
    let (from, to) = sel.ends();
    let mut out = Vec::new();
    for i in from.atom..=to.atom.min(atoms.len().saturating_sub(1)) {
        let Some(a) = atoms.get(i) else { break };
        if !a.region.selectable() {
            continue;
        }
        let start = if i == from.atom {
            on_boundary(&a.text, from.byte)
        } else {
            0
        };
        let end = if i == to.atom {
            on_boundary(&a.text, to.byte)
        } else {
            a.text.len()
        };
        if start < end {
            out.push((i, start..end));
        }
    }
    out
}

/// How far apart two runs have to be, in line heights, before the text between
/// them reads as a new line rather than a continuation.
///
/// Half a line height groups a lane word, a badge and a title into the line
/// they look like. Two line heights is the other end: past that there is
/// visible air, which is a paragraph break rather than the next line.
const SAME_LINE: f32 = 0.5;
const SAME_BLOCK: f32 = 2.0;

/// What a selection puts on the clipboard.
///
/// **A browser gets line breaks for free because it knows which elements are
/// blocks. Nothing here does** — every run is a sibling in a flex tree, and the
/// tree says nothing about whether two of them ended up on one line. So the
/// join is geometric, and it is the part of this feature most able to be wrong
/// without looking wrong: the highlight on screen stays perfect while the text
/// on the clipboard runs together.
///
/// Four rules, in order of how far apart the runs are:
///
/// - same band → one space, because they are one line to the eye
/// - the next band down → a newline
/// - further → a blank line, which is a paragraph
/// - across a region edge → a blank line ALWAYS, whatever the geometry says,
///   because a rail row can sit level with a paragraph in the card and
///   geometry alone would call those one line
pub fn copy_text(atoms: &[Atom], sel: &Sel) -> String {
    let spans = spans(atoms, sel);
    let mut out = String::new();
    let mut prev: Option<&Atom> = None;
    for (i, range) in spans {
        let a = &atoms[i];
        if let Some(p) = prev {
            out.push_str(gap(p, a));
        }
        out.push_str(&a.text[range]);
        prev = Some(a);
    }
    out
}

/// The separator between two runs, by the rules in [`copy_text`].
fn gap(prev: &Atom, next: &Atom) -> &'static str {
    if prev.region != next.region {
        return "\n\n";
    }
    let line = prev.h.max(next.h).max(1.0);
    let drop = (next.mid() - prev.mid()).abs();
    if drop <= line * SAME_LINE {
        " "
    } else if drop <= line * SAME_BLOCK {
        "\n"
    } else {
        "\n\n"
    }
}

/// Undo the tube's barrel warp for one pointer position.
///
/// `rect` is the tube in window pixels, `(px, py)` the pointer in the same
/// space, and the answer is where that pointer would be on the FLAT layout —
/// the coordinates every zone was recorded in. This is the terminal's own
/// inverse, [`crate::pane::warp_screen_to_content`], applied to a point
/// instead of to a cell: the bench is bent by exactly the shader the grid is
/// bent by, so it un-bends by exactly the same map.
///
/// The centre is a fixed point and `k = 0` is the identity, both by the
/// formula rather than by special case, and the tests hold it to that.
pub fn unwarp(rect: (f32, f32, f32, f32), k1: f32, k2: f32, px: f32, py: f32) -> (f32, f32) {
    let (rx, ry, rw, rh) = rect;
    if rw <= 0.0 || rh <= 0.0 {
        return (px, py);
    }
    let (lx, ly) = crate::pane::warp_screen_to_content((px - rx) / rw, (py - ry) / rh, k1, k2);
    (rx + lx * rw, ry + ly * rh)
}

/// Where a key takes the review gallery.
///
/// Its own function because the gallery is MODAL and modal key handling is
/// where surfaces quietly go wrong: a key the overlay does not use must not
/// fall through to the thing underneath, or a left arrow aimed at the gallery
/// walks the caret in a composer nobody can see. Every key is answered here,
/// including the ones whose answer is "nothing".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gallery {
    Back,
    Forward,
    Close,
    /// Used by the gallery and meaning nothing — swallowed, not passed on.
    Ignore,
}

pub fn gallery_key(key: &str) -> Gallery {
    match key {
        "left" | "up" | "h" => Gallery::Back,
        "right" | "down" | "l" | "space" => Gallery::Forward,
        "escape" | "q" | "enter" => Gallery::Close,
        _ => Gallery::Ignore,
    }
}

/// Every answered question on this bench, oldest first.
///
/// Oldest first because a review is a story of how you got here, and the rail
/// — which is newest first, because a rail is about what just happened — reads
/// the other way. The two orders are not a disagreement; they answer different
/// questions.
///
/// An empty list is a real answer: nothing has been answered yet, and the
/// button that opens this is simply not offered.
pub fn reviewed(surfaces: &[crate::surface::Surface]) -> Vec<Reviewed> {
    use crate::surface::{Answered, Kind};
    surfaces
        .iter()
        .filter_map(|s| {
            let Kind::Question(q) = &s.kind else {
                return None;
            };
            let answer = match &q.answer {
                Answered::Waiting => return None,
                Answered::Chose(i) => q
                    .options
                    .get(*i)
                    .map(|o| o.label.clone())
                    // A chosen index with no option at it is a bench and a
                    // transcript that disagree, and saying so beats printing
                    // the number nobody can read.
                    .unwrap_or_else(|| "answered \u{b7} the option is unavailable".into()),
                Answered::Typed(said) => said.clone(),
                Answered::ChoseUnknown => "answered \u{b7} how is unavailable".into(),
            };
            Some(Reviewed {
                title: s.title.clone(),
                answer,
            })
        })
        .collect()
}

/// What to do with the live question we are tracking, given what the screen
/// says right now.
///
/// **A failed parse is not an answered question.** The picker scrolls: on a
/// long multi-select the question line and the first option slide off the top,
/// and the reader that looks for a question above the first numbered row finds
/// nothing and returns [`None`]. Treating that as "the question is over"
/// retired a surface while the agent was still visibly waiting, and the whole
/// card vanished out from under a person mid-click — Parker, after unticking
/// one box: *"unclicking an option makes the entire element disappear"*.
///
/// The evidence that a question is over is the agent no longer waiting. That
/// is the only thing that retires one here. Everything else keeps what we have,
/// which is the same rule as everywhere else on this surface: if we cannot say
/// what changed, change nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiveMove {
    /// Leave the bench exactly as it is.
    Keep,
    /// The agent has stopped waiting; take the live copy down.
    Retire,
    /// A different question is up; take the old one down and put this one up.
    Replace,
}

/// How many consecutive sweeps of "the agent is not waiting" before a live
/// question comes down.
///
/// The screen is a SENSOR, and one sample of it is not a state change. Making
/// the window fullscreen resizes the pseudoterminal, the agent redraws its
/// whole TUI, and for a beat in the middle of that redraw the picker's footer
/// is not on screen — so `needs_input` reads false, and a question nobody had
/// answered was retired on the strength of one blink. Parker, having found the
/// trigger himself: *"it really seems linked to when i hit fullscreen super f
/// ... yes confirmed - it takes about 3 seconds while the waiting on you and
/// question element just disappear"*.
///
/// Three sweeps at one a second, which covers a redraw comfortably. The cost
/// is that a genuinely answered question lingers about two seconds longer than
/// it used to, and that is the right side to be wrong on: a stale card is read
/// and dismissed, a vanished one is a person wondering what they did.
///
/// # The rule this is an instance of
///
/// **Arrive on the first sample, leave only on a settled one.** A screen read
/// is evidence that something IS there; it is never evidence that something is
/// NOT, because the same blank frame is produced by a repaint, a resize, a
/// clear and a scroll. So appearing is immediate and disappearing is
/// debounced, and the asymmetry is the whole design rather than a tuning
/// choice — see [`live_move`], where `Replace` fires at once and `Retire`
/// waits.
///
/// # What the audit of the rest of this window found
///
/// Every other consumer of a screen read was checked after this bug, because
/// one instance of a pattern is usually not one instance. The results, so the
/// next person does not have to repeat the walk:
///
/// - **Destructive, and the only one:** the live question. A wrong reading
///   REMOVED a surface, and nothing brought it back until the agent repainted
///   — which is why this was the instance anybody noticed.
/// - **Self-healing, left alone:** `rail_state` and `needs_input` are both
///   recomputed from the screen every scan, so a repaint makes the tab badge
///   and the rail lane flicker and the next scan puts them right. Visible, not
///   lossy. Debouncing them at the source would delay a question ARRIVING,
///   which is the half that must stay instant.
/// - **Edge-triggered, not polled:** the bell latches on a terminal event and
///   clears on a human one — focus, a keypress, a notification click. No
///   screen reading in either direction.
/// - **Read-only:** `recent_lines` into the dashboard card, `screen_signature`
///   as a repaint comparator, `top_is_human` as a scroll helper. A wrong
///   answer costs one frame or one scroll step and nothing persists.
/// - **Not screen-driven at all:** everything that touches DISK. Surfaces are
///   written by the file transport and the derived half reads the transcript,
///   so a misread screen has never been able to delete a file — the bench is
///   in memory, and that is why this bug erased a card rather than a record.
pub const SETTLE_SWEEPS: u8 = 3;

/// Decide it. `waiting` is whether the agent is still stopped on a person,
/// `parsed` what the screen could be read as this instant, `tracked` the live
/// question already on the bench, and `quiet_for` how many sweeps in a row
/// the agent has looked like it is no longer waiting.
pub fn live_move(
    waiting: bool,
    parsed: Option<&crate::surface::SurfaceId>,
    tracked: Option<&crate::surface::SurfaceId>,
    quiet_for: u8,
) -> LiveMove {
    match (waiting, parsed, tracked) {
        // Nothing up, nothing tracked.
        (_, None, None) => LiveMove::Keep,
        // Something to show and nothing showing it.
        (true, Some(_), None) => LiveMove::Replace,
        (false, Some(_), None) => LiveMove::Keep,
        // The agent looks like it has moved on — but only once it has looked
        // that way for long enough to be believed. This is the ONLY
        // retirement, and it is now the only DEBOUNCED one.
        (false, _, Some(_)) => {
            if quiet_for >= SETTLE_SWEEPS {
                LiveMove::Retire
            } else {
                LiveMove::Keep
            }
        }
        // Still waiting, and the screen cannot be read: keep what we have.
        (true, None, Some(_)) => LiveMove::Keep,
        (true, Some(now), Some(was)) => {
            if now == was {
                LiveMove::Keep
            } else {
                LiveMove::Replace
            }
        }
    }
}

/// Where an option sits in the picker's own up/down order.
///
/// Not the same number as its position in the options list, and the gap is a
/// real defect rather than a tidiness point. The picker draws its Submit
/// button BETWEEN the last real option and the trailing `Chat about this`, so
/// on a five-option multi-select the order is 1,2,3,4,5,Submit,6 — and
/// pressing down five times from the top lands on Submit, not on option six.
/// Answering by option index would have committed the wrong thing on every
/// multi-select that carries a trailing option, silently, in a menu the person
/// cannot see while the bench is up.
pub fn nav_index(option: usize, submit_at: Option<usize>) -> usize {
    match submit_at {
        Some(at) if option >= at => option + 1,
        _ => option,
    }
}

/// One edit a key can ask of a line.
///
/// A table rather than a match arm per key in the handler, because the handler
/// is not a place an assertion can reach and these are CONVENTIONS — the point
/// of them is that a person already knows them, so getting one wrong is worse
/// than not having it. Parker: *"ctrl left arrow ctrl A --- we have a bunch of
/// text editor rules that are CONVENTIONS THAT WE MUST SUPPORT in our text
/// entry"*.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edit {
    Backspace,
    Delete,
    /// ctrl+w and ctrl+backspace: take the word behind the caret.
    KillWordLeft,
    /// alt+d and ctrl+delete: take the word in front of it.
    KillWordRight,
    /// ctrl+u: everything from the caret back to the start.
    KillToStart,
    /// ctrl+k: everything from the caret to the end.
    KillToEnd,
    /// ctrl+a: the whole draft, selected. See [`Line::mark_all`].
    SelectAll,
    /// shift+enter, alt+enter: a line break IN the draft. It used to be a
    /// literal newline typed at the agent while the mirror read it as submit
    /// and emptied itself — Parker: *"ouch - pain"* (#614).
    Newline,
    /// A motion with nothing held: move the caret, drop any selection.
    Move(Motion),
    /// The SAME motion with shift held: move the caret, and grow the
    /// selection behind it.
    ///
    /// A pair of variants over one [`Motion`] rather than sixteen flat ones.
    /// A motion added to that enum cannot then ship with a bare form and no
    /// shift-held one, because there is nowhere to put the omission — which
    /// is exactly how shift+arrow came to be missing: it needed eight new
    /// arms and got none.
    Extend(Motion),
    /// ctrl+z: the last change, taken back.
    Undo,
    /// Sent, and the line starts again.
    Submit,
}

/// Where a motion puts the caret, independent of whether shift was held.
///
/// Split out of [`Edit`] so the bare arrow and the shift-held arrow are the
/// same movement by construction. A selection that could extend somewhere a
/// plain arrow would not have gone is a second word rule, a second wrap rule
/// and a second off-by-one, all of which this file has paid for before.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
    Up,
    Down,
}

/// Which edit a keystroke asks for, if any.
///
/// Readline's bindings, because that is what is on the far end: a person who
/// types `ctrl+w` expects a word to go, and the alt-prefixed pair is here too,
/// since a terminal that sends meta rather than control is the ordinary case on
/// this desk.
///
/// **`ctrl+a` is the one place this box is NOT readline**, reversed on
/// 2026-09-18. It was start-of-line, on the reasoning that the far end reads it
/// that way; the box is a text area on a screen and every text area a person
/// uses answers that chord with select-all. Parker: *"Ctrl+a does not highlight
/// all in the workbench text area"*. Start-of-line is still on `home` and on
/// `ctrl+b`-style motion, and the far end still receives the same `\x01` byte —
/// which puts ITS caret at column zero, exactly where the replacement wants it.
/// See [`replace_bytes`] for the other half of the trick.
///
/// [`None`] means this is not an edit — a printable character, or a chord that
/// belongs to somebody else — and the caller passes it through untouched.
/// What the far end has to be told when a SELECTED draft is being replaced.
///
/// `ctrl+a` already travelled, so the agent's own editor is sitting at column
/// zero with the whole line still in front of it. One `ctrl+k` takes the rest,
/// and whatever the person typed follows it as an ordinary keystroke. Nothing
/// here guesses at the far end's contents — it kills from a caret we know the
/// position of, which is the only reason this is safe on a line the mirror may
/// have drifted from.
pub fn replace_bytes() -> Vec<u8> {
    vec![0x0b]
}

/// A line typed at the agent BESIDE an unsent draft, which is put back after.
///
/// The composer is a mirror: every keystroke that built the draft has already
/// gone down the pseudoterminal, so the agent's own line editor is holding that
/// draft right now. Anything the bench needs to say on its own account — a
/// dial's `/model`, a `/effort` — therefore cannot simply be typed, because it
/// would land on the END of what the person is writing and the return key would
/// send both as one prompt. Which is exactly what happened: the dial wrote its
/// command into the composer, sent it, and the person's half-finished prompt
/// went to the agent glued to a slash command and answered at the OLD strength.
/// Parker: *"if I enter a prompt into the workbench prompt area and THEN click
/// to change model or effort, THE PROMPT DISAPPEARS — it sent the prompt at the
/// PREVIOUS effort/model. Intended: the prompt PERSISTS, changing the effort
/// SNEAKS behind the prompt."*
///
/// So it sneaks: take the far end's caret to column zero and kill forward — the
/// same pair the composer already uses to replace a selected draft, and the
/// only erase on this side that never guesses at the far end's contents — send
/// the command, type the draft back, and leave the caret where the person had
/// it. One write, in order, so nothing can arrive between the parts.
///
/// **What does not survive is a pasted image.** `[Image #7]` is the agent's own
/// reference to something it read off the clipboard; the erase takes it and no
/// retyping brings it back. The caller answers for that by clearing the mirror's
/// count (see [`Line::forget_pastes`]) rather than leaving the box claiming an
/// attachment the agent no longer holds.
///
/// **It is now TWO writes, not one, and the second one waits.** Typing
/// `/effort max` does not change the effort — the harness answers it with a
/// modal picker of its own, *"Change effort level?"*, and until somebody
/// presses Yes nothing has happened. Everything written after the command
/// therefore lands in that picker rather than in a line editor, which is
/// where the retyped draft was going: into a menu, where its digits pick
/// options. Parker: *"all that happens is the /effort <value> is pre-pended
/// to the prompt and not PUSHED THROUGH and CONFIRMED... the prompt in
/// progress should be saved, deleted, then the effort value pushed through to
/// the prompt, confirmed, and then the user's prompt pasted back in, all
/// seamlessly"*. So this half erases and commands; [`restore_bytes`] is the
/// other half, and [`dial_step`] decides when it is safe to send.
pub fn dial_bytes(command: &str, draft: &Line) -> Vec<u8> {
    let mut out = Vec::new();
    let text = draft.text();
    if text.chars().count() > 0 {
        out.extend(caret_move(draft.caret(), 0));
        out.extend(replace_bytes());
    }
    out.extend(typed_line(command));
    out
}

/// The draft, typed back where it was, with the caret where it was.
///
/// Types the text the person had at the moment of the press — NOT the text
/// the mirror is holding now. Anything they typed while the harness was being
/// answered was held in the bench's own queue (see `bench_may_write`) and is
/// drained straight after this, so replaying the original and then the held
/// keystrokes reproduces exactly what the far end would have had if the dial
/// had never been touched. Restoring the CURRENT text instead would apply
/// every one of those edits twice.
pub fn restore_bytes(text: &str, caret: usize) -> Vec<u8> {
    let end = text.chars().count();
    if end == 0 {
        return Vec::new();
    }
    let mut out = text.as_bytes().to_vec();
    out.extend(caret_move(end, caret.min(end)));
    out
}

/// A dial press that has been typed and is waiting on the harness.
///
/// The draft is carried here rather than read back off the composer because
/// the composer keeps moving: the person goes on typing into a box whose
/// keystrokes are being held, and the text that has to be typed back is the
/// one that was erased.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DialSent {
    /// Which dial was pressed — it decides which confirmation is OURS.
    pub which: Dial,
    /// The draft at the moment of the press.
    pub text: String,
    /// Where their caret was in it.
    pub caret: usize,
    /// When the command went out.
    pub sent_ms: u64,
    /// Has the window already pressed Yes on the harness's picker?
    pub answered: bool,
}

/// What to do about a dial press that is in flight.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialStep {
    /// Nothing yet — the harness has not put its question up.
    Wait,
    /// Its picker is up: walk from `from` to `to` and press return.
    Answer { to: usize, from: usize },
    /// Over. Type the draft back and let the held keystrokes go.
    Settle,
}

/// How long to wait for the harness to ask before deciding it never will.
///
/// Generous against a redraw, short against a person: the dials are pressable
/// only from a state where the harness is at its prompt ([`dials_live`]), so
/// the picker — when there is one — is drawn on the next frame, and the only
/// reason to wait a second and a half is that a frame can be slow. A harness
/// that simply applies the command (Codex, or an effort level the harness
/// does not think is worth asking about) never asks at all, and the draft
/// must not be held hostage to a question nobody is going to pose.
pub const DIAL_ASK_MS: u64 = 1500;

/// The dial's state machine, over what is on the bottom of the screen.
///
/// Pure, so its cases can be asserted without a terminal — and they are the
/// cases that decide whether a person ever sees their sentence again.
///
/// **A MENU ON SCREEN IS THE STOP CONDITION, not a clock.** There is
/// deliberately no give-up while [`crate::screenread::Picker`] says something
/// is up: typing a draft into a picker is the harm this whole mechanism
/// exists to avoid, and a keystroke that lands in one cannot be taken back.
/// Holding, by contrast, costs nothing that is not already lost — the far end
/// is a modal picker and will not read a sentence from anybody until it is
/// answered. So the draft waits, visible in the composer the whole time, and
/// goes in the moment the screen is a line editor again. The person answering
/// the question themselves ends it exactly as our own keypress does.
pub fn dial_step(sent: &DialSent, picker: crate::screenread::Picker, now_ms: u64) -> DialStep {
    use crate::screenread::Picker;
    match (picker, sent.answered) {
        // Its question is up and nobody has answered it. This is the press
        // the person already made, arriving where the harness can hear it.
        (Picker::Confirm { yes, cursor }, false) => DialStep::Answer {
            to: yes,
            from: cursor,
        },
        // Anything still up — ours after we pressed, or somebody else's — is
        // a screen with no line editor on it. Wait.
        (Picker::Confirm { .. } | Picker::Other, _) => DialStep::Wait,
        // Answered and gone. Done.
        (Picker::None, true) => DialStep::Settle,
        // Never asked. Either the harness took the command outright or it is
        // never going to ask, and both end the same way.
        (Picker::None, false) => {
            if now_ms.saturating_sub(sent.sent_ms) >= DIAL_ASK_MS {
                DialStep::Settle
            } else {
                DialStep::Wait
            }
        }
    }
}

/// The value of a flag in a launch or resume command, if it carries one.
///
/// Both spellings, because both are typed: `--model opus` and `--model=opus`.
/// A flag with no value after it answers [`None`] rather than swallowing the
/// next flag — `claude --resume --model` is a broken command line, and reading
/// `--model` as the model would put the word "--model" on a button.
pub fn flag_value(cmd: &str, flag: &str) -> Option<String> {
    let eq = format!("{flag}=");
    let mut words = cmd.split_whitespace().peekable();
    while let Some(w) = words.next() {
        if let Some(v) = w.strip_prefix(eq.as_str()) {
            return (!v.is_empty()).then(|| v.to_string());
        }
        if w == flag {
            return words
                .peek()
                .copied()
                .filter(|v| !v.starts_with('-'))
                .map(str::to_string);
        }
    }
    None
}

/// Does this keystroke ask the composer for HISTORY rather than for an edit?
///
/// Up and down mean two things in a box that has both rows and a history, and
/// which one they mean is decided here rather than in the handler, so it can
/// be asserted. `recalling` is the caller's: a draft that is empty, or a
/// recall already in flight.
///
/// **Shift is excluded.** `shift+up` is *select upward* in every text box and
/// it has to stay that, even mid-recall — otherwise extending a selection
/// hands back last week's prompt.
pub fn recalls_history(key: &str, ctrl: bool, alt: bool, shift: bool, recalling: bool) -> bool {
    !ctrl && !alt && !shift && recalling && matches!(key, "up" | "down")
}

/// Which motion a key asks for, before shift is considered.
///
/// Split out so every motion gets its shift-held form for free in
/// [`line_edit`]. Adding an arrow here gives it a selection at the same
/// moment it gets a move, which is the invariant the flat table could not
/// hold.
fn motion_for(key: &str, ctrl: bool, alt: bool) -> Option<Motion> {
    Some(match key {
        "left" if ctrl || alt => Motion::WordLeft,
        "right" if ctrl || alt => Motion::WordRight,
        "left" => Motion::Left,
        "right" => Motion::Right,
        "up" if !ctrl && !alt => Motion::Up,
        "down" if !ctrl && !alt => Motion::Down,
        "home" => Motion::Home,
        "end" => Motion::End,
        // The readline letters, which are motions too and have always been
        // in this table. They take shift the same way.
        "e" if ctrl => Motion::End,
        "b" if ctrl => Motion::Left,
        "f" if ctrl => Motion::Right,
        _ => return None,
    })
}

pub fn line_edit(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<Edit> {
    // A MOTION FIRST, then shift decides what it does to the selection.
    // Written this way rather than as eight more arms because eight arms is
    // what nobody wrote, and the box went a month with no way to highlight
    // anything (#625).
    if let Some(m) = motion_for(key, ctrl, alt) {
        return Some(if shift {
            Edit::Extend(m)
        } else {
            Edit::Move(m)
        });
    }
    Some(match key {
        "backspace" if ctrl || alt => Edit::KillWordLeft,
        "backspace" => Edit::Backspace,
        "delete" if ctrl || alt => Edit::KillWordRight,
        "delete" => Edit::Delete,
        // The table takes SHIFT now, because the one key whose meaning shift
        // changes is the one that used to empty the box: `shift+enter` is a
        // line, `enter` alone is the send.
        "enter" if shift || alt => Edit::Newline,
        "enter" => Edit::Submit,
        "z" if ctrl => Edit::Undo,
        "a" if ctrl => Edit::SelectAll,
        // ctrl+e, ctrl+b and ctrl+f are motions and are answered by
        // `motion_for` above, so they get shift for free like the arrows.
        "w" if ctrl => Edit::KillWordLeft,
        "d" if alt => Edit::KillWordRight,
        "u" if ctrl => Edit::KillToStart,
        "k" if ctrl => Edit::KillToEnd,
        _ => return None,
    })
}

/// The bytes that move an agent's own line editor from one column to another.
///
/// The column itself is no longer computed here. It used to be — `x` over a
/// measured advance — and that function was deleted rather than fixed, because
/// every version of it was a guess about how the text system would lay a
/// string out, and a wrapped line has no single column at all. The composer
/// asks `gpui::TextLayout::index_for_position` instead, which is the text
/// system answering about the text it actually drew.
///
/// Arrow keys, one per column, because that is the only movement every line
/// editor on the far end agrees on. `home`/`end` would be fewer bytes and are
/// not portable: readline, Claude's editor and a raw shell all answer
/// differently to the several escape sequences that claim to mean "start of
/// line", and a wrong guess moves the caret somewhere nobody asked for.
pub fn caret_move(from: usize, to: usize) -> Vec<u8> {
    let (seq, n) = if to > from {
        (b"\x1b[C", to - from)
    } else {
        (b"\x1b[D", from - to)
    };
    seq.repeat(n)
}

/// Is this keystroke a paste?
///
/// All three chords a person might use, because they arrive from three
/// different habits and getting one wrong means an image silently does not
/// paste: `ctrl+v` from every graphical app, `ctrl+shift+v` from every
/// terminal, and `shift+insert` from X11 and from anyone who learned this
/// before either. The key is matched case-insensitively — a keyboard with
/// shift held reports `V` on some platforms and `v` on others, and that
/// difference has nothing to do with what the person meant.
pub fn is_paste_chord(key: &str, ctrl: bool, shift: bool) -> bool {
    if key.eq_ignore_ascii_case("v") && ctrl {
        return true;
    }
    key.eq_ignore_ascii_case("insert") && shift
}

/// The image type to ask a clipboard for, out of everything it is offering.
///
/// Wayland clipboards are a LIST of types, and a copied image usually arrives
/// alongside a text one — a browser offers `text/html` next to its
/// `image/png`, and whoever reads the text first gets markup instead of the
/// picture. gpui's own clipboard read does exactly that, so a paste that a
/// person means as an image comes back as a paragraph of HTML. Asking the
/// list directly is how that is caught, and it is a list rather than a single
/// guess because `image/png` is not the only answer.
///
/// Ordered by what an agent can actually do with the file, not by fidelity:
/// PNG first because every model reads it, SVG last because it is really text
/// and only some do. Returns [`None`] when nothing on offer is an image —
/// which is a fact, not a failure, and the caller then pastes the text.
pub fn best_image_mime<S: AsRef<str>>(offered: &[S]) -> Option<&'static str> {
    const ORDER: [&str; 7] = [
        "image/png",
        "image/jpeg",
        "image/webp",
        "image/gif",
        "image/bmp",
        "image/tiff",
        "image/svg+xml",
    ];
    ORDER.into_iter().find(|want| {
        offered
            .iter()
            .any(|o| o.as_ref().trim().eq_ignore_ascii_case(want))
    })
}

/// The file extension to save an image mime type under.
///
/// The extension is not decoration: it is the whole reason the path works.
/// An agent handed `/tmp/x` opens bytes; handed `/tmp/x.png` it knows to look
/// at a picture, and so does every viewer the desktop would launch.
pub fn ext_of_image_mime(mime: &str) -> Option<&'static str> {
    Some(match mime.trim().to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        _ => return None,
    })
}

/// The text a set of dropped — or pasted — file paths becomes.
///
/// A path is typed rather than the file being read, on the same terms as the
/// clipboard's image paste: a pseudoterminal carries bytes, and every agent
/// worth dropping a file into already opens a filename.
///
/// **One path is one word.** A path is quoted the moment it holds whitespace
/// or anything a shell would act on, because the far end is a line editor: an
/// unquoted `Screenshot 2026-09-18.png` arrives as two arguments and nothing
/// downstream can put it back together. Quoting is single-quote and an
/// embedded quote closes, escapes and reopens (`'\''`), which is the one form
/// every POSIX shell agrees on and which an agent reading the line also
/// understands.
///
/// A newline inside a filename is legal and would SUBMIT the line half-typed,
/// so control characters become spaces — the same trade
/// [`crate::pane::TerminalView::bench_paste`] already makes for pasted text.
/// The path is then wrong, and it was unusable either way; what it no longer
/// does is send half a sentence to the agent.
pub fn paths_as_words(paths: &[std::path::PathBuf]) -> String {
    paths
        .iter()
        .map(|p| path_word(&p.to_string_lossy()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// One path, as one word on the far end's line.
fn path_word(raw: &str) -> String {
    let flat: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    // Bare when nothing in it can be misread. The list is what a shell acts
    // on plus the quotes themselves; `-` and `.` and `/` are left alone, so
    // an ordinary path still reads as an ordinary path.
    let plain = !flat.is_empty()
        && !flat
            .chars()
            .any(|c| c.is_whitespace() || "\"'\\$`&|;<>()[]{}*?!#~^".contains(c));
    if plain {
        return flat;
    }
    format!("'{}'", flat.replace('\'', r"'\''"))
}

/// What a bench of this size, on a pane of this kind, actually shows.
///
/// Every one of these was a condition written inline in the render, and each
/// one cost a round trip with a photograph to find: the rail took a third of a
/// pane, the composer vanished below a size threshold, the hint line stayed
/// when there was no room for it. A render is not a testable position — you
/// cannot assert a screenshot — so the decisions moved out here where a table
/// of sizes can hold them still.
///
/// The render's job is now to draw this, not to decide it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shows {
    pub rail: RailFit,
    pub how: Embodiment,
    /// The line into the agent. Present on every agent pane at every size —
    /// see [`crate::benchdraw::composer`] for why this is not negotiable.
    pub composer: bool,
    /// The composer at its small size: no hint line, tighter padding.
    pub tight: bool,
    /// One line naming what the keys do, under an unarmed composer.
    pub hint: bool,
    /// Draw the agent's own scrollback into the bench's main area.
    ///
    /// **Off.** The mirror was scaffolding: it proved the bench was attached
    /// to a real terminal at a point when that was in doubt, and every round of
    /// feedback since has been about the surfaces beside it. Parker: *"we are
    /// ready to HIDE the terminal mirror scroll ... I feel like the machine
    /// might need this to EXIST, but the user should not see it after today"*.
    ///
    /// He is right that it still has to exist, and it does — but not here. The
    /// machine reads the SCREEN, not this rendering of it: `live_rows` feeds
    /// the question reader, `recent_lines` feeds the dashboard card, and both
    /// run whether or not a single pixel of scrollback is drawn on the bench.
    /// Nothing was disconnected to turn this off, which is why it is a flag
    /// rather than a deletion: `TD_BENCHMIRROR=1` puts it back for anybody
    /// debugging what the reader is seeing.
    pub mirror: bool,
    /// How wide the composer's text area is, for choosing a type size.
    pub composer_w: f32,
    /// How tall the composer may grow before it starts scrolling instead.
    ///
    /// A share of the pane rather than a constant: the composer has to hold a
    /// long message without clipping it, AND it must not grow until it has
    /// eaten the conversation it is a reply to. A third is where those two
    /// stop arguing.
    pub composer_max: f32,
}

/// Resolve what a pane of this size shows.
///
/// `armed` is whether the composer already holds a line, because the hint is
/// an invitation and an invitation to something already accepted is clutter.
pub fn shows(pane_w: f32, pane_h: f32, is_agent: bool, rail_wanted: bool, armed: bool) -> Shows {
    let rail = rail_fit(pane_w, rail_wanted);
    let rail_px = match rail {
        RailFit::Open(w) => w as f32,
        RailFit::Ticks => RAIL_TICK_W,
        RailFit::Hidden => 0.0,
    };
    let how = embodiment(pane_w - rail_px, pane_h);
    let tight = how == Embodiment::Summary;
    Shows {
        rail,
        how,
        composer: is_agent,
        tight,
        hint: is_agent && !tight && !armed,
        mirror: is_agent && std::env::var_os("TD_BENCHMIRROR").is_some(),
        composer_max: (pane_h * COMPOSER_SHARE).max(COMPOSER_MIN_MAX),
        // The pane, less the rail beside it and the composer's own padding
        // and chrome. An estimate, and only ever used to pick a SIZE.
        composer_w: (pane_w - rail_px - 90.0).max(80.0),
    }
}

/// Which end of the bench's body its content is anchored to.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anchor {
    /// Reads downward from the top. A card.
    Top,
    /// Reads upward from the floor. A conversation.
    Bottom,
    /// Centred in the upper four fifths of the box. An offer.
    ///
    /// Not a third alignment for its own sake. `Top` means *against the
    /// ceiling*, and an offer pinned there sits in the pane's top inch with the
    /// whole surface empty beneath it — the mirror image of the floor problem
    /// [`body_anchor`] was written to fix, and the same complaint in the other
    /// direction. Parker: *"move down to top 80% of vertical space (not smashed
    /// into the top)"*.
    ///
    /// Four fifths rather than the full height, so it lands ABOVE centre: an
    /// offer is still read downward from, and dead-centring would leave as much
    /// nothing above it as below.
    Eye,
}

/// Where the body's content sits in a box taller than it is.
///
/// Three things share that box and they do not read the same way.
///
/// A **conversation** reads from the BOTTOM — newest last, just above the
/// composer you answer it in — and top-aligning it once left the transcript
/// floating in a field of empty with the input stranded at the far edge.
///
/// A **card** reads from the TOP, and pinning one to the floor of a 900px pane
/// put an opened table under an acre of nothing.
///
/// An **offer** reads from the top too, for a stronger reason than typography:
/// it is the only thing on the surface and it is a thing to press. Bottom-
/// anchored it inherited the transcript's alignment and went to the floor —
/// Parker, on the build that did it: *"SPINNING up a new agent in workbench —
/// the ACTIon for this is WAAAAAAY at the bottome of the screen"*. A call to
/// action is not history, and nothing about it belongs at the bottom of a tall
/// pane.
///
/// But it does not belong against the ceiling either, which is where "not the
/// floor" first put it. An offer gets [`Anchor::Eye`] — centred in the upper
/// four fifths — and the card keeps [`Anchor::Top`], because a card that may be
/// taller than the box has to start at the top or its own head goes off-screen.
/// An offer never can: it is three short lines by construction.
///
/// A card OVER an offer is a card, and anchors like one.
///
/// `card` is whether a card is IN the body — not whether a person opened one.
/// Those were the same question until the overview learned to stand the newest
/// reply in the room without anybody opening it, and then they were not: the
/// stand-in is a card, reads from the top like a card, and was being bottom-
/// anchored because nothing had selected it. Bottom-anchoring is for the
/// conversation and for nothing else, and it matters twice over now that the
/// body scrolls — a `justify_end` scroll container cannot be scrolled to its
/// own top, which is the trap the composer already carries a paragraph about.
///
/// The two branches arrived at this function from opposite ends and the merge
/// keeps both: `card` is the branch's widened question, and an offer still gets
/// `Eye` rather than `Top`. Folding them the branch's way — `card || offering`
/// — would have left the `else if offering` arm below unreachable, which is the
/// same defect in the other direction: an arm nobody can reach says the offer
/// was never given a home.
pub fn body_anchor(card: bool, offering: bool) -> Anchor {
    if card {
        Anchor::Top
    } else if offering {
        Anchor::Eye
    } else {
        Anchor::Bottom
    }
}

/// The person's message cut to what the block will draw, SAYING when it cut.
///
/// `lines` is read one line longer than the block has room for, so this can
/// tell a message that ended from a message that ran on. A paragraph stopped
/// mid-word with nothing marking the stop reads as the person having typed
/// exactly that much, which is the same lie a silently clipped title tells —
/// the ellipsis is the bench's own mark and the rest of it already uses one.
pub fn ask_clipped(mut lines: Vec<String>, keep: usize) -> Vec<String> {
    if lines.len() <= keep {
        return lines;
    }
    lines.truncate(keep);
    if let Some(last) = lines.last_mut() {
        last.push('\u{2026}');
    }
    lines
}

/// How many lines of THE PERSON'S OWN MESSAGE the main area draws above the
/// reply — `None` when it draws none.
///
/// The overview is the feed of what the agent said, and for a while that was
/// all it was: the newest reply stood in the room with nothing above it, so
/// the one thing a person could not read on that surface was the thing they
/// had themselves just asked. It was legible in the pane's mirrored
/// conversation and on the agent wall's card, which are two places that are
/// not the one being looked at. Parker: *"I DON'T SEE THE HUMAN MESSAGE IN
/// THE FULL SCREEN VIEW OF OVERVIEW!!! The human message must be separate
/// from the agent outputs! AND visible in the Main view of overview, not the
/// right spine summary"*.
///
/// Four things decide it, and each rules the block out on its own:
///
/// - the OVERVIEW. It is the shelf that holds a conversation; a document
///   opened off the artifacts shelf is not an answer to anything anybody
///   said, and captioning it with a question would invent a relationship.
/// - the STAND-IN, never a card the person opened from the rail. The only
///   message this window can read out of a pane is the LATEST one, so putting
///   it over a reply from four turns ago would caption an old answer with a
///   new question — [`Bench::standing_in`] is that distinction.
/// - an AGENT. A shell pane's `>` is a prompt, not a message, and the bench
///   would be labelling somebody's last `cd` as a thing they said.
/// - ROOM. `Summary` is a pane too small to read a paragraph in, and the
///   reply is what that pane is for.
///
/// Two rungs rather than one number: a full pane can hold the opening of a
/// long ask, a compact one gets the first line and its wrap. Both are counts
/// of CONTINUATION lines — the message's first line always comes.
pub fn ask_lines(shelf: Shelf, stand_in: bool, agent: bool, how: Embodiment) -> Option<usize> {
    if shelf != Shelf::Overview || !stand_in || !agent {
        return None;
    }
    match how {
        Embodiment::Full => Some(4),
        Embodiment::Compact => Some(2),
        Embodiment::Summary => None,
    }
}

/// WHAT A PROMPT RECORD SAYS ABOUT THE TURN IT OPENED — the headline for the
/// rail's row, and whose voice it was.
///
/// Pure, so the part with judgement in it is tested rather than asserted about
/// by a grep over the call site. Three cases, and the third is the one that
/// matters: the harness can hand the hook a prompt with no text at all, and a
/// turn certainly began. `(None, None)` is that — an unknown headline and an
/// unclaimed voice — which the row draws as *nothing recorded what opened it*
/// and which [`Bench::turn_began`] refuses to move an opened card for.
pub fn turn_opening(effect: &crate::channel::Effect) -> (Option<String>, Option<Voice>) {
    match effect {
        // The FIRST LINE of what they said. The rail has one line to say what
        // a turn is about, and the opening of a message is what a person
        // recognises their own turn by.
        crate::channel::Effect::Asked { text } => {
            (text.lines().next().map(str::to_string), Some(Voice::Person))
        }
        crate::channel::Effect::Woken(w) => (
            Some(crate::benchdraw::woken_says(w).0),
            Some(Voice::Harness),
        ),
        _ => (None, None),
    }
}

/// WHAT THE LIVE CARD SAYS: its lane word, and the sentence under the action.
///
/// Split out of the renderer so the words can be read by a test, for the same
/// reason [`crate::benchdraw::woken_says`] was — a `Div` cannot be asked what
/// text is inside it, and the thing that can be wrong here is what a person
/// ends up reading.
///
/// **The card outlives the turn, and this is where it stops pretending.**
/// [`Bench::turn_settled`] is called by a reply ARRIVING, never by the agent
/// going idle, so a turn that ended having presented nothing keeps its place
/// at the head of the feed. Every row below `Working` is that case: the honest
/// sentence, rather than a card still claiming to be in flight or — worse —
/// the previous turn's answer sliding back into the room.
///
/// # The at-rest rows say what HAPPENED, never what will not
///
/// This shipped as `NOTHING PRESENTED · this turn ended without presenting a
/// reply`, and that sentence is a claim about the FUTURE made at the one moment
/// it cannot be checked. `Done` is raised off the bell, which the pane's own
/// 120ms clock sets on the working→idle edge; the reply comes from the
/// harness's stop hook, on its own schedule. The two race on EVERY turn, so
/// every turn had a window — short, and there on all of them — where the card
/// confidently announced that nothing was coming while the reply was in flight.
///
/// That is this feature's own disease: a surface stating something it has not
/// got, in the gap before it has it. The repair is not a grace period, which
/// would be a guess wearing a number. It is to say the two things that ARE
/// observable — the turn ended, and nothing has landed — and to leave the
/// question of whether anything ever will to the only thing that can answer it,
/// which is a reply arriving or not.
///
/// `Exited` keeps its finality, and earns it: the process is gone, so *nothing
/// further is coming* is a fact about the present rather than a prediction.
pub fn live_says(state: AgentState) -> (&'static str, &'static str) {
    match state {
        AgentState::Working => ("IN FLIGHT", "the reply lands here when the turn ends"),
        AgentState::Reading => ("IN FLIGHT", "it is reading what the bench just typed"),
        AgentState::Paused => ("PAUSED", "you stopped this turn; it has not started again"),
        AgentState::Asking => ("WAITING ON YOU", "it asked something before it could go on"),
        AgentState::Blocked => ("BLOCKED", "the turn stopped on something that went wrong"),
        AgentState::Done | AgentState::Idle => {
            ("THE TURN ENDED", "no reply has landed on the bench")
        }
        AgentState::Exited => ("AGENT GONE", "the agent left before a reply landed"),
    }
}

/// Whether the strip's dials can be pressed in this state.
///
/// A dial press types a slash command, and a slash command typed mid-turn does
/// not take effect mid-turn: it sits in the harness's own line editor and fires
/// whenever the turn happens to end. So the change would land at a moment
/// nobody chose, on a turn nobody meant it for — which is worse than a control
/// that plainly cannot be pressed yet.
///
/// `Reading` is grey for a sharper reason than `Working`: the bench has just
/// written into that terminal and the agent has not moved, so a second write is
/// two strings interleaving in one line editor.
///
/// `Exited` is grey because there is nothing there to tell.
///
/// `Paused` is live, and it is the state this whole rule was getting in the
/// way of. The paragraph above says a dial press mid-turn lands at a moment
/// nobody chose — so a person who notices the wrong model mid-turn had, until
/// now, no move at all except ending the session. Pausing is how they choose
/// the moment: the harness is back at its prompt, the slash command lands
/// there, and the turn goes again on the model they meant. Parker: *"we failed
/// to set the effort or model correctly and need to stop the turn without
/// stopping the agent running entirely"*.
pub fn dials_live(state: AgentState) -> bool {
    match state {
        AgentState::Idle
        | AgentState::Done
        | AgentState::Asking
        | AgentState::Blocked
        | AgentState::Paused => true,
        AgentState::Working | AgentState::Reading | AgentState::Exited => false,
    }
}

/// What the strip offers to do to the TURN — as opposed to the session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TurnControl {
    /// Stop the running turn, leaving the agent where it is.
    Pause,
    /// Tell a stopped turn to carry on.
    Resume,
}

/// The message the resume sends. Parker: *"resume turn... just sends the
/// message to the agent --- proceed with turn (or whatever you think works
/// best for this)"*.
///
/// A sentence rather than a bare word, because it is going into a transcript
/// somebody reads later, where it lands directly under the harness's own
/// `[Request interrupted by user]`. "Continue" would also have been ambiguous
/// with the slash command of the same name, which is the one reading that
/// would do something entirely different.
pub const RESUME_SAY: &str = "Proceed with the turn.";

/// Pause, resume, or neither — the whole control, decided here.
///
/// **Neither is the common answer, and that is the requirement rather than a
/// fallback.** Parker: *"this display will be TOTALLY HIDDEN unless a turn is
/// running... a COMPLETED turn should NOT allow a user to 'resume turn' ...
/// ONLY a PAUSED turn should allow this!!! and then WHILE a turn is paused the
/// pause button it TOTALLY HIDDEN"*. So the two are never both drawn, and on
/// five of the eight states neither is — a strip that offered RESUME beside a
/// finished turn would be offering to restart something that ended on purpose.
///
/// `Reading` gets the pause with `Working`: from the person's side the turn
/// they just sent is running, and that eight-second window is precisely when
/// somebody notices they sent it on the wrong model. The cost of being early
/// is one interrupt arriving at a prompt, which clears a line; the cost of
/// being late is the turn.
///
/// `Asking` and `Blocked` get neither, and they lose nothing by it: they are
/// already stopped, their dials are already live, and there is no turn in
/// flight to take back.
///
/// `agent_present` is the same question [`strip_verb`] asks — a pane whose
/// agent has gone reads as an agent pane for a while yet, and there is no turn
/// in an empty room.
pub fn turn_control(state: AgentState, agent_present: bool) -> Option<TurnControl> {
    if !agent_present {
        return None;
    }
    match state {
        AgentState::Working | AgentState::Reading => Some(TurnControl::Pause),
        AgentState::Paused => Some(TurnControl::Resume),
        AgentState::Asking
        | AgentState::Blocked
        | AgentState::Done
        | AgentState::Exited
        | AgentState::Idle => None,
    }
}

/// Does the question pinned above the composer get drawn, given what is
/// already open in the body?
///
/// One question, one place. The waiting block exists so that being asked
/// something is never scrolled away from — it sits below the body and outside
/// its scroll for that reason — and when the card in the body IS that same
/// question, the pin has nothing left to protect: the question, its options
/// and its chips are all already on screen, six hundred pixels up, in a card
/// that additionally carries the verbs. Parker, at two copies of one picker:
/// *"Decisions tab repeats question display --- this SHOULD NOT be double
/// printed!"*
///
/// It is the same failure this file has fixed twice before at smaller scale —
/// `kind · title` above a heading that said `kind · title`, and a question
/// asked three times inside one card. The shape of the bug is always two
/// renderers each correctly drawing the thing they were told to draw.
pub fn draws_waiting_block(showing: Option<&SurfaceId>, waiting: &SurfaceId) -> bool {
    showing != Some(waiting)
}

/// What the strip's trailing verb offers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StripVerb {
    /// End the agent that is in this pane.
    End,
    /// Start one, because the pane has an agent-shaped hole in it.
    Launch,
}

/// The verb for a pane that has an agent, or had one.
///
/// One slot, two states, and the second is the one that did not exist: a pane
/// whose agent has gone still reads as an agent pane — it has the surfaces, the
/// history and the strip — and until now the only way to start another in it
/// was to close the pane. Parker: *"if we do end an agent session from the
/// workbench, then we want to be able to start a new session from the workbench
/// as well."*
///
/// `agent_present` is whether a conversational agent is in the pane RIGHT NOW,
/// which is a different question from what the bench's status ladder says.
/// An agent that exits cleanly demotes the pane to a shell and the ladder goes
/// on describing the last thing it saw; an agent whose whole pane died reads
/// `Exited`. Both mean the same thing here — there is no agent to end, and the
/// only useful verb is the one that starts another.
pub fn strip_verb(state: AgentState, agent_present: bool) -> StripVerb {
    if !agent_present || state == AgentState::Exited {
        StripVerb::Launch
    } else {
        StripVerb::End
    }
}

/// Does RETURN start an agent on this bench?
///
/// A bench with no agent in it offers exactly one thing, and until now the key
/// that means *do the obvious thing* did nothing there at all: `reading_key`
/// answers `Act`, `Act` takes the selected surface's first action, and a bench
/// nobody has run an agent on has no surfaces to select. Parker, arriving at
/// one: *"FROM FRESH WORKBENCH — the spin up agent should be ACTIVATED IF I
/// HIT RETURN!"*.
///
/// `selected` is the guard and it is the whole subtlety. A pane whose agent has
/// QUIT also offers the launch — one slot, two states, see [`strip_verb`] — but
/// it is still holding everything that agent presented, and return on an open
/// card means *take this card's first verb*. So the launch is only what return
/// does when there is nothing else for it to do.
pub fn return_launches(verb: StripVerb, selected: bool) -> bool {
    verb == StripVerb::Launch && !selected
}

// ---------------------------------------------------------------------------
// the bench's type
// ---------------------------------------------------------------------------

/// One rung of the bench's type ramp.
///
/// Before this existed the surface carried **fifteen** distinct sizes —
/// `8.5`, `9`, `9.5`, `10`, `10.5`, `11`, `11.5`, `12`, `12.5`, `13`, `13.5`,
/// `15`, `17`, `18`, `20` — each written as a literal at the point of use.
/// That is not a type system, it is accretion: half of those pairs differ by
/// half a point, which no reader can see and every future edit has to choose
/// between. Nine rungs, named for the job rather than the number, and the
/// number lives in exactly one table.
///
/// The old value each rung absorbed is recorded beside it in [`Step::base`],
/// so the collapse is auditable rather than asserted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    /// The faintest marks: shelf legends, a weight's channel names, `TL;DR`.
    Tag,
    /// A label with a word in it — `REVIEW`, a confidence, a register name.
    Fine,
    /// Secondary rows: a measure, a subtitle, an id.
    Note,
    /// The reading size for a list — a row of a table, a bullet, a consequence.
    Small,
    /// Body copy. The default, and what an unremarkable sentence gets.
    Body,
    /// A card's own line: a title in the rail, a question's text.
    Lead,
    /// A heading over a block.
    Head,
    /// The big line on a title card.
    Title,
    /// A single glyph standing alone.
    Display,
}

impl Step {
    /// Every rung, smallest first — the ramp's ordering, written down.
    ///
    /// Used by the tests rather than by the render, which asks for rungs by
    /// name; kept in the shipped build anyway, because it is the list a table
    /// test walks to assert the ramp is strictly increasing, and a ramp with a
    /// repeat in it is two names for one size.
    #[allow(dead_code)]
    pub const ALL: [Step; 9] = [
        Step::Tag,
        Step::Fine,
        Step::Note,
        Step::Small,
        Step::Body,
        Step::Lead,
        Step::Head,
        Step::Title,
        Step::Display,
    ];

    /// The rung's size in points **before the pane's gauge**, and the literals
    /// it replaced.
    pub fn base(self) -> f32 {
        match self {
            Step::Tag => 9.0,      // was 8.5 and 9.0
            Step::Fine => 9.5,     // was 9.5
            Step::Note => 10.0,    // was 10.0 and 10.5
            Step::Small => 11.0,   // was 11.0 and 11.5
            Step::Body => 12.0,    // was 12.0 and 12.5
            Step::Lead => 13.0,    // was 13.0 and 13.5
            Step::Head => 15.0,    // was 15.0
            Step::Title => 17.0,   // was 17.0 and 18.0
            Step::Display => 20.0, // was 20.0
        }
    }
}

/// The bench's type, resolved against one pane's text-size gauge.
///
/// **Why the bench has a gauge at all.** The workbench is the second face of a
/// pane, and a pane is already a display with its own monitor controls — text
/// size, brightness, contrast, colour, gamma, warp. Every one of those reached
/// the terminal grid and stopped at the bench, so a person who had sized their
/// terminal to their eyes flipped faces and got somebody else's idea of 11
/// point. Parker: *"all of the pane gauges should ALSO affect the workbench"*.
///
/// **The gauge is a multiplier, not a percent.** `grade.text_size` runs
/// `0.6..=2.0` with `1.0` neutral, so the slider reading `65%` on the tray is
/// a factor of `1.51` — the dial makes type BIGGER for most of its travel, and
/// reading its percent as a shrink gets the direction of the whole feature
/// backwards.
///
/// **Boxes move with the type.** Anything sized to hold text — a label column,
/// a marker — goes through [`Type::px`] rather than staying a literal, because
/// type that grows inside a box that does not is how an exact fit starts
/// wrapping down a row.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Type {
    k: f32,
}

impl Type {
    /// Resolve against a pane's effective `grade.text_size`.
    ///
    /// A non-finite or non-positive gauge resolves to neutral rather than to
    /// zero: a bench drawn at 0pt is a blank rectangle, which reads as the
    /// feature being broken rather than as a dial being wrong.
    pub fn at(gauge: f32) -> Self {
        let k = if gauge.is_finite() && gauge > 0.0 {
            gauge
        } else {
            1.0
        };
        Self { k }
    }

    /// Neutral — the ramp at its authored sizes. For tests and for any caller
    /// that genuinely has no pane.
    pub fn neutral() -> Self {
        Self { k: 1.0 }
    }

    /// A rung, in points, with the gauge applied.
    pub fn pt(self, step: Step) -> f32 {
        step.base() * self.k
    }

    /// Any other bench measurement that must move with the type — a label
    /// column's width, a status marker's height, a gutter that holds a glyph.
    pub fn px(self, base: f32) -> f32 {
        base * self.k
    }

    /// The factor itself, for the composer's own dynamic ramp.
    pub fn k(self) -> f32 {
        self.k
    }
}

/// Pick a type size that fits this much text in this much room.
///
/// **Two steps, and a floor.** The research says font shrinking is not how
/// chat composers handle long drafts — auto-grow to a cap and then scroll is
/// the dominant pattern everywhere — and the accessibility guidance puts the
/// bottom of comfortable reading at 16px, with 12px the last defensible size
/// for dense UI. So shrinking is worth exactly the range between those, which
/// buys about 40% more text on screen, and then it must stop: 6-point type
/// showing a whole prompt is not showing anybody anything.
///
/// The estimate is deliberate and its error is harmless. Characters-per-line
/// is computed from a 0.6 advance ratio, which is roughly true of a monospaced
/// face and only roughly; being a step out picks a slightly wrong SIZE and
/// never a wrong position, because the caret and the wrapping are laid out by
/// the text system rather than by this arithmetic.
///
/// **The base ramp is half what it was**, and the pane's gauge is applied on
/// top. It ran `17 / 14.5 / 12.5` and the top step was the size of a heading:
/// Parker, on the line he types to an agent — *"The text size in the agent
/// prompt: SMALLER! maybe 1/2 the size as default... and should follow scaling
/// of the pane config"*. Both halves of that are here: the steps are halved,
/// and `k` is the pane's `grade.text_size`.
///
/// **The floor is in RENDERED points, not in base points.** That is the whole
/// reason the accessibility note above survives a halving. `12px` was the last
/// defensible size for dense UI when there was one fixed ramp; with a dial in
/// front of it the same claim has to be made about what the eye receives, so
/// the floor is applied last, after the gauge — a pane turned down to `0.6`
/// gets [`COMPOSER_MIN_PT`], not a base step multiplied into illegibility.
pub fn composer_pt(chars: usize, box_w: f32, box_h: f32, k: f32) -> f32 {
    let k = if k.is_finite() && k > 0.0 { k } else { 1.0 };
    let step = |i: usize| (COMPOSER_STEPS[i] * k).max(COMPOSER_MIN_PT);
    if box_w <= 0.0 || box_h <= 0.0 {
        return step(0);
    }
    for i in 0..COMPOSER_STEPS.len() {
        let pt = step(i);
        let per_line = (box_w / (pt * 0.6)).max(1.0);
        let lines = (chars as f32 / per_line).ceil();
        let room = (box_h / (pt * 1.35)).floor();
        if lines <= room {
            return pt;
        }
    }
    // Past the floor it scrolls instead, which is what every chat composer
    // does and what the eye can actually follow.
    step(COMPOSER_STEPS.len() - 1)
}

/// The composer's three sizes **before the pane's text-size gauge**, largest
/// first. Halved from `17 / 14.5 / 12.5`; the shrink between them is what buys
/// a long draft more room before it starts scrolling.
pub const COMPOSER_STEPS: [f32; 3] = [8.5, 7.25, 6.25];

/// …and what the eye actually receives never goes below this, whatever the
/// gauge. Applied after the multiplier, so it is a claim about legibility
/// rather than about arithmetic.
pub const COMPOSER_MIN_PT: f32 = 9.5;

/// How much of a draft is past what the box can show, in characters.
///
/// `None` when it all fits. A value here is not a failure — it is the ordinary
/// state of a long prompt, and the composer says so rather than silently
/// hiding the top of somebody's paragraph.
pub fn composer_hidden(chars: usize, box_w: f32, box_h: f32, k: f32) -> Option<usize> {
    let pt = composer_pt(chars, box_w, box_h, k);
    if box_w <= 0.0 || box_h <= 0.0 {
        return None;
    }
    let per_line = (box_w / (pt * 0.6)).max(1.0);
    let room = (box_h / (pt * 1.35)).floor().max(1.0);
    let shown = (per_line * room) as usize;
    (chars > shown).then(|| chars - shown)
}

/// The most of a pane the composer may take before it scrolls.
pub const COMPOSER_SHARE: f32 = 0.33;

/// …and never less than this, so a short pane still shows a few lines rather
/// than a third of nothing.
pub const COMPOSER_MIN_MAX: f32 = 96.0;

/// How much of a pane the rail may take. Roughly a third is the most a shelf
/// can have before the thing it is a shelf FOR stops being the main event.
pub const RAIL_SHARE: f32 = 0.30;

/// Narrower than this and the rows stop being readable, so it is ticks or
/// nothing.
pub const RAIL_MIN_W: f32 = 132.0;

// ---------------------------------------------------------------------------
// the bench
// ---------------------------------------------------------------------------

/// What the agent in this pane is doing, as a value rather than as a string.
///
/// It was a `String` with a glyph baked into it — `"\u{2753} your turn"` — drawn
/// at eleven points in the terminal font, and it looked like a log line on a
/// surface that is otherwise a designed object. Parker: *"This your turn looks
/// goinky given the slick modern design ---- kill that. OR PERHAPS MAKE A
/// TITLE card which prompts the user to the agent state."*
///
/// A type rather than a tidier string, because the card needs three things
/// from it — a word, a colour and whether it is urgent — and a string can only
/// answer the first by being parsed, which is what the old strip did
/// (`state.contains("your turn")`) and how the colour got out of step with
/// the word.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AgentState {
    /// Stopped, and cannot continue without a person.
    Asking,
    /// Stopped on something that went wrong.
    Blocked,
    /// Finished its turn.
    Done,
    /// The process is gone.
    Exited,
    /// Mid-turn.
    Working,
    /// The bench just typed an answer into the terminal and the agent has
    /// not moved yet. Parker: the on-screen rule *"will need an additional
    /// agent state for 'reading your instructions'"*.
    Reading,
    /// A turn a PERSON stopped, that has not started again.
    ///
    /// The one state on this list that is not read off the screen. Every other
    /// rung is a sensor — a spinner, a picker, a bell, a dead process — and
    /// this one is a thing the bench did and remembers doing: it sent an
    /// interrupt into a running turn and nothing has started since. The
    /// screen cannot tell it from `Idle`, which is exactly why it has to be
    /// held here. Parker: *"the primary use case for this is that we failed to
    /// set the effort or model correctly and need to stop the turn without
    /// stopping the agent running entirely"*.
    ///
    /// It is the reason [`dials_live`] says yes to it. A paused pane is the
    /// one moment on this surface where the dials are both wanted and safe:
    /// the harness is back at its prompt, so the slash command lands now
    /// rather than at the end of a turn nobody chose.
    Paused,
    /// Attached, nothing happening.
    Idle,
}

impl AgentState {
    /// What to call it, in the words a person would use out loud.
    ///
    /// Sentence case and no glyph: the card carries a coloured dot, and a
    /// glyph plus a colour plus a word is the same fact said three times.
    pub fn word(self) -> &'static str {
        match self {
            AgentState::Asking => "Waiting on you",
            AgentState::Blocked => "Blocked",
            AgentState::Done => "Finished",
            AgentState::Exited => "Exited",
            AgentState::Working => "Working",
            AgentState::Reading => "Reading your answer",
            AgentState::Paused => "Paused",
            AgentState::Idle => "Idle",
        }
    }

    /// Which meaning-colour it borrows, so the card is tinted by the same
    /// table as everything else on the bench.
    pub fn tint(self) -> Tint {
        match self {
            // Paused joins the waiting pair for the same reason they are
            // there: nothing will happen on this pane until a person does
            // something. That it was the person's own doing changes who is
            // surprised, not who is holding the turn.
            AgentState::Asking | AgentState::Blocked | AgentState::Paused => Tint::Waiting,
            AgentState::Done => Tint::Settled,
            AgentState::Working | AgentState::Reading => Tint::Pending,
            // Nothing is being claimed about an idle or departed agent, and
            // grey is how this surface says so everywhere else.
            AgentState::Idle | AgentState::Exited => Tint::Unknown,
        }
    }

    /// Does this state want a person to look at it now?
    ///
    /// Three states stop an agent and only two shout. `Paused` is the third
    /// and it is quiet on purpose: a person who pressed pause four seconds ago
    /// does not need the surface to break the news. It keeps the waiting
    /// COLOUR, so the pane is findable in a window of twenty; it does not take
    /// the phosphor, which is this bench's word for *you did not know this*.
    pub fn urgent(self) -> bool {
        matches!(self, AgentState::Asking | AgentState::Blocked)
    }
}

/// How long after the bench types into a pane the bar keeps saying the agent
/// is reading it, if nothing else moves. Long enough for a picker to redraw
/// and a turn to start; short enough that a dead agent does not read forever.
pub const READING_WINDOW_MS: u64 = 8_000;

/// What the agent is doing, from the pane's sensors, decided in one place.
///
/// The order is the old ladder with two rungs added above it. `reading` — the
/// bench typed within [`READING_WINDOW_MS`] and the agent has not started
/// working — outranks `asking`, because a picker stays on screen for a moment
/// after the keys land, and a bar saying "Waiting on you" over an answer just
/// given is the surface lying about its own state. It never outranks a pane
/// that is blocked or gone: nothing is reading there.
///
/// `paused` sits above even that, and the height is not a preference — it is
/// what the rung has to clear to ever be true at all:
///
/// - **Above `reading`,** because pausing IS a write. The interrupt goes down
///   the same pseudoterminal as a message and stamps the same clock, so a
///   pause would otherwise read "Reading your answer" for the next eight
///   seconds — the bench describing its own keystroke instead of the turn it
///   just stopped.
/// - **Above `done`,** because a harness may well ring its bell on the way
///   back to the prompt, and Parker's rule is exactly that the two must not be
///   confused: *"a COMPLETED turn should NOT allow a user to 'resume turn' ...
///   ONLY a PAUSED turn should allow this"*. A finished turn and a stopped one
///   look identical from outside and offer opposite controls.
/// - **Above `asking`,** because the question that was on screen when the
///   interrupt landed is still on the bench for a sweep or two after it stops
///   being real.
///
/// The two guards are what make it self-correcting rather than a latch that
/// can lie. `!thinking`: if the interrupt did not take, or a new turn has
/// started by any route at all — the terminal face, a hook, the person typing
/// — the screen says `Working` and the screen wins, because a pause that did
/// not stop anything is not a pause. `!exited`: there is nothing to resume in
/// a pane whose process is gone.
pub fn agent_state(
    asking: bool,
    blocked: bool,
    done: bool,
    exited: bool,
    thinking: bool,
    reading: bool,
    paused: bool,
) -> AgentState {
    if paused && !thinking && !exited {
        AgentState::Paused
    } else if reading && !thinking && !blocked && !exited {
        AgentState::Reading
    } else if asking {
        AgentState::Asking
    } else if blocked {
        AgentState::Blocked
    } else if done {
        AgentState::Done
    } else if exited {
        AgentState::Exited
    } else if thinking {
        AgentState::Working
    } else {
        AgentState::Idle
    }
}

/// Where a row stands in its shelf's story.
///
/// A rail of decisions is not a list, it is a HISTORY with a head, and the
/// head is the only row most readers are looking for: what is in force right
/// now. Drawn as a flat newest-first column with every row the same weight,
/// nothing says which one that is — Parker, on two answered questions in
/// identical green: *"the ORDER of the decision items is not clear what the
/// CURRENT STANDING decision is --- that needs DISTINCTION visually!"*.
///
/// Three states, because there are three: something is waiting on a person,
/// something is the answer that currently holds, and everything else is the
/// record of how it got there.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Standing {
    /// Unanswered, and the one to deal with. Loudest, pinned to the top
    /// whatever its arrival order — an unanswered question is not history.
    ///
    /// At most ONE row in a shelf is ever this, which is what makes the glow
    /// mean something.
    Waiting,
    /// Also unanswered, and behind something else.
    ///
    /// Three questions all marked WAITING ON YOU, all glowing, all with the
    /// same red edge, is three rows shouting the same thing and no answer to
    /// *which one first* — Parker: *"The GLOW implies elevated attention...
    /// so then DEGLOW the demoted older ones and EVEN tone down the intensity
    /// of their left border"*.
    ///
    /// It is a real state rather than a rendering trick: the agent's picker
    /// takes one answer at a time, so everything behind the head genuinely is
    /// queued. Still marked as waiting, because it is — just not first.
    Queued,
    /// The newest settled row: what stands right now.
    Current,
    /// How it got here. Still readable, deliberately quieter.
    Past,
}

/// One row of the rail, ready to draw.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub id: SurfaceId,
    pub title: String,
    pub subtitle: String,
    pub kind: &'static str,
    /// The short word this row wears, or [`None`] where the state chip
    /// already says everything. See [`crate::surface::Shelf::badge`].
    pub badge: Option<String>,
    pub tint: Tint,
    pub standing: Standing,
    pub selected: bool,
    /// Arrived while the person was on the other face, or on another shelf.
    pub unseen: bool,
}

/// Everything one pane's workbench holds.
///
/// Per pane, deliberately. A surface belongs to the conversation that made it,
/// and a shared store would put one agent's diagram on another agent's bench.
#[derive(Clone, Debug)]
pub struct Bench {
    /// Arrival order, oldest first, capped at
    /// [`crate::surface::PANE_HISTORY_CAP`].
    surfaces: Vec<Surface>,
    face: Face,
    shelf: Shelf,
    selected: Option<SurfaceId>,
    /// The person's own preference for the rail. Physics may still overrule it.
    rail_wanted: bool,
    unseen: HashSet<SurfaceId>,
    /// Which tab of a response card the reader picked.
    ///
    /// **Absent is not the first one.** Absent means they have not chosen, which
    /// is a different fact: the renderer may collapse it to the first group at
    /// draw time, and this map may not store that collapse — otherwise a reply
    /// updated to carry a new first group would move under a reader who never
    /// touched it, and nothing would be able to tell that from a choice.
    tab: std::collections::HashMap<SurfaceId, crate::surface::Group>,
    /// Which register inside a tab, for the tabs where they picked one.
    ///
    /// Keyed by the pair, so reading the technical brief and then going to the
    /// evidence tab and back brings the technical brief back rather than the
    /// tl;dr.
    reg: std::collections::HashMap<(SurfaceId, crate::surface::Group), String>,
    /// The turn that has begun and presented nothing yet.
    ///
    /// Not a surface, and deliberately not in `surfaces`: nothing was
    /// presented, so there is nothing to store, nothing to retire and nothing
    /// to survive the process. See [`LiveTurn`].
    live: Option<LiveTurn>,
    /// How many turns this bench has seen begin, so two of them in one
    /// millisecond are still two rows and two scroll positions.
    turns: u32,
}

/// What a live turn's row calls itself where a surface would name its kind.
///
/// It is not a [`Kind`] and never becomes one — a kind is a shape a payload
/// arrived in, and nothing has arrived. The rail reads this word to give the
/// row its own lane vocabulary: a turn that has said nothing does not
/// `STAND NOW` for anything. See [`crate::benchdraw`]'s lane table.
pub const LIVE_TURN_KIND: &str = "turn";

/// Who opened a turn.
///
/// Absent is a third answer and not a synonym for either: the harness's prompt
/// record can arrive carrying no text at all, and then a turn certainly began
/// and this window cannot say whose it was. See [`Bench::turn_began`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Voice {
    /// A person typed it.
    Person,
    /// The harness opened it — a background task reporting in, a peer session
    /// talking. See [`crate::channel::Woken`].
    Harness,
}

/// A TURN IN FLIGHT, standing in the feed for the reply that has not landed.
///
/// The overview is a feed of replies, and a reply is a thing an agent presents
/// at the END of a turn — so for the whole length of a turn the newest reply
/// in the store belonged to the PREVIOUS one, and the rail said `STANDS NOW`
/// over it. Parker, with the bench four minutes into a turn beside the
/// terminal running it: *"when the CURRENT turn is running in workbench... we
/// see the LAST turn persisting -- the CURRENT turn should IMMEDIATELY make a
/// new overview card, and we flip to that"*.
///
/// So the turn itself becomes the card. It carries what is known at the moment
/// it begins — the opening words and whose voice they are — and the renderer
/// adds what is only true this instant: the agent's own gerund, its clock, its
/// tokens and the tool call it is inside. Nothing else. *"only the action that
/// is LIVE -- ie. not the terminal scrollback style."* The terminal face is
/// one keystroke away and is the surface that keeps the log.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct LiveTurn {
    /// The row's identity, unique to this turn. Never names a surface, which
    /// is what makes it unopenable — see [`Bench::select`].
    pub id: SurfaceId,
    /// The turn's opening line, for the rail row that has one line to say what
    /// this turn is about.
    ///
    /// `None` means this window did not see how the turn opened — an unhooked
    /// pane noticing the spinner start — which is drawn as that rather than as
    /// an empty string.
    pub headline: Option<String>,
    pub voice: Option<Voice>,
    /// When it began, on the window's clock.
    pub at_ms: u64,
}

/// What a press turns into.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Dispatch {
    /// The window does it: open a file or a URL with the desktop's handler.
    Open(String),
    /// The agent hears about it, as a line typed into its own terminal.
    Tell(ActionReport),
    /// Raw bytes into the pane's pseudoterminal — keystrokes, not a sentence.
    ///
    /// The difference from [`Dispatch::Tell`] is who is listening. `Tell`
    /// writes a line for an agent to READ on its next turn; this drives a
    /// program that is waiting at a prompt right now, which is how a menu
    /// gets answered and how typed text reaches a REPL.
    Keys {
        bytes: Vec<u8>,
        /// What it did, for whoever is looking at the surface afterwards.
        note: String,
    },
    /// Put text on the system clipboard, and do nothing else.
    ///
    /// Local like [`Dispatch::Open`], and separate from it because the desktop
    /// handler is the wrong instrument: `Open` hands a path to whatever the
    /// machine has registered for it, and this carries the surface's own words
    /// with no file in the story at all. It exists because a comment has no
    /// verb that reaches the agent, so lifting the words by hand is the only
    /// route there is — see [`crate::surface::Action::Copy`].
    Clipboard(String),
    /// Nothing to do, and a reason worth showing rather than a silent no-op.
    Refused(String),
}

// Chords the WINDOW owns are `crate::keylayer::window_chord`. They moved there
// with the rest of the question *who owns this keystroke*, and the window is the
// top of that ladder. A pointer rather than a re-export: a second name for one
// table is the shape of the drift the table exists to prevent.

/// `alt+<n>` selects the nth shelf outright, rather than cycling to it.
///
/// `tab` already walks the shelves and will keep doing so; this is for landing
/// on one directly. Parker asked for it on the bench's existing `alt+<key>`
/// pattern after `alt+m` shipped, and proposed the four keys sitting under the
/// right hand in tab order — `alt+v b n m` for overview, artifacts, decisions,
/// comments. The idea is right and two of those keys are already spoken for:
///
/// - **`alt+v` splits the focused pane**, Tilix-style, alongside `alt+h`. It is
///   in [`crate::keylayer::window_chord`], in the module header and on the keybindings sheet.
/// - **`alt+b` is readline's word-back**, which reaches the agent's own prompt
///   from the bench composer — `keystroke_bytes` passes it through deliberately.
///
/// So the keys are the DIGITS, which are positional in the same way the letter
/// row would have been, free on both faces, and — unlike four hand-picked
/// letters — they extend on their own the day a fifth shelf appears. The
/// mapping is `Shelf::ALL`'s own order, so there is no second list to keep in
/// step with the tab strip.
///
/// Modifiers are checked here rather than at the call site because getting them
/// wrong is silent: [`reading_key`] reads a bare digit as ANSWERING option `n`
/// of a waiting question, so a chord that let an unmodified `2` through would
/// answer somebody's picker instead of changing tab.
pub fn shelf_chord(key: &str, alt: bool, control: bool) -> Option<Shelf> {
    if !alt || control {
        return None;
    }
    let n: usize = key.parse().ok()?;
    Shelf::ALL.get(n.checked_sub(1)?).copied()
}

/// Does this keystroke put a CHARACTER in front of a person?
///
/// The bench starts talking on any printable key, so that there is no "click
/// here first" — and the test for printable was `key_char` alone, with no look
/// at the modifiers. gpui fills `key_char` for `alt+r` with `"r"`, so the chord
/// read as the letter r: the bench opened a composer where the FOCUS reader
/// should have been, and then typed nothing into it, because the encoder
/// correctly refused the chord one layer down. An empty box where a reader was
/// asked for.
///
/// `shift` is deliberately absent from the refusal — `shift+a` is the
/// character `A`.
pub fn types_a_character(key_char: Option<&str>, alt: bool, control: bool, platform: bool) -> bool {
    if alt || control || platform {
        return false;
    }
    key_char.is_some_and(|c| !c.is_empty() && !c.chars().any(char::is_control))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reading {
    Down,
    Up,
    NextShelf,
    /// Take the selected surface's first action.
    Act,
    /// Answer the selected question with this zero-based option.
    Choose(usize),
    /// Start talking to the agent, carrying this keystroke through.
    Talk,
    /// Not ours. **The terminal underneath gets it** — `ctrl+c` interrupts the
    /// turn you are watching, a function key reaches the app that binds it.
    ///
    /// It used to be called `Ignore` and it was a swallow: `bench_key` ended
    /// every path in `stop_propagation`, so a key with no meaning here died on
    /// the workbench face instead of reaching the agent whose conversation was on
    /// the screen. Renamed rather than re-documented, because the old name is
    /// what made the swallow look deliberate at the one call site that mattered.
    Pass,
}

/// Decide what a key does on a reading bench.
///
/// `answerable` is how many options the selected question is waiting on —
/// `None` when the selection is not a waiting question, which is what makes a
/// digit ambiguous everywhere else and therefore just a character.
pub fn reading_key(key: &str, printable: bool, answerable: Option<usize>) -> Reading {
    match key {
        "down" => return Reading::Down,
        "up" => return Reading::Up,
        "tab" => return Reading::NextShelf,
        "enter" => return Reading::Act,
        _ => {}
    }
    if let Some(n) = key.parse::<usize>().ok().filter(|_| key.len() == 1) {
        if let Some(options) = answerable {
            if n >= 1 && n <= options {
                return Reading::Choose(n - 1);
            }
        }
    }
    if printable {
        Reading::Talk
    } else {
        Reading::Pass
    }
}

/// The keystrokes that move a terminal menu from `cursor` to `target` and
/// press return.
///
/// Arrow keys rather than the option's number, deliberately. A digit is
/// ambiguous across pickers — in some it jumps, in some it selects
/// immediately, in some it types into a filter — while up, down and return
/// mean the same thing in every one of them. It also degrades honestly: if
/// the cursor is not where we believe, the selection lands on the wrong row
/// rather than on a row nobody can predict.
pub fn menu_keys(target: usize, cursor: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let (step, n) = if target >= cursor {
        (&b"\x1b[B"[..], target - cursor)
    } else {
        (&b"\x1b[A"[..], cursor - target)
    };
    for _ in 0..n {
        out.extend_from_slice(step);
    }
    out.push(b'\r');
    out
}

/// One line of text typed at whatever is waiting in the pane.
///
/// A carriage return, not a newline: a line editor reading a pseudoterminal
/// sees `\r` as "submit" and `\n` as a literal newline in the buffer, which is
/// how a prompt ends up with a blank line in it instead of being sent.
pub fn typed_line(text: &str) -> Vec<u8> {
    // CRLF collapses to ONE space, not two. Pasted text is full of it, and a
    // prompt with doubled gaps everywhere reads as the box mangling what was
    // pasted into it.
    let mut bytes: Vec<u8> = text
        .replace("\r\n", "\n")
        .replace(['\n', '\r'], " ")
        .into_bytes();
    bytes.push(b'\r');
    bytes
}

/// What pressing a verb will do, in the bytes it will do it with.
///
/// The same [`crate::surface::ActionReport::to_prompt`] that types the line
/// produces the preview, so a chip cannot promise one thing and type another —
/// the label came from the agent, and the whole point of the click is to be a
/// check on the agent. Local verbs, the ones the window performs itself, say
/// what they open instead, and say plainly when they would open nothing.
pub fn verb_preview(
    surface: &Surface,
    action: &crate::surface::Action,
    target: Option<&str>,
    comment: Option<&str>,
    tag: Option<&str>,
) -> String {
    use crate::surface::{Action, ActionReport, Kind};
    match action {
        Action::Open => match &surface.kind {
            Kind::Artifact(a) => format!("opens {}", a.href),
            _ => "opens nothing \u{2014} this surface is not a document".to_string(),
        },
        Action::OpenSource => match surface.source.as_ref().and_then(|s| s.files.first()) {
            Some(f) => format!("opens {f}"),
            None => "opens nothing \u{2014} this surface names no source".to_string(),
        },
        Action::Copy => match &surface.kind {
            Kind::Comment(_) | Kind::Markdown(_) => {
                "copies the text \u{2014} the agent is not told".to_string()
            }
            _ => "copies nothing \u{2014} this surface is not text".to_string(),
        },
        _ => ActionReport {
            surface: surface.id.clone(),
            action: action.clone(),
            target: target.map(str::to_string),
            comment: comment.map(str::to_string),
        }
        .to_prompt(tag),
    }
}

impl Default for Bench {
    /// The rail is OPEN by default, and that is the one field here whose
    /// default is a design decision rather than a zero value.
    ///
    /// A pane's own rail is how a person finds out the bench has a history at
    /// all; folded to ticks on first sight it is a column of marks nobody has
    /// a reason to click. Physics still overrules it — a pane too narrow to
    /// hold a rail and a readable surface gets the ticks anyway.
    fn default() -> Bench {
        Bench {
            surfaces: Vec::new(),
            face: Face::default(),
            shelf: Shelf::default(),
            selected: None,
            rail_wanted: true,
            unseen: HashSet::new(),
            tab: std::collections::HashMap::new(),
            reg: std::collections::HashMap::new(),
            // A fresh bench is not mid-turn. `None` here is the honest one:
            // no turn has begun that this bench saw.
            live: None,
            turns: 0,
        }
    }
}

impl Bench {
    pub fn new() -> Bench {
        Bench::default()
    }

    /// Empty the bench, keeping what the READER chose about it.
    ///
    /// Called on the agent-departed edge. A surface belongs to the conversation
    /// that made it, so when that conversation ends the bench it was on stops
    /// being this pane's history — the next agent launched here is a different
    /// conversation and would otherwise inherit a stranger's diagrams. Until
    /// this existed there was no caller anywhere that emptied a `Bench`, which
    /// is why the one pane an agent had actually worked in was the one pane
    /// that could never offer a clean start.
    ///
    /// **The face, the shelf and the rail preference survive.** Those are
    /// settings the person made about this pane, not facts about the agent that
    /// left: someone reading on the workbench face when their agent exits
    /// should still be on the workbench face, looking at an empty rail. The
    /// per-surface state goes with the surfaces, because selection, unseen
    /// marks and the chosen tab and register are each keyed by a `SurfaceId`
    /// that no longer names anything.
    ///
    /// Nothing on disk is touched. The conversation's record outlives the
    /// process, which is the whole reason for keeping one.
    ///
    /// # Why this destructures instead of assigning through `self`
    ///
    /// The failure this function exists to prevent is a surface outliving the
    /// conversation that made it, and the way it would come back is a FUTURE
    /// field: somebody adds a seventh map keyed by `SurfaceId` — a pinned set,
    /// a scroll offset per card, a per-surface note — and does not think to
    /// clear it here. Nothing would fail. The bench would look empty and hold
    /// one agent's state against the next agent's surfaces, which is the exact
    /// defect in a smaller and much harder-to-see form.
    ///
    /// An exhaustive destructuring pattern makes that a **compile error at this
    /// line**, naming the field. The `_` bindings are the ones that survive on
    /// purpose, so adding a field forces a decision about which group it is in
    /// rather than letting silence pick.
    ///
    /// A test cannot do this job: a test can only assert about fields that
    /// existed when it was written.
    pub fn clear_surfaces(&mut self) {
        let Bench {
            surfaces,
            selected,
            unseen,
            tab,
            reg,
            live,
            turns,
            // Kept — the reader's settings about this pane, not facts about the
            // agent that left.
            face: _,
            shelf: _,
            rail_wanted: _,
        } = self;
        surfaces.clear();
        *selected = None;
        unseen.clear();
        tab.clear();
        reg.clear();
        // A turn belongs to the conversation that was having it. An agent that
        // left is not four minutes into anything, and a card saying it was
        // would be the one lie this whole surface exists to stop telling.
        *live = None;
        *turns = 0;
    }

    pub fn face(&self) -> Face {
        self.face
    }

    /// Flip the face. Returns the face now showing.
    pub fn toggle_face(&mut self) -> Face {
        self.face = self.face.other();
        if self.face == Face::Workbench {
            // Looking at the bench is what makes its contents seen — but only
            // the shelf actually on screen. A decision waiting on another tab
            // has not been looked at just because a document on this one was.
            self.mark_shelf_seen();
        }
        self.face
    }

    pub fn set_face(&mut self, face: Face) {
        if self.face != face {
            self.toggle_face();
        }
    }

    pub fn shelf(&self) -> Shelf {
        self.shelf
    }

    pub fn set_shelf(&mut self, shelf: Shelf) {
        self.shelf = shelf;
        // Selection follows the shelf: a selected surface that is not on this
        // tab would render a body the rail has no row for, which reads as the
        // pane showing something at random.
        if !self
            .selected
            .as_ref()
            .and_then(|id| self.get(id))
            .is_some_and(|s| shelf.holds(s.kind.shelf()))
        {
            // The head row, and ONLY if it names a surface. The head of the
            // overview can be the turn in flight: selecting that id would set
            // the card to something `get` cannot resolve — an empty room with
            // nothing in the state saying why — and skipping past it to the
            // reply underneath would open the previous turn's answer over a
            // turn that is still running, which is the defect [`LiveTurn`]
            // exists to end. Selecting nothing is right: the live turn then
            // stands in the room, which is where a person switching back to
            // this shelf meant to arrive.
            self.selected = self
                .rows()
                .first()
                .map(|r| r.id.clone())
                .filter(|id| self.get(id).is_some());
        }
        if self.face == Face::Workbench {
            self.mark_shelf_seen();
        }
    }

    pub fn rail_wanted(&self) -> bool {
        self.rail_wanted
    }

    pub fn toggle_rail(&mut self) {
        self.rail_wanted = !self.rail_wanted;
    }

    /// Whether this bench holds no surfaces at all.
    ///
    /// **Nothing reads this, and that is on purpose.** Its one caller was the
    /// condition deciding whether to offer `LAUNCH AGENT`, and asking a bench
    /// whether it is empty is the wrong question for that: an agent's surfaces
    /// outlive the agent, so the pane that had actually run one was the pane
    /// that could never start another. It is kept because "is there anything on
    /// this bench" is a reasonable thing to ask — but a caller reaching for it
    /// to decide something about the agent's PRESENCE should stop and ask that
    /// instead.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.surfaces.is_empty()
    }

    /// Everything, newest first — what the ticks strip draws.
    pub fn all_newest_first(&self) -> impl Iterator<Item = &Surface> {
        self.surfaces.iter().rev()
    }

    /// Change one question in place — a cursor the screen reader found, an
    /// answer the channel recorded. Nothing else about the surface moves, and
    /// a surface that is not a question is left alone and reported.
    pub fn with_question_mut(
        &mut self,
        id: &SurfaceId,
        f: impl FnOnce(&mut crate::surface::Question),
    ) -> bool {
        match self.surfaces.iter_mut().find(|s| s.id == *id) {
            Some(Surface {
                kind: Kind::Question(q),
                ..
            }) => {
                f(q);
                true
            }
            _ => false,
        }
    }

    pub fn get(&self, id: &SurfaceId) -> Option<&Surface> {
        self.surfaces.iter().find(|s| &s.id == id)
    }

    /// How many surfaces sit on each shelf, and how many of those are unseen.
    pub fn counts(&self, shelf: Shelf) -> (usize, usize) {
        let on = self.surfaces.iter().filter(|s| shelf.holds(s.kind.shelf()));
        let mut total = 0;
        let mut unseen = 0;
        for s in on {
            total += 1;
            if self.unseen.contains(&s.id) {
                unseen += 1;
            }
        }
        // The turn in flight is a row on the overview, so it is one of the
        // overview's things — a tab reading `4` above five rows is a surface
        // disagreeing with itself. Never UNSEEN: it is the thing in the room.
        if shelf == Shelf::Overview && self.live.is_some() {
            total += 1;
        }
        (total, unseen)
    }

    /// The rows of the active shelf, newest first.
    ///
    /// Newest first because a bench is about what just happened; the store on
    /// disk is what remembers the order things were made in.
    pub fn rows(&self) -> Vec<Row> {
        self.rows_for(self.shelf)
    }

    pub fn rows_for(&self, shelf: Shelf) -> Vec<Row> {
        // THE TURN IN FLIGHT IS THE FIRST ROW, and it is here rather than in
        // the renderer so that one list is the rail, the keyboard's `step` and
        // the shelf's count — three readers of one fact that would otherwise
        // have three chances to disagree.
        let live = (shelf == Shelf::Overview)
            .then_some(self.live.as_ref())
            .flatten()
            .map(|t| Row {
                id: t.id.clone(),
                title: t.headline.clone().unwrap_or_else(|| {
                    "a turn began \u{b7} nothing recorded what opened it".to_string()
                }),
                subtitle: "the reply has not landed yet".to_string(),
                kind: LIVE_TURN_KIND,
                badge: None,
                tint: Tint::Pending,
                standing: Standing::Past,
                // It cannot be opened, so it can never be the selection; and it
                // is the thing in the room, so it was never missed.
                selected: false,
                unseen: false,
            });
        let mut rows: Vec<Row> = live
            .into_iter()
            .chain(
                self.surfaces
                    .iter()
                    .rev()
                    .filter(|s| shelf.holds(s.kind.shelf()))
                    .map(|s| Row {
                        selected: self.selected.as_ref() == Some(&s.id),
                        unseen: self.unseen.contains(&s.id),
                        id: s.id.clone(),
                        title: s.title.clone(),
                        subtitle: s.subtitle(),
                        kind: s.kind.id(),
                        badge: shelf.badge(&s.kind, false),
                        tint: tint_of(&s.kind),
                        // Filled in below: standing is a property of a row's
                        // place in the shelf, which no row can know about
                        // itself.
                        standing: Standing::Past,
                    }),
            )
            .collect();
        stand(&mut rows);
        rows
    }

    /// Every answered question on this bench, oldest first — what the review
    /// gallery walks. See [`reviewed`].
    pub fn reviewable(&self) -> Vec<Reviewed> {
        reviewed(&self.surfaces)
    }

    /// The surface OPENED as a card over the conversation, if any.
    ///
    /// `None` is the ordinary state, and the main area then shows the
    /// conversation. Opening a card is a deliberate act — a click on a rail
    /// row — and closing it returns to the conversation rather than to
    /// another card.
    pub fn selected(&self) -> Option<&Surface> {
        self.selected.as_ref().and_then(|id| self.get(id))
    }

    /// What the main area draws: the opened card, or — on the overview, with
    /// nothing opened — the newest response.
    ///
    /// The overview is a feed of replies, and a feed that showed nothing until
    /// a row was clicked would be a rail with an empty room beside it. So the
    /// newest reply stands in the room by default, the way the last message
    /// does in any conversation, and a person who opens an older one from the
    /// rail keeps it until they close it. This is a property of the SHELF,
    /// not of an arrival: an arrival still selects nothing, still moves no
    /// shelf, and a card the person opened stays open under it.
    ///
    /// A TURN IN FLIGHT OUTRANKS THE STAND-IN and yields `None` here, because
    /// the two answer the same question — *what is this conversation's newest
    /// thing* — and the live turn is the newer of them. Returning the previous
    /// reply as well would draw both, which is the defect [`LiveTurn`] exists
    /// to end.
    pub fn showing(&self) -> Option<&Surface> {
        if self.live_standing().is_some() {
            return None;
        }
        self.selected().or_else(|| {
            (self.shelf == Shelf::Overview)
                .then(|| {
                    self.surfaces
                        .iter()
                        .rev()
                        .find(|s| matches!(s.kind, Kind::Response(_)))
                })
                .flatten()
        })
    }

    /// The turn in flight, if this bench has one.
    pub fn live(&self) -> Option<&LiveTurn> {
        self.live.as_ref()
    }

    /// The turn in flight WHEN IT IS THE THING IN THE ROOM — the overview, with
    /// no card opened over it.
    ///
    /// The same three conditions the stand-in reply has, for the same reasons:
    /// another shelf is not a conversation, and a card the person opened is a
    /// document they navigated to and is theirs until they close it.
    pub fn live_standing(&self) -> Option<&LiveTurn> {
        (self.shelf == Shelf::Overview && self.selected.is_none())
            .then_some(self.live.as_ref())
            .flatten()
    }

    /// A turn began. Mints the card the overview flips to.
    ///
    /// Always replaces: a second prompt while the first turn's card is still up
    /// is a second turn, and the row that names it is the new one.
    ///
    /// **Only a turn the PERSON opened takes an opened card.** They typed, so
    /// they moved themselves, and the room follows them to what they just said.
    /// A wake-up did not move them and neither did a prompt record that carried
    /// no words — the card stays where they left it, and the new turn waits at
    /// the head of the rail. `Voice` being an [`Option`] is what makes those
    /// two different from a turn known to be the harness's.
    pub fn turn_began(&mut self, headline: Option<String>, voice: Option<Voice>, at_ms: u64) {
        self.turns = self.turns.wrapping_add(1);
        self.live = Some(LiveTurn {
            id: SurfaceId(format!("turn:{at_ms}:{}", self.turns)),
            headline: headline.filter(|h| !h.trim().is_empty()),
            voice,
            at_ms,
        });
        if voice == Some(Voice::Person) {
            self.selected = None;
        }
    }

    /// The agent started working and no record said so first.
    ///
    /// The fallback for a pane whose harness has no hooks installed, and for a
    /// turn typed at the terminal face of a pane that does: the spinner is the
    /// only evidence either produces. It fills a gap and never overwrites — a
    /// turn already begun knows more about itself (the words, the voice) than
    /// an edge can ever say.
    pub fn turn_seen_working(&mut self, headline: Option<String>, at_ms: u64) {
        if self.live.is_none() {
            // Voice stays UNKNOWN however good the headline is. A screen
            // cannot tell a person typing from the harness pasting a task
            // notification in — they arrive identically, which is the whole
            // reason [`crate::channel::woken_by`] exists on the hook side —
            // so the words are offered and the attribution is not claimed.
            self.turn_began(headline, None, at_ms);
        }
    }

    /// A reply landed. The turn is over and its card is the reply's.
    ///
    /// **Called by the reply arriving, never by the agent going idle.** A turn
    /// ends before its reply is presented — the harness's own stop hook writes
    /// the record afterwards — so retiring on idle would put the PREVIOUS
    /// turn's card back in the room for that gap, which is this feature's own
    /// defect in miniature. A turn that ends having presented nothing keeps its
    /// card, and [`live_says`] is where the card says so.
    pub fn turn_settled(&mut self) {
        self.live = None;
    }

    /// True when the card in the room is the overview's STAND-IN — the newest
    /// reply, standing there because nobody opened anything.
    ///
    /// Not the same question as "is a card showing". A card the person OPENED
    /// is a document they navigated to; the stand-in is the tail of a
    /// conversation, and only the tail can honestly be captioned with the
    /// latest thing the person said. See [`ask_lines`].
    ///
    /// A turn in flight is the tail by definition, and the message above it is
    /// the one that opened it — which is the caption at its most accurate this
    /// block ever gets.
    pub fn standing_in(&self) -> bool {
        self.selected.is_none() && (self.live_standing().is_some() || self.showing().is_some())
    }

    /// Read one group of a response card.
    pub fn pick_tab(&mut self, id: &SurfaceId, group: crate::surface::Group) {
        self.tab.insert(id.clone(), group);
    }

    /// Show one register inside its own group — and open that group, because a
    /// chip you can press is a chip in the tab you are looking at, and storing
    /// the tab alongside it is what keeps the two from disagreeing later.
    pub fn pick_register(&mut self, id: &SurfaceId, key: &str) {
        let group = self.group_of_key(id, key);
        self.tab.insert(id.clone(), group);
        self.reg.insert((id.clone(), group), key.to_string());
    }

    /// Which tab the reader chose here, if they chose one.
    pub fn picked_tab(&self, id: &SurfaceId) -> Option<crate::surface::Group> {
        self.tab.get(id).copied()
    }

    /// Which register they chose inside that tab, if they chose one.
    pub fn picked_register(&self, id: &SurfaceId, group: crate::surface::Group) -> Option<&str> {
        self.reg.get(&(id.clone(), group)).map(String::as_str)
    }

    /// The group a key belongs to, looked up through the surface rather than
    /// guessed from the key's spelling — except for the three keys that have
    /// no section to look up: the brief, the plain reply and the doubts.
    fn group_of_key(&self, id: &SurfaceId, key: &str) -> crate::surface::Group {
        if crate::surface::Register::is_brief(key) || crate::surface::Register::is_layman(key) {
            return crate::surface::Group::Reading;
        }
        if crate::surface::Register::is_doubts(key) {
            return crate::surface::Group::Evidence;
        }
        self.get(id)
            .and_then(|s| match &s.kind {
                Kind::Response(r) => r.sections.iter().find(|x| x.key == key),
                _ => None,
            })
            .map(|s| crate::surface::Group::of(s.register))
            .unwrap_or(crate::surface::Group::Other)
    }

    /// Forget what the reader was reading on a surface that is gone.
    fn forget_picks(&mut self, id: &SurfaceId) {
        self.tab.remove(id);
        self.reg.retain(|(sid, _), _| sid != id);
    }

    /// Open one as a card. Marks it seen, because opening is looking.
    ///
    /// THE TURN IN FLIGHT IS NOT A DOCUMENT, so its row does not open one —
    /// pressing it closes whatever is open instead, which is what "go to the
    /// turn in flight" means on a surface where the room already holds it. A
    /// row that swallowed the press and did nothing would be a control that
    /// lies, and one that set `selected` to an id naming no surface would
    /// silently empty the room.
    pub fn select(&mut self, id: &SurfaceId) {
        if self.live.as_ref().is_some_and(|t| &t.id == id) {
            self.selected = None;
            return;
        }
        if self.get(id).is_some() {
            self.selected = Some(id.clone());
            self.unseen.remove(id);
        }
    }

    /// Close the card and go back to the conversation.
    pub fn close_card(&mut self) -> bool {
        self.selected.take().is_some()
    }

    /// The question this bench is waiting on, if one is.
    ///
    /// Drawn inline in the conversation rather than needing to be found and
    /// clicked: an agent that has stopped and cannot continue is the one
    /// thing on this surface a person must not have to go looking for.
    /// **The selection wins when it is itself a question still owed an answer.**
    /// Without that, pressing a step of the round navigator selected a surface
    /// nothing read, and answering one question of a round left the block on the
    /// card just answered — both gestures the navigator exists for did nothing
    /// a person could see.
    ///
    /// Otherwise the newest waiting question, and then a correction that only
    /// appears on rounds: `rev()` takes the LAST arrival, which was right while
    /// questions arrived one at a time and the newest was the live one. A round
    /// arrives whole, so its last arrival is its LAST question — a round of two
    /// opened on `Orphans` with `Ended state` unanswered behind it. Where the
    /// newest waiting question belongs to a round, the block starts at that
    /// round's first open step instead.
    pub fn waiting_question(&self) -> Option<&Surface> {
        fn waiting(s: &Surface) -> bool {
            matches!(&s.kind, Kind::Question(q)
                if q.answer == crate::surface::Answered::Waiting)
        }
        if let Some(sel) = self.selected().filter(|s| waiting(s)) {
            return Some(sel);
        }
        let newest = self.surfaces.iter().rev().find(|s| waiting(s))?;
        let first_open = match &newest.kind {
            Kind::Question(q) => q.round.as_ref().and_then(|r| {
                // `next_open` wraps, so starting at the last step lands on the
                // first open one from the top.
                let at = r.next_open(r.steps.len().checked_sub(1)?)?;
                r.steps.get(at)?.id.clone()
            }),
            _ => None,
        };
        match first_open {
            Some(id) => self.surfaces.iter().find(|s| s.id == id).or(Some(newest)),
            None => Some(newest),
        }
    }

    /// Move the selection within the active shelf. `+1` is down the rail.
    pub fn step(&mut self, delta: i32) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let at = rows.iter().position(|r| r.selected).unwrap_or(0) as i32;
        let next = (at + delta).clamp(0, rows.len() as i32 - 1) as usize;
        let id = rows[next].id.clone();
        self.select(&id);
    }

    /// Take a payload. Returns the id if something changed, so a caller can
    /// decide whether the pane is worth repainting.
    pub fn apply(&mut self, post: Post) -> Option<SurfaceId> {
        match post.op {
            Op::Retire => {
                let before = self.surfaces.len();
                self.surfaces.retain(|s| s.id != post.id);
                self.unseen.remove(&post.id);
                self.forget_picks(&post.id);
                // Closes the card if it was the one open. It does NOT open a
                // neighbour: a surface appearing under the reader because
                // another one was retired is the rail choosing for them.
                if self.selected.as_ref() == Some(&post.id) {
                    self.selected = None;
                }
                (self.surfaces.len() != before).then_some(post.id)
            }
            Op::Present | Op::Update => {
                let incoming = post.surface?;
                let id = incoming.id.clone();
                // THE REPLY LANDING IS WHAT RETIRES THE TURN, and it is done
                // here rather than at the pane's `present` so that one door
                // does both halves. A reply reaches this bench by four roads —
                // the MCP verb, the hook's own copy, a file dropped in the
                // pane's directory, and a replay off disk at startup — and a
                // retirement written beside any one of them would leave the
                // other three standing a finished turn over its own answer.
                if matches!(incoming.kind, Kind::Response(_)) {
                    self.turn_settled();
                }
                match self.surfaces.iter_mut().find(|s| s.id == id) {
                    Some(existing) if post.op == Op::Update => existing.merge(incoming),
                    Some(existing) => {
                        // A second arrival must not blur who wrote this. Since
                        // `present_surface` persists on its way past, the
                        // watcher re-delivers every verb-sent surface as a
                        // FileDrop seconds later — and FileDrop cannot name a
                        // writer at all. Keeping the more precise origin means
                        // the round trip through disk costs the card nothing;
                        // without it, "presented by this pane's agent" decays
                        // into "writer unknown" on its own.
                        let mut incoming = incoming;
                        if incoming.origin.precision() < existing.origin.precision() {
                            incoming.origin = existing.origin.clone();
                        }
                        // The same surface presented again is not a change.
                        // The derived half re-presents every sweep with a
                        // fresh clock, and taking that as new work reset every
                        // row's age to zero once a second and repainted the
                        // pane to say so. Only the clock is allowed to differ.
                        let mut probe = incoming.clone();
                        probe.arrived_ms = existing.arrived_ms;
                        if probe == *existing {
                            return None;
                        }
                        *existing = incoming;
                    }
                    None => {
                        self.surfaces.push(incoming);
                        // The cap drops the OLDEST, never the newest: a bench
                        // that silently refused new work once it was full
                        // would look exactly like an agent that stopped.
                        if self.surfaces.len() > crate::surface::PANE_HISTORY_CAP {
                            let dropped = self.surfaces.remove(0);
                            self.unseen.remove(&dropped.id);
                            self.forget_picks(&dropped.id);
                            if self.selected.as_ref() == Some(&dropped.id) {
                                self.selected = None;
                            }
                        }
                    }
                }
                let on_screen = self.face == Face::Workbench
                    && self
                        .get(&id)
                        .is_some_and(|s| self.shelf.holds(s.kind.shelf()));
                if on_screen {
                    self.unseen.remove(&id);
                } else {
                    self.unseen.insert(id.clone());
                }
                // NOTHING is selected by an arrival, and no arrival moves the
                // shelf. Both used to happen, and both were wrong for the
                // same reason: the rail is a shelf a person browses, not a
                // remote control for the main area. A live question stealing
                // the shelf meant clicking `artifacts` bounced straight back
                // to `decisions` a second later, which is the surface
                // fighting the hand.
                //
                // What a waiting question DOES get is the conversation: it is
                // drawn inline where the agent is talking, by
                // [`Bench::waiting_question`], because that is where the
                // person is already looking.
                Some(id)
            }
        }
    }

    fn mark_shelf_seen(&mut self) {
        let ids: Vec<SurfaceId> = self
            .surfaces
            .iter()
            .filter(|s| self.shelf.holds(s.kind.shelf()))
            .map(|s| s.id.clone())
            .collect();
        for id in ids {
            self.unseen.remove(&id);
        }
    }

    /// Anything anywhere on this bench the person has not looked at.
    ///
    /// **A probe, not a surface.** The one thing that drew this number was the
    /// bare `2` beside the BENCH toggle, and that badge is gone — see the face
    /// toggle in `pane.rs`. What remains live is the SET it counts: every shelf
    /// row asks `unseen.contains(id)` for its own dot, and `shelf_counts` asks
    /// it per shelf. Those are where the distinction is worth drawing, because
    /// a row says what it is and a digit on the header did not.
    ///
    /// So this stays `cfg(test)`: four tests assert on the marking rules
    /// through it, and gating it means the compiler will say so the day
    /// somebody wants a whole-bench count on a real surface again, rather than
    /// letting a dead aggregate sit around looking supported.
    #[cfg(test)]
    pub fn unseen_total(&self) -> usize {
        self.unseen.len()
    }

    /// Press a button on the selected surface.
    pub fn act(
        &mut self,
        action: &Action,
        target: Option<String>,
        comment: Option<String>,
    ) -> Dispatch {
        // The card if one is open, else whatever the agent is waiting on —
        // because the answer chips are drawn in the conversation, where
        // nothing is "selected" at all.
        let Some(surface) = self.selected().or_else(|| self.waiting_question()).cloned() else {
            return Dispatch::Refused("nothing on this bench to act on".into());
        };
        // A verdict on one hunk is recorded here as well as sent: the person
        // has answered, and a row that still reads "undecided" after they
        // decided is the surface lying about its own state.
        if matches!(action, Action::AcceptPart | Action::RejectPart) {
            let verdict = if matches!(action, Action::AcceptPart) {
                Verdict::Accepted
            } else {
                Verdict::Rejected
            };
            let Some(t) = target.clone() else {
                return Dispatch::Refused("that verb needs a part to act on".into());
            };
            let mut found = false;
            if let Some(live) = self.surfaces.iter_mut().find(|s| s.id == surface.id) {
                if let Kind::Changeset(c) = &mut live.kind {
                    if let Some(h) = c.hunks.iter_mut().find(|h| h.id == t) {
                        h.verdict = verdict;
                        found = true;
                    }
                }
            }
            if !found {
                return Dispatch::Refused(format!("no part called {t} on this surface"));
            }
        }
        // Answering a question drives the agent's own menu. Recorded here as
        // well as sent, for the same reason a hunk verdict is: a row that
        // still reads "waiting on you" after you answered is the surface
        // lying about its own state.
        if *action == Action::Choose {
            let Some(pick) = target.as_ref().and_then(|t| t.parse::<usize>().ok()) else {
                return Dispatch::Refused("choosing needs an option to choose".into());
            };
            let mut keys = None;
            let mut label = String::new();
            if let Some(live) = self.surfaces.iter_mut().find(|s| s.id == surface.id) {
                if let Kind::Question(q) = &mut live.kind {
                    match q.options.get(pick) {
                        None => {
                            return Dispatch::Refused(format!(
                                "this question has no option {}",
                                pick + 1
                            ))
                        }
                        Some(option) => {
                            label = option.label.clone();
                            // A question we merely observed carries a cursor,
                            // and only those can be answered by driving the
                            // menu. A declared one has no menu behind it, so
                            // the answer goes as a sentence instead.
                            keys = q.cursor.map(|at| menu_keys(pick, at));
                            q.answer = crate::surface::Answered::Chose(pick);
                            q.cursor = None;
                        }
                    }
                }
            }
            return match keys {
                Some(bytes) => Dispatch::Keys {
                    bytes,
                    note: format!("chose {label:?}"),
                },
                None => Dispatch::Tell(ActionReport {
                    surface: surface.id.clone(),
                    action: Action::Choose,
                    target: Some(label),
                    comment,
                }),
            };
        }
        // Local first, and asked of the action rather than pattern-matched
        // here: [`Action::is_local`] is the one place that decides which verbs
        // the window performs and which ones wake an agent up, and a second
        // copy of that judgement is how the two would come to disagree.
        if action.is_local() {
            return match action {
                Action::Open => match &surface.kind {
                    Kind::Artifact(a) => Dispatch::Open(a.href.clone()),
                    _ => Dispatch::Refused("this surface is not a document to open".into()),
                },
                Action::OpenSource => match surface.source.as_ref().and_then(|s| s.files.first()) {
                    Some(f) => Dispatch::Open(f.clone()),
                    None => Dispatch::Refused("this surface names no source".into()),
                },
                // Only the kinds that ARE text. A refusal naming what is
                // missing beats copying a rendering of a diagram that nobody
                // would recognise when they pasted it.
                Action::Copy => match &surface.kind {
                    Kind::Comment(c) => Dispatch::Clipboard(c.body.clone()),
                    Kind::Markdown(m) => Dispatch::Clipboard(m.body.clone()),
                    _ => Dispatch::Refused("this surface has no text to copy".into()),
                },
                other => Dispatch::Refused(format!(
                    "{} is marked local but nothing here performs it",
                    other.id()
                )),
            };
        }
        Dispatch::Tell(ActionReport {
            surface: surface.id.clone(),
            action: action.clone(),
            target,
            comment,
        })
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::surface::parse;
    use serde_json::json;

    const NOW: u64 = 1_758_000_000_000;

    fn post(value: serde_json::Value) -> Post {
        parse(&value, NOW).expect("valid")
    }

    fn doc(id: &str, title: &str) -> Post {
        post(json!({
            "td": "0.1", "kind": "markdown", "id": id, "title": title,
            "model": { "body": "body" }
        }))
    }

    fn decision(id: &str) -> Post {
        post(json!({
            "td": "0.1", "kind": "decision", "id": id, "title": "Which way?",
            "model": { "question": "Which way?", "options": [{"name":"A"}] }
        }))
    }

    fn changeset(id: &str) -> Post {
        post(json!({
            "td": "0.1", "kind": "changeset", "id": id, "title": "A change",
            "model": { "hunks": [
                { "id": "h1", "file": "a.rs", "patch": "+one" },
                { "id": "h2", "file": "b.rs", "patch": "-two" }
            ]}
        }))
    }

    fn response(id: &str, plain: &str) -> Post {
        post(json!({
            "td": "0.3", "kind": "response", "id": id, "title": plain,
            "model": {
                "layman": plain,
                "technical": "big words",
                "asks": ["pick one"],
                "doubts": ["maybe"]
            }
        }))
    }

    #[test]
    fn turn_vitals_exist_only_while_working_and_an_unread_screen_says_so() {
        use crate::hud::{AgentState as HudState, AgentStatus};
        let idle = AgentStatus {
            state: HudState::Idle,
            elapsed: Some("3m".into()),
            turn_tokens: Some(8_000),
            ..AgentStatus::default()
        };
        assert_eq!(
            turn_vitals(&idle),
            None,
            "the last turn's numbers are not this turn's"
        );
        let working = AgentStatus {
            state: HudState::Working,
            elapsed: Some("3m 27s".into()),
            turn_tokens: Some(8_000),
            doing: Some("Calling lean-ctx 3 times".into()),
            ..AgentStatus::default()
        };
        let v = turn_vitals(&working).expect("a turn in flight");
        assert_eq!(v.elapsed.as_deref(), Some("3m 27s"));
        assert_eq!(v.tokens, Some(8_000));
        assert!(!v.is_unread());
        let narrow = AgentStatus {
            state: HudState::Working,
            ..AgentStatus::default()
        };
        let v = turn_vitals(&narrow).expect("still a turn");
        assert!(
            v.is_unread(),
            "working, and the screen carried no numbers: unread, not zero"
        );
    }

    #[test]
    fn the_overview_is_the_feed_of_replies_and_holds_nothing_else() {
        // For a day the overview was a view over everything, and the test
        // here said so. Parker: *"the OVERVIEW tab of the workbench will no
        // longer show artifacts or decisions, it will only show the
        // responses."* A document, a decision and a changeset land on the
        // other two shelves; a reply lands here; the counts agree with the
        // rows, because both go through `Shelf::holds`.
        let mut b = Bench::new();
        b.apply(doc("a", "A"));
        b.apply(decision("d"));
        b.apply(changeset("c"));
        assert_eq!(b.counts(Shelf::Overview), (0, 0));
        assert!(b.rows_for(Shelf::Overview).is_empty());
        assert_eq!(b.counts(Shelf::Artifacts), (1, 1));
        assert_eq!(
            b.counts(Shelf::Decisions),
            (2, 2),
            "a changeset is a thing to answer"
        );
        b.apply(response("r", "Done."));
        assert_eq!(b.counts(Shelf::Overview), (1, 1));
        assert_eq!(
            b.rows_for(Shelf::Overview)[0].subtitle,
            "Done.",
            "the gist is the row"
        );
        assert_eq!(
            b.rows_for(Shelf::Overview)[0].badge.as_deref(),
            Some("1 doubt")
        );
        // Looking at the overview marks the replies seen, and only them.
        b.set_shelf(Shelf::Overview);
        b.set_face(Face::Workbench);
        assert_eq!(b.unseen_total(), 3, "the other shelves were not looked at");
    }

    #[test]
    fn the_overview_shows_the_newest_reply_until_a_person_opens_another() {
        let mut b = Bench::new();
        assert!(b.showing().is_none(), "nothing to show");
        b.apply(response("r1", "First."));
        b.apply(response("r2", "Second."));
        assert_eq!(
            b.showing().map(|s| s.id.as_str()),
            Some("r2"),
            "the newest stands"
        );
        assert!(
            b.selected().is_none(),
            "and nothing was selected to get it there"
        );
        b.select(&SurfaceId("r1".into()));
        assert_eq!(
            b.showing().map(|s| s.id.as_str()),
            Some("r1"),
            "an opened card stays"
        );
        b.apply(response("r3", "Third."));
        assert_eq!(
            b.showing().map(|s| s.id.as_str()),
            Some("r1"),
            "an arrival does not take the room from an opened card"
        );
        b.close_card();
        assert_eq!(b.showing().map(|s| s.id.as_str()), Some("r3"));
        b.set_shelf(Shelf::Artifacts);
        assert!(
            b.showing().is_none(),
            "the newest reply stands in on the overview only; another shelf shows its own card or nothing"
        );
    }

    #[test]
    fn the_turn_in_flight_takes_the_room_from_the_previous_turns_reply() {
        // The defect, stated as the test: a reply is presented at the END of a
        // turn, so for the whole length of the next one the newest reply in
        // the store belongs to the turn before it — and the room drew that
        // under the new turn's question. Parker: *"we see the LAST turn
        // persisting -- the CURRENT turn should IMMEDIATELY make a new
        // overview card, and we flip to that"*.
        let mut b = Bench::new();
        b.apply(response("r1", "First."));
        assert_eq!(b.showing().map(|s| s.id.as_str()), Some("r1"));
        assert!(b.live().is_none(), "no turn has begun");

        b.turn_began(Some("what now?".into()), Some(Voice::Person), 100);
        assert!(
            b.showing().is_none(),
            "the previous turn's reply does not share the room with the turn in flight"
        );
        assert!(b.live_standing().is_some(), "the turn is what stands");
        assert!(
            b.standing_in(),
            "and it is a stand-in, so the person's own message is captioned over it"
        );
        assert_eq!(
            ask_lines(b.shelf(), b.standing_in(), true, Embodiment::Full),
            Some(4),
            "the YOU block draws over a live turn exactly as it does over a reply"
        );

        let rows = b.rows_for(Shelf::Overview);
        assert_eq!(rows.len(), 2, "the turn and the reply before it");
        assert_eq!(rows[0].kind, LIVE_TURN_KIND);
        assert_eq!(rows[0].title, "what now?", "the row says what the turn is");
        assert_eq!(
            rows[0].subtitle, "the reply has not landed yet",
            "and the row says what is missing, which is the whole of its news"
        );
        assert_eq!(rows[0].standing, Standing::Current, "it is the head");
        assert!(!rows[0].unseen, "the thing in the room was never missed");
        assert_eq!(
            b.counts(Shelf::Overview).0,
            rows.len(),
            "the tab's number and the rail's rows are one fact"
        );

        // The reply landing is what ends it, and then the reply stands.
        b.apply(response("r2", "Done."));
        b.turn_settled();
        assert_eq!(b.showing().map(|s| s.id.as_str()), Some("r2"));
        assert_eq!(b.rows_for(Shelf::Overview).len(), 2, "two replies, no turn");
        assert_eq!(b.counts(Shelf::Overview).0, 2);
    }

    #[test]
    fn a_turn_that_ended_presenting_nothing_keeps_its_card_and_says_so() {
        // `turn_settled` is called by a reply ARRIVING, never by the agent
        // going idle — so an interrupted turn keeps the head of the feed, and
        // what changes is the words. The alternative is retiring on idle,
        // which puts the previous turn's answer back in the room for the gap
        // between a turn ending and its reply landing: this defect, smaller.
        let mut b = Bench::new();
        b.apply(response("r1", "First."));
        b.turn_began(Some("stop".into()), Some(Voice::Person), 1);
        assert!(b.showing().is_none());
        // Nothing arrived. The card is still the turn's.
        assert!(b.live_standing().is_some());
        assert_eq!(live_says(AgentState::Working).0, "IN FLIGHT");
        assert_ne!(
            live_says(AgentState::Idle),
            live_says(AgentState::Working),
            "a turn that ended must not read as one still running"
        );

        // THE AT-REST ROWS MAY NOT PREDICT. `Done` is raised off the bell, on
        // the pane's own 120ms clock; the reply comes from the harness's stop
        // hook on its own schedule. They race on every turn, so a sentence
        // claiming nothing is coming is drawn — briefly, but on ALL of them —
        // while the reply is still in flight. Say what happened; leave what
        // will happen to the reply arriving or not arriving.
        for state in [AgentState::Idle, AgentState::Done, AgentState::Exited] {
            let (label, sentence) = live_says(state);
            assert!(
                !sentence.contains("without presenting") && !label.contains("NOTHING"),
                "{state:?} is predicting the future during the gap before a reply lands: \
                 {label} / {sentence}"
            );
        }
        assert_eq!(
            live_says(AgentState::Idle),
            ("THE TURN ENDED", "no reply has landed on the bench"),
            "both halves are observable right now, and stay true if nothing ever comes"
        );
        assert_eq!(
            live_says(AgentState::Exited).0,
            "AGENT GONE",
            "the one at-rest row allowed to be final: the process is gone"
        );
        for state in [
            AgentState::Asking,
            AgentState::Blocked,
            AgentState::Done,
            AgentState::Exited,
            AgentState::Idle,
            AgentState::Paused,
            AgentState::Reading,
            AgentState::Working,
        ] {
            let (l, s) = live_says(state);
            assert!(!l.is_empty() && !s.is_empty(), "{state:?} says nothing");
        }
    }

    #[test]
    fn whose_voice_opened_the_turn_decides_whether_an_opened_card_is_taken() {
        let mut b = Bench::new();
        b.apply(response("r1", "First."));
        b.apply(response("r2", "Second."));
        b.select(&SurfaceId("r1".into()));

        b.turn_began(Some("a task finished".into()), Some(Voice::Harness), 1);
        assert_eq!(
            b.showing().map(|s| s.id.as_str()),
            Some("r1"),
            "a wake-up did not move the person, so it does not move their card"
        );
        assert!(
            b.live_standing().is_none(),
            "it waits at the head of the rail"
        );

        b.turn_began(None, None, 2);
        assert_eq!(
            b.showing().map(|s| s.id.as_str()),
            Some("r1"),
            "and neither does a turn whose voice is unknown"
        );

        b.turn_began(Some("what now?".into()), Some(Voice::Person), 3);
        assert!(
            b.selected().is_none() && b.live_standing().is_some(),
            "they typed, so they moved themselves: the room follows"
        );
    }

    #[test]
    fn the_turn_in_flight_is_not_a_document() {
        let mut b = Bench::new();
        b.apply(response("r1", "First."));
        b.turn_began(Some("what now?".into()), Some(Voice::Person), 1);
        let live_id = b.live().expect("a turn").id.clone();

        b.select(&SurfaceId("r1".into()));
        assert!(b.selected().is_some(), "a reply opens");
        b.select(&live_id);
        assert!(
            b.selected().is_none() && b.live_standing().is_some(),
            "pressing the turn's row goes back to the turn, it does not open a card"
        );

        // The keyboard reaches it and lands in the same place.
        b.select(&SurfaceId("r1".into()));
        b.step(-1);
        assert!(
            b.selected().is_none(),
            "stepping up onto the turn closes the card"
        );

        // Coming back to this shelf lands on the turn, not on the answer to the
        // turn before it. `set_shelf` opens the head row — and the head row is
        // the live turn, which opens nothing.
        b.select(&SurfaceId("r1".into()));
        b.set_shelf(Shelf::Artifacts);
        b.set_shelf(Shelf::Overview);
        assert!(
            b.selected.is_none(),
            "the head of the overview is the turn, and a turn is not a card to open"
        );
        assert!(
            b.live_standing().is_some(),
            "so the turn is what a returning reader sees"
        );

        // With no turn in flight the old behaviour is untouched: the head is a
        // reply and the shelf opens it.
        b.turn_settled();
        b.set_shelf(Shelf::Artifacts);
        b.set_shelf(Shelf::Overview);
        assert_eq!(b.selected().map(|s| s.id.as_str()), Some("r1"));
    }

    #[test]
    fn the_spinner_fills_a_gap_and_never_overwrites_the_record() {
        // The fallback for a pane whose harness has no hooks, and for a turn
        // typed at the terminal face of a pane that does.
        let mut b = Bench::new();
        b.turn_seen_working(Some("scraped off the screen".into()), 1);
        let t = b.live().expect("the spinner opened one");
        assert_eq!(t.headline.as_deref(), Some("scraped off the screen"));
        assert_eq!(
            t.voice, None,
            "a screen cannot tell a person typing from a notification pasted in"
        );

        let mut b = Bench::new();
        b.turn_began(Some("their exact words".into()), Some(Voice::Person), 1);
        b.turn_seen_working(Some("scraped off the screen".into()), 2);
        let t = b.live().expect("still the first one");
        assert_eq!(
            t.headline.as_deref(),
            Some("their exact words"),
            "the record knows more than the edge and is not overwritten by it"
        );
        assert_eq!(t.voice, Some(Voice::Person));

        // An empty headline is an absent one, not a blank row.
        let mut b = Bench::new();
        b.turn_began(Some("   ".into()), None, 1);
        assert_eq!(b.live().expect("a turn").headline, None);
        assert!(
            b.rows_for(Shelf::Overview)[0]
                .title
                .contains("nothing recorded"),
            "and the row says the opener is unknown rather than showing a blank"
        );
    }

    #[test]
    fn a_prompt_record_says_who_opened_the_turn_or_says_it_cannot_tell() {
        use crate::channel::{Effect, Woken};
        let (headline, voice) = turn_opening(&Effect::Asked {
            text: "fix the card\nand then the rail".into(),
        });
        assert_eq!(
            headline.as_deref(),
            Some("fix the card"),
            "the opening line, not the whole message"
        );
        assert_eq!(voice, Some(Voice::Person));

        let (headline, voice) = turn_opening(&Effect::Woken(Woken::Task {
            summary: Some("the gate went green".into()),
        }));
        assert!(
            headline.as_deref().is_some_and(|h| h.contains("WOKEN")),
            "a wake-up is drawn as itself: {headline:?}"
        );
        assert_eq!(voice, Some(Voice::Harness));

        // The case that is neither, and the reason `Voice` is an `Option`: the
        // harness handed the hook a prompt with no words in it. A turn began
        // and this window cannot say whose.
        let (headline, voice) = turn_opening(&Effect::Nothing);
        assert_eq!((headline, voice), (None, None));
    }

    #[test]
    fn a_reply_arriving_retires_the_turn_however_it_arrived() {
        // On `apply`, not beside one caller of it. A reply reaches a bench by
        // the MCP verb, the hook's own copy, a file dropped in the pane's
        // directory and a replay off disk — and a retirement written next to
        // any one road leaves the other three standing a finished turn over
        // its own answer.
        let mut b = Bench::new();
        b.turn_began(Some("go".into()), Some(Voice::Person), 1);
        b.apply(doc("d1", "A drawing"));
        assert!(b.live().is_some(), "an artifact is not a reply");
        b.apply(decision("k1"));
        assert!(b.live().is_some(), "and neither is a decision");
        b.apply(response("r1", "Done."));
        assert!(b.live().is_none(), "the reply landed; the turn is over");
        assert_eq!(b.showing().map(|s| s.id.as_str()), Some("r1"));
    }

    #[test]
    fn an_agent_that_left_is_not_four_minutes_into_anything() {
        let mut b = Bench::new();
        b.apply(response("r1", "First."));
        b.turn_began(Some("what now?".into()), Some(Voice::Person), 1);
        b.clear_surfaces();
        assert!(b.live().is_none(), "the turn went with the conversation");
        assert!(b.rows_for(Shelf::Overview).is_empty());
        assert_eq!(b.counts(Shelf::Overview), (0, 0));
    }

    #[test]
    fn a_message_that_ran_on_says_it_ran_on() {
        let three = || vec!["one".to_string(), "two".to_string(), "three".to_string()];
        assert_eq!(ask_clipped(three(), 3), three(), "it all fitted: no mark");
        assert_eq!(ask_clipped(three(), 4), three(), "room to spare: no mark");
        assert_eq!(
            ask_clipped(three(), 2),
            vec!["one".to_string(), "two\u{2026}".to_string()],
            "the cut is on the last line drawn, where the reader is looking",
        );
        assert!(ask_clipped(Vec::new(), 2).is_empty(), "nothing to mark");
    }

    #[test]
    fn the_overview_captions_the_standing_reply_with_what_you_asked() {
        // The block is drawn for the reply that is STANDING IN, on the
        // overview, in a pane with room — and for nothing else. Every other
        // row here is a case where the latest thing the person said is not
        // what the thing in the room is answering.
        let mut b = Bench::new();
        b.apply(response("r1", "First."));
        b.apply(response("r2", "Second."));
        assert!(
            b.standing_in(),
            "the newest reply stands with nobody opening it"
        );
        assert_eq!(
            ask_lines(b.shelf(), b.standing_in(), true, Embodiment::Full),
            Some(4),
        );
        assert_eq!(
            ask_lines(b.shelf(), b.standing_in(), true, Embodiment::Compact),
            Some(2),
            "a compact pane gets the opening of it rather than nothing",
        );
        assert_eq!(
            ask_lines(b.shelf(), b.standing_in(), true, Embodiment::Summary),
            None,
            "a pane too small to read a paragraph in shows the reply alone",
        );
        assert_eq!(
            ask_lines(b.shelf(), b.standing_in(), false, Embodiment::Full),
            None,
            "a shell's prompt is not a message somebody sent",
        );
        // Opened from the rail: an older reply, captioned with the newest
        // question, would be the window inventing a pairing.
        b.select(&SurfaceId("r1".into()));
        assert!(!b.standing_in(), "an opened card is not the stand-in");
        assert_eq!(
            ask_lines(b.shelf(), b.standing_in(), true, Embodiment::Full),
            None,
        );
        b.close_card();
        // Another shelf: a document is not an answer to anything said.
        b.set_shelf(Shelf::Artifacts);
        assert_eq!(
            ask_lines(b.shelf(), b.standing_in(), true, Embodiment::Full),
            None,
        );
        // And an overview with nothing in it: the main area is the mirrored
        // conversation, which carries the person's own turns already.
        let empty = Bench::new();
        assert!(!empty.standing_in());
        assert_eq!(
            ask_lines(empty.shelf(), empty.standing_in(), true, Embodiment::Full),
            None,
        );
    }

    /// The card every demo and every screenshot starts from: the seeded reply,
    /// untouched, opening on its reading group with four registers under it.
    ///
    /// Written because a photograph of the live card showed the `next` tab open
    /// on a window nobody had pressed — which turned out to be a stray click
    /// from the desk the demo had borrowed, not a wrong default. A picture of a
    /// surface mid-interaction looks exactly like a picture of a surface with a
    /// broken default, and only an assertion tells the two apart.
    #[test]
    fn the_seeded_reply_opens_on_its_reading_group_with_nobody_having_pressed() {
        use crate::surface::Group;
        let mut b = Bench::new();
        for (_, doc) in crate::surfacefeed::demo_surfaces() {
            if let Ok(p) = crate::surface::parse(&doc, NOW) {
                b.apply(p);
            }
        }
        let showing = b.showing().expect("the demo shows a reply").clone();
        let Kind::Response(r) = &showing.kind else {
            panic!("the overview stands in with the newest response");
        };
        let tabs = tabbed(r, false);
        assert_eq!(
            tabs.iter().map(|(g, l)| (*g, l.len())).collect::<Vec<_>>(),
            vec![(Group::Reading, 3), (Group::Evidence, 2), (Group::Next, 1)],
            "three readings: the brief, the plain reply, the technical one"
        );
        assert_eq!(b.picked_tab(&showing.id), None, "nobody has pressed");
        assert_eq!(
            resolve_tab(b.picked_tab(&showing.id), &tabs),
            Some(Group::Reading),
            "so it opens on the reading group"
        );
        assert_eq!(
            resolve_leaf(None, &tabs[0].1).map(|l| l.key()),
            Some("layman"),
            "…on the plain brief, which a brief in the same group does not displace"
        );
    }

    #[test]
    fn nothing_is_picked_until_the_reader_picks_it_and_a_pick_is_per_tab() {
        use crate::surface::Group;
        let mut b = Bench::new();
        b.apply(response("r", "Gist."));
        let id = SurfaceId("r".into());

        // ABSENT, not "the first one". The renderer collapses absent to the
        // first group at draw time; the map may not store that collapse, or
        // nothing downstream could tell a choice from a default.
        assert_eq!(b.picked_tab(&id), None);
        assert_eq!(b.picked_register(&id, Group::Reading), None);

        b.pick_register(&id, "technical");
        assert_eq!(
            b.picked_register(&id, Group::Reading).as_deref(),
            Some("technical")
        );
        assert_eq!(
            b.picked_tab(&id),
            Some(Group::Reading),
            "pressing a chip opens the tab it is in, so the two cannot disagree later"
        );

        // A pick is keyed by the PAIR: going to another tab and back brings
        // back what you were reading, rather than resetting to its first chip.
        b.pick_tab(&id, Group::Evidence);
        assert_eq!(b.picked_tab(&id), Some(Group::Evidence));
        assert_eq!(
            b.picked_register(&id, Group::Evidence),
            None,
            "a tab nobody has read inside has no register picked"
        );
        assert_eq!(
            b.picked_register(&id, Group::Reading).as_deref(),
            Some("technical"),
            "and the reading tab still remembers"
        );

        // The doubts are not a section, and a press on them still lands in
        // evidence rather than falling through to `other`.
        b.pick_register(&id, "doubts");
        assert_eq!(b.picked_tab(&id), Some(Group::Evidence));
        assert_eq!(
            b.picked_register(&id, Group::Evidence).as_deref(),
            Some("doubts")
        );

        // Retiring forgets both, so an id reused later starts fresh rather than
        // inheriting a stranger's reading position.
        b.apply(post(json!({ "td": "0.3", "op": "retire", "id": "r" })));
        b.apply(response("r", "Gist again."));
        assert_eq!(b.picked_tab(&id), None);
        assert_eq!(b.picked_register(&id, Group::Evidence), None);
    }

    /// The suppression rules, which are the ones that would ship broken: a
    /// restyle justified by saving space on a six-register card has to be
    /// checked against the one-register card, which is what most replies are.
    #[test]
    fn a_reply_only_gets_the_chrome_it_needs_and_the_commonest_reply_gets_none() {
        use crate::surface::Group;
        let mut b = Bench::new();
        // The commonest reply there is: a plain brief and nothing else.
        b.apply(post(json!({
            "td": "0.3", "kind": "response", "id": "a", "title": "t",
            "model": { "layman": "Just the brief." }
        })));
        b.apply(response("b", "Gist."));
        let tabs_of =
            |b: &Bench, id: &str, promoted: bool| match &b.get(&SurfaceId(id.into())).unwrap().kind
            {
                Kind::Response(r) => tabbed(r, promoted)
                    .into_iter()
                    .map(|(g, l)| (g, l.iter().map(|x| x.key().to_string()).collect::<Vec<_>>()))
                    .collect::<Vec<_>>(),
                _ => unreachable!(),
            };
        let only_gist = tabs_of(&b, "a", false);
        assert_eq!(only_gist.len(), 1, "one group");
        assert_eq!(only_gist[0].0, Group::Reading);
        assert_eq!(only_gist[0].1.len(), 1, "holding one leaf");
        // …and therefore no chrome at all on the commonest reply there is.
        let raw_gist = match &b.get(&SurfaceId("a".into())).unwrap().kind {
            Kind::Response(r) => tabbed(r, false),
            _ => unreachable!(),
        };
        assert!(!draws_strip(&raw_gist), "a strip of one tab is not drawn");
        assert!(
            !draws_chips(&raw_gist[0].1),
            "nor a chip row naming the only thing on screen"
        );

        // The full house: reading, evidence (the doubts), next.
        let full = tabs_of(&b, "b", false);
        let groups: Vec<Group> = full.iter().map(|(g, _)| *g).collect();
        assert_eq!(groups, vec![Group::Reading, Group::Evidence, Group::Next]);
        assert!(
            !groups.contains(&Group::Other),
            "the other tab appears only when it holds something"
        );
        assert_eq!(full[0].1[0], "layman", "the plain brief leads its group");
        assert!(full[0].1.iter().any(|k| k == "technical"));
        assert!(
            full[1].1.iter().any(|k| k == "doubts"),
            "the doubts are evidence"
        );

        // A promoted escalation takes the asks OUT: the summons above the card
        // already printed them, and printing them twice is the bug this file
        // has shipped twice.
        let promoted = tabs_of(&b, "b", true);
        assert!(
            !promoted.iter().flat_map(|(_, l)| l).any(|k| k == "asks"),
            "the asks left with the escalation"
        );

        // Resolution: absent picks land on the first of each, and a pick for a
        // group that is not there any more falls back rather than drawing an
        // empty card.
        let bench_tabs = match &b.get(&SurfaceId("b".into())).unwrap().kind {
            Kind::Response(r) => tabbed(r, false),
            _ => unreachable!(),
        };
        let reading = &bench_tabs[0].1;
        assert!(
            draws_strip(&bench_tabs) && draws_chips(reading),
            "a reply with three groups and two readings gets both rows"
        );
        assert!(
            !draws_chips(&bench_tabs[1].1),
            "…but a tab holding one thing still draws no chip row"
        );
        assert_eq!(resolve_tab(None, &bench_tabs), Some(Group::Reading));
        assert_eq!(
            resolve_tab(Some(Group::Other), &bench_tabs),
            Some(Group::Reading)
        );
        assert_eq!(
            resolve_tab(Some(Group::Next), &bench_tabs),
            Some(Group::Next)
        );
        assert_eq!(resolve_leaf(None, reading).map(|l| l.key()), Some("layman"));
        assert_eq!(
            resolve_leaf(Some("technical"), reading).map(|l| l.key()),
            Some("technical"),
            "the technical brief stays, one chip away"
        );
        assert!(
            !reading
                .iter()
                .any(|l| l.key() == "tldr" || l.key() == "eli5"),
            "the tl;dr and the ELI5 are not readings any more"
        );
        assert_eq!(
            resolve_leaf(Some("gone"), reading).map(|l| l.key()),
            Some("layman"),
            "a key that is not there any more falls back to the first"
        );
    }

    /// The brief comes LAST in the readings and moves no default.
    ///
    /// The whole risk in adding a reading is that it takes the slot the card
    /// opens on, and a first cut of this one did: *"it should be ordered last!
    /// Plain Brief is still the default."* Since `resolve_leaf` takes the first
    /// leaf when nobody has picked, position and default are one fact — so this
    /// asserts both the order and what an unpicked card resolves to, on a reply
    /// that has a brief AND on one that does not.
    #[test]
    fn the_brief_comes_last_in_the_readings_and_the_plain_one_stays_the_default() {
        use crate::surface::Group;
        let mut b = Bench::new();
        let plain = "The effort row offered invented words; it now passes the harness's own flag.";
        b.apply(post(json!({
            "td": "0.4", "kind": "response", "id": "with", "title": "t",
            "model": {
                "brief": "The launcher's effort setting is a real flag now. Install the build to use it.",
                "layman": plain,
                "technical": "launcher::Effort is the union of the harnesses' own levels."
            }
        })));
        b.apply(post(json!({
            "td": "0.4", "kind": "response", "id": "without", "title": "t",
            "model": {
                "layman": plain,
                "technical": "launcher::Effort is the union of the harnesses' own levels."
            }
        })));
        let readings = |id: &str| match &b.get(&SurfaceId(id.into())).unwrap().kind {
            Kind::Response(r) => {
                tabbed(r, false)
                    .into_iter()
                    .find(|(g, _)| *g == Group::Reading)
                    .expect("every reply has a reading")
                    .1
            }
            _ => unreachable!(),
        };

        let with = readings("with");
        assert_eq!(
            with.iter().map(|l| l.key()).collect::<Vec<_>>(),
            ["layman", "technical", "brief"],
            "the brief is the rung you drop to, so it sits at the end"
        );
        assert_eq!(
            resolve_leaf(None, &with).map(|l| l.key()),
            Some("layman"),
            "and an unpicked card opens where it always has"
        );
        assert_eq!(
            with.iter().map(|l| l.label()).collect::<Vec<_>>(),
            ["Plain brief", "Technical brief", "Brief"],
        );
        // Reachable, which is the whole point of it being a chip.
        assert_eq!(
            resolve_leaf(Some("brief"), &with).map(|l| l.key()),
            Some("brief")
        );

        let without = readings("without");
        assert_eq!(
            without.iter().map(|l| l.key()).collect::<Vec<_>>(),
            ["layman", "technical"],
            "no brief, no chip for one"
        );
        assert_eq!(
            resolve_leaf(None, &without).map(|l| l.key()),
            Some("layman"),
            "and a reply that sent no brief is untouched in every respect"
        );
    }

    #[test]
    fn an_arrival_opens_nothing_and_moves_no_shelf() {
        // The rail is a shelf a person browses, not a remote control for the
        // main area. Parker found this the hard way: a live question arriving
        // every second stole the shelf, so clicking `artifacts` bounced
        // straight back to `decisions` — the surface fighting the hand.
        let mut b = Bench::new();
        b.set_shelf(Shelf::Artifacts);
        b.apply(decision("d1"));
        assert_eq!(
            b.shelf(),
            Shelf::Artifacts,
            "the shelf stays where it was put"
        );
        assert!(b.selected().is_none(), "nothing opens itself");
    }

    #[test]
    fn a_waiting_question_is_found_without_being_selected() {
        // It is drawn in the conversation instead, where the person is
        // already looking — so it needs to be findable, not selected.
        let mut b = Bench::new();
        b.apply(doc("a", "A document"));
        b.apply(question("q", Some(0)));
        assert!(b.selected().is_none());
        assert_eq!(b.waiting_question().map(|s| s.id.as_str()), Some("q"));
    }

    #[test]
    fn an_answered_question_is_no_longer_waiting() {
        let mut b = Bench::new();
        b.apply(question("q", Some(0)));
        b.act(&Action::Choose, Some("0".into()), None);
        assert!(
            b.waiting_question().is_none(),
            "answered is not waiting, or the conversation keeps asking"
        );
    }

    #[test]
    fn a_card_opens_on_a_click_and_closes_back_to_the_conversation() {
        let mut b = Bench::new();
        b.apply(doc("a", "A document"));
        assert!(b.selected().is_none(), "the conversation is the default");
        b.select(&SurfaceId("a".into()));
        assert_eq!(b.selected().map(|s| s.id.as_str()), Some("a"));
        assert!(b.close_card());
        assert!(b.selected().is_none());
        assert!(!b.close_card(), "closing a closed card is not an event");
    }

    #[test]
    fn answering_works_from_the_conversation_with_nothing_selected() {
        // The chips live in the conversation, where nothing is selected at
        // all — so `act` has to find the waiting question by itself.
        let mut b = Bench::new();
        b.apply(question("q", Some(0)));
        assert!(b.selected().is_none());
        match b.act(&Action::Choose, Some("1".into()), None) {
            Dispatch::Keys { bytes, .. } => assert_eq!(bytes, b"\x1b[B\r"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn presenting_the_same_id_twice_replaces_rather_than_stacks() {
        let mut b = Bench::new();
        b.apply(doc("same", "First"));
        b.apply(doc("same", "Second"));
        assert_eq!(b.all_newest_first().count(), 1);
        assert_eq!(b.rows_for(Shelf::Artifacts)[0].title, "Second");
    }

    /// The departed edge, as a property of the type rather than of the pane.
    ///
    /// Two halves, and they pull opposite ways on purpose. Everything keyed by
    /// a `SurfaceId` goes, because those ids no longer name anything — a
    /// selection pointing at a surface that is gone is how a bench ends up
    /// drawing a card nobody can dismiss. Everything the READER chose about the
    /// pane stays, because an agent leaving is not a reason to move somebody to
    /// a different face.
    #[test]
    fn clearing_a_bench_empties_it_and_keeps_what_the_reader_chose() {
        let mut b = Bench::new();
        b.apply(doc("one", "First"));
        b.apply(doc("two", "Second"));
        b.set_face(Face::Workbench);
        b.set_shelf(Shelf::Artifacts);
        let chosen = b.rows_for(Shelf::Artifacts)[0].id.clone();
        b.select(&chosen);
        b.pick_register(&chosen, "technical");
        assert!(!b.is_empty(), "the fixture has to start non-empty");

        b.clear_surfaces();

        assert!(b.is_empty(), "the surfaces are gone");
        assert_eq!(b.all_newest_first().count(), 0);
        assert!(
            b.selected().is_none(),
            "a selection cannot outlive its surface"
        );
        assert!(
            b.picked_tab(&chosen).is_none(),
            "per-surface state is keyed by an id that no longer names anything"
        );
        // Read through the fields directly, not through accessors. `unseen` and
        // `reg` have no public reader, so an accessor-only assertion would call
        // this clean while either still held the departed conversation's ids —
        // and `unseen` is the one that would show, as a badge counting surfaces
        // nobody can open.
        assert!(
            b.unseen.is_empty(),
            "unseen marks name surfaces that are gone"
        );
        assert!(
            b.reg.is_empty(),
            "register choices are keyed by the same ids"
        );
        assert!(b.tab.is_empty(), "and so are tab choices");
        assert!(b.surfaces.is_empty(), "the store behind is_empty()");
        assert_eq!(
            b.face(),
            Face::Workbench,
            "the face is the reader's, not the agent's"
        );
        assert_eq!(b.shelf(), Shelf::Artifacts, "and so is the shelf");
        assert!(
            b.rail_wanted(),
            "the rail preference is the reader's too — and its default is true, \
             so a clear that reset it would look correct on a fresh bench and \
             only show on one where the reader had folded the rail"
        );
    }

    /// Clearing a bench that is already clear changes nothing.
    ///
    /// Worth a test rather than an assumption because the caller is an EDGE.
    /// `set_mode` returns early when the mode is unchanged today, so the
    /// departed edge fires once — but "once" is a property of a caller three
    /// files away, and the pane's mode is derived from two sensors that can
    /// disagree (the host's classification, and the kernel through our own
    /// descriptor). If a flap ever makes it fire twice, the second call must be
    /// free rather than destructive.
    #[test]
    fn clearing_a_bench_twice_is_the_same_as_clearing_it_once() {
        let mut b = Bench::new();
        b.apply(doc("one", "First"));
        b.set_face(Face::Workbench);
        b.clear_surfaces();
        let once = format!("{b:?}");
        b.clear_surfaces();
        assert_eq!(once, format!("{b:?}"), "the second clear took something");
    }

    /// A clear must not be reachable from anything except an agent leaving, and
    /// the bench must still work afterwards.
    ///
    /// The case this pins is the pane that gets its agent BACK. A cleared bench
    /// is not a dead bench: the next conversation presents into the same
    /// `Bench`, and if a clear left `selected` pointing at nothing while the
    /// rail repopulated, the first surface of the new conversation would arrive
    /// into a bench that believes something else is open.
    #[test]
    fn a_cleared_bench_accepts_the_next_conversations_surfaces() {
        let mut b = Bench::new();
        b.apply(doc("old", "The agent that left"));
        b.clear_surfaces();

        b.apply(doc("new", "The agent that arrived"));

        assert_eq!(b.all_newest_first().count(), 1);
        assert_eq!(
            b.rows_for(Shelf::Artifacts)[0].title,
            "The agent that arrived"
        );
        assert!(
            b.showing().is_none()
                || b.showing().map(|s| s.title.as_str()) != Some("The agent that left"),
            "nothing from the departed conversation is still standing"
        );
    }

    /// A surface presented over MCP is now written to disk on its way past, so
    /// the watcher reads the file back and re-delivers the same id as a
    /// `FileDrop` moments later. That second arrival knows strictly less — the
    /// file transport cannot name a writer at all — and it must not be allowed
    /// to overwrite what the verb established.
    ///
    /// Without the precision rule this passes silently in the wrong direction:
    /// the card keeps its title and its body and quietly stops saying whose
    /// agent wrote it, which is the one thing `Origin` exists to carry.
    #[test]
    fn a_file_rearrival_does_not_blur_who_presented_it() {
        let mut b = Bench::new();

        let mut first = doc("carried", "By the verb");
        let named = crate::surface::Origin::Mcp {
            pid: 4242,
            own: Some(false),
        };
        first.surface.as_mut().unwrap().origin = named.clone();
        b.apply(first);

        // the same document, read back off the disk it was just written to
        let mut echo = doc("carried", "By the verb");
        echo.surface.as_mut().unwrap().origin = crate::surface::Origin::FileDrop;
        let changed = b.apply(echo);

        assert!(
            changed.is_none(),
            "a round trip through disk is not new work and must not repaint"
        );
        let row = b.surfaces.iter().find(|s| s.id.as_str() == "carried");
        assert_eq!(
            row.map(|s| s.origin.clone()),
            Some(named),
            "the file echo overwrote a more precise origin"
        );

        // …and the rule is one-way: a genuinely better origin still wins.
        let mut upgraded = doc("carried", "By the verb");
        let own = crate::surface::Origin::Mcp {
            pid: 4242,
            own: Some(true),
        };
        upgraded.surface.as_mut().unwrap().origin = own.clone();
        b.apply(upgraded);
        assert_eq!(
            b.surfaces
                .iter()
                .find(|s| s.id.as_str() == "carried")
                .map(|s| s.origin.clone()),
            Some(own),
            "a more precise origin must still be able to replace a weaker one"
        );
    }

    /// The derived half re-presents whatever it read from the transcript on
    /// every sweep. A present that changes nothing must answer `None`, or every
    /// agent pane repaints once a second for as long as it lives — and a
    /// surface the person has already looked at must not go back to unseen.
    #[test]
    fn an_identical_re_present_is_not_a_change() {
        let mut b = Bench::new();
        assert!(
            b.apply(doc("same", "First")).is_some(),
            "the first arrival is a change"
        );
        b.mark_shelf_seen();
        let seen = b.unseen_total();
        assert!(b.apply(doc("same", "First")).is_none(), "nothing changed");
        assert_eq!(
            b.unseen_total(),
            seen,
            "an identical re-present is not news"
        );
        assert!(
            b.apply(doc("same", "Second")).is_some(),
            "a different title is a change"
        );
    }

    #[test]
    fn reading_your_answer_outranks_waiting_on_you_and_nothing_else() {
        // (asking, blocked, done, exited, thinking, reading) → state
        let rows = [
            // the window after a press: picker still up, agent not moving
            (
                (true, false, false, false, false, true),
                AgentState::Reading,
            ),
            // the agent picked it up
            (
                (false, false, false, false, true, true),
                AgentState::Working,
            ),
            // an answer typed into a blocked or dead pane reads nothing —
            // and asking still outranks blocked, exactly as it did before
            (
                (false, true, false, false, false, true),
                AgentState::Blocked,
            ),
            ((true, true, false, false, false, false), AgentState::Asking),
            ((false, false, false, true, false, true), AgentState::Exited),
            // the old ladder, untouched when nothing was typed
            (
                (true, false, false, false, false, false),
                AgentState::Asking,
            ),
            ((false, false, true, false, false, false), AgentState::Done),
            (
                (false, false, false, false, true, false),
                AgentState::Working,
            ),
            ((false, false, false, false, false, false), AgentState::Idle),
        ];
        for ((a, b, d, e, t, r), want) in rows {
            assert_eq!(
                agent_state(a, b, d, e, t, r, false),
                want,
                "{a} {b} {d} {e} {t} {r}"
            );
        }
        assert_eq!(AgentState::Reading.tint(), Tint::Pending);
        assert!(
            !AgentState::Reading.urgent(),
            "an answer being read is not a demand"
        );
    }

    /// Each of these rows is a rung the pause had to clear to be true at all,
    /// and each of them is a state the OLD ladder would have reported instead.
    #[test]
    fn a_turn_a_person_stopped_outranks_every_sensor_but_the_two_that_deny_it() {
        // (asking, blocked, done, exited, thinking, reading) with paused set
        let rows = [
            // The interrupt is a write, so it stamps the reading clock. Its
            // own keystroke must not be what the bar describes.
            (
                (false, false, false, false, false, true),
                AgentState::Paused,
                "a pause is not the bench reading itself",
            ),
            // The harness rang its bell on the way back to the prompt. This
            // is the row Parker's rule is about: a finished turn offers no
            // resume and a stopped one must.
            (
                (false, false, true, false, false, false),
                AgentState::Paused,
                "a bell after an interrupt is not a finished turn",
            ),
            // A question left on the bench from before the interrupt.
            (
                (true, false, false, false, false, false),
                AgentState::Paused,
                "a stale picker does not outrank the stop",
            ),
            (
                (false, true, false, false, false, false),
                AgentState::Paused,
                "the stop is what happened most recently",
            ),
            (
                (false, false, false, false, false, false),
                AgentState::Paused,
                "the plain case",
            ),
            // ...and the two that deny it. Neither is a preference: a turn
            // that is running was not stopped, and a pane with no process in
            // it has nothing to resume.
            (
                (false, false, false, false, true, false),
                AgentState::Working,
                "an interrupt that did not take is not a pause",
            ),
            (
                (false, false, false, true, false, false),
                AgentState::Exited,
                "there is nothing to resume in an empty pane",
            ),
        ];
        for ((a, b, d, e, t, r), want, why) in rows {
            assert_eq!(agent_state(a, b, d, e, t, r, true), want, "{why}");
        }
        // And with the latch down, every one of those rows reads what it
        // always read — the rung is additive, not a reshuffle.
        assert_eq!(
            agent_state(false, false, true, false, false, false, false),
            AgentState::Done
        );
        assert_eq!(
            agent_state(false, false, false, false, false, true, false),
            AgentState::Reading
        );
    }

    /// The control is hidden far more often than it is drawn, and the states
    /// where it is hidden are the point of the feature.
    #[test]
    fn the_turn_control_is_drawn_only_on_a_turn_that_is_running_or_stopped() {
        use AgentState::*;
        assert_eq!(turn_control(Working, true), Some(TurnControl::Pause));
        assert_eq!(
            turn_control(Reading, true),
            Some(TurnControl::Pause),
            "the window you notice the wrong model in"
        );
        assert_eq!(
            turn_control(Paused, true),
            Some(TurnControl::Resume),
            "the only state that may be resumed"
        );
        for quiet in [Asking, Blocked, Done, Exited, Idle] {
            assert_eq!(
                turn_control(quiet, true),
                None,
                "{quiet:?} has no turn to pause and none to resume"
            );
        }
        // The two are never both on the strip, in any state at all.
        for st in [
            Asking, Blocked, Done, Exited, Working, Reading, Paused, Idle,
        ] {
            assert!(
                turn_control(st, false).is_none(),
                "{st:?} with no agent in the pane offers nothing"
            );
        }
        // Pressing pause while paused is not reachable, because the state
        // that offers resume is the state the pause left behind.
        assert_ne!(turn_control(Paused, true), Some(TurnControl::Pause));
    }

    /// A paused pane is the one stopped state whose dials were dead, and
    /// making them live is the whole reason to stop.
    #[test]
    fn pausing_is_what_makes_a_dial_reachable_mid_turn() {
        assert!(!dials_live(AgentState::Working), "the case being escaped");
        assert!(
            dials_live(AgentState::Paused),
            "a pause that did not free the dials would have bought nothing"
        );
        assert_eq!(
            strip_verb(AgentState::Paused, true),
            StripVerb::End,
            "pausing a turn does not change what ends the session"
        );
    }

    /// The pin exists to stop a question being scrolled away from. It has
    /// nothing to protect when the question is the card on screen.
    #[test]
    fn a_question_opened_from_the_rail_is_not_also_pinned_under_itself() {
        let open = SurfaceId("q-1".into());
        let other = SurfaceId("q-2".into());
        assert!(
            !draws_waiting_block(Some(&open), &open),
            "the open card is the question; pinning a second copy is the bug"
        );
        assert!(
            draws_waiting_block(Some(&other), &open),
            "reading one card must never hide a different question"
        );
        assert!(
            draws_waiting_block(None, &open),
            "with nothing open the pin is the only copy there is"
        );
    }

    #[test]
    fn presenting_the_same_surface_again_changes_nothing_and_keeps_its_arrival() {
        // The derived half re-presents every sweep with a fresh clock. That
        // is not a change, and treating it as one reset every row's age to
        // zero once a second and repainted the pane to say so.
        let mut b = Bench::new();
        let mut first = doc("same", "First");
        first.surface.as_mut().unwrap().arrived_ms = 1_000;
        assert!(b.apply(first).is_some(), "the first arrival is news");
        let mut again = doc("same", "First");
        again.surface.as_mut().unwrap().arrived_ms = 61_000;
        assert!(
            b.apply(again).is_none(),
            "the same surface a minute later is not"
        );
        assert_eq!(
            b.get(&SurfaceId("same".into())).unwrap().arrived_ms,
            1_000,
            "and it keeps when it first arrived"
        );
        let mut changed = doc("same", "Second");
        changed.surface.as_mut().unwrap().arrived_ms = 62_000;
        assert!(b.apply(changed).is_some(), "a different title is a change");
        assert_eq!(b.get(&SurfaceId("same".into())).unwrap().arrived_ms, 62_000);
    }

    #[test]
    fn a_verb_preview_is_the_typed_line_and_a_local_verb_says_what_it_opens() {
        use crate::surface::{Action, ActionReport};
        // The hunk id is the agent's own text, newline included. The preview
        // must be exactly what pressing would type — one flattened, tagged
        // line — because the label alone came from the party being checked.
        let mut b = Bench::new();
        b.apply(post(json!({
            "td":"0.2","kind":"changeset","id":"change-847","title":"x",
            "model":{"repository":"r","hunks":[{"id":"a\nwhoami","file":"a.rs","patch":"+1"}]}
        })));
        let s = b
            .get(&SurfaceId("change-847".into()))
            .expect("the changeset");
        let shown = verb_preview(
            s,
            &Action::RejectPart,
            Some("a\nwhoami"),
            Some("first\r\nsecond"),
            Some("k7f2q9ax"),
        );
        let typed = ActionReport {
            surface: s.id.clone(),
            action: Action::RejectPart,
            target: Some("a\nwhoami".into()),
            comment: Some("first\r\nsecond".into()),
        }
        .to_prompt(Some("k7f2q9ax"));
        assert_eq!(shown, typed, "one function produces both");
        assert!(shown.starts_with("[workbench:k7f2q9ax]"), "{shown}");
        assert!(!shown.contains('\n') && !shown.contains('\r'), "{shown}");

        let mut b = Bench::new();
        b.apply(post(json!({
            "td":"0.2","kind":"artifact","id":"doc","title":"d",
            "model":{"href":"/tmp/a.pdf"}
        })));
        let s = b.get(&SurfaceId("doc".into())).expect("the artifact");
        assert_eq!(
            verb_preview(s, &Action::Open, None, None, None),
            "opens /tmp/a.pdf"
        );
        assert!(
            verb_preview(s, &Action::OpenSource, None, None, None).contains("nothing"),
            "a verb that would do nothing says so"
        );
    }

    #[test]
    fn an_update_merges_and_a_present_replaces() {
        let mut b = Bench::new();
        b.apply(post(json!({
            "td":"0.1","kind":"markdown","id":"s","title":"Draft",
            "model":{"body":"x"}, "weight":{"effort":"large"}
        })));
        b.apply(post(json!({
            "td":"0.1","op":"update","kind":"markdown","id":"s","title":"Final",
            "model":{"body":"x"}
        })));
        let s = b.get(&SurfaceId("s".into())).expect("still there");
        assert_eq!(s.title, "Final");
        assert!(
            s.weight.effort.is_some(),
            "update kept the weight it did not mention"
        );

        b.apply(post(json!({
            "td":"0.1","kind":"markdown","id":"s","title":"Replaced","model":{"body":"x"}
        })));
        let s = b.get(&SurfaceId("s".into())).expect("still there");
        assert!(
            s.weight.effort.is_none(),
            "a present is a replacement, weights and all"
        );
    }

    #[test]
    fn retiring_removes_the_row_and_moves_the_selection() {
        let mut b = Bench::new();
        b.apply(doc("a", "A"));
        b.apply(doc("b", "B"));
        b.select(&SurfaceId("b".into()));
        b.apply(post(json!({ "td":"0.1","op":"retire","id":"b" })));
        assert_eq!(b.all_newest_first().count(), 1);
        assert!(
            b.selected().is_none(),
            "retiring the open card returns to the conversation, not to another card"
        );
    }

    #[test]
    fn the_cap_drops_the_oldest_not_the_newest() {
        let mut b = Bench::new();
        for i in 0..(crate::surface::PANE_HISTORY_CAP + 5) {
            b.apply(doc(&format!("s{i}"), &format!("Doc {i}")));
        }
        assert_eq!(
            b.all_newest_first().count(),
            crate::surface::PANE_HISTORY_CAP
        );
        assert!(b.get(&SurfaceId("s0".into())).is_none(), "the oldest went");
        let newest = format!("s{}", crate::surface::PANE_HISTORY_CAP + 4);
        assert!(b.get(&SurfaceId(newest)).is_some(), "the newest stayed");
    }

    #[test]
    fn arrivals_are_unseen_until_the_bench_is_actually_showing_them() {
        let mut b = Bench::new();
        b.apply(doc("a", "A"));
        assert_eq!(b.unseen_total(), 1, "it arrived behind the terminal face");

        b.set_shelf(Shelf::Artifacts);
        b.toggle_face();
        assert_eq!(b.face(), Face::Workbench);
        assert_eq!(
            b.unseen_total(),
            0,
            "looking at the shelf is what marks it seen"
        );

        b.apply(decision("d"));
        assert_eq!(
            b.unseen_total(),
            1,
            "a decision on ANOTHER shelf is not seen by looking at this one"
        );
        b.set_shelf(Shelf::Decisions);
        assert_eq!(b.unseen_total(), 0);
    }

    #[test]
    fn stepping_stays_inside_the_shelf_and_does_not_wrap() {
        let mut b = Bench::new();
        b.apply(doc("a", "A"));
        b.apply(doc("b", "B"));
        b.apply(decision("d"));
        b.set_shelf(Shelf::Artifacts);
        b.select(&SurfaceId("b".into())); // newest first, so "b" is row 0
        b.step(1);
        assert_eq!(b.selected().map(|s| s.id.as_str()), Some("a"));
        b.step(1);
        assert_eq!(
            b.selected().map(|s| s.id.as_str()),
            Some("a"),
            "clamped, not wrapped"
        );
        b.step(-5);
        assert_eq!(b.selected().map(|s| s.id.as_str()), Some("b"));
    }

    #[test]
    fn rows_are_newest_first() {
        let mut b = Bench::new();
        b.apply(doc("a", "A"));
        b.apply(doc("b", "B"));
        assert_eq!(b.rows_for(Shelf::Artifacts)[0].title, "B");
        b.apply(response("r1", "One."));
        b.apply(response("r2", "Two."));
        assert_eq!(
            b.rows()[0].title,
            "Two.",
            "the overview too, and it is the default shelf"
        );
    }

    #[test]
    fn a_changeset_is_pending_until_every_part_has_a_verdict() {
        let mut b = Bench::new();
        b.apply(changeset("c"));
        assert_eq!(b.rows_for(Shelf::Decisions)[0].tint, Tint::Pending);
        b.select(&SurfaceId("c".into()));
        b.act(&Action::AcceptPart, Some("h1".into()), None);
        assert_eq!(
            b.rows_for(Shelf::Decisions)[0].tint,
            Tint::Pending,
            "one answered part is not an answered changeset"
        );
        b.act(&Action::RejectPart, Some("h2".into()), None);
        assert_eq!(b.rows_for(Shelf::Decisions)[0].tint, Tint::Settled);
    }

    #[test]
    fn a_verdict_is_recorded_locally_and_also_told_to_the_agent() {
        let mut b = Bench::new();
        b.apply(changeset("c"));
        b.select(&SurfaceId("c".into()));
        let out = b.act(
            &Action::RejectPart,
            Some("h1".into()),
            Some("wrong seam".into()),
        );
        match out {
            Dispatch::Tell(report) => {
                assert_eq!(report.action, Action::RejectPart);
                assert_eq!(report.target.as_deref(), Some("h1"));
                assert!(report.to_prompt(None).contains("wrong seam"));
            }
            other => panic!("{other:?}"),
        }
        let s = b.get(&SurfaceId("c".into())).unwrap();
        match &s.kind {
            Kind::Changeset(c) => {
                assert_eq!(
                    c.hunks[0].verdict,
                    Verdict::Rejected,
                    "the row tells the truth after"
                );
                assert_eq!(
                    c.hunks[1].verdict,
                    Verdict::Undecided,
                    "and only about that part"
                );
            }
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn a_verdict_on_a_part_that_does_not_exist_is_refused_with_its_name() {
        let mut b = Bench::new();
        b.apply(changeset("c"));
        b.select(&SurfaceId("c".into()));
        match b.act(&Action::AcceptPart, Some("nope".into()), None) {
            Dispatch::Refused(why) => assert!(why.contains("nope"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn opening_is_the_windows_job_and_approving_is_the_agents() {
        let mut b = Bench::new();
        b.apply(post(json!({
            "td":"0.1","kind":"artifact","id":"a","title":"Report",
            "model":{"href":"/home/parker/r.html"}
        })));
        b.select(&SurfaceId("a".into()));
        assert_eq!(
            b.act(&Action::Open, None, None),
            Dispatch::Open("/home/parker/r.html".into())
        );
        assert!(matches!(
            b.act(&Action::Approve, None, None),
            Dispatch::Tell(_)
        ));
    }

    #[test]
    fn opening_something_that_is_not_a_document_says_so() {
        let mut b = Bench::new();
        b.apply(decision("d"));
        b.select(&SurfaceId("d".into()));
        assert!(matches!(
            b.act(&Action::Open, None, None),
            Dispatch::Refused(_)
        ));
    }

    #[test]
    fn acting_on_an_empty_bench_is_refused_rather_than_ignored() {
        let mut b = Bench::new();
        match b.act(&Action::Approve, None, None) {
            Dispatch::Refused(why) => assert!(why.contains("nothing on this bench"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn open_source_uses_the_first_file_and_refuses_when_there_is_none() {
        let mut b = Bench::new();
        b.apply(post(json!({
            "td":"0.1","kind":"markdown","id":"s","title":"Note","model":{"body":"b"},
            "source": { "files": ["/home/parker/x.rs"] }
        })));
        b.select(&SurfaceId("s".into()));
        assert_eq!(
            b.act(&Action::OpenSource, None, None),
            Dispatch::Open("/home/parker/x.rs".into())
        );
        b.apply(doc("t", "No source"));
        b.select(&SurfaceId("t".into()));
        assert!(matches!(
            b.act(&Action::OpenSource, None, None),
            Dispatch::Refused(_)
        ));
    }

    fn question(id: &str, cursor: Option<usize>) -> Post {
        let mut p = post(json!({
            "td": "0.1", "kind": "question", "id": id, "title": "Which page?",
            "model": { "question": "Which page?", "options": [
                { "label": "Status page" }, { "label": "Decision brief" }, { "label": "Delete it" }
            ]}
        }));
        if let Some(s) = p.surface.as_mut() {
            if let Kind::Question(q) = &mut s.kind {
                q.cursor = cursor;
            }
        }
        p
    }

    /// A round of two, as the channel presents it: two cards, each carrying the
    /// same steps and its own `current`.
    fn round_of_two(done_first: bool) -> (Post, Post) {
        let steps = |cur: usize| crate::surface::Round {
            steps: vec![
                crate::surface::Step {
                    label: "Ended state".into(),
                    done: done_first,
                    id: Some(SurfaceId("ask-hook-t-0".into())),
                },
                crate::surface::Step {
                    label: "Orphans".into(),
                    done: false,
                    id: Some(SurfaceId("ask-hook-t-1".into())),
                },
            ],
            submitting: false,
            current: Some(cur),
        };
        let one = |id: &str, cur: usize, answered: bool| {
            let mut p = question(id, None);
            if let Some(s) = p.surface.as_mut() {
                if let Kind::Question(q) = &mut s.kind {
                    q.round = Some(steps(cur));
                    if answered {
                        q.answer = crate::surface::Answered::Chose(0);
                    }
                }
            }
            p
        };
        (
            one("ask-hook-t-0", 0, done_first),
            one("ask-hook-t-1", 1, false),
        )
    }

    #[test]
    fn a_round_opens_on_its_first_open_step_not_its_last_arrival() {
        // `rev()` was right while questions arrived one at a time and the newest
        // was the live one. A round arrives WHOLE, so the last arrival is its
        // LAST question — the bench opened a round of two on `Orphans` with
        // `Ended state` unanswered behind it, which is the jam Parker
        // photographed from the terminal side.
        let mut b = Bench::new();
        let (first, second) = round_of_two(false);
        b.apply(first);
        b.apply(second);
        assert_eq!(
            b.waiting_question().map(|s| s.id.0.clone()),
            Some("ask-hook-t-0".into()),
            "a fresh round starts at step one"
        );
    }

    #[test]
    fn the_selection_decides_which_step_of_a_round_the_block_draws() {
        // Both gestures the navigator exists for come through here: pressing a
        // step selects that card, and answering one advances the selection to
        // the next. If the block ignored the selection, both would select
        // something nothing drew and appear to do nothing at all.
        let mut b = Bench::new();
        let (first, second) = round_of_two(false);
        b.apply(first);
        b.apply(second);
        b.select(&SurfaceId("ask-hook-t-1".into()));
        assert_eq!(
            b.waiting_question().map(|s| s.id.0.clone()),
            Some("ask-hook-t-1".into()),
            "pressing a step must move the block to it"
        );
        // A selection that is NOT an open question does not hijack the block —
        // selecting a changeset while a question waits still draws the question.
        b.apply(changeset("c1"));
        b.select(&SurfaceId("c1".into()));
        assert_eq!(
            b.waiting_question().map(|s| s.id.0.clone()),
            Some("ask-hook-t-0".into()),
            "a non-question selection falls back to the round's first open step"
        );
    }

    #[test]
    fn an_answered_step_stops_being_where_the_block_stands() {
        // After step one is answered the round's first OPEN step is step two,
        // which is where a person is sent. The answered card is still on the
        // bench and still reachable from the navigator; it is simply no longer
        // what the block opens on.
        let mut b = Bench::new();
        let (first, second) = round_of_two(true);
        b.apply(first);
        b.apply(second);
        assert_eq!(
            b.waiting_question().map(|s| s.id.0.clone()),
            Some("ask-hook-t-1".into()),
            "an answered first step hands the block to the second"
        );
    }

    #[test]
    fn answering_a_derived_question_drives_the_menu_and_records_the_answer() {
        let mut b = Bench::new();
        b.apply(question("q1", Some(0)));
        b.select(&SurfaceId("q1".into()));
        match b.act(&Action::Choose, Some("1".into()), None) {
            Dispatch::Keys { bytes, note } => {
                assert_eq!(bytes, b"\x1b[B\r", "one down, then return");
                assert!(note.contains("Decision brief"), "{note}");
            }
            other => panic!("{other:?}"),
        }
        match &b.get(&SurfaceId("q1".into())).unwrap().kind {
            Kind::Question(q) => {
                assert_eq!(q.answer, crate::surface::Answered::Chose(1));
                assert_eq!(q.cursor, None, "the menu is gone once it has been answered");
            }
            other => panic!("{}", other.id()),
        }
        assert_eq!(
            b.rows_for(Shelf::Decisions)[0].tint,
            Tint::Settled,
            "an answered question stops shouting"
        );
    }

    #[test]
    fn a_declared_question_has_no_menu_so_the_answer_is_a_sentence() {
        let mut b = Bench::new();
        b.apply(question("q2", None));
        b.select(&SurfaceId("q2".into()));
        match b.act(&Action::Choose, Some("2".into()), None) {
            Dispatch::Tell(report) => {
                assert_eq!(report.action, Action::Choose);
                assert_eq!(report.target.as_deref(), Some("Delete it"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn choosing_an_option_that_is_not_there_is_refused_by_number() {
        let mut b = Bench::new();
        b.apply(question("q3", Some(0)));
        b.select(&SurfaceId("q3".into()));
        match b.act(&Action::Choose, Some("9".into()), None) {
            Dispatch::Refused(why) => assert!(why.contains("option 10"), "{why}"),
            other => panic!("{other:?}"),
        }
        match b.act(&Action::Choose, None, None) {
            Dispatch::Refused(why) => assert!(why.contains("needs an option"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_ordinary_character_starts_talking_rather_than_being_swallowed() {
        // The whole difference between a form and a terminal. Before this,
        // typing on the bench did nothing until you had found and clicked a
        // box, which is the thing that made it feel like a viewer.
        assert_eq!(reading_key("a", true, None), Reading::Talk);
        assert_eq!(reading_key("/", true, None), Reading::Talk);
        assert_eq!(reading_key("space", true, None), Reading::Talk);
    }

    /// The window's gestures survive whichever face a pane is showing.
    ///
    /// Every entry is a chord the workspace binds. Before this table existed
    /// the list was written out inside `keystroke_bytes` and nowhere else, so
    /// the bench — which ends every key path by stopping propagation — took all
    /// of them and `alt+w` did nothing at all on the workbench face (#524).
    /// Every shelf has a chord, the chord is its place in the strip, and no
    /// chord is one the WINDOW has already claimed.
    ///
    /// The last clause is the one worth having. The obvious keys for this were
    /// the four sitting under the right hand in tab order — `alt+v b n m` — and
    /// `alt+v` is the pane split, advertised in the module header and on the
    /// keybindings sheet. Nothing would have failed if it had been taken: the
    /// bench would simply have stopped splitting, on one face, and the report
    /// would have been "the split key broke" weeks later. A collision between
    /// two chord tables is invisible to a compiler and to every test that does
    /// not go looking, so this goes looking.
    ///
    /// It walks `Shelf::ALL` rather than a list of digits, so a fifth shelf
    /// arrives already bound — and if its chord ever collides with a window
    /// chord, this fails on the day the shelf is added rather than on the day
    /// somebody notices their split is gone.
    #[test]
    fn every_shelf_has_a_chord_and_no_chord_belongs_to_the_window() {
        for (i, shelf) in Shelf::ALL.iter().enumerate() {
            let key = (i + 1).to_string();
            assert_eq!(
                shelf_chord(&key, true, false),
                Some(*shelf),
                "alt+{key} should land on {shelf:?}"
            );
            assert!(
                !crate::keylayer::window_chord(&key, true, false),
                "alt+{key} is a WINDOW chord as well as a shelf chord. One of \
                 them will silently stop working — this is exactly what ruled \
                 out alt+v for the overview."
            );
        }
        // The modifier IS the chord. A bare digit answers a waiting question in
        // `reading_key`, so letting one through here would change tab instead
        // of answering somebody's picker — or worse, do both.
        assert_eq!(shelf_chord("1", false, false), None, "a bare digit answers");
        assert_eq!(
            shelf_chord("1", true, true),
            None,
            "ctrl+alt walks the tree"
        );
        // And nothing outside the strip resolves, including the off-by-one that
        // an index-from-zero reading would produce.
        assert_eq!(shelf_chord("0", true, false), None);
        let past_the_end = (Shelf::ALL.len() + 1).to_string();
        assert_eq!(shelf_chord(&past_the_end, true, false), None);
        assert_eq!(
            shelf_chord("m", true, false),
            None,
            "the note box, not a shelf"
        );
    }

    /// A modified keystroke is not a character, whatever `key_char` says.
    #[test]
    fn a_chord_is_not_typing_even_when_gpui_hands_over_a_letter() {
        // The exact shape of the bug: gpui fills `key_char` for alt+r with
        // "r", so the bench read a chord as the letter and opened a composer
        // where the FOCUS reader should have been.
        assert!(!types_a_character(Some("r"), true, false, false), "alt+r");
        assert!(!types_a_character(Some("c"), false, true, false), "ctrl+c");
        assert!(!types_a_character(Some("k"), false, false, true), "super+k");
        // Ordinary typing is untouched, shift included — shift+a IS a character.
        assert!(types_a_character(Some("a"), false, false, false));
        assert!(types_a_character(Some("A"), false, false, false));
        assert!(types_a_character(Some("/"), false, false, false));
        // And the two non-answers stay non-answers.
        assert!(!types_a_character(None, false, false, false));
        assert!(!types_a_character(Some(""), false, false, false));
        assert!(!types_a_character(Some("\u{1b}"), false, false, false));
    }

    #[test]
    fn a_digit_answers_only_when_there_is_a_question_to_answer() {
        // Otherwise it is just a character, and it goes to the agent like any
        // other — a `2` typed into a prompt must not silently pick option two
        // of something that is not on screen.
        assert_eq!(reading_key("2", true, Some(3)), Reading::Choose(1));
        assert_eq!(reading_key("2", true, None), Reading::Talk);
        assert_eq!(
            reading_key("4", true, Some(3)),
            Reading::Talk,
            "a number past the end of the menu is not an answer"
        );
        assert_eq!(reading_key("0", true, Some(3)), Reading::Talk, "one-based");
    }

    #[test]
    fn navigation_keys_are_the_ones_that_are_not_characters() {
        assert_eq!(reading_key("down", false, None), Reading::Down);
        assert_eq!(reading_key("up", false, None), Reading::Up);
        assert_eq!(reading_key("tab", false, None), Reading::NextShelf);
        assert_eq!(reading_key("enter", false, None), Reading::Act);
        // `j` and `k` used to walk the rail, and could not: they are letters,
        // and a bench you can type at cannot spend letters on navigation.
        assert_eq!(reading_key("j", true, None), Reading::Talk);
        assert_eq!(reading_key("k", true, None), Reading::Talk);
    }

    #[test]
    fn a_non_printable_key_is_swallowed_so_it_cannot_reach_a_working_agent() {
        assert_eq!(reading_key("f5", false, None), Reading::Pass);
        assert_eq!(reading_key("home", false, None), Reading::Pass);
    }

    #[test]
    fn menu_keys_walk_in_both_directions_and_always_press_return() {
        assert_eq!(menu_keys(0, 0), b"\r".to_vec(), "already there");
        assert_eq!(menu_keys(2, 0), b"\x1b[B\x1b[B\r".to_vec());
        assert_eq!(menu_keys(0, 2), b"\x1b[A\x1b[A\r".to_vec());
        assert!(menu_keys(5, 1).ends_with(b"\r"));
    }

    #[test]
    fn a_typed_line_ends_in_a_carriage_return_and_holds_no_newlines() {
        // A newline inside the text would put a blank line in the agent's
        // prompt buffer instead of submitting it, which looks exactly like
        // the send having silently failed.
        let bytes = typed_line("fix the login bug");
        assert_eq!(bytes.last(), Some(&b'\r'));
        assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 0);
        let pasted = typed_line("first\nsecond\r\nthird");
        assert_eq!(String::from_utf8_lossy(&pasted), "first second third\r");
    }

    #[test]
    fn a_new_bench_wants_its_rail_open() {
        // Photographed on the build before it was: the first frame of the
        // workbench showed a column of ticks nobody had a reason to click,
        // and the history the rail exists to show was invisible.
        let b = Bench::new();
        assert!(b.rail_wanted());
        assert_eq!(
            rail_fit(900.0, b.rail_wanted()),
            RailFit::Open(RAIL_W as u32)
        );
    }

    #[test]
    fn the_rail_yields_to_the_pane_before_it_yields_to_the_person() {
        assert_eq!(
            rail_fit(900.0, true),
            RailFit::Open(RAIL_W as u32),
            "capped"
        );
        assert_eq!(rail_fit(900.0, false), RailFit::Ticks, "closed by hand");
        assert_eq!(rail_fit(300.0, true), RailFit::Ticks, "too narrow to open");
        assert_eq!(
            rail_fit(200.0, true),
            RailFit::Hidden,
            "too narrow for anything"
        );
    }

    #[test]
    fn the_caret_moves_through_a_line_the_way_a_text_field_does() {
        let mut l = Line::new();
        l.insert("hello");
        assert_eq!((l.text(), l.caret()), ("hello", 5));

        // Back three and type: the insert lands where the caret is.
        l.left();
        l.left();
        l.left();
        assert_eq!(l.caret(), 2);
        l.insert("X");
        assert_eq!((l.text(), l.caret()), ("heXllo", 3));

        // Backspace takes what is behind, delete takes what is in front.
        assert!(l.backspace());
        assert_eq!((l.text(), l.caret()), ("hello", 2));
        assert!(l.delete());
        assert_eq!((l.text(), l.caret()), ("helo", 2));

        // The ends hold rather than wrapping or panicking.
        l.home();
        assert_eq!(l.caret(), 0);
        assert!(!l.backspace(), "nothing behind the start");
        l.left();
        assert_eq!(l.caret(), 0);
        l.end();
        assert_eq!(l.caret(), 4);
        assert!(!l.delete(), "nothing in front of the end");
        l.right();
        assert_eq!(l.caret(), 4);
    }

    #[test]
    fn the_editing_conventions_a_person_already_knows_all_work() {
        // Word motion, both directions and both modifiers. The point of a
        // convention is that somebody already knows it, so one of these being
        // wrong is worse than the whole set being absent.
        for (key, ctrl, alt, want) in [
            ("left", true, false, Edit::Move(Motion::WordLeft)),
            ("left", false, true, Edit::Move(Motion::WordLeft)),
            ("right", true, false, Edit::Move(Motion::WordRight)),
            ("right", false, true, Edit::Move(Motion::WordRight)),
            ("left", false, false, Edit::Move(Motion::Left)),
            ("right", false, false, Edit::Move(Motion::Right)),
            // Readline's own chords, because the far end is readline-shaped —
            // except ctrl+a, which is select-all here: this is a text area on a
            // screen, and start-of-line is on `home`.
            ("a", true, false, Edit::SelectAll),
            ("e", true, false, Edit::Move(Motion::End)),
            ("b", true, false, Edit::Move(Motion::Left)),
            ("f", true, false, Edit::Move(Motion::Right)),
            ("w", true, false, Edit::KillWordLeft),
            ("u", true, false, Edit::KillToStart),
            ("k", true, false, Edit::KillToEnd),
            ("d", false, true, Edit::KillWordRight),
            ("backspace", true, false, Edit::KillWordLeft),
            ("delete", true, false, Edit::KillWordRight),
            ("home", false, false, Edit::Move(Motion::Home)),
            ("end", false, false, Edit::Move(Motion::End)),
            ("enter", false, false, Edit::Submit),
        ] {
            assert_eq!(
                line_edit(key, ctrl, alt, false),
                Some(want),
                "{key} ctrl={ctrl} alt={alt}"
            );
        }
        // A plain letter is a letter. `a` unmodified must reach the line as
        // text, or typing the word "and" would send the caret home twice.
        for key in ["a", "e", "w", "k", "u", "d", "b", "f", "z"] {
            assert_eq!(
                line_edit(key, false, false, false),
                None,
                "{key} alone is text"
            );
        }
        assert_eq!(line_edit("f5", false, false, false), None);
    }

    #[test]
    fn shift_enter_is_a_line_and_enter_alone_is_the_send() {
        // #614: `line_edit` took no shift, so `shift+enter` read as Submit
        // and emptied a box the agent still held the draft of. The table
        // takes shift now, and the one key it changes is this one.
        assert_eq!(line_edit("enter", false, false, true), Some(Edit::Newline));
        assert_eq!(line_edit("enter", false, true, false), Some(Edit::Newline));
        assert_eq!(line_edit("enter", false, false, false), Some(Edit::Submit));
        assert_eq!(line_edit("z", true, false, false), Some(Edit::Undo));
        assert_eq!(
            line_edit("up", false, false, false),
            Some(Edit::Move(Motion::Up))
        );
        assert_eq!(
            line_edit("down", false, false, false),
            Some(Edit::Move(Motion::Down))
        );
        // Shift on a letter is still a letter.
        assert_eq!(line_edit("a", false, false, true), None);

        let mut l = Line::holding("first");
        l.apply(Edit::Newline);
        l.insert("second");
        assert_eq!(l.text(), "first\nsecond");
        assert_eq!(l.line_col(), (1, 6));
        l.apply(Edit::Move(Motion::Up));
        assert_eq!(l.caret(), 5, "row 0, column clamped to the row");
        l.apply(Edit::Move(Motion::Up));
        assert_eq!(l.caret(), 0, "up on the first row is home");
        l.apply(Edit::Move(Motion::Down));
        assert_eq!(l.line_col(), (1, 0));
        l.apply(Edit::Move(Motion::Down));
        assert_eq!(l.caret(), l.chars(), "down on the last row is end");
    }

    #[test]
    fn a_draft_takes_its_changes_back_in_order() {
        let mut l = Line::new();
        assert!(!l.undo(), "nothing to take back");
        l.insert("one");
        l.insert(" two");
        l.apply(Edit::KillWordLeft);
        assert_eq!(l.text(), "one ");
        assert!(l.undo());
        assert_eq!(l.text(), "one two");
        assert!(l.undo());
        assert_eq!((l.text(), l.caret()), ("one", 3));
        assert!(l.undo());
        assert_eq!(l.text(), "");
        assert!(!l.undo());
        // A replaced selection comes back whole.
        let mut r = Line::holding("keep this");
        r.apply(Edit::SelectAll);
        r.insert("X");
        assert_eq!(r.text(), "X");
        assert!(r.undo());
        assert_eq!(r.text(), "keep this");
        // Motion is not a change: undo after arrows takes back the last EDIT.
        let mut m = Line::holding("abc");
        m.apply(Edit::Move(Motion::Left));
        m.apply(Edit::Move(Motion::Home));
        assert!(!m.undo(), "arrows leave nothing to undo");
    }

    #[test]
    fn ctrl_a_selects_the_whole_draft_and_the_next_key_replaces_it() {
        let mut l = Line::holding("the whole thing");
        l.apply(Edit::SelectAll);
        assert_eq!(l.sel_range(), Some((0, 15)), "ctrl+a selects all of it");
        // The caret ends at the END now. It sat at zero while the far end's
        // readline had been moved there by the same byte; the composer owns
        // the draft, so the convention every text box follows wins.
        assert_eq!(l.caret(), 15);
        l.insert("x");
        assert_eq!(l.text(), "x", "typing replaces the selection");
        assert_eq!(l.sel_range(), None, "and the selection is spent");

        // Delete and backspace take the selection whole rather than one char.
        let mut l = Line::holding("gone");
        l.apply(Edit::SelectAll);
        l.apply(Edit::Backspace);
        assert_eq!(l.text(), "");
        let mut l = Line::holding("gone");
        l.apply(Edit::SelectAll);
        l.apply(Edit::Delete);
        assert_eq!(l.text(), "");

        // Any motion drops the selection and leaves the text alone. A plain
        // right collapses to the far edge, which is where it already was.
        let mut l = Line::holding("kept");
        l.apply(Edit::SelectAll);
        l.apply(Edit::Move(Motion::Right));
        assert_eq!(l.sel_range(), None, "a motion drops the selection");
        assert_eq!(l.text(), "kept");
        assert_eq!(l.caret(), 4, "and collapses to the end it was already at");

        // A plain LEFT collapses to the other edge rather than stepping one
        // character back off the caret.
        let mut l = Line::holding("kept");
        l.apply(Edit::SelectAll);
        l.apply(Edit::Move(Motion::Left));
        assert_eq!(l.caret(), 0, "left collapses to the selection's start");
        assert_eq!(l.text(), "kept");

        // An empty draft has nothing to select — a highlight over nothing is a
        // control that looks armed and does nothing.
        let mut l = Line::new();
        l.apply(Edit::SelectAll);
        assert_eq!(l.sel_range(), None);

        // The far end is told to kill from the caret ctrl+a just moved.
        assert_eq!(replace_bytes(), vec![0x0b]);
    }

    // ── shift selects, and the anchor is the whole point ────────────────────
    //
    // These fail against the parent commit at the TABLE: `line_edit` had no
    // shift-held motion arms, so every one of them resolved to the bare move
    // and `sel_range` did not exist to assert on.

    /// Every motion the table answers has a shift-held form, and the two are
    /// the same movement. This is the guard on the invariant `Edit::Move` and
    /// `Edit::Extend` exist to hold: a motion cannot gain a bare form without
    /// gaining a selecting one.
    #[test]
    fn every_motion_key_has_a_shift_held_form_that_moves_identically() {
        let keys = [
            ("left", false, false),
            ("right", false, false),
            ("left", true, false),
            ("right", true, false),
            ("up", false, false),
            ("down", false, false),
            ("home", false, false),
            ("end", false, false),
            ("e", true, false),
            ("b", true, false),
            ("f", true, false),
        ];
        for (key, ctrl, alt) in keys {
            let bare = line_edit(key, ctrl, alt, false)
                .unwrap_or_else(|| panic!("{key} ctrl={ctrl} is not in the table"));
            let held = line_edit(key, ctrl, alt, true)
                .unwrap_or_else(|| panic!("{key} ctrl={ctrl} has no shift form"));
            let (Edit::Move(a), Edit::Extend(b)) = (bare, held) else {
                panic!("{key} ctrl={ctrl}: expected Move/Extend, got {bare:?}/{held:?}");
            };
            assert_eq!(a, b, "{key} must move the same way with shift held");

            // …and they land the caret in the same place, over a draft with a
            // line break in it so `up`/`down` are doing real work.
            let seed = || {
                let mut l = Line::holding("alpha beta\ngamma delta");
                l.place(6);
                l
            };
            let (mut moved, mut extended) = (seed(), seed());
            moved.apply(bare);
            extended.apply(held);
            assert_eq!(
                moved.caret(),
                extended.caret(),
                "{key} ctrl={ctrl}: shift changed WHERE the caret went"
            );
        }
    }

    #[test]
    fn the_anchor_stays_put_while_the_caret_walks() {
        let mut l = Line::holding("alpha beta gamma");
        l.place(6);
        for _ in 0..4 {
            l.apply(Edit::Extend(Motion::Right));
        }
        assert_eq!(l.sel_range(), Some((6, 10)), "four rights grew from six");
        assert_eq!(l.selected_text(), Some("beta"));

        // Back the other way, through the anchor and out the far side. The
        // anchor does not move, so the range flips rather than collapsing.
        for _ in 0..6 {
            l.apply(Edit::Extend(Motion::Left));
        }
        assert_eq!(l.sel_range(), Some((4, 6)), "a backwards selection");
        assert_eq!(l.selected_text(), Some("a "));
    }

    #[test]
    fn a_selection_walked_back_onto_its_anchor_is_no_selection() {
        let mut l = Line::holding("alpha");
        l.place(2);
        l.apply(Edit::Extend(Motion::Right));
        assert_eq!(l.sel_range(), Some((2, 3)));
        l.apply(Edit::Extend(Motion::Left));
        assert_eq!(
            l.sel_range(),
            None,
            "a range of nothing is not a selection, and would eat the caret"
        );
    }

    /// The exact sequence from #615, pinned so it cannot come back.
    ///
    /// Select all, click into the middle, type. The old model left the
    /// highlight standing through the click — `seek` was public and moved
    /// only the caret — so the next character replaced the whole draft on
    /// one side and appended on the other. `place` is now the only way in
    /// and it does both halves, and there is no second copy to disagree.
    #[test]
    fn a_click_into_a_selection_drops_it_and_types_where_you_clicked() {
        let mut l = Line::holding("alpha beta gamma");
        l.apply(Edit::SelectAll);
        assert_eq!(l.sel_range(), Some((0, 16)), "the whole draft is selected");

        l.place(9);
        assert_eq!(l.sel_range(), None, "the click dropped the selection");
        assert_eq!(l.caret(), 9, "and left the caret where it landed");

        l.insert("X");
        assert_eq!(
            l.text(),
            "alpha betXa gamma",
            "typing inserts at the click rather than replacing the draft"
        );
    }

    #[test]
    fn place_clamps_past_the_end_rather_than_panicking() {
        let mut l = Line::holding("short");
        l.apply(Edit::SelectAll);
        l.place(9_999);
        assert_eq!(l.caret(), 5);
        assert_eq!(l.sel_range(), None);
    }

    #[test]
    fn typing_over_a_selection_replaces_only_the_selected_run() {
        // The bit could not do this: `insert` wiped the whole draft, because
        // select-all was the only selection that existed.
        let mut l = Line::holding("alpha beta gamma");
        l.place(6);
        for _ in 0..4 {
            l.apply(Edit::Extend(Motion::Right));
        }
        l.insert("BETA");
        assert_eq!(l.text(), "alpha BETA gamma");
        assert_eq!(l.caret(), 10);
        assert_eq!(l.sel_range(), None);
    }

    #[test]
    fn backspace_and_delete_take_the_selected_run_and_leave_the_rest() {
        for edit in [Edit::Backspace, Edit::Delete] {
            let mut l = Line::holding("alpha beta gamma");
            l.place(5);
            for _ in 0..5 {
                l.apply(Edit::Extend(Motion::Right));
            }
            assert_eq!(l.selected_text(), Some(" beta"));
            l.apply(edit);
            assert_eq!(l.text(), "alpha gamma", "{edit:?} took only the run");
            assert_eq!(l.caret(), 5, "{edit:?} left the caret where it started");
            assert_eq!(l.sel_range(), None);
        }
    }

    #[test]
    fn a_shift_selection_survives_being_extended_by_a_different_motion() {
        let mut l = Line::holding("alpha beta gamma");
        l.place(0);
        l.apply(Edit::Extend(Motion::WordRight));
        let after_word = l.sel_range().expect("a word is selected");
        l.apply(Edit::Extend(Motion::End));
        assert_eq!(
            l.sel_range(),
            Some((after_word.0, 16)),
            "the anchor held across two different motions"
        );
    }

    #[test]
    fn shift_selects_across_a_line_break() {
        let mut l = Line::holding("alpha\nbeta");
        l.place(3);
        l.apply(Edit::Extend(Motion::Down));
        let (lo, hi) = l.sel_range().expect("down selected into the next row");
        assert_eq!(lo, 3);
        assert!(hi > 5, "the range crossed the newline, hi={hi}");
        assert!(l.selected_text().expect("text").contains('\n'));
    }

    #[test]
    fn the_selection_is_byte_correct_over_multibyte_text() {
        // A char range read as bytes would slice an em dash in half and panic.
        let mut l = Line::holding("é—ü ascii");
        l.place(0);
        for _ in 0..3 {
            l.apply(Edit::Extend(Motion::Right));
        }
        assert_eq!(l.sel_range(), Some((0, 3)), "three characters");
        assert_eq!(l.selected_text(), Some("é—ü"), "not three bytes");
        assert_eq!(l.sel_bytes(), Some(0..7));
    }

    #[test]
    fn undo_and_submit_and_clear_all_drop_the_selection() {
        let mut l = Line::holding("alpha beta");
        l.place(0);
        l.apply(Edit::Extend(Motion::WordRight));
        l.apply(Edit::Undo);
        assert_eq!(l.sel_range(), None, "undo leaves no stale highlight");

        let mut l = Line::holding("alpha beta");
        l.place(0);
        l.apply(Edit::Extend(Motion::WordRight));
        l.apply(Edit::Submit);
        assert_eq!(l.sel_range(), None);
        assert_eq!(l.text(), "");
    }

    #[test]
    fn an_unselecting_key_is_not_given_a_shift_meaning_by_accident() {
        // Everything that is NOT a motion must read the same with shift held,
        // or a shifted capital letter would silently become an edit. `enter`
        // is the one deliberate exception and has its own test.
        for key in ["backspace", "delete", "z", "a", "w", "u", "k", "d"] {
            for (ctrl, alt) in [(false, false), (true, false), (false, true)] {
                let bare = line_edit(key, ctrl, alt, false);
                let held = line_edit(key, ctrl, alt, true);
                assert_eq!(
                    bare, held,
                    "{key} ctrl={ctrl} alt={alt} changed meaning under shift"
                );
            }
        }
        // …and `enter` is the exception, on purpose.
        assert_eq!(line_edit("enter", false, false, false), Some(Edit::Submit));
        assert_eq!(line_edit("enter", false, false, true), Some(Edit::Newline));
    }

    #[test]
    fn shift_up_selects_and_never_recalls_history() {
        // Up and down mean history in an empty box or mid-recall, and a row
        // otherwise. Shift takes them away from history in every case — the
        // bug this guards is a person extending a selection and being handed
        // last week's prompt.
        assert!(
            recalls_history("up", false, false, false, true),
            "a bare up while recalling is history"
        );
        assert!(
            !recalls_history("up", false, false, true, true),
            "shift+up is a selection even mid-recall"
        );
        assert!(
            !recalls_history("down", false, false, true, true),
            "and so is shift+down"
        );
        assert!(
            !recalls_history("up", false, false, false, false),
            "with a draft and no recall in flight, up is a row"
        );
        for key in ["left", "right", "home", "a"] {
            assert!(
                !recalls_history(key, false, false, false, true),
                "{key} is not a history key"
            );
        }
        for (ctrl, alt) in [(true, false), (false, true)] {
            assert!(
                !recalls_history("up", ctrl, alt, false, true),
                "a modified up belongs to word motion or the window"
            );
        }
    }

    #[test]
    fn a_launch_command_names_the_model_and_the_effort_it_was_given() {
        let cmd = "claude --resume abc --model opus --effort xhigh";
        assert_eq!(flag_value(cmd, "--model").as_deref(), Some("opus"));
        assert_eq!(flag_value(cmd, "--effort").as_deref(), Some("xhigh"));
        // The other spelling, which the panel also prints.
        assert_eq!(
            flag_value("codex resume --model=gpt-5", "--model").as_deref(),
            Some("gpt-5")
        );
        // A flag with nothing after it is not a value, and neither is the next
        // flag along.
        assert_eq!(flag_value("claude --resume --model", "--model"), None);
        assert_eq!(flag_value("claude --model --effort high", "--model"), None);
        // A command that never says is the case the dial has to keep saying it
        // does not know about.
        assert_eq!(flag_value("claude --resume abc", "--model"), None);
    }

    #[test]
    fn word_motion_and_the_kills_agree_with_every_editor_on_this_desk() {
        let mut l = Line::holding("the quick brown fox");
        // Back one word from the end.
        l.apply(Edit::Move(Motion::WordLeft));
        assert_eq!(l.caret(), 16, "start of `fox`");
        l.apply(Edit::Move(Motion::WordLeft));
        assert_eq!(l.caret(), 10, "start of `brown`");
        // Forward again.
        l.apply(Edit::Move(Motion::WordRight));
        assert_eq!(l.caret(), 15, "end of `brown`");

        // ctrl+w takes the word behind and nothing else.
        let mut l = Line::holding("the quick brown fox");
        l.apply(Edit::KillWordLeft);
        assert_eq!(l.text(), "the quick brown ");
        assert_eq!(l.caret(), 16);
        // Twice more, and the trailing space goes with the word.
        l.apply(Edit::KillWordLeft);
        assert_eq!(l.text(), "the quick ");

        // ctrl+u and ctrl+k, from the middle.
        let mut l = Line::holding("the quick brown fox");
        l.place(10);
        l.apply(Edit::KillToStart);
        assert_eq!((l.text(), l.caret()), ("brown fox", 0));
        l.place(5);
        l.apply(Edit::KillToEnd);
        assert_eq!((l.text(), l.caret()), ("brown", 5));

        // Every one of them holds at the ends rather than panicking.
        let mut l = Line::new();
        for edit in [
            Edit::Move(Motion::WordLeft),
            Edit::Move(Motion::WordRight),
            Edit::KillWordLeft,
            Edit::KillWordRight,
            Edit::KillToStart,
            Edit::KillToEnd,
            Edit::Backspace,
            Edit::Delete,
        ] {
            l.apply(edit);
            assert_eq!((l.text(), l.caret()), ("", 0), "{edit:?} on an empty line");
        }
    }

    #[test]
    fn word_motion_does_not_split_a_multi_byte_character() {
        // The caret is in characters, and a kill takes a byte range — so a
        // line of em dashes and accents is where an off-by-one would panic
        // rather than merely misbehave.
        let mut l = Line::holding("caf\u{e9} \u{2014} r\u{e9}sum\u{e9} na\u{ef}ve");
        l.apply(Edit::KillWordLeft);
        assert!(
            l.text().starts_with("caf\u{e9} \u{2014} r\u{e9}sum\u{e9} "),
            "{}",
            l.text()
        );
        l.apply(Edit::Move(Motion::WordLeft));
        l.apply(Edit::KillToEnd);
        assert_eq!(l.text(), "caf\u{e9} \u{2014} ");
    }

    #[test]
    fn a_pasted_image_is_counted_beside_the_line_not_written_into_it() {
        let mut l = Line::new();
        l.insert("look at this");
        l.note_paste();
        assert_eq!(l.pasted(), 1);
        assert_eq!(
            l.text(),
            "look at this",
            "the mirror must stay exactly what was typed — the agent numbers \
             its own images, and a guess here puts every later caret out"
        );
        assert_eq!(l.caret(), 12, "and the caret with it");
        // Sending resets both: the next line has no images in it yet.
        l.clear();
        assert_eq!(l.pasted(), 0);
    }

    #[test]
    fn a_caret_in_characters_survives_text_that_is_not_ascii() {
        // The bug a byte index would have: the caret lands INSIDE a glyph and
        // the next insert splits it into mojibake. Every one of these is
        // multi-byte, and one is multi-CHAR (a flag is two scalars).
        let mut l = Line::holding("a—é🇨🇦b");
        assert_eq!(l.caret(), l.chars());
        for _ in 0..l.chars() {
            l.left();
        }
        assert_eq!(l.caret(), 0);
        l.right();
        l.insert("!");
        assert!(l.text().starts_with("a!—"), "{}", l.text());
        // And the byte index the caret resolves to is still a char
        // boundary, which is the whole point: `insert` panics otherwise.
        l.end();
        l.insert("z");
        assert!(l.text().ends_with("bz"), "{}", l.text());
    }

    #[test]
    fn escape_peels_overlays_and_stops_at_a_question() {
        // Outermost first. The dial menu is above the gallery because it is
        // the newest thing on the glass and the cheapest thing to lose — and
        // because without this rung escape reached past an open menu and
        // emptied the composer, trading somebody's sentence for a list of
        // five words that stayed on screen anyway.
        assert_eq!(peel(true, true, true, true, true), Peel::Dial);
        assert_eq!(peel(false, true, true, true, true), Peel::Gallery);
        assert_eq!(peel(false, false, true, true, true), Peel::Typing);

        // THE FLOOR. A card holding a question somebody is being waited on is
        // not something escape may take away — every other meaning of the key
        // here removes the thing the agent is waiting with.
        assert_eq!(peel(false, false, false, true, true), Peel::Nothing);

        // An ANSWERED card is a record, and a record closes like anything
        // else.
        assert_eq!(peel(false, false, false, true, false), Peel::Card);
    }

    /// The bench is a base surface, and escape does not leave one.
    ///
    /// This test FAILED before the change that added it: both rows answered
    /// `Peel::Face`, which flipped the pane to the terminal. Run it against the
    /// parent commit to see that — a table test whose rows were written after
    /// the table proves nothing, and these two are a single enum comparison,
    /// which is exactly the shape that silently tests nothing.
    #[test]
    fn escape_with_nothing_left_stays_on_the_bench() {
        assert_eq!(
            peel(false, false, false, false, false),
            Peel::Nothing,
            "a quiet bench is still the surface you are on"
        );
        assert_eq!(
            peel(false, false, false, false, true),
            Peel::Nothing,
            "a waiting question with no card open is on the rail, not under escape"
        );
    }

    #[test]
    fn the_last_zone_painted_takes_the_click_and_edges_do_not_double_claim() {
        let z = |x, y, w, h, hit| Zone { x, y, w, h, hit };
        let zones = vec![
            z(0.0, 0.0, 100.0, 100.0, Hit::CloseCard),
            // Painted later, on top of the first.
            z(50.0, 50.0, 100.0, 100.0, Hit::Review),
        ];
        assert_eq!(hit_at(&zones, 10.0, 10.0), Some(&Hit::CloseCard));
        assert_eq!(
            hit_at(&zones, 75.0, 75.0),
            Some(&Hit::Review),
            "the one on top"
        );
        assert_eq!(hit_at(&zones, 200.0, 200.0), None);
        // Left/top inclusive, right/bottom exclusive: the shared edge at 100
        // belongs to the later zone only.
        assert_eq!(hit_at(&zones, 100.0, 60.0), Some(&Hit::Review));
        assert_eq!(hit_at(&zones, 99.9, 10.0), Some(&Hit::CloseCard));
        assert_eq!(hit_at(&[], 1.0, 1.0), None);
    }

    #[test]
    fn unwarping_is_the_identity_when_the_tube_is_flat_and_at_its_centre() {
        let rect = (100.0, 50.0, 800.0, 600.0);
        // No curvature: every point maps to itself.
        for (x, y) in [
            (100.0, 50.0),
            (500.0, 350.0),
            (899.0, 649.0),
            (137.5, 612.25),
        ] {
            let (fx, fy) = unwarp(rect, 0.0, 0.0, x, y);
            assert!(
                (fx - x).abs() < 1e-3 && (fy - y).abs() < 1e-3,
                "{x},{y} -> {fx},{fy}"
            );
        }
        // Real curvature: the centre does not move, and the map is symmetric
        // about it — a point left of centre un-bends by as much as its mirror
        // right of centre. Both follow from the formula being radial.
        let (k1, k2) = (0.12, 0.04);
        let (cx, cy) = unwarp(rect, k1, k2, 500.0, 350.0);
        assert!((cx - 500.0).abs() < 1e-3 && (cy - 350.0).abs() < 1e-3);
        let (lx, _) = unwarp(rect, k1, k2, 300.0, 350.0);
        let (rx, _) = unwarp(rect, k1, k2, 700.0, 350.0);
        assert!(
            ((500.0 - lx) - (rx - 500.0)).abs() < 1e-3,
            "symmetric: {lx} {rx}"
        );
        // And an empty rect cannot divide by zero.
        assert_eq!(unwarp((0.0, 0.0, 0.0, 0.0), k1, k2, 3.0, 4.0), (3.0, 4.0));
    }

    #[test]
    fn a_scripted_bench_verb_reaches_the_focused_pane_first() {
        // The focused pane qualifies: it wins, even when an earlier one does.
        assert_eq!(bench_target(&[true, true, true], Some(2)), Some(2));
        // The focused pane does not qualify: the first that does.
        assert_eq!(bench_target(&[false, true, true], Some(0)), Some(1));
        // Nothing focused: the first that qualifies.
        assert_eq!(bench_target(&[false, false, true], None), Some(2));
        // A focus index off the end is not an eligible pane.
        assert_eq!(bench_target(&[true], Some(5)), Some(0));
        // Nobody qualifies: nobody, whatever is focused.
        assert_eq!(bench_target(&[false, false], Some(1)), None);
        assert_eq!(bench_target(&[], None), None);
    }

    #[test]
    fn the_bench_is_exempt_from_the_tubes_vignette_and_the_terminal_is_not() {
        assert_eq!(vignette_on(Face::Workbench, 0.7), 0.0);
        assert_eq!(vignette_on(Face::Workbench, 0.0), 0.0);
        assert_eq!(vignette_on(Face::Terminal, 0.7), 0.7);
        assert_eq!(vignette_on(Face::Terminal, 0.0), 0.0);
    }

    #[test]
    fn the_pointer_is_a_hand_over_a_control_text_over_the_composer_and_nothing_elsewhere() {
        assert_eq!(Hit::Composer.pointer(), Pointer::Text);
        assert_eq!(Hit::Arm.pointer(), Pointer::Arrow);
        assert_eq!(Hit::Nothing.pointer(), Pointer::Arrow);
        for pressed in [
            Hit::Choose(0),
            Hit::PressNav(3),
            Hit::Review,
            Hit::CloseCard,
            Hit::Launch,
            Hit::ToggleRail,
            Hit::Shelf(crate::surface::Shelf::Decisions),
            Hit::OpenRow(crate::surface::SurfaceId("s".to_string())),
            Hit::GalleryBack,
            Hit::GalleryForward,
            Hit::GalleryClose,
        ] {
            assert_eq!(pressed.pointer(), Pointer::Hand, "{pressed:?}");
        }
    }

    #[test]
    fn a_wheel_over_the_composer_moves_the_composer_and_elsewhere_moves_only_the_mirror() {
        // The composer wins whether or not a mirror is showing.
        assert_eq!(
            wheel_target(Some(&Hit::Composer), false, false),
            Wheel::Composer
        );
        assert_eq!(
            wheel_target(Some(&Hit::Composer), false, true),
            Wheel::Composer
        );
        // Off the composer with the mirror showing: the mirror, whatever zone
        // the body records there.
        assert_eq!(wheel_target(Some(&Hit::Arm), false, true), Wheel::Mirror);
        assert_eq!(wheel_target(None, false, true), Wheel::Mirror);
        // Off the composer with no mirror and no card: nothing moves — and the
        // turn is consumed, so the flat hit-test underneath never sees it.
        assert_eq!(wheel_target(Some(&Hit::Arm), false, false), Wheel::Nothing);
        assert_eq!(
            wheel_target(Some(&Hit::Choose(1)), false, false),
            Wheel::Nothing
        );
        assert_eq!(wheel_target(None, false, false), Wheel::Nothing);
    }

    /// The arm that did not exist, which is why a card taller than its pane
    /// could be clipped with no gesture able to reach the rest of it.
    #[test]
    fn a_wheel_over_a_card_moves_the_card() {
        // The body's own zone is `Arm` — the whole body arms the composer — so
        // this is the zone a turn over a card actually lands on, and it used to
        // resolve to `Nothing`.
        assert_eq!(wheel_target(Some(&Hit::Arm), true, false), Wheel::Card);
        assert_eq!(wheel_target(None, true, false), Wheel::Card);
        // The composer still wins over the card it sits under: the pointer is
        // the thing that decides, not which is more interesting.
        assert_eq!(
            wheel_target(Some(&Hit::Composer), true, false),
            Wheel::Composer
        );
        // A card beats the mirror. The two cannot both be up today — the
        // mirror is drawn in the arm that has no card — so this pins the order
        // rather than describing a screen that exists.
        assert_eq!(wheel_target(Some(&Hit::Arm), true, true), Wheel::Card);
        // And with no card the old answer is unchanged, which is the half a
        // regression would break silently.
        assert_eq!(wheel_target(Some(&Hit::Arm), false, true), Wheel::Mirror);
    }

    #[test]
    fn a_wheel_turn_is_held_between_the_top_of_the_draft_and_its_end() {
        // From the top, a turn down (a negative delta, gpui's sign) reveals
        // more; past the end it stops at the end.
        assert_eq!(wheel_offset(0.0, -30.0, 100.0), -30.0);
        assert_eq!(wheel_offset(-90.0, -30.0, 100.0), -100.0);
        // Back up past the first line stops at the first line.
        assert_eq!(wheel_offset(-10.0, 30.0, 100.0), 0.0);
        // Nothing hidden: the wheel does nothing, in either direction.
        assert_eq!(wheel_offset(0.0, -30.0, 0.0), 0.0);
        assert_eq!(wheel_offset(0.0, 30.0, 0.0), 0.0);
        // A negative "max" is a container smaller than its content has ever
        // been — treated as nothing hidden, never as a range that flips.
        assert_eq!(wheel_offset(0.0, -30.0, -5.0), 0.0);
    }

    #[test]
    fn the_view_follows_the_caret_only_while_the_caret_is_at_the_end() {
        assert!(follows(0, 0));
        assert!(follows(12, 12));
        assert!(!follows(11, 12));
        assert!(!follows(0, 12));
    }

    /// The warp's inverse at the CORNERS, where it matters, with the live
    /// window's rectangle and the tube's real coefficients.
    ///
    /// There is no forward map on the CPU — the shader is the forward map —
    /// so where the tube SHOWS a flat point is found by fixed-point iteration
    /// on the inverse: the screen point whose un-bend is the flat point. Then
    /// the two things a person's click would prove are asserted for each
    /// corner: a flat lookup of the eye's point MISSES the control (so the
    /// un-bend is doing real work, and this test cannot pass vacuously), and
    /// the un-bent lookup lands on it. The one live click ever read back
    /// under the warp was at the centre, where the displacement was 0.1 px;
    /// this is the test of the other 0.1% of the pane.
    #[test]
    fn a_control_in_the_corner_is_hit_where_the_tube_shows_it_not_where_it_was_laid() {
        let rect = (80.0_f32, 60.0_f32, 1500.0_f32, 1000.0_f32);
        let (k1, k2) = (0.2_f32, 0.086_f32);
        let (rx, ry, rw, rh) = rect;
        let side = 20.0;
        let corners = [
            (rx, ry, Hit::CloseCard),
            (rx + rw - side, ry, Hit::ToggleRail),
            (rx, ry + rh - side, Hit::GalleryBack),
            (rx + rw - side, ry + rh - side, Hit::GalleryForward),
        ];
        let zones: Vec<Zone> = corners
            .iter()
            .map(|(x, y, hit)| Zone {
                x: *x,
                y: *y,
                w: side,
                h: side,
                hit: hit.clone(),
            })
            .collect();
        // Where the tube shows a flat point.
        let shown = |flat: (f32, f32)| {
            let (mut sx, mut sy) = flat;
            for _ in 0..64 {
                let (ux, uy) = unwarp(rect, k1, k2, sx, sy);
                sx += flat.0 - ux;
                sy += flat.1 - uy;
            }
            let (ux, uy) = unwarp(rect, k1, k2, sx, sy);
            assert!(
                (ux - flat.0).abs() < 0.01 && (uy - flat.1).abs() < 0.01,
                "the iteration did not converge for {flat:?}: {ux},{uy}"
            );
            (sx, sy)
        };
        for (x, y, hit) in &corners {
            let flat = (x + side / 2.0, y + side / 2.0);
            let (sx, sy) = shown(flat);
            let moved = ((sx - flat.0).powi(2) + (sy - flat.1).powi(2)).sqrt();
            eprintln!(
                "corner {hit:?}: flat {flat:?} shown at ({sx:.1},{sy:.1}), {moved:.1}px away"
            );
            // Measured 76.8 px at this curvature — nearly four times the
            // control — so a flat lookup of the eye's point MUST miss, or
            // the un-bend below is proving nothing.
            assert!(
                moved > 40.0,
                "the tube barely moves {hit:?} ({moved:.1}px); the test would prove nothing"
            );
            assert_ne!(
                hit_at(&zones, sx, sy),
                Some(hit),
                "a flat lookup of where the tube shows {hit:?} must miss it"
            );
            let (fx, fy) = unwarp(rect, k1, k2, sx, sy);
            assert_eq!(
                hit_at(&zones, fx, fy),
                Some(hit),
                "un-bent lookup of {hit:?}"
            );
        }
        // The composer's top-left corner, where the wheel question was asked
        // (#485): if the tube moved it under two pixels the wheel could have
        // stayed flat. It moves it 62.5 px at this curvature.
        let composer_tl = (rx + 16.0, ry + rh - 90.0);
        let (sx, sy) = shown(composer_tl);
        let moved = ((sx - composer_tl.0).powi(2) + (sy - composer_tl.1).powi(2)).sqrt();
        eprintln!(
            "composer top-left: flat {composer_tl:?} shown at ({sx:.1},{sy:.1}), {moved:.1}px away"
        );
        assert!(moved > 20.0, "composer corner moves {moved:.1}px");
    }

    #[test]
    fn the_gallery_answers_every_key_including_the_ones_it_ignores() {
        // The pair a person reaches for, and the pair beside them on a
        // keyboard somebody is already resting a hand on.
        assert_eq!(gallery_key("left"), Gallery::Back);
        assert_eq!(gallery_key("right"), Gallery::Forward);
        assert_eq!(gallery_key("up"), Gallery::Back);
        assert_eq!(gallery_key("down"), Gallery::Forward);
        // Three ways out, because a modal nobody can close is a trap.
        for out in ["escape", "q", "enter"] {
            assert_eq!(gallery_key(out), Gallery::Close, "{out}");
        }
        // And everything else is SWALLOWED rather than passed down. A key
        // that fell through would reach the composer underneath, which the
        // person cannot see and did not mean to type into.
        for other in ["a", "f5", "tab", "backspace", "1"] {
            assert_eq!(gallery_key(other), Gallery::Ignore, "{other}");
        }
    }

    #[test]
    fn the_review_gallery_reads_oldest_first_and_names_the_answer() {
        let mut b = Bench::new();
        b.apply(decision("one"));
        b.apply(decision("two"));
        // Nothing answered yet: an empty gallery, and the button that opens
        // it is simply not offered.
        assert!(reviewed(&b.all_newest_first().cloned().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn a_question_only_goes_away_when_the_agent_stops_waiting() {
        let a = SurfaceId("a".into());
        let b = SurfaceId("b".into());

        let settled = SETTLE_SWEEPS;

        // The first bug: still waiting, screen unreadable because the picker
        // scrolled its own question line off the top. Keep the card.
        assert_eq!(live_move(true, None, Some(&a), 0), LiveMove::Keep);

        // The only thing that takes a card down, and only once it has been
        // true for long enough to believe.
        assert_eq!(live_move(false, None, Some(&a), settled), LiveMove::Retire);
        assert_eq!(
            live_move(false, Some(&a), Some(&a), settled),
            LiveMove::Retire,
            "not waiting wins over a stale parse"
        );

        // Ordinary progress through a round.
        assert_eq!(live_move(true, Some(&b), Some(&a), 0), LiveMove::Replace);
        assert_eq!(live_move(true, Some(&a), Some(&a), 0), LiveMove::Keep);

        // First arrival, and the quiet cases.
        assert_eq!(live_move(true, Some(&a), None, 0), LiveMove::Replace);
        assert_eq!(live_move(true, None, None, 0), LiveMove::Keep);
        assert_eq!(live_move(false, None, None, settled), LiveMove::Keep);
        assert_eq!(
            live_move(false, Some(&a), None, settled),
            LiveMove::Keep,
            "a question read off a screen nobody is waiting on is not live"
        );
    }

    #[test]
    fn a_redraw_does_not_answer_a_question() {
        // Fullscreen resizes the pseudoterminal and the agent repaints its
        // whole TUI. For a beat in the middle of that the picker's footer is
        // not on screen, so the pane reads "not waiting" — and the card used
        // to come down on the strength of that one blink.
        let a = SurfaceId("a".into());
        // LITERAL sweep counts, not `0..SETTLE_SWEEPS`.
        //
        // Written against the constant, this test goes vacuous the moment
        // somebody sets the constant to zero: the loop body never runs and the
        // retirement assertion passes on the very first sample. A mutation run
        // caught exactly that — the debounce was disabled and the test stayed
        // green, which is worse than having no test at all.
        assert_eq!(
            live_move(false, None, Some(&a), 0),
            LiveMove::Keep,
            "the blink itself must not retire anything"
        );
        assert_eq!(live_move(false, None, Some(&a), 1), LiveMove::Keep);
        assert_eq!(live_move(false, None, Some(&a), 2), LiveMove::Keep);
        // And a genuine answer still lands, a sweep later.
        assert_eq!(live_move(false, None, Some(&a), 3), LiveMove::Retire);
        assert_eq!(live_move(false, None, Some(&a), 9), LiveMove::Retire);
        // The constant and the numbers above have to agree, or this test is
        // asserting something other than what ships.
        assert_eq!(SETTLE_SWEEPS, 3, "the numbers in this test are literal");
    }

    #[test]
    fn an_option_after_the_submit_button_is_one_arrow_further_down() {
        // The picker's order on a five-option multi-select with a trailing
        // "Chat about this": 1,2,3,4,5,Submit,6. Submit is at 5.
        let at = Some(5);
        for before in 0..5 {
            assert_eq!(nav_index(before, at), before, "options above Submit");
        }
        assert_eq!(nav_index(5, at), 6, "the option BELOW Submit shifts down");
        assert_eq!(nav_index(6, at), 7);
        // No Submit button, no shift — which is every single-choice question.
        for i in 0..8 {
            assert_eq!(nav_index(i, None), i);
        }
    }

    #[test]
    fn moving_the_agents_caret_costs_one_arrow_per_column() {
        assert_eq!(caret_move(3, 3), Vec::<u8>::new(), "already there");
        assert_eq!(caret_move(0, 2), b"\x1b[C\x1b[C".to_vec());
        assert_eq!(caret_move(5, 3), b"\x1b[D\x1b[D".to_vec());
    }

    #[test]
    fn a_command_beside_an_empty_draft_is_just_the_command() {
        // Nothing to move out of the way and nothing to put back, so the bytes
        // are the ones this has always sent — which is what keeps the ordinary
        // case (press a dial, type nothing) exactly as it was.
        assert_eq!(
            dial_bytes("/model opus", &Line::new()),
            typed_line("/model opus")
        );
        assert!(
            restore_bytes("", 0).is_empty(),
            "nothing was taken away, so nothing is typed back"
        );
    }

    #[test]
    fn a_command_beside_a_draft_erases_it_first_and_types_it_back_after() {
        // The bug this exists for: the command used to be typed at the END of
        // the person's unsent prompt, and the return key sent both as one — the
        // prompt answered at the strength it was being changed away from.
        let mut draft = Line::holding("count the tests");
        for _ in 0..5 {
            draft.apply(Edit::Move(Motion::Left));
        }
        let caret = draft.caret();
        assert_eq!(caret, 10, "ten characters in, mid-word");

        let sent = dial_bytes("/effort max", &draft);
        let mut want = Vec::new();
        want.extend(caret_move(caret, 0)); // to column zero...
        want.extend(replace_bytes()); // ...and kill what is ahead
        want.extend(typed_line("/effort max")); // the command, sent alone
        assert_eq!(sent, want);

        // The erase happens before the command. That ordering is the fix.
        let kill = sent.iter().position(|b| *b == 0x0b).expect("the kill");
        let submit = sent.iter().position(|b| *b == b'\r').expect("the return");
        assert!(
            kill < submit,
            "the line is cleared before the command is sent"
        );
        // And the draft is NOT in this write at all. It used to be, in the
        // same breath as the command — which put it into the harness's
        // confirmation picker, where the digits in somebody's sentence pick
        // options. It comes back in its own write, once that picker is gone.
        assert!(
            !sent.windows(5).any(|w| w == b"count"),
            "the draft went out beside the command again: {:?}",
            String::from_utf8_lossy(&sent)
        );
        let mut back = Vec::new();
        back.extend_from_slice(b"count the tests");
        back.extend(caret_move(15, caret));
        assert_eq!(restore_bytes("count the tests", caret), back);
    }

    #[test]
    fn a_dial_waits_for_the_harnesss_own_question_then_answers_it() {
        use crate::screenread::Picker;
        // The four screens a press can be looking at, and what each one is
        // worth. Claude Code 2.1.274 answers `/effort max` with a picker
        // headed "Change effort level?" — so the press is not the change, and
        // everything written before that picker is answered lands IN it.
        let sent = DialSent {
            which: Dial::Effort,
            text: "the draft".into(),
            caret: 3,
            sent_ms: 1_000,
            answered: false,
        };
        // Nothing on screen yet, and no time gone: hold.
        assert_eq!(dial_step(&sent, Picker::None, 1_100), DialStep::Wait);
        // Its picker, with the cursor on the second row and Yes on the first.
        assert_eq!(
            dial_step(&sent, Picker::Confirm { yes: 0, cursor: 1 }, 1_200),
            DialStep::Answer { to: 0, from: 1 }
        );
        // Answered, still up: give the keypress a moment.
        let answered = DialSent {
            answered: true,
            ..sent.clone()
        };
        assert_eq!(
            dial_step(&answered, Picker::Confirm { yes: 0, cursor: 0 }, 1_300),
            DialStep::Wait
        );
        // Answered and gone. The draft goes back.
        assert_eq!(dial_step(&answered, Picker::None, 1_300), DialStep::Settle);
        // NEVER ASKED, and the window is up: a harness that simply applies
        // the command must not cost the person their sentence. This is the
        // case that makes the hold safe to have at all.
        assert_eq!(
            dial_step(&sent, Picker::None, 1_000 + DIAL_ASK_MS),
            DialStep::Settle
        );
        // SOMEBODY ELSE'S MENU, long past every deadline, is still a menu.
        //
        // This is the case a clock gets wrong. The draft is held rather than
        // typed, because typing it into a picker is the harm — its digits are
        // option numbers — and holding costs nothing that is not already
        // lost: no picker reads a sentence from anybody. It ends when the
        // screen is a line editor again, whoever answers.
        for waited in [1_100, 1_000 + DIAL_ASK_MS, 1_000 + 600_000] {
            assert_eq!(
                dial_step(&sent, Picker::Other, waited),
                DialStep::Wait,
                "a draft was typed into a menu after {waited}ms"
            );
            assert_eq!(dial_step(&answered, Picker::Other, waited), DialStep::Wait);
        }
    }

    #[test]
    fn an_open_dial_menu_takes_the_next_click_wherever_it_lands() {
        // The complaint: a menu opened to look at, not picked from, stayed on
        // the glass over everything. Its own two controls are what it is, and
        // everything else — including the bench's bare background, which
        // reaches no control at all — closes it.
        assert!(dial_dismisses(true, None), "the empty background dismisses");
        assert!(dial_dismisses(true, Some(&Hit::CloseCard)));
        assert!(dial_dismisses(true, Some(&Hit::Composer)));
        assert!(!dial_dismisses(true, Some(&Hit::Dial(Dial::Effort))));
        assert!(
            !dial_dismisses(true, Some(&Hit::Dial(Dial::Model))),
            "the OTHER dial opens its own list rather than dismissing this one"
        );
        assert!(!dial_dismisses(true, Some(&Hit::DialPick(Dial::Effort, 2))));
        // And with nothing open, nothing is ever swallowed.
        for hit in [None, Some(&Hit::CloseCard), Some(&Hit::Composer)] {
            assert!(!dial_dismisses(false, hit));
        }
    }

    /// The dial's command goes in beside the draft, not through the composer.
    ///
    /// [`aside_bytes`] only helps if it is the thing that runs, and the bug was
    /// a call site rather than a calculation: `bench_dial_pick` used
    /// `bench_say`, which puts its argument IN the composer — the person's
    /// unsent prompt — and sends that. Nothing in the type system stops that
    /// line coming back, so this reads the call site.
    ///
    /// Comments are stripped first, so neither this test's prose nor the call
    /// site's own explanation can satisfy it.
    #[test]
    fn the_dial_types_beside_the_draft_rather_than_through_the_composer() {
        let src = include_str!("pane/bench.rs");
        let code: String = src
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let at = code.find("fn bench_dial_pick(").expect("the dial's press");
        let body = &code[at..];
        // The first closing brace at the impl's own indent ends the function;
        // every brace inside it is deeper.
        let end = body.find("\n    }").map(|i| i + 6).unwrap_or(body.len());
        let body = &body[..end];
        assert!(
            body.contains("dial_bytes("),
            "the dial no longer types beside the draft:\n{body}"
        );
        assert!(
            !body.contains("bench_say("),
            "the dial types through the composer, which holds the person's prompt:\n{body}"
        );
        // And it ARMS. The erase is only half a fix: without the wait, the
        // draft is typed back into the harness's confirmation picker, and the
        // dial never gets confirmed at all. `wb_dial_sent` is what makes the
        // rest of the press happen, and a call site that drops it would leave
        // a composer that erases your sentence and changes nothing.
        assert!(
            body.contains("wb_dial_sent = Some("),
            "the press does not wait for the harness to confirm it:\n{body}"
        );
    }

    #[test]
    fn a_retyped_draft_no_longer_claims_the_images_it_lost() {
        // An `[Image #7]` is the agent's reference to something it read off the
        // clipboard itself, so the erase takes it and no retyping restores it.
        // The count is the mirror's only claim about attachments, and a claim
        // that outlives what it describes is worse than no claim.
        let mut draft = Line::holding("look at this");
        draft.note_paste();
        assert_eq!(draft.pasted(), 1);
        draft.forget_pastes();
        assert_eq!(draft.pasted(), 0);
        assert_eq!(draft.text(), "look at this", "the words are not the image");
    }

    // -----------------------------------------------------------------
    // What a pane of a given size shows.
    //
    // A table, because these arrived one photograph at a time and each one
    // cost a round trip: the rail taking a third of a pane, the composer
    // disappearing below a threshold, the hint line staying where there was
    // no room for it. None of them was a rendering bug — each was a rule
    // written where no assertion could reach it.
    // -----------------------------------------------------------------

    #[test]
    fn an_agent_pane_always_has_somewhere_to_type() {
        // The rule with no exceptions. A bench you cannot answer from is a
        // viewer, and it is the main reason to have the bench open at all.
        for (w, h) in [
            (1400., 900.),
            (700., 600.),
            (460., 400.),
            (300., 200.),
            (220., 150.),
            (120., 90.),
        ] {
            let sh = shows(w, h, true, true, false);
            assert!(sh.composer, "{w}x{h} left an agent pane with no composer");
        }
        // And a shell has none at any size: "type to the agent" with no agent
        // on the far end is an offer nobody can accept.
        assert!(!shows(1400., 900., false, true, false).composer);
    }

    #[test]
    fn a_small_pane_tightens_the_composer_rather_than_dropping_it() {
        let roomy = shows(1400., 900., true, true, false);
        assert!(!roomy.tight, "a big pane draws the full composer");
        assert!(roomy.hint, "and says what the keys do");

        let cramped = shows(260., 200., true, true, false);
        assert!(cramped.composer, "still there");
        assert!(cramped.tight, "at its small size");
        assert!(!cramped.hint, "without the hint it has no room for");
    }

    /// The gauge a tray slider reading 65% actually hands over. Written out
    /// rather than spelled `0.65`, because reading the percent as the factor is
    /// the one mistake that inverts this whole feature: the channel runs
    /// `0.6..=2.0`, so most of the slider's travel makes type BIGGER.
    const AT_65_PERCENT: f32 = 1.51;

    #[test]
    fn the_slider_percent_is_not_the_factor() {
        // The trap, pinned. A pane photographed at "text size 65%" is running a
        // 1.51x multiplier, and anyone who reads that as "shrink to 65%" builds
        // the dial backwards.
        let k = crate::theme::GradeKey::TextSize;
        let at_65 = k.from_percent(65.0);
        assert!(
            (at_65 - AT_65_PERCENT).abs() < 0.01,
            "65% on the tray is {at_65}x, not 0.65x"
        );
        assert!(at_65 > 1.0, "most of this slider's travel is ENLARGEMENT");
        // Neutral is barely a third of the way along, which is why the dial
        // looks like it is turned down when it is turned up.
        let neutral_at = k.to_percent(1.0);
        assert!(
            (28.0..30.0).contains(&neutral_at),
            "neutral sits at {neutral_at}%"
        );
    }

    #[test]
    fn the_type_ramp_is_ordered_and_every_rung_is_reachable() {
        // Nine rungs, strictly increasing. A ramp with a repeat in it is two
        // names for one size, and the second name never gets used.
        let sizes: Vec<f32> = Step::ALL.iter().map(|s| s.base()).collect();
        for pair in sizes.windows(2) {
            assert!(
                pair[1] > pair[0],
                "the ramp is not strictly increasing: {pair:?}"
            );
        }
        assert_eq!(Step::ALL.len(), 9);
        // The span is the one the surface actually needed — the smallest label
        // to the one standalone glyph. Pinned so a rung cannot be quietly
        // widened into a size nothing was designed at.
        assert_eq!(sizes.first().copied(), Some(9.0));
        assert_eq!(sizes.last().copied(), Some(20.0));
    }

    #[test]
    fn the_gauge_multiplies_every_rung_and_neutral_is_the_identity() {
        let neutral = Type::neutral();
        for step in Step::ALL {
            assert_eq!(
                neutral.pt(step),
                step.base(),
                "a neutral gauge must be the identity at {step:?}"
            );
        }
        // …and at Parker's own setting every rung moves by exactly the factor,
        // which is what makes this ONE dial rather than nine tuned constants.
        let big = Type::at(AT_65_PERCENT);
        for step in Step::ALL {
            let want = step.base() * AT_65_PERCENT;
            assert!(
                (big.pt(step) - want).abs() < 1e-4,
                "{step:?}: {} != {want}",
                big.pt(step)
            );
        }
        // A box that holds text scales with the text in it, off the same factor.
        assert!((big.px(110.) - 110. * AT_65_PERCENT).abs() < 1e-3);
        assert_eq!(neutral.px(110.), 110.);
    }

    /// Unknown is not zero, at the one place it would be invisible.
    #[test]
    fn a_gauge_that_is_not_a_number_reads_as_neutral_never_as_zero() {
        // Each of these is a plausible way for a gauge to arrive broken — an
        // unparsed field, a division that went wrong, a channel read before it
        // was set. Every one of them, resolved naively, renders the whole bench
        // at zero points: a blank rectangle, which reads as the FEATURE being
        // broken rather than as one number being wrong.
        for bad in [f32::NAN, 0.0, -1.0, -0.0, f32::INFINITY, f32::NEG_INFINITY] {
            let t = Type::at(bad);
            assert_eq!(
                t.pt(Step::Body),
                Step::Body.base(),
                "a gauge of {bad} must fall back to neutral"
            );
            assert!(t.pt(Step::Tag) > 0.0, "nothing on the bench draws at 0pt");
        }
        // The composer takes the same guarantee through its own entry point.
        for bad in [f32::NAN, 0.0, -1.0, f32::INFINITY] {
            let pt = composer_pt(40, 900., 300., bad);
            assert!(pt >= COMPOSER_MIN_PT, "a gauge of {bad} gave {pt}pt");
        }
    }

    #[test]
    fn the_composer_halved_its_ramp_and_moved_its_floor_to_what_the_eye_gets() {
        // Half of the old 17 / 14.5 / 12.5, to the quarter point. The ask was
        // exact — *"SMALLER! maybe 1/2 the size as default"* — so the arithmetic
        // is asserted rather than eyeballed.
        for (old, new) in [(17.0, 0), (14.5, 1), (12.5, 2)] {
            let want = old / 2.0;
            assert!(
                (COMPOSER_STEPS[new] - want).abs() <= 0.15,
                "step {new} is {} and half of {old} is {want}",
                COMPOSER_STEPS[new]
            );
        }

        // A roomy box at a short prompt: the top step, with the gauge on it.
        assert_eq!(composer_pt(40, 900., 300., 1.0), COMPOSER_MIN_PT);
        let big = composer_pt(40, 900., 300., AT_65_PERCENT);
        assert!(
            (big - COMPOSER_STEPS[0] * AT_65_PERCENT).abs() < 1e-4,
            "{big}"
        );
        // The whole point of the change, stated as the comparison Parker will
        // make: what he sees now is meaningfully smaller than the 17pt he was
        // objecting to, at his own gauge.
        assert!(big < 17.0 * 0.85, "{big} is not smaller than the old 17pt");

        // Enough text to need a step down. It takes MORE text to get there than
        // it used to, which is the halving working rather than a bug: 1,200
        // characters forced a step at the old sizes and comfortably fits now.
        assert_eq!(
            composer_pt(1200, 900., 300., AT_65_PERCENT),
            big,
            "1,200 characters used to force a step down and should no longer"
        );
        let mid = composer_pt(2500, 900., 300., AT_65_PERCENT);
        assert!(mid < big, "a long draft steps down: {mid} vs {big}");
        // A 1,500-word prompt — roughly 9,000 characters — must NOT be
        // squeezed into unreadability. It bottoms out and scrolls instead.
        let floor = composer_pt(9000, 900., 300., AT_65_PERCENT);
        assert!(floor >= COMPOSER_MIN_PT, "the floor holds at {floor}");
        assert_eq!(composer_pt(500_000, 900., 300., AT_65_PERCENT), floor);

        // THE FLOOR IS IN RENDERED POINTS. Read the bottom of the dial off the
        // dial itself rather than writing 0.6 here: what has to hold is a claim
        // about the range a person can actually reach, and a literal would keep
        // passing after somebody widened it.
        let bottom = crate::theme::GradeKey::TextSize.from_percent(0.0);
        let tiny = composer_pt(40, 900., 300., bottom);
        assert_eq!(tiny, COMPOSER_MIN_PT, "a turned-down pane still reads");
        assert!(
            COMPOSER_STEPS[0] * bottom < COMPOSER_MIN_PT,
            "…and it had to: the base ramp alone would put this at {}pt",
            COMPOSER_STEPS[0] * bottom
        );

        // And a box with no room yet does not divide by zero on the first frame.
        assert!(composer_pt(100, 0., 0., 1.0) >= COMPOSER_MIN_PT);

        // The dial's top end is the ramp this replaced, exactly. Turned all the
        // way up, a person gets 17 / 14.5 / 12.5 back — so nothing was taken
        // away, it was moved from a constant to a place they can reach.
        let top = crate::theme::GradeKey::TextSize.from_percent(100.0);
        for (i, old) in [17.0, 14.5, 12.5].into_iter().enumerate() {
            let now = (COMPOSER_STEPS[i] * top).max(COMPOSER_MIN_PT);
            assert!((now - old).abs() < 0.3, "step {i}: {now} vs the old {old}");
        }
    }

    #[test]
    fn a_draft_too_long_to_show_says_how_much_is_hidden() {
        // Absence of a number means it all fits; a number means it does not,
        // and the composer can say so rather than quietly clipping.
        assert_eq!(composer_hidden(40, 900., 300., 1.0), None);
        let over = composer_hidden(9000, 900., 300., 1.0).expect("9k chars cannot fit");
        assert!(over > 0 && over < 9000, "{over}");
        // Monotonic: a longer draft never hides less.
        let more = composer_hidden(20_000, 900., 300., 1.0).expect("20k cannot fit");
        assert!(more > over, "{more} vs {over}");
        assert_eq!(composer_hidden(100, 0., 0., 1.0), None, "no box, no claim");
        // The two agree under a gauge as well as at neutral — they must, since
        // one calls the other, and this is the assertion that catches a future
        // edit teaching only one of them about the dial. BIGGER type hides MORE
        // of the same draft, which is the direction that surprises people.
        //
        // Measured at the TOP of the dial, and the reason is worth keeping: for
        // most of the slider's travel the legibility floor is what the smallest
        // step resolves to at every gauge, so two different gauges render the
        // same size and hide the same amount. The dial only reaches this
        // comparison once the halved bottom step clears the floor on its own —
        // which is a fact about the floor doing its job, and it cost a wrong
        // assertion here to notice.
        let at_neutral = composer_hidden(9000, 900., 300., 1.0).expect("fits nothing");
        let top = crate::theme::GradeKey::TextSize.from_percent(100.0);
        assert!(
            COMPOSER_STEPS[2] * top > COMPOSER_MIN_PT,
            "below this the floor binds at both gauges and there is nothing to compare"
        );
        let enlarged = composer_hidden(9000, 900., 300., top).expect("fits less");
        assert!(
            enlarged > at_neutral,
            "enlarging the type must hide more, not less: {enlarged} vs {at_neutral}"
        );
    }

    #[test]
    fn an_offer_sits_at_eye_level_and_a_transcript_on_the_floor() {
        // The whole table. Two booleans, and three different answers — an offer
        // is neither of the other two, which is the point of the third variant.
        assert_eq!(
            body_anchor(false, true),
            Anchor::Eye,
            "an offer is not against the ceiling"
        );
        assert_eq!(body_anchor(true, false), Anchor::Top, "an opened card");
        assert_eq!(
            body_anchor(true, true),
            Anchor::Top,
            "a card over an offer is a card: it may be taller than the box"
        );
        assert_eq!(
            body_anchor(false, false),
            Anchor::Bottom,
            "a conversation still sits on its composer"
        );
    }

    /// The dials are grey for exactly the states where a press would land on a
    /// turn nobody meant it for.
    #[test]
    fn a_dial_is_pressable_only_when_a_keystroke_would_be_read_now() {
        for live in [
            AgentState::Idle,
            AgentState::Done,
            AgentState::Asking,
            AgentState::Blocked,
            AgentState::Paused,
        ] {
            assert!(dials_live(live), "{live:?} is a state a person can type in");
        }
        for grey in [AgentState::Working, AgentState::Reading, AgentState::Exited] {
            assert!(!dials_live(grey), "{grey:?} must not take a press");
        }
    }

    /// One slot, two verbs, and the second is the one that did not exist.
    #[test]
    fn the_strips_verb_ends_a_live_agent_and_starts_the_next_one() {
        assert_eq!(
            strip_verb(AgentState::Exited, true),
            StripVerb::Launch,
            "a dead pane has nothing to end"
        );
        // Every other state has something to end — including Blocked and
        // Reading, which are exactly the states a person reaches for this in.
        for live in [
            AgentState::Idle,
            AgentState::Done,
            AgentState::Asking,
            AgentState::Blocked,
            AgentState::Working,
            AgentState::Reading,
            AgentState::Paused,
        ] {
            assert_eq!(strip_verb(live, true), StripVerb::End, "{live:?}");
        }
    }

    /// The case the whole second half of the ask is about: the agent quit
    /// cleanly, so the pane is a shell again and the ladder still reads Idle.
    /// Nothing in the STATE says the agent is gone, which is why the verb
    /// cannot be decided from the state alone.
    #[test]
    fn an_agent_that_quit_cleanly_leaves_the_launch_verb_behind_it() {
        for state in [
            AgentState::Idle,
            AgentState::Done,
            AgentState::Asking,
            AgentState::Blocked,
            AgentState::Working,
            AgentState::Reading,
            AgentState::Paused,
            AgentState::Exited,
        ] {
            assert_eq!(
                strip_verb(state, false),
                StripVerb::Launch,
                "no agent present, whatever {state:?} claims"
            );
        }
    }

    /// Return starts an agent only where there is nothing else for it to do —
    /// and the case that makes the guard load-bearing is the pane whose agent
    /// QUIT: it offers the launch and it is still holding that agent's cards.
    #[test]
    fn return_starts_an_agent_only_on_a_bench_with_nothing_else_on_it() {
        assert!(
            return_launches(StripVerb::Launch, false),
            "a fresh bench offers one thing and return should take it"
        );
        assert!(
            !return_launches(StripVerb::Launch, true),
            "an open card's first verb still wins the key"
        );
        assert!(
            !return_launches(StripVerb::End, false),
            "there is already an agent in this pane"
        );
        assert!(!return_launches(StripVerb::End, true));
    }

    /// The list hangs under the dial that opened it, sharing its right edge —
    /// and the number it is placed with is relative to the bench's root, not to
    /// the window, because that is the box it is a child of.
    #[test]
    fn a_dials_list_hangs_under_that_dial_and_not_under_the_strips_end() {
        // A root at (100, 50) 800 wide; a dial 420 across it, 20 tall.
        let root = Rect {
            x: 100.,
            y: 50.,
            w: 800.,
            h: 400.,
        };
        let model = Rect {
            x: 520.,
            y: 60.,
            w: 70.,
            h: 20.,
        };
        let effort = Rect {
            x: 600.,
            y: 60.,
            w: 60.,
            h: 20.,
        };
        let (right, top) = dial_drop(Some(model), Some(root)).unwrap();
        // 900 (root's right edge) − 590 (the dial's) = 310.
        assert_eq!(right, 310.);
        assert_eq!(top, 60. + 20. + DIAL_DROP_GAP - 50.);
        // The whole complaint: the two dials must not resolve to one place.
        let (other, _) = dial_drop(Some(effort), Some(root)).unwrap();
        assert_ne!(right, other, "each dial drops under itself");
        assert_eq!(other, 240.);
    }

    /// An unmeasured rectangle is not the origin. Before anything has painted
    /// there is no answer, and the caller keeps its old fixed corner rather
    /// than stacking the list in the top-left of the bench.
    #[test]
    fn an_unmeasured_dial_has_no_place_rather_than_the_corner() {
        let r = Rect {
            x: 0.,
            y: 0.,
            w: 10.,
            h: 10.,
        };
        assert_eq!(dial_drop(None, Some(r)), None);
        assert_eq!(dial_drop(Some(r), None), None);
        assert_eq!(dial_drop(None, None), None);
    }

    #[test]
    fn the_three_anchors_are_three_different_answers() {
        // A third variant that collapsed onto one of the other two would be a
        // rename, not a placement — and the whole complaint was that "not the
        // floor" had silently meant "the ceiling".
        let all = [Anchor::Top, Anchor::Bottom, Anchor::Eye];
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert_ne!(a, b, "{a:?} and {b:?} must be distinguishable");
            }
        }
    }

    #[test]
    fn the_composer_grows_but_never_eats_the_conversation() {
        // Both halves of the rule, which pull against each other: a long
        // message must not be clipped, and the box must not take the pane.
        for h in [1400., 900., 600., 300., 160.] {
            let sh = shows(1200., h, true, true, true);
            assert!(
                sh.composer_max >= COMPOSER_MIN_MAX,
                "{h}: {} is too short to hold a sentence",
                sh.composer_max
            );
            if h >= 400. {
                assert!(
                    sh.composer_max <= h * 0.4,
                    "{h}: the composer could take {} of it",
                    sh.composer_max
                );
            }
        }
    }

    #[test]
    fn the_terminal_mirror_is_off_unless_somebody_asks_for_it() {
        // It is scaffolding, and the surfaces beside it are the product. The
        // flag exists so that a person debugging the screen reader can see
        // what it sees, not so that anybody has to live with it.
        assert!(
            !shows(1400., 900., true, true, false).mirror,
            "the bench does not draw the agent's scrollback by default"
        );
    }

    #[test]
    fn the_hint_is_an_invitation_and_stops_once_it_is_accepted() {
        assert!(shows(1400., 900., true, true, false).hint);
        assert!(
            !shows(1400., 900., true, true, true).hint,
            "a line is already being typed; the invitation is clutter"
        );
    }

    #[test]
    fn the_rail_never_takes_the_pane_the_bench_needs() {
        // 540 is a real pane on this machine — a tiled half beside a left
        // bar. At a flat 208 the rail took 38% of it.
        for w in [540., 700., 900., 1400.] {
            let sh = shows(w, 800., true, true, false);
            if let RailFit::Open(rail) = sh.rail {
                let body = w - rail as f32;
                assert!(
                    body >= w * 0.66,
                    "{w}: the rail took {rail}, leaving {body} for the bench"
                );
            }
        }
    }

    // -----------------------------------------------------------------
    // Which decision is the one in force.
    // -----------------------------------------------------------------

    #[test]
    fn the_shelf_puts_what_is_waiting_first_and_names_what_stands() {
        let mut b = Bench::new();
        b.apply(decision("old"));
        b.apply(decision("new"));
        b.set_shelf(Shelf::Decisions);

        // Two unanswered: the newest is the one to deal with, the other is
        // queued behind it. Neither pretends to stand.
        let rows = b.rows_for(Shelf::Decisions);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].standing, Standing::Waiting);
        assert_eq!(rows[1].standing, Standing::Queued);
        assert!(
            !rows.iter().any(|r| r.standing == Standing::Current),
            "nothing stands while a question is open"
        );
    }

    #[test]
    fn every_agent_state_says_something_a_person_would_say() {
        use AgentState::*;
        for st in [
            Asking, Blocked, Done, Exited, Working, Reading, Paused, Idle,
        ] {
            let w = st.word();
            assert!(!w.is_empty(), "{st:?} has no word");
            assert!(
                w.chars().all(|c| c.is_alphanumeric() || c == ' '),
                "{st:?} still carries a glyph: {w:?}"
            );
            assert!(
                w.chars().next().is_some_and(char::is_uppercase),
                "{st:?} is not sentence case: {w:?}"
            );
        }
        // The two that SURPRISE a person are the two that are urgent, and they
        // are the only two — a "Finished" agent needs nothing from anybody,
        // and a paused one is holding still because somebody told it to.
        assert!(Asking.urgent() && Blocked.urgent());
        for st in [Done, Exited, Working, Reading, Paused, Idle] {
            assert!(!st.urgent(), "{st:?} should not be shouting");
        }
        // And the colour comes from the same table as every other meaning on
        // the bench, rather than from parsing the word.
        assert_eq!(Asking.tint(), Tint::Waiting);
        assert_eq!(
            Paused.tint(),
            Tint::Waiting,
            "quiet is about the phosphor, not about being findable"
        );
        assert_eq!(Done.tint(), Tint::Settled);
        assert_eq!(Idle.tint(), Tint::Unknown);
    }

    #[test]
    fn a_shelf_never_shows_two_live_questions_waiting_at_once() {
        // Only one picker can be on a screen, so two LIVE waiting questions on
        // one pane is not a state that exists — it is the bench failing to
        // retire one. Four of them had stacked up when Parker looked at it.
        //
        // Asserted over the bench rather than over `stand`, because the defect
        // was in what reached the bench: a sweep that could report the new
        // question or the retirement of the old one, but never both, so the
        // old one was simply left behind.
        let mut b = Bench::new();
        b.apply(decision("first"));
        b.apply(retire("first"));
        b.apply(decision("second"));
        b.set_shelf(Shelf::Decisions);
        let rows = b.rows_for(Shelf::Decisions);
        assert_eq!(rows.len(), 1, "the answered one should have gone: {rows:?}");
        assert_eq!(rows[0].id.0, "second");
        assert_eq!(rows[0].standing, Standing::Waiting);
    }

    #[test]
    fn exactly_one_row_in_a_shelf_is_ever_lit() {
        // The invariant the whole hierarchy rests on. A glow is a claim about
        // where to look, so two of them is no claim at all — and the shelf
        // that prompted this had three, all identical.
        let cases: Vec<Vec<Tint>> = vec![
            vec![],
            vec![Tint::Settled],
            vec![Tint::Waiting],
            vec![Tint::Waiting, Tint::Waiting, Tint::Waiting],
            vec![Tint::Settled, Tint::Settled, Tint::Settled],
            vec![Tint::Settled, Tint::Waiting, Tint::Settled, Tint::Waiting],
            vec![Tint::Pending, Tint::Ident, Tint::Unknown],
        ];
        for tints in cases {
            let mut rows: Vec<Row> = tints
                .iter()
                .enumerate()
                .map(|(i, t)| row_stub(&format!("r{i}"), *t))
                .collect();
            let n = rows.len();
            stand(&mut rows);
            let lit = rows.iter().filter(|r| r.standing.lit()).count();
            assert!(lit <= 1, "{n} rows of {tints:?} lit {lit} of them at once");
            if n > 0 {
                assert_eq!(lit, 1, "{tints:?}: a non-empty shelf has a head");
                assert!(rows[0].standing.lit(), "and the head is the first row");
            }
        }
    }

    #[test]
    fn the_queue_behind_the_head_is_still_unanswered_but_not_shouting() {
        let mut rows = vec![
            row_stub("newest", Tint::Waiting),
            row_stub("older", Tint::Waiting),
            row_stub("oldest", Tint::Waiting),
        ];
        stand(&mut rows);
        assert_eq!(rows[0].standing, Standing::Waiting);
        assert_eq!(rows[1].standing, Standing::Queued);
        assert_eq!(rows[2].standing, Standing::Queued);
        assert!(!rows[1].standing.lit() && !rows[2].standing.lit());
        // Queued is NOT Past: these are still unanswered, and a row that says
        // it is history when somebody is still waiting on it is a lie.
        assert!(!rows.iter().any(|r| r.standing == Standing::Past));
    }

    #[test]
    fn the_newest_settled_row_is_the_one_that_stands() {
        // Built directly, because what matters is the RULE over a list and
        // not how the list was filled.
        let mut rows = vec![
            row_stub("c", Tint::Settled),
            row_stub("b", Tint::Settled),
            row_stub("a", Tint::Settled),
        ];
        stand(&mut rows);
        assert_eq!(
            rows[0].standing,
            Standing::Current,
            "newest first, so row 0"
        );
        assert_eq!(rows[1].standing, Standing::Past);
        assert_eq!(rows[2].standing, Standing::Past);
        assert_eq!(
            rows.iter()
                .filter(|r| r.standing == Standing::Current)
                .count(),
            1,
            "exactly one thing can stand"
        );
    }

    #[test]
    fn a_waiting_row_rises_above_settled_ones_however_late_it_arrived() {
        // Arrival order newest-first: two settled, then an older unanswered
        // one. The unanswered one is what a person has to deal with, so it
        // goes to the top and nothing below it claims to be current.
        let mut rows = vec![
            row_stub("newest", Tint::Settled),
            row_stub("middle", Tint::Settled),
            row_stub("asked-long-ago", Tint::Waiting),
        ];
        stand(&mut rows);
        assert_eq!(rows[0].id.0, "asked-long-ago");
        assert_eq!(rows[0].standing, Standing::Waiting);
        assert!(rows[0].standing.lit(), "and it is the only lit row");
        assert!(
            !rows.iter().any(|r| r.standing == Standing::Current),
            "while somebody is being waited on, nothing else stands"
        );
    }

    #[test]
    fn an_empty_shelf_does_not_panic_and_nothing_stands() {
        let mut rows: Vec<Row> = Vec::new();
        stand(&mut rows);
        assert!(rows.is_empty());
    }

    fn retire(id: &str) -> crate::surface::Post {
        crate::surface::Post {
            op: crate::surface::Op::Retire,
            id: SurfaceId(id.into()),
            pane: None,
            surface: None,
        }
    }

    fn row_stub(id: &str, tint: Tint) -> Row {
        Row {
            id: SurfaceId(id.into()),
            title: id.into(),
            subtitle: String::new(),
            kind: "question",
            badge: None,
            tint,
            standing: Standing::Past,
            selected: false,
            unseen: false,
        }
    }

    #[test]
    fn every_paste_chord_a_person_might_use_is_a_paste() {
        assert!(is_paste_chord("v", true, false), "the graphical chord");
        assert!(is_paste_chord("v", true, true), "the terminal chord");
        assert!(
            is_paste_chord("V", true, true),
            "shift may capitalise the key"
        );
        assert!(is_paste_chord("insert", false, true), "the old chord");
        // And nothing else is. `v` alone is a letter someone is typing to an
        // agent, and swallowing it would make the composer eat a character.
        assert!(!is_paste_chord("v", false, false));
        assert!(!is_paste_chord("c", true, false));
        assert!(!is_paste_chord("insert", false, false));
    }

    #[test]
    fn an_image_on_the_clipboard_beats_the_text_beside_it() {
        // Exactly what a browser offers when you copy an image. Before this,
        // the text won and the paste was a line of markup.
        let firefox = ["text/html", "text/_moz_htmlcontext", "image/png"];
        assert_eq!(best_image_mime(&firefox), Some("image/png"));

        // A screenshot tool offers only the one type.
        assert_eq!(best_image_mime(&["image/png"]), Some("image/png"));

        // Preference order, not list order.
        assert_eq!(
            best_image_mime(&["image/svg+xml", "image/jpeg"]),
            Some("image/jpeg")
        );

        // Nothing here is an image, and that is an answer.
        assert_eq!(
            best_image_mime(&["text/plain", "STRING", "UTF8_STRING"]),
            None
        );

        // Compositors vary the spelling and the whitespace.
        assert_eq!(best_image_mime(&[" IMAGE/PNG "]), Some("image/png"));
    }

    #[test]
    fn every_offered_image_type_has_an_extension_to_save_it_under() {
        // The two lists have to agree or a paste writes a file whose name
        // says nothing about what is in it. Walk the preference order and
        // demand an extension for each.
        for mime in [
            "image/png",
            "image/jpeg",
            "image/webp",
            "image/gif",
            "image/bmp",
            "image/tiff",
            "image/svg+xml",
        ] {
            assert!(
                best_image_mime(&[mime]) == Some(mime) && ext_of_image_mime(mime).is_some(),
                "{mime} is offered but cannot be saved"
            );
        }
        assert_eq!(ext_of_image_mime("text/plain"), None);
    }

    #[test]
    fn a_dropped_path_arrives_as_one_word() {
        use std::path::PathBuf;
        let p = |s: &str| PathBuf::from(s);

        // The ordinary case stays bare: a quoted path in the middle of a
        // sentence to an agent reads as a quotation, so quotes are spent only
        // where they buy something.
        assert_eq!(
            paths_as_words(&[p("/home/parker/notes.md")]),
            "/home/parker/notes.md"
        );

        // The case this function exists for. Unquoted, the far end's line
        // editor sees two words and the file cannot be found.
        assert_eq!(
            paths_as_words(&[p("/home/parker/Screenshot 2026-09-18.png")]),
            "'/home/parker/Screenshot 2026-09-18.png'"
        );

        // Several files: one word each, separated by one space.
        assert_eq!(
            paths_as_words(&[p("/tmp/a.png"), p("/tmp/b c.png")]),
            "/tmp/a.png '/tmp/b c.png'"
        );

        // A quote inside the name closes, escapes and reopens.
        assert_eq!(paths_as_words(&[p("/tmp/it's.txt")]), r"'/tmp/it'\''s.txt'");

        // Anything a shell would act on is quoted even without a space —
        // `$HOME` in a filename is a real filename, not a variable.
        for hostile in ["/tmp/$HOME", "/tmp/a;rm -rf b", "/tmp/a|b", "/tmp/*"] {
            let out = paths_as_words(&[p(hostile)]);
            assert!(
                out.starts_with('\'') && out.ends_with('\''),
                "{hostile} went out unquoted as {out}"
            );
        }

        // A newline in a filename would SUBMIT the line half-written. It
        // becomes a space, inside quotes, which is wrong about the file and
        // right about the sentence.
        let out = paths_as_words(&[p("/tmp/two\nlines.txt")]);
        assert!(!out.contains('\n'), "a newline reached the line: {out}");
        assert_eq!(out, "'/tmp/two lines.txt'");

        // Nothing dropped is not an empty word; it is nothing typed.
        assert_eq!(paths_as_words(&[]), "");
    }

    #[test]
    fn the_rail_takes_a_share_of_a_middling_pane_rather_than_a_third_of_it() {
        // 540 is a real pane on this machine — a tiled half beside a left bar.
        // At a flat 208 the rail took 38% of it and left the conversation 332
        // pixels, which is below every readable threshold there is.
        match rail_fit(540.0, true) {
            RailFit::Open(w) => {
                assert!(w <= 170, "{w} is still too much of 540");
                assert!(540.0 - w as f32 >= 370.0, "the body keeps the bulk");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn what_fits_depends_on_the_room_and_nothing_else() {
        assert_eq!(embodiment(1200.0, 800.0), Embodiment::Full);
        assert_eq!(embodiment(573.0, 900.0), Embodiment::Full, "a tiled half");
        assert_eq!(embodiment(400.0, 800.0), Embodiment::Compact, "narrow");
        assert_eq!(embodiment(900.0, 200.0), Embodiment::Compact, "short");
        assert_eq!(embodiment(250.0, 800.0), Embodiment::Summary, "a strip");
        assert_eq!(embodiment(900.0, 120.0), Embodiment::Summary, "a sliver");
    }

    #[test]
    fn every_kind_has_a_tint_and_unclassified_is_the_only_unknown() {
        let mut b = Bench::new();
        b.apply(doc("a", "A"));
        b.apply(decision("d"));
        b.apply(changeset("c"));
        b.apply(crate::surface::parse_lenient(
            &json!({ "td":"0.1","kind":"hologram","id":"u","title":"?" }),
            NOW,
            "u.json",
        ));
        let made = b.rows_for(Shelf::Artifacts);
        assert!(made.iter().any(|r| r.tint == Tint::Ident));
        assert!(
            made.iter().any(|r| r.tint == Tint::Unknown),
            "unclassified files with the made"
        );
        let asked = b.rows_for(Shelf::Decisions);
        assert!(asked.iter().any(|r| r.tint == Tint::Waiting));
        assert!(
            asked.iter().any(|r| r.tint == Tint::Pending),
            "a changeset is a thing to answer"
        );
    }

    #[test]
    fn the_face_toggles_both_ways_and_says_which_it_landed_on() {
        let mut b = Bench::new();
        assert_eq!(
            b.face(),
            Face::Terminal,
            "a pane is a terminal until told otherwise"
        );
        assert_eq!(b.toggle_face(), Face::Workbench);
        assert_eq!(b.toggle_face(), Face::Terminal);
        b.set_face(Face::Terminal);
        assert_eq!(
            b.face(),
            Face::Terminal,
            "setting the face it already wears is not a flip"
        );
    }

    // -- selecting the bench's text ----------------------------------------

    /// A card-shaped page: a heading, two wrapped body lines, a three-run
    /// line of chips, then a rail row beside it. Geometry in the same units
    /// the real atoms carry, so the band rules are exercised for real.
    fn page() -> Vec<Atom> {
        let a = |x: f32, y: f32, w: f32, text: &str, region| Atom {
            x,
            y,
            w,
            h: 14.0,
            text: text.to_string(),
            region,
        };
        vec![
            a(10.0, 10.0, 200.0, "The strip backs out", Region::Body),
            a(
                10.0,
                30.0,
                300.0,
                "The undo target was the whole",
                Region::Body,
            ),
            a(
                10.0,
                46.0,
                300.0,
                "session, so a second click",
                Region::Body,
            ),
            // one line, three runs, tops a pixel or two apart
            a(10.0, 80.0, 40.0, "WAITING", Region::Body),
            a(54.0, 81.0, 20.0, "3", Region::Body),
            a(78.0, 80.0, 120.0, "on you", Region::Body),
            // the rail, level with the card's second line on purpose
            a(
                400.0,
                30.0,
                180.0,
                "Should the undo be per-branch?",
                Region::Rail,
            ),
            // chrome, sitting between two selectable runs in index order
            a(10.0, 120.0, 90.0, "END SESSION", Region::Chrome),
            a(10.0, 150.0, 300.0, "A guard now refuses", Region::Body),
        ]
    }

    fn caret(atom: usize, byte: usize) -> Caret {
        Caret { atom, byte }
    }

    #[test]
    fn a_point_inside_a_run_takes_that_run() {
        let p = page();
        assert_eq!(atom_at(&p, 15.0, 15.0), Some(0));
        assert_eq!(atom_at(&p, 420.0, 35.0), Some(6), "the rail row");
    }

    #[test]
    fn chrome_is_never_the_answer_even_under_the_pointer() {
        let p = page();
        // Dead centre of END SESSION.
        let picked = atom_at(&p, 40.0, 127.0).expect("something selectable");
        assert_ne!(
            picked, 7,
            "a press on the strip must not anchor a selection"
        );
        assert!(p[picked].region.selectable());
    }

    #[test]
    fn dragging_into_the_margin_stays_on_the_readers_line() {
        let p = page();
        // Far to the right of the second body line, level with it. The
        // heading above is nearer as the crow flies; the band must win.
        assert_eq!(
            atom_at(&p, 900.0, 36.0),
            Some(6),
            "level with the rail row, which extends furthest right"
        );
        // Left of the card entirely, level with the third line.
        assert_eq!(atom_at(&p, -50.0, 52.0), Some(2));
    }

    #[test]
    fn an_empty_list_selects_nothing_rather_than_panicking() {
        assert_eq!(atom_at(&[], 10.0, 10.0), None);
        assert_eq!(
            atom_at(
                &[Atom {
                    x: 0.,
                    y: 0.,
                    w: 9.,
                    h: 9.,
                    text: String::new(),
                    region: Region::Body
                }],
                1.0,
                1.0
            ),
            None,
            "an empty run is not somewhere a caret can go"
        );
    }

    #[test]
    fn ends_order_a_backwards_drag() {
        let up = Sel {
            anchor: caret(5, 2),
            head: caret(1, 0),
        };
        assert_eq!(up.ends(), (caret(1, 0), caret(5, 2)));
        let within = Sel {
            anchor: caret(2, 9),
            head: caret(2, 3),
        };
        assert_eq!(within.ends(), (caret(2, 3), caret(2, 9)));
    }

    #[test]
    fn a_selection_inside_one_run_is_one_span() {
        let p = page();
        let s = Sel {
            anchor: caret(1, 4),
            head: caret(1, 8),
        };
        assert_eq!(spans(&p, &s), vec![(1, 4..8)]);
        assert_eq!(copy_text(&p, &s), "undo");
    }

    #[test]
    fn a_selection_across_runs_is_partial_whole_partial() {
        let p = page();
        let s = Sel {
            anchor: caret(1, 22),
            head: caret(3, 4),
        };
        assert_eq!(
            spans(&p, &s),
            vec![(1, 22..29), (2, 0..26), (3, 0..4)],
            "the middle run comes whole"
        );
    }

    #[test]
    fn a_wrapped_paragraph_copies_with_newlines_and_a_line_with_spaces() {
        let p = page();
        let s = Sel {
            anchor: caret(1, 0),
            head: caret(5, 6),
        };
        assert_eq!(
            copy_text(&p, &s),
            "The undo target was the whole\nsession, so a second click\n\nWAITING 3 on you",
            "wrapped lines join with a newline, the chip line with spaces, \
             and the visible gap before it with a blank line"
        );
    }

    #[test]
    fn crossing_into_the_rail_always_breaks_the_line() {
        let p = page();
        // The rail row sits level with body run 1 — geometry alone would call
        // these one line, and the region rule must override it.
        let s = Sel {
            anchor: caret(1, 0),
            head: caret(6, 30),
        };
        let out = copy_text(&p, &s);
        assert!(
            out.ends_with("\n\nShould the undo be per-branch?"),
            "a rail row level with a paragraph still starts a block: {out:?}"
        );
    }

    #[test]
    fn a_rail_row_level_with_a_paragraph_still_starts_a_block() {
        // The sharp version of the test above, which the geometry answered on
        // its own: there the two runs sat fifty pixels apart, so the region
        // rule never fired and deleting it outright left every test green.
        // Here the mids are IDENTICAL — every geometric rule says "one line,
        // join with a space" — so the region is the only thing that can
        // produce a break.
        let a = |x: f32, text: &str, region| Atom {
            x,
            y: 40.0,
            w: 150.0,
            h: 14.0,
            text: text.to_string(),
            region,
        };
        let p = vec![
            a(10.0, "the card says this", Region::Body),
            a(400.0, "the rail says this", Region::Rail),
        ];
        let s = Sel {
            anchor: caret(0, 0),
            head: caret(1, 18),
        };
        assert_eq!(
            copy_text(&p, &s),
            "the card says this\n\nthe rail says this",
            "a rail row drawn level with a paragraph is not the same line as \
             it, whatever the geometry says"
        );
        // The control: same geometry, same region, still one line.
        let one = vec![
            a(10.0, "the card says this", Region::Body),
            a(400.0, "and so does this", Region::Body),
        ];
        let s = Sel {
            anchor: caret(0, 0),
            head: caret(1, 16),
        };
        assert_eq!(copy_text(&one, &s), "the card says this and so does this");
    }

    #[test]
    fn a_chrome_run_between_two_body_runs_is_skipped_not_truncated() {
        let p = page();
        let s = Sel {
            anchor: caret(6, 0),
            head: caret(8, 7),
        };
        let picked: Vec<usize> = spans(&p, &s).into_iter().map(|(i, _)| i).collect();
        assert_eq!(picked, vec![6, 8], "END SESSION is passed over, not a wall");
        assert!(!copy_text(&p, &s).contains("END SESSION"));
    }

    #[test]
    fn selecting_the_whole_page_loses_no_selectable_run() {
        let p = page();
        let last = p.len() - 1;
        let s = Sel {
            anchor: caret(0, 0),
            head: caret(last, p[last].text.len()),
        };
        let out = copy_text(&p, &s);
        for a in p.iter().filter(|a| a.region.selectable()) {
            assert!(
                out.contains(&a.text),
                "a whole-page selection dropped {:?} — this is the failure the \
                 feature cannot show on screen",
                a.text
            );
        }
    }

    #[test]
    fn an_empty_selection_copies_nothing() {
        let p = page();
        let s = Sel::at(caret(2, 5));
        assert!(s.is_empty());
        assert!(spans(&p, &s).is_empty());
        assert_eq!(copy_text(&p, &s), "");
    }

    #[test]
    fn a_multibyte_offset_walks_back_to_a_boundary_instead_of_panicking() {
        // "café" — the é is two bytes, so 4 is mid-character.
        let text = "café au lait";
        assert_eq!(on_boundary(text, 4), 3);
        assert_eq!(on_boundary(text, 5), 5);
        assert_eq!(on_boundary(text, 999), text.len());

        let p = vec![Atom {
            x: 0.,
            y: 0.,
            w: 90.,
            h: 14.,
            text: text.to_string(),
            region: Region::Body,
        }];
        let s = Sel {
            anchor: caret(0, 0),
            head: caret(0, 4),
        };
        assert_eq!(
            copy_text(&p, &s),
            "caf",
            "sliced on a boundary, not through é"
        );
    }

    #[test]
    fn a_selection_is_dropped_when_the_tree_moved_under_it() {
        let p = page();
        let s = Sel {
            anchor: caret(1, 0),
            head: caret(2, 5),
        };
        assert!(s.still_valid(&p, "The undo target was the whole"));
        assert!(
            !s.still_valid(&p, "something else entirely"),
            "the anchor's index now points at different text, so the \
             selection describes something the reader never dragged over"
        );
        assert!(!s.still_valid(&[], "The undo target was the whole"));
    }

    #[test]
    fn a_head_past_the_end_of_its_run_is_not_valid() {
        let p = page();
        let s = Sel {
            anchor: caret(1, 0),
            head: caret(2, 9_999),
        };
        assert!(!s.still_valid(&p, "The undo target was the whole"));
    }

    #[test]
    fn spans_never_index_outside_the_list() {
        let p = page();
        // A stale selection pointing past the end must not panic.
        let s = Sel {
            anchor: caret(1, 0),
            head: caret(99, 4),
        };
        let _ = spans(&p, &s);
        let _ = copy_text(&p, &s);
    }
}
