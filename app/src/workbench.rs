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
        Kind::Unclassified(_) => Tint::Unknown,
    }
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

/// How much of a pane the rail may take. Roughly a third is the most a shelf
/// can have before the thing it is a shelf FOR stops being the main event.
pub const RAIL_SHARE: f32 = 0.30;

/// Narrower than this and the rows stop being readable, so it is ticks or
/// nothing.
pub const RAIL_MIN_W: f32 = 132.0;

// ---------------------------------------------------------------------------
// the bench
// ---------------------------------------------------------------------------

/// One row of the rail, ready to draw.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub id: SurfaceId,
    pub title: String,
    pub subtitle: String,
    pub kind: &'static str,
    pub tint: Tint,
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
            .is_some_and(|s| s.kind.shelf() == shelf)
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
        let on = self.surfaces.iter().filter(|s| s.kind.shelf() == shelf);
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
        self.surfaces
            .iter()
            .rev()
            .filter(|s| s.kind.shelf() == shelf)
            .map(|s| Row {
                selected: self.selected.as_ref() == Some(&s.id),
                unseen: self.unseen.contains(&s.id),
                id: s.id.clone(),
                title: s.title.clone(),
                subtitle: s.subtitle(),
                kind: s.kind.id(),
                tint: tint_of(&s.kind),
            })
            .collect()
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
                    Some(existing) => *existing = incoming,
                    None => {
                        self.surfaces.push(incoming);
                        // The cap drops the OLDEST, never the newest: a bench
                        // that silently refused new work once it was full
                        // would look exactly like an agent that stopped.
                        if self.surfaces.len() > crate::surface::PANE_HISTORY_CAP {
                            let dropped = self.surfaces.remove(0);
                            self.unseen.remove(&dropped.id);
                            if self.selected.as_ref() == Some(&dropped.id) {
                                self.selected = None;
                            }
                        }
                    }
                }
                let on_screen = self.face == Face::Workbench
                    && self.get(&id).is_some_and(|s| s.kind.shelf() == self.shelf);
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
            .filter(|s| s.kind.shelf() == self.shelf)
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
        assert_eq!(b.rows()[0].title, "Second");
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
    fn counts_are_per_shelf_and_report_their_own_unseen() {
        let mut b = Bench::new();
        b.apply(doc("a", "A"));
        b.apply(doc("b", "B"));
        b.apply(decision("d"));
        assert_eq!(b.counts(Shelf::Artifacts), (2, 2));
        assert_eq!(b.counts(Shelf::Decisions), (1, 1));
        assert_eq!(b.counts(Shelf::Other), (0, 0));
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
        assert_eq!(b.rows()[0].title, "B");
    }

    #[test]
    fn a_changeset_is_pending_until_every_part_has_a_verdict() {
        let mut b = Bench::new();
        b.apply(changeset("c"));
        assert_eq!(b.rows_for(Shelf::Other)[0].tint, Tint::Pending);
        b.select(&SurfaceId("c".into()));
        b.act(&Action::AcceptPart, Some("h1".into()), None);
        assert_eq!(
            b.rows_for(Shelf::Other)[0].tint,
            Tint::Pending,
            "one answered part is not an answered changeset"
        );
        b.act(&Action::RejectPart, Some("h2".into()), None);
        assert_eq!(b.rows_for(Shelf::Other)[0].tint, Tint::Settled);
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
                assert!(report.to_prompt().contains("wrong seam"));
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
        assert_eq!(b.rows_for(Shelf::Artifacts)[0].tint, Tint::Ident);
        assert_eq!(b.rows_for(Shelf::Decisions)[0].tint, Tint::Waiting);
        let other = b.rows_for(Shelf::Other);
        assert!(other.iter().any(|r| r.tint == Tint::Unknown));
        assert!(other.iter().any(|r| r.tint == Tint::Pending));
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
