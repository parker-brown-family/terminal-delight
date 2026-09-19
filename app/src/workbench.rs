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
    /// The gist. Always present: a response without one does not parse.
    Gist,
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
            Leaf::Gist => "tldr",
            Leaf::Section(s) => &s.key,
            Leaf::Doubts => "doubts",
        }
    }

    /// The word on its chip.
    pub fn label(self) -> &'a str {
        match self {
            Leaf::Gist => "tl;dr",
            Leaf::Section(s) => &s.label,
            Leaf::Doubts => "doubts",
        }
    }

    pub fn group(self) -> crate::surface::Group {
        match self {
            Leaf::Gist => crate::surface::Group::Reading,
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
    let mut leaves: Vec<Leaf<'_>> = vec![Leaf::Gist];
    leaves.extend(
        r.sections
            .iter()
            .filter(|s| !(promoted_asks && s.register == crate::surface::Register::Asks))
            .map(Leaf::Section),
    );
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
    /// The WHOLE line is selected — what `ctrl+a` leaves behind.
    ///
    /// A boolean rather than a range because select-all is the only selection
    /// this box has: there is no mouse drag over the draft and no shift+arrow
    /// here, so a range would be three states wide and only ever hold two of
    /// them. The next printable character or delete replaces everything; any
    /// motion drops it.
    marked: bool,
}

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
            marked: false,
        }
    }

    /// Select the whole draft — `ctrl+a`, the convention every text box on this
    /// desk answers to.
    ///
    /// The caret goes to the START rather than staying where it was, because
    /// the agent's own line editor is readline-shaped and the keystroke that
    /// reached it moved ITS caret to column zero. The mirror has to agree, or
    /// the replacement typed next lands in a different place on each side.
    /// Parker, on the box not answering the chord: *"Ctrl+a does not highlight
    /// all in the workbench text area"*.
    pub fn mark_all(&mut self) {
        self.marked = !self.text.is_empty();
        self.caret = 0;
    }

    /// Is the whole draft selected?
    pub fn marked(&self) -> bool {
        self.marked
    }

    /// Drop the selection without touching the text — any motion does this.
    pub fn clear_mark(&mut self) {
        self.marked = false;
    }

    /// Take the selection: empty the line and report that it happened, so the
    /// caller knows to tell the far end to do the same.
    pub fn take_marked(&mut self) -> bool {
        if !self.marked {
            return false;
        }
        self.clear();
        true
    }

    /// Record that an image went to the agent with this line.
    pub fn note_paste(&mut self) {
        self.pasted += 1;
    }

    /// The images are no longer on the agent's line, so stop saying they are.
    ///
    /// One caller: [`aside_bytes`] erases the far end's line to make room for a
    /// command and types the TEXT back, and an attachment is not text — the
    /// agent's `[Image #7]` is a reference into something it read from the
    /// clipboard itself, and nothing this side can retype it. A count that
    /// outlived the thing it counted would be the mirror lying about the line.
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

    /// Insert at the caret and step over it. A selected line is REPLACED,
    /// which is what a person who just pressed `ctrl+a` is expecting.
    pub fn insert(&mut self, s: &str) {
        if self.marked {
            self.clear();
        }
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

    /// Put the caret at a character index, clamped to the line.
    pub fn seek(&mut self, to: usize) {
        self.caret = to.min(self.chars());
    }

    pub fn left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.caret = (self.caret + 1).min(self.chars());
    }

    pub fn home(&mut self) {
        self.caret = 0;
    }

    pub fn end(&mut self) {
        self.caret = self.chars();
    }

    /// Apply an [`Edit`]. The one place a key becomes a change to this line,
    /// so the mirror and the agent's own editor cannot drift by having two
    /// slightly different ideas of what `ctrl+w` does.
    pub fn apply(&mut self, edit: Edit) {
        // Every edit but the two that USE the selection drops it. Doing it here
        // rather than in each arm is what stops a new arm silently inheriting a
        // stale highlight.
        if !matches!(edit, Edit::SelectAll | Edit::Backspace | Edit::Delete) {
            self.clear_mark();
        }
        match edit {
            Edit::SelectAll => self.mark_all(),
            Edit::Left => self.left(),
            Edit::Right => self.right(),
            Edit::WordLeft => self.caret = self.word_start(),
            Edit::WordRight => self.caret = self.word_end(),
            Edit::Home => self.home(),
            Edit::End => self.end(),
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
        self.marked = false;
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

pub fn peel(gallery: bool, typing: bool, card_open: bool, card_waits: bool) -> Peel {
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
    Left,
    Right,
    WordLeft,
    WordRight,
    Home,
    End,
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
    /// Sent, and the line starts again.
    Submit,
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
pub fn aside_bytes(command: &str, draft: &Line) -> Vec<u8> {
    let mut out = Vec::new();
    let text = draft.text();
    let end = text.chars().count();
    if end > 0 {
        out.extend(caret_move(draft.caret(), 0));
        out.extend(replace_bytes());
    }
    out.extend(typed_line(command));
    if end > 0 {
        out.extend_from_slice(text.as_bytes());
        out.extend(caret_move(end, draft.caret()));
    }
    out
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

pub fn line_edit(key: &str, ctrl: bool, alt: bool) -> Option<Edit> {
    Some(match key {
        "left" if ctrl || alt => Edit::WordLeft,
        "right" if ctrl || alt => Edit::WordRight,
        "left" => Edit::Left,
        "right" => Edit::Right,
        "home" => Edit::Home,
        "end" => Edit::End,
        "backspace" if ctrl || alt => Edit::KillWordLeft,
        "backspace" => Edit::Backspace,
        "delete" if ctrl || alt => Edit::KillWordRight,
        "delete" => Edit::Delete,
        "enter" => Edit::Submit,
        "a" if ctrl => Edit::SelectAll,
        "e" if ctrl => Edit::End,
        "b" if ctrl => Edit::Left,
        "f" if ctrl => Edit::Right,
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
pub fn dials_live(state: AgentState) -> bool {
    match state {
        AgentState::Idle | AgentState::Done | AgentState::Asking | AgentState::Blocked => true,
        AgentState::Working | AgentState::Reading | AgentState::Exited => false,
    }
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
            AgentState::Idle => "Idle",
        }
    }

    /// Which meaning-colour it borrows, so the card is tinted by the same
    /// table as everything else on the bench.
    pub fn tint(self) -> Tint {
        match self {
            AgentState::Asking | AgentState::Blocked => Tint::Waiting,
            AgentState::Done => Tint::Settled,
            AgentState::Working | AgentState::Reading => Tint::Pending,
            // Nothing is being claimed about an idle or departed agent, and
            // grey is how this surface says so everywhere else.
            AgentState::Idle | AgentState::Exited => Tint::Unknown,
        }
    }

    /// Does this state want a person to look at it now?
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
/// The order is the old ladder with one rung added at the top: `reading` —
/// the bench typed within [`READING_WINDOW_MS`] and the agent has not started
/// working — outranks `asking`, because a picker stays on screen for a moment
/// after the keys land, and a bar saying "Waiting on you" over an answer just
/// given is the surface lying about its own state. It never outranks a pane
/// that is blocked or gone: nothing is reading there.
pub fn agent_state(
    asking: bool,
    blocked: bool,
    done: bool,
    exited: bool,
    thinking: bool,
    reading: bool,
) -> AgentState {
    if reading && !thinking && !blocked && !exited {
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

/// What a keystroke means on the bench while it is READING.
///
/// Extracted from the pane's key handler so the mode rules are a table rather
/// than a branch inside a gpui closure. The rules are small and easy to get
/// subtly wrong — a digit means "answer" only when there is a question to
/// answer, and any ordinary character has to start talking rather than being
/// swallowed — and neither of those can be tested through a render.
/// Chords the WINDOW owns — never a pane's content, on either of its faces.
///
/// # Why this is one table and not two
///
/// A pane can be showing a terminal or a bench, and both of those are *content
/// inside a window*. The window's own gestures — close this pane, split it,
/// open the FOCUS reader, move the highlight — have to survive whichever one is
/// on top, and the way they survive is that the thing on top declines to take
/// them.
///
/// The terminal face has always done this, in `pane::keystroke_bytes`: a short
/// list of alt chords it refuses to encode, so they bubble up to the workspace
/// instead of arriving at somebody's shell as `ESC w`. **The bench never got
/// one**, and it ends both of its key paths by stopping propagation — so on the
/// workbench face every chord in that list was dead. `alt+w` did nothing at
/// all, which is worse than the state it replaced, because the face toggle that
/// used to sit on `alt+w` was at least handled upstream (#524).
///
/// Extracting the list rather than copying it is the point. Two lists drift,
/// and the drift is invisible: nothing fails to compile, nothing fails a test,
/// a chord just quietly stops working on one face.
///
/// # What is deliberately NOT here
///
/// Only chords carrying `alt` (or `control`+`alt`) qualify, and that boundary
/// is doing real work in both directions:
///
/// - `ctrl+c` must reach a running agent. A bench that refused it would take
///   away the only way to interrupt a turn.
/// - `alt+b` / `alt+f` are readline's word motion, which the composer mirrors
///   through [`line_edit`] — so a blanket "the window takes every alt chord"
///   would break typing.
/// - A PLAIN arrow walks the bench's rail and a PLAIN escape peels its
///   overlays. Only the modified forms leave.
pub fn window_chord(key: &str, alt: bool, control: bool) -> bool {
    // ctrl+alt+<anything> walks the left bar's tree. Matched on the modifiers
    // alone, because that pair is not an editing chord anywhere.
    if control && alt {
        return true;
    }
    if !alt {
        return false;
    }
    matches!(
        key,
        "left" | "right" | "up" | "down" | "r" | "v" | "h" | "w" | "k"
    )
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
    /// Not ours; swallowed so it cannot reach a working agent.
    Ignore,
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
        Reading::Ignore
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
        }
    }
}

impl Bench {
    pub fn new() -> Bench {
        Bench::default()
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
            self.selected = self.rows().first().map(|r| r.id.clone());
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
        let mut rows: Vec<Row> = self
            .surfaces
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
                // Filled in below: standing is a property of a row's place in
                // the shelf, which no row can know about itself.
                standing: Standing::Past,
            })
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
    pub fn showing(&self) -> Option<&Surface> {
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

    /// True when the card in the room is the overview's STAND-IN — the newest
    /// reply, standing there because nobody opened anything.
    ///
    /// Not the same question as "is a card showing". A card the person OPENED
    /// is a document they navigated to; the stand-in is the tail of a
    /// conversation, and only the tail can honestly be captioned with the
    /// latest thing the person said. See [`ask_lines`].
    pub fn standing_in(&self) -> bool {
        self.selected.is_none() && self.showing().is_some()
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
    /// guessed from the key's spelling — except for the two keys that have no
    /// section to look up.
    fn group_of_key(&self, id: &SurfaceId, key: &str) -> crate::surface::Group {
        if crate::surface::Register::is_tldr(key) {
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
    pub fn select(&mut self, id: &SurfaceId) {
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
    pub fn waiting_question(&self) -> Option<&Surface> {
        self.surfaces.iter().rev().find(|s| {
            matches!(&s.kind, Kind::Question(q)
                if q.answer == crate::surface::Answered::Waiting)
        })
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
                match self.surfaces.iter_mut().find(|s| s.id == id) {
                    Some(existing) if post.op == Op::Update => existing.merge(incoming),
                    Some(existing) => {
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

    fn response(id: &str, tldr: &str) -> Post {
        post(json!({
            "td": "0.3", "kind": "response", "id": id, "title": tldr,
            "model": {
                "tldr": tldr,
                "eli5": "small words",
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
            vec![(Group::Reading, 4), (Group::Evidence, 2), (Group::Next, 1)],
        );
        assert_eq!(b.picked_tab(&showing.id), None, "nobody has pressed");
        assert_eq!(
            resolve_tab(b.picked_tab(&showing.id), &tabs),
            Some(Group::Reading),
            "so it opens on the reading group"
        );
        assert_eq!(
            resolve_leaf(None, &tabs[0].1).map(|l| l.key()),
            Some("tldr"),
            "…on the gist"
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
        // The commonest reply there is: a gist and nothing else.
        b.apply(post(json!({
            "td": "0.3", "kind": "response", "id": "a", "title": "t",
            "model": { "tldr": "Just the gist." }
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
        assert_eq!(full[0].1[0], "tldr", "the gist leads its group");
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
            "a reply with three groups and four readings gets both rows"
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
        assert_eq!(resolve_leaf(None, reading).map(|l| l.key()), Some("tldr"));
        assert_eq!(
            resolve_leaf(Some("technical"), reading).map(|l| l.key()),
            Some("technical")
        );
        assert_eq!(
            resolve_leaf(Some("gone"), reading).map(|l| l.key()),
            Some("tldr"),
            "a key that is not there any more falls back to the first"
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
                agent_state(a, b, d, e, t, r),
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
    #[test]
    fn the_windows_chords_are_never_a_panes_to_take() {
        // The workspace's own bindings, each read off the handler that binds
        // it: close (main.rs `if ks.key.as_str() == "w"`), the FOCUS reader
        // ("r"), the two splits, and directional pane focus.
        for key in ["w", "r", "v", "h", "left", "right", "up", "down"] {
            assert!(
                window_chord(key, true, false),
                "alt+{key} is the window's and must leave the pane"
            );
        }
        // ctrl+alt+<anything> walks the left bar's tree, on the modifiers alone.
        assert!(window_chord("up", true, true));
        assert!(window_chord("q", true, true), "the pair, not the letter");

        // …and the boundary, which is the half that keeps typing working. Each
        // of these reaching the workspace would break something a person does
        // constantly.
        assert!(
            !window_chord("c", false, true),
            "ctrl+c interrupts an agent"
        );
        assert!(
            !window_chord("b", true, false),
            "alt+b is readline's word-back"
        );
        assert!(!window_chord("f", true, false), "alt+f is word-forward");
        assert!(
            !window_chord("up", false, false),
            "a plain arrow walks the rail"
        );
        assert!(
            !window_chord("escape", false, false),
            "plain esc peels overlays"
        );
        assert!(!window_chord("a", false, false));
        assert!(!window_chord("enter", false, false));
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
        assert_eq!(reading_key("f5", false, None), Reading::Ignore);
        assert_eq!(reading_key("home", false, None), Reading::Ignore);
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
            ("left", true, false, Edit::WordLeft),
            ("left", false, true, Edit::WordLeft),
            ("right", true, false, Edit::WordRight),
            ("right", false, true, Edit::WordRight),
            ("left", false, false, Edit::Left),
            ("right", false, false, Edit::Right),
            // Readline's own chords, because the far end is readline-shaped —
            // except ctrl+a, which is select-all here: this is a text area on a
            // screen, and start-of-line is on `home`.
            ("a", true, false, Edit::SelectAll),
            ("e", true, false, Edit::End),
            ("b", true, false, Edit::Left),
            ("f", true, false, Edit::Right),
            ("w", true, false, Edit::KillWordLeft),
            ("u", true, false, Edit::KillToStart),
            ("k", true, false, Edit::KillToEnd),
            ("d", false, true, Edit::KillWordRight),
            ("backspace", true, false, Edit::KillWordLeft),
            ("delete", true, false, Edit::KillWordRight),
            ("home", false, false, Edit::Home),
            ("end", false, false, Edit::End),
            ("enter", false, false, Edit::Submit),
        ] {
            assert_eq!(
                line_edit(key, ctrl, alt),
                Some(want),
                "{key} ctrl={ctrl} alt={alt}"
            );
        }
        // A plain letter is a letter. `a` unmodified must reach the line as
        // text, or typing the word "and" would send the caret home twice.
        for key in ["a", "e", "w", "k", "u", "d", "b", "f", "z"] {
            assert_eq!(line_edit(key, false, false), None, "{key} alone is text");
        }
        assert_eq!(line_edit("f5", false, false), None);
    }

    #[test]
    fn ctrl_a_selects_the_whole_draft_and_the_next_key_replaces_it() {
        let mut l = Line::holding("the whole thing");
        l.apply(Edit::SelectAll);
        assert!(l.marked(), "ctrl+a selects");
        // The caret sits where the far end's does after the same byte, so the
        // replacement lands in the same place on both sides.
        assert_eq!(l.caret(), 0);
        l.insert("x");
        assert_eq!(l.text(), "x", "typing replaces the selection");
        assert!(!l.marked(), "and the selection is spent");

        // Delete and backspace take the selection whole rather than one char.
        let mut l = Line::holding("gone");
        l.apply(Edit::SelectAll);
        l.apply(Edit::Backspace);
        assert_eq!(l.text(), "");
        let mut l = Line::holding("gone");
        l.apply(Edit::SelectAll);
        l.apply(Edit::Delete);
        assert_eq!(l.text(), "");

        // Any motion drops the selection and leaves the text alone.
        let mut l = Line::holding("kept");
        l.apply(Edit::SelectAll);
        l.apply(Edit::Right);
        assert!(!l.marked(), "a motion drops the selection");
        assert_eq!(l.text(), "kept");

        // An empty draft has nothing to select — a highlight over nothing is a
        // control that looks armed and does nothing.
        let mut l = Line::new();
        l.apply(Edit::SelectAll);
        assert!(!l.marked());

        // The far end is told to kill from the caret ctrl+a just moved.
        assert_eq!(replace_bytes(), vec![0x0b]);
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
        l.apply(Edit::WordLeft);
        assert_eq!(l.caret(), 16, "start of `fox`");
        l.apply(Edit::WordLeft);
        assert_eq!(l.caret(), 10, "start of `brown`");
        // Forward again.
        l.apply(Edit::WordRight);
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
        l.seek(10);
        l.apply(Edit::KillToStart);
        assert_eq!((l.text(), l.caret()), ("brown fox", 0));
        l.seek(5);
        l.apply(Edit::KillToEnd);
        assert_eq!((l.text(), l.caret()), ("brown", 5));

        // Every one of them holds at the ends rather than panicking.
        let mut l = Line::new();
        for edit in [
            Edit::WordLeft,
            Edit::WordRight,
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
        l.apply(Edit::WordLeft);
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
        // Outermost first.
        assert_eq!(peel(true, true, true, true), Peel::Gallery);
        assert_eq!(peel(false, true, true, true), Peel::Typing);

        // THE FLOOR. A card holding a question somebody is being waited on is
        // not something escape may take away — every other meaning of the key
        // here removes the thing the agent is waiting with.
        assert_eq!(peel(false, false, true, true), Peel::Nothing);

        // An ANSWERED card is a record, and a record closes like anything
        // else.
        assert_eq!(peel(false, false, true, false), Peel::Card);
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
            peel(false, false, false, false),
            Peel::Nothing,
            "a quiet bench is still the surface you are on"
        );
        assert_eq!(
            peel(false, false, false, true),
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
            aside_bytes("/model opus", &Line::new()),
            typed_line("/model opus")
        );
    }

    #[test]
    fn a_command_beside_a_draft_erases_it_first_and_types_it_back() {
        // The bug this exists for: the command used to be typed at the END of
        // the person's unsent prompt, and the return key sent both as one — the
        // prompt answered at the strength it was being changed away from.
        let mut draft = Line::holding("count the tests");
        for _ in 0..5 {
            draft.apply(Edit::Left);
        }
        let caret = draft.caret();
        assert_eq!(caret, 10, "ten characters in, mid-word");

        let bytes = aside_bytes("/effort max", &draft);
        let mut want = Vec::new();
        want.extend(caret_move(caret, 0)); // to column zero...
        want.extend(replace_bytes()); // ...and kill what is ahead
        want.extend(typed_line("/effort max")); // the command, sent alone
        want.extend_from_slice(b"count the tests"); // the draft, back again
        want.extend(caret_move(15, caret)); // and the caret where it was
        assert_eq!(bytes, want);

        // The two orderings that make it a fix rather than a rearrangement: the
        // erase happens before the command, and the draft is retyped after the
        // command's return — never before it, which is the old bug exactly.
        let kill = bytes.iter().position(|b| *b == 0x0b).expect("the kill");
        let submit = bytes.iter().position(|b| *b == b'\r').expect("the return");
        let back = bytes
            .windows(5)
            .position(|w| w == b"count")
            .expect("the draft goes back");
        assert!(
            kill < submit,
            "the line is cleared before the command is sent"
        );
        assert!(
            submit < back,
            "the draft is retyped after the command, not into it"
        );
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
            body.contains("aside_bytes("),
            "the dial no longer types beside the draft:\n{body}"
        );
        assert!(
            !body.contains("bench_say("),
            "the dial types through the composer, which holds the person's prompt:\n{body}"
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
        for st in [Asking, Blocked, Done, Exited, Working, Idle] {
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
        // The two that stop an agent are the two that are urgent, and they are
        // the only two — a "Finished" agent needs nothing from anybody.
        assert!(Asking.urgent() && Blocked.urgent());
        for st in [Done, Exited, Working, Idle] {
            assert!(!st.urgent(), "{st:?} should not be shouting");
        }
        // And the colour comes from the same table as every other meaning on
        // the bench, rather than from parsing the word.
        assert_eq!(Asking.tint(), Tint::Waiting);
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
}
