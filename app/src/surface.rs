//! TDSP — the Terminal Delight Surface Protocol.
//!
//! A terminal shows an agent's *transcript*. This shows an agent's *work*.
//!
//! The distinction is the whole module. An agent that has just finished
//! something has two different things to say: a running commentary, which
//! belongs in the scroll and is already handled by a pseudoterminal, and a
//! **work object** — a decision to take, a change to approve, a document to
//! read — which today gets flattened into the same stream of characters and
//! buried under the next four hundred lines of tool output.
//!
//! So an agent hands us a small JSON object describing *what it made*, and this
//! window decides how that thing should look. The agent never describes pixels.
//! It never sends a component tree, a width, a colour or a layout. It says
//! `architecture`, and Terminal Delight already knows what architecture looks
//! like in a bent phosphor tube at 344 pixels.
//!
//! ```text
//!    agent                    TDSP                    Terminal Delight
//!   ───────                  ──────                  ──────────────────
//!   "I made a         →   { "kind":              →   the architecture
//!    diagram of          "architecture",             renderer, this
//!    the pipeline"        "model": {…} }              theme, this skin,
//!                                                     this curvature
//! ```
//!
//! # Why a fixed catalogue
//!
//! Google's A2UI ships both a dynamic schema, where the agent invents the
//! component tree, and a fixed one, where the host authored the components
//! ahead of time and the agent streams only data into them. Their own docs call
//! the fixed one the faster and more reliable of the two, because no model has
//! to generate a valid schema at runtime. A terminal with one visual language
//! and a pixel post-pass has more reason to prefer fixed than a web host does:
//! an agent that can invent layout will eventually invent a layout that does
//! not belong in this window.
//!
//! The catalogue is therefore **six kinds and a variant for everything else**.
//! Six is small enough to render each one excellently and to hold the whole
//! vocabulary in your head, and every one of them is something this house
//! already produces in prose today.
//!
//! # Unknown is a variant, undeclared is another one
//!
//! Nothing here is dropped for being unreadable. A kind this build does not
//! know becomes [`Kind::Unclassified`] carrying the reason and the raw payload,
//! and it is drawn as a plain block that says nothing was claimed about it —
//! the same refusal [`crate::slot`] makes when a subscription with no collector
//! would otherwise draw a bar of length zero.
//!
//! The same rule reaches inside the envelope, and it is the reason
//! [`Confidence`] has an `Unknown` arm while the field itself is an `Option`:
//! an agent that *says* it does not know is telling you something, and an agent
//! that never mentioned confidence is telling you nothing. Collapsing those two
//! into one value would invent an answer to a question nobody asked.
//!
//! # What this module is not
//!
//! It is not a renderer, and it does not touch gpui. Everything here is data
//! and pure functions over data, so every rule is a unit test rather than a
//! screenshot — the same shape as [`crate::tree`] and [`crate::slot`], for the
//! same reason.

use serde_json::{json, Map, Value};

/// The protocol version this build speaks, `major.minor`.
///
/// A payload naming a **newer major** is refused: the envelope may have been
/// re-cut under it. A newer minor is accepted, because a minor bump may only
/// add optional fields — that is the promise the number makes.
pub const TDSP_VERSION: &str = "0.1";

/// Longest title a rail row can carry before it stops being readable at the
/// rail's width. Measured against [`crate::workbench::RAIL_W`], not guessed.
pub const TITLE_MAX_CHARS: usize = 72;

/// How many surfaces one pane keeps. A pane is a workbench, not an archive:
/// the store on disk is what remembers, and this is what is at hand.
pub const PANE_HISTORY_CAP: usize = 64;

// ---------------------------------------------------------------------------
// identity
// ---------------------------------------------------------------------------

/// A surface's stable identity, chosen by the agent or minted here.
///
/// Stable is the operative word: an agent that presents the same `id` twice is
/// *updating* one work object, and the rail must not grow a second row for it.
/// That is A2UI's model and it is the right one — a surface has a lifetime, and
/// a protocol whose only verb is "append" cannot express a diagram being
/// corrected.
#[derive(Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct SurfaceId(pub String);

impl SurfaceId {
    /// Keep it to what a filename and a log line can both carry.
    fn sanitise(raw: &str) -> Option<SurfaceId> {
        let cleaned: String = raw
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
            .take(96)
            .collect();
        (!cleaned.is_empty()).then_some(SurfaceId(cleaned))
    }

    /// A last-resort identity for a payload that named none.
    ///
    /// Derived from the content rather than from a clock, so the same anonymous
    /// payload presented twice updates one row instead of stacking two — which
    /// is what a retrying agent does, and the failure mode is a rail full of
    /// duplicates of one diagram.
    fn from_content(kind: &str, title: &str) -> SurfaceId {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in kind
            .bytes()
            .chain(b"\x00".iter().copied())
            .chain(title.bytes())
        {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100_0000_01b3);
        }
        SurfaceId(format!("anon-{hash:016x}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// the envelope
// ---------------------------------------------------------------------------

/// What the agent is doing to the workbench with this payload.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Op {
    /// Put it on the bench. An id already present is replaced in place, which
    /// makes `present` idempotent and makes a retry harmless.
    #[default]
    Present,
    /// Merge into the surface already carrying this id. Fields the payload does
    /// not mention are left alone — the difference from `present`, and the
    /// reason both exist: a progress update should not have to restate a
    /// document to change its title.
    Update,
    /// Take it off the bench. The row goes; the file on disk, if there is one,
    /// is the agent's own business.
    Retire,
}

impl Op {
    fn parse(raw: Option<&str>) -> Result<Op, String> {
        match raw {
            None | Some("present") => Ok(Op::Present),
            Some("update") => Ok(Op::Update),
            Some("retire") => Ok(Op::Retire),
            Some(other) => Err(format!(
                "`op` must be present, update or retire — got {other:?}"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Op::Present => "present",
            Op::Update => "update",
            Op::Retire => "retire",
        }
    }
}

/// How much the agent is willing to stand behind this.
///
/// The four words are the house's own, already used in every brief and every
/// falsifiable issue, so an agent on this machine has been writing them in
/// prose for months. Typing them costs nobody a new habit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Confidence {
    /// Something was run and a number was read.
    Measured,
    /// Follows from evidence that was not itself tested.
    Inferred,
    /// Pattern-matching, declared as such. A legitimate thing to say.
    Hunch,
    /// The agent looked and cannot tell. **Not the same as saying nothing** —
    /// see the module header.
    Unknown,
}

impl Confidence {
    fn parse(raw: &str) -> Option<Confidence> {
        Some(match raw {
            "measured" => Confidence::Measured,
            "inferred" => Confidence::Inferred,
            "hunch" => Confidence::Hunch,
            "unknown" => Confidence::Unknown,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Confidence::Measured => "measured",
            Confidence::Inferred => "inferred",
            Confidence::Hunch => "hunch",
            Confidence::Unknown => "unknown",
        }
    }
}

/// Roughly how much work the thing on the bench represents.
///
/// Deliberately coarse. A number of hours is a promise an agent cannot keep and
/// a reader will hold it to; four buckets are a shape, and a shape is what a
/// person scanning a rail actually uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Effort {
    /// Minutes. A rename, a copy change, a one-line config edit.
    Small,
    /// An hour or two, one surface, one file.
    Medium,
    /// A day. Several files, one subsystem.
    Large,
    /// More than a day, or unknown in a way that means "more than a day".
    Epic,
}

/// How tangled the work is, which is not the same question as how big.
///
/// A thousand-line rename is Large and Trivial. A forty-line change to a
/// scheduler is Small and Deep. Keeping them apart is the point.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Complexity {
    Trivial,
    Moderate,
    Involved,
    Hairy,
}

/// How far down the change reaches, and into what.
///
/// The field a reviewer actually wants and nobody writes down: not "how long",
/// but "if this is wrong, how much else is wrong with it".
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Foundation {
    /// The system it lands in, in the agent's own words — `attention rail`,
    /// `host protocol`, `theme parser`. Free text on purpose: a fixed
    /// enumeration of one codebase's subsystems would be stale within a month.
    pub system: String,
    pub depth: Depth,
}

/// How deep in the stack the work sits.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Depth {
    /// A leaf. Nothing depends on it; getting it wrong is visible immediately.
    Leaf,
    /// A component others call.
    Component,
    /// A subsystem several components depend on.
    Subsystem,
    /// Bedrock. A wire format, a persisted schema, an invariant — the kind of
    /// thing whose mistakes are paid for by everything above it, for months.
    Bedrock,
}

/// The four weights, every one of them optional.
///
/// Optional is load-bearing: a surface with no weights is honest about being
/// unweighed, and a renderer that defaulted them to zero would invent a
/// confident "trivial, leaf, measured" for a payload that said nothing at all.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Weight {
    pub effort: Option<Effort>,
    pub complexity: Option<Complexity>,
    pub foundation: Option<Foundation>,
    pub confidence: Option<Confidence>,
}

impl Weight {
    /// Did the agent weigh this at all?
    pub fn is_silent(&self) -> bool {
        self.effort.is_none()
            && self.complexity.is_none()
            && self.foundation.is_none()
            && self.confidence.is_none()
    }

    fn parse(value: Option<&Value>) -> Weight {
        let Some(map) = value.and_then(Value::as_object) else {
            return Weight::default();
        };
        let word = |key: &str| map.get(key).and_then(Value::as_str);
        Weight {
            effort: word("effort").and_then(|w| match w {
                "small" | "s" => Some(Effort::Small),
                "medium" | "m" => Some(Effort::Medium),
                "large" | "l" => Some(Effort::Large),
                "epic" | "xl" => Some(Effort::Epic),
                _ => None,
            }),
            complexity: word("complexity").and_then(|w| match w {
                "trivial" => Some(Complexity::Trivial),
                "moderate" => Some(Complexity::Moderate),
                "involved" => Some(Complexity::Involved),
                "hairy" => Some(Complexity::Hairy),
                _ => None,
            }),
            foundation: map
                .get("foundation")
                .and_then(Value::as_object)
                .and_then(|f| {
                    let depth = match f.get("depth").and_then(Value::as_str)? {
                        "leaf" => Depth::Leaf,
                        "component" => Depth::Component,
                        "subsystem" => Depth::Subsystem,
                        "bedrock" => Depth::Bedrock,
                        _ => return None,
                    };
                    Some(Foundation {
                        system: f
                            .get("system")
                            .and_then(Value::as_str)
                            .unwrap_or("unnamed")
                            .to_string(),
                        depth,
                    })
                }),
            confidence: word("confidence").and_then(Confidence::parse),
        }
    }
}

/// Where the thing on the bench came from, so a reader can check it.
///
/// The drill-back, and the field the shipped `declare_deliverable` verb does
/// not have. A surface that cannot be traced to the turn, the file or the
/// command that produced it is a summary, and a summary that cannot be checked
/// is the thing this codebase keeps refusing to ship.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Source {
    /// Files this work touched or came from.
    pub files: Vec<String>,
    /// The command that produced it, whole and never elided.
    pub command: Option<String>,
    /// A turn, a commit, an issue — whatever names the moment.
    pub reference: Option<String>,
}

impl Source {
    fn parse(value: Option<&Value>) -> Option<Source> {
        let map = value?.as_object()?;
        let src = Source {
            files: map
                .get("files")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
            command: map
                .get("command")
                .and_then(Value::as_str)
                .map(str::to_string),
            reference: map
                .get("reference")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        (!src.files.is_empty() || src.command.is_some() || src.reference.is_some()).then_some(src)
    }
}

// ---------------------------------------------------------------------------
// actions — the channel back
// ---------------------------------------------------------------------------

/// Something a person can do to a work object, which the agent then hears about.
///
/// This is the half that makes it an interaction surface rather than a viewer.
/// A rendered diff you can only look at is a picture of a diff; one where
/// "this hunk is wrong" travels back into the agent's loop is a conversation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Action {
    /// Open it with the desktop's own handler. Never our own viewer: the person
    /// already told their machine where Markdown goes.
    Open,
    /// Say something about it — the comment rides back with the action.
    Comment,
    /// Yes. The agent may proceed.
    Approve,
    /// No, and usually with a comment saying why.
    Reject,
    /// Take this specific part.
    AcceptPart,
    /// Leave this specific part.
    RejectPart,
    /// Put the question back to the agent in its own terminal.
    AskAgent,
    /// Go to where this came from — a file, a line, a turn.
    OpenSource,
    /// A verb this build has never heard of. Offered anyway, labelled as the
    /// agent spelled it, because refusing to show it would make the agent's
    /// vocabulary silently smaller than it said it was.
    Custom(String),
}

impl Action {
    fn parse(raw: &str) -> Action {
        match raw {
            "open" => Action::Open,
            "comment" => Action::Comment,
            "approve" => Action::Approve,
            "reject" => Action::Reject,
            "accept_part" | "accept_hunk" => Action::AcceptPart,
            "reject_part" | "reject_hunk" => Action::RejectPart,
            "ask_agent" => Action::AskAgent,
            "open_source" => Action::OpenSource,
            other => Action::Custom(other.to_string()),
        }
    }

    /// The wire spelling — what travels back to the agent.
    pub fn id(&self) -> &str {
        match self {
            Action::Open => "open",
            Action::Comment => "comment",
            Action::Approve => "approve",
            Action::Reject => "reject",
            Action::AcceptPart => "accept_part",
            Action::RejectPart => "reject_part",
            Action::AskAgent => "ask_agent",
            Action::OpenSource => "open_source",
            Action::Custom(s) => s,
        }
    }

    /// What the button says. A custom verb wears its own wire name, tidied.
    pub fn label(&self) -> String {
        match self {
            Action::Open => "open".into(),
            Action::Comment => "comment".into(),
            Action::Approve => "approve".into(),
            Action::Reject => "reject".into(),
            Action::AcceptPart => "accept".into(),
            Action::RejectPart => "reject".into(),
            Action::AskAgent => "ask".into(),
            Action::OpenSource => "source".into(),
            Action::Custom(s) => s.replace('_', " "),
        }
    }

    /// Does pressing it need something typed first?
    pub fn wants_comment(&self) -> bool {
        matches!(self, Action::Comment | Action::AskAgent)
    }

    /// Is this one TD performs itself rather than sending to the agent?
    ///
    /// Opening a file is the window's job; approving a plan is the agent's
    /// business. Keeping the two apart is what stops the workbench from waking
    /// an agent up to do something the desktop could have done.
    pub fn is_local(&self) -> bool {
        matches!(self, Action::Open | Action::OpenSource)
    }
}

/// A person's answer, on its way back to the agent that asked.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ActionReport {
    pub surface: SurfaceId,
    pub action: Action,
    /// Which part of the surface, when the action is about a part — a hunk id,
    /// a row id, a node id.
    pub target: Option<String>,
    pub comment: Option<String>,
}

impl ActionReport {
    /// The JSON an agent reads out of the action journal.
    pub fn to_json(&self) -> Value {
        let mut map = Map::new();
        map.insert("td".into(), json!(TDSP_VERSION));
        map.insert("type".into(), json!("action"));
        map.insert("surface".into(), json!(self.surface.as_str()));
        map.insert("action".into(), json!(self.action.id()));
        if let Some(t) = &self.target {
            map.insert("target".into(), json!(t));
        }
        if let Some(c) = &self.comment {
            map.insert("comment".into(), json!(c));
        }
        Value::Object(map)
    }

    /// The one line Terminal Delight types into the agent's own terminal.
    ///
    /// The action channel needs no new plumbing, because a pseudoterminal is
    /// already a two-way pipe to a program waiting for a human to say
    /// something. This is that sentence. It is deliberately readable rather
    /// than encoded: an agent that has never heard of TDSP still receives a
    /// plain English instruction naming the thing and the verb, and does the
    /// right thing anyway.
    pub fn to_prompt(&self) -> String {
        let mut line = format!(
            "[workbench] {} on surface {}",
            self.action.id(),
            self.surface.as_str()
        );
        if let Some(t) = &self.target {
            line.push_str(&format!(" · {t}"));
        }
        if let Some(c) = &self.comment {
            let trimmed = c.replace('\n', " ");
            line.push_str(&format!(" — {trimmed}"));
        }
        line
    }
}

// ---------------------------------------------------------------------------
// the catalogue
// ---------------------------------------------------------------------------

/// A document, an image, a PDF, a pull request — something with a location.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Artifact {
    /// Absolute path or full URL. A relative path is refused at parse time: it
    /// would resolve against the *terminal's* directory, not the agent's, and
    /// the failure is silent and wrong rather than loud.
    pub href: String,
    /// `text/markdown`, `application/pdf`, `image/png` … Optional, and when
    /// absent the row says so rather than guessing from the extension.
    pub mime: Option<String>,
    /// One line about what it is.
    pub summary: Option<String>,
}

/// Prose with structure — the register most agent output already has.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Markdown {
    pub body: String,
}

/// Rows of comparable things.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Table {
    pub columns: Vec<String>,
    /// Cells as strings. A cell the agent could not fill is `None` and prints
    /// `unavailable`, which is not the same as an empty cell it filled with
    /// nothing.
    pub rows: Vec<Vec<Option<String>>>,
}

/// A node in an architecture drawing.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Node {
    pub id: String,
    pub label: String,
    /// `running`, `failed`, `proposed` — free text, rendered as a pill.
    pub state: Option<String>,
    /// Which boundary this node sits inside — a machine, a process, a cloud.
    pub group: Option<String>,
}

/// An edge between two nodes.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: Option<String>,
}

/// Boxes and arrows: the picture Parker asks for by name.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Architecture {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Edges naming a node that was never declared. Kept rather than dropped,
    /// and drawn as a dangling stub, because an arrow to nowhere is a fact
    /// about the agent's model and deleting it hides a real mistake.
    pub dangling: Vec<Edge>,
}

/// One reviewable part of a change.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Hunk {
    pub id: String,
    pub file: String,
    /// The diff text itself, unified format, as the agent produced it.
    pub patch: String,
    pub added: usize,
    pub removed: usize,
    /// `accepted`, `rejected`, or — the default and the honest one — undecided.
    pub verdict: Verdict,
}

/// Three states, because a proposed edit, a rejected one and an unread one are
/// three different things and a boolean can hold two.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Verdict {
    #[default]
    Undecided,
    Accepted,
    Rejected,
}

/// A set of changes a person can walk through and answer.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Changeset {
    pub repository: Option<String>,
    pub hunks: Vec<Hunk>,
}

/// One way the decision could go.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Choice {
    pub id: String,
    pub name: String,
    /// What it is genuinely better at. An alternative with no honest case is a
    /// decision taken from the reader rather than offered to them.
    pub case: Option<String>,
    pub cost: Option<String>,
    /// Exactly one option may carry this. Two recommendations is none.
    pub recommended: bool,
}

/// A call to make, with the reasoning attached.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Decision {
    pub question: String,
    pub options: Vec<Choice>,
    /// What the agent thinks follows from the recommended option — the
    /// paragraph a decision record always wants and rarely has.
    pub consequences: Vec<String>,
}

/// Something arrived and this build cannot type it.
///
/// Not an error state. It is what most output honestly is, and it renders as
/// plain text with a visible marker saying nothing was claimed about it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Unclassified {
    /// Why it could not be typed, in words that tell the agent what to change.
    pub reason: String,
    /// The payload, kept whole. A reader can always see what was actually sent.
    pub raw: String,
}

/// The catalogue: six kinds and the honest default.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Kind {
    Artifact(Artifact),
    Markdown(Markdown),
    Table(Table),
    Architecture(Architecture),
    Changeset(Changeset),
    Decision(Decision),
    Unclassified(Unclassified),
}

impl Kind {
    /// The wire name, and the word on the rail's kind chip.
    pub fn id(&self) -> &'static str {
        match self {
            Kind::Artifact(_) => "artifact",
            Kind::Markdown(_) => "markdown",
            Kind::Table(_) => "table",
            Kind::Architecture(_) => "architecture",
            Kind::Changeset(_) => "changeset",
            Kind::Decision(_) => "decision",
            Kind::Unclassified(_) => "unclassified",
        }
    }

    /// Which tab of the pane's rail this kind files under.
    ///
    /// Three tabs, not seven. A rail that needs a tab per kind has stopped
    /// being a shortlist and become a second file manager.
    pub fn shelf(&self) -> Shelf {
        match self {
            Kind::Decision(_) => Shelf::Decisions,
            Kind::Artifact(_) | Kind::Markdown(_) | Kind::Table(_) | Kind::Architecture(_) => {
                Shelf::Artifacts
            }
            Kind::Changeset(_) | Kind::Unclassified(_) => Shelf::Other,
        }
    }

    /// The actions that always make sense for this kind, before the agent adds
    /// its own. An agent that lists none still gets a usable surface.
    pub fn default_actions(&self) -> Vec<Action> {
        match self {
            Kind::Artifact(_) => vec![Action::Open, Action::Comment],
            Kind::Markdown(_) | Kind::Table(_) => vec![Action::Comment],
            Kind::Architecture(_) => vec![Action::Comment, Action::AskAgent],
            Kind::Changeset(_) => vec![
                Action::AcceptPart,
                Action::RejectPart,
                Action::Comment,
                Action::Approve,
            ],
            Kind::Decision(_) => vec![Action::Approve, Action::Reject, Action::Comment],
            Kind::Unclassified(_) => vec![Action::AskAgent],
        }
    }
}

/// The three shelves of a pane's own rail.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub enum Shelf {
    #[default]
    Artifacts,
    Decisions,
    Other,
}

impl Shelf {
    pub const ALL: [Shelf; 3] = [Shelf::Artifacts, Shelf::Decisions, Shelf::Other];

    pub fn label(self) -> &'static str {
        match self {
            Shelf::Artifacts => "artifacts",
            Shelf::Decisions => "decisions",
            Shelf::Other => "other",
        }
    }
}

// ---------------------------------------------------------------------------
// the surface itself
// ---------------------------------------------------------------------------

/// One work object on a pane's bench.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Surface {
    pub id: SurfaceId,
    pub title: String,
    pub kind: Kind,
    pub weight: Weight,
    /// What a person may do about it. Never empty: the kind's own defaults are
    /// merged in, so a surface always offers at least one thing.
    pub actions: Vec<Action>,
    pub source: Option<Source>,
    /// Unix milliseconds, as stamped by whoever accepted the payload. Not the
    /// agent's clock: an agent's idea of the time is one more thing that can be
    /// wrong, and ordering the rail by it would let a bad clock jump the queue.
    pub arrived_ms: u64,
}

impl Surface {
    /// The one line the rail row shows under the title.
    pub fn subtitle(&self) -> String {
        match &self.kind {
            Kind::Artifact(a) => a
                .summary
                .clone()
                .or_else(|| a.mime.clone())
                .unwrap_or_else(|| short_href(&a.href)),
            Kind::Markdown(m) => format!("{} words", m.body.split_whitespace().count()),
            Kind::Table(t) => format!("{} rows · {} columns", t.rows.len(), t.columns.len()),
            Kind::Architecture(a) => {
                let mut s = format!("{} nodes · {} edges", a.nodes.len(), a.edges.len());
                if !a.dangling.is_empty() {
                    s.push_str(&format!(" · {} dangling", a.dangling.len()));
                }
                s
            }
            Kind::Changeset(c) => {
                let (add, rem) = c
                    .hunks
                    .iter()
                    .fold((0, 0), |(a, r), h| (a + h.added, r + h.removed));
                format!("{} hunks · +{add} −{rem}", c.hunks.len())
            }
            Kind::Decision(d) => match d.options.iter().find(|o| o.recommended) {
                Some(o) => format!("{} options · recommends {}", d.options.len(), o.name),
                None => format!("{} options · no recommendation", d.options.len()),
            },
            Kind::Unclassified(u) => u.reason.clone(),
        }
    }

    /// Merge an update into this surface, leaving unmentioned fields alone.
    pub fn merge(&mut self, other: Surface) {
        if !other.title.is_empty() {
            self.title = other.title;
        }
        if !matches!(other.kind, Kind::Unclassified(_)) {
            self.kind = other.kind;
        }
        if !other.weight.is_silent() {
            self.weight = other.weight;
        }
        if !other.actions.is_empty() {
            self.actions = other.actions;
        }
        if other.source.is_some() {
            self.source = other.source;
        }
        self.arrived_ms = other.arrived_ms;
    }
}

/// An accepted payload: what to do, and to which surface.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Post {
    pub op: Op,
    pub id: SurfaceId,
    /// Which pane the agent is speaking for, when it said. Absent means "the
    /// pane this arrived through", which the transport fills in.
    pub pane: Option<u32>,
    /// `None` on a retire — there is nothing left to draw.
    pub surface: Option<Surface>,
}

// ---------------------------------------------------------------------------
// parsing
// ---------------------------------------------------------------------------

/// Strict parse, for a caller that can hand the reason back to the agent.
///
/// Used by the MCP verb, where an error message is delivered to something that
/// can read it and try again. The file and transcript transports use
/// [`parse_lenient`] instead, because there is nobody there to tell.
pub fn parse(value: &Value, now_ms: u64) -> Result<Post, String> {
    let map = value
        .as_object()
        .ok_or_else(|| "a surface payload is a JSON object".to_string())?;

    match map.get("td").and_then(Value::as_str) {
        None => {
            return Err(format!(
                "missing `td` version field — this build speaks TDSP {TDSP_VERSION}"
            ))
        }
        Some(v) => check_version(v)?,
    }

    let op = Op::parse(map.get("op").and_then(Value::as_str))?;
    let kind_name = map.get("kind").and_then(Value::as_str);
    let title = map
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .chars()
        .take(TITLE_MAX_CHARS)
        .collect::<String>();

    let id = match map.get("id").and_then(Value::as_str) {
        Some(raw) => SurfaceId::sanitise(raw).ok_or_else(|| {
            "`id` must contain at least one letter, digit, dash, dot, colon or underscore"
                .to_string()
        })?,
        None if op == Op::Retire => {
            return Err("`retire` needs the `id` of the surface to take off the bench".into())
        }
        None => SurfaceId::from_content(kind_name.unwrap_or("unclassified"), &title),
    };

    let pane = map.get("pane").and_then(Value::as_u64).map(|p| p as u32);

    if op == Op::Retire {
        return Ok(Post {
            op,
            id,
            pane,
            surface: None,
        });
    }

    let kind_name = kind_name.ok_or_else(|| {
        format!(
            "missing `kind` — this build renders {}",
            catalogue_names().join(", ")
        )
    })?;
    let model = map.get("model");
    let kind = parse_kind(kind_name, model).map_err(|e| e.strict)?;

    Ok(Post {
        op,
        surface: Some(assemble(id.clone(), kind, title, map, now_ms)),
        id,
        pane,
    })
}

/// Parse anything, and never lose it.
///
/// A payload that cannot be typed still becomes a surface — an
/// [`Unclassified`] one carrying the reason and the raw bytes. This is the
/// entry point for the file drop and the transcript fence, where a rejection
/// would be a silent deletion: nobody is reading the return value, and the
/// agent has already moved on.
pub fn parse_lenient(value: &Value, now_ms: u64, fallback_title: &str) -> Post {
    match parse(value, now_ms) {
        Ok(post) => post,
        Err(reason) => {
            let title = value
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or(fallback_title)
                .chars()
                .take(TITLE_MAX_CHARS)
                .collect::<String>();
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .and_then(SurfaceId::sanitise)
                .unwrap_or_else(|| SurfaceId::from_content("unclassified", &title));
            let kind = Kind::Unclassified(Unclassified {
                reason,
                raw: truncate_raw(value),
            });
            let empty = Map::new();
            let map = value.as_object().unwrap_or(&empty);
            Post {
                op: Op::Present,
                pane: map.get("pane").and_then(Value::as_u64).map(|p| p as u32),
                surface: Some(assemble(id.clone(), kind, title, map, now_ms)),
                id,
            }
        }
    }
}

/// Everything the envelope carries that is not the model.
fn assemble(
    id: SurfaceId,
    kind: Kind,
    title: String,
    map: &Map<String, Value>,
    now_ms: u64,
) -> Surface {
    let declared: Vec<Action> = map
        .get("actions")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(Action::parse)
                .collect()
        })
        .unwrap_or_default();
    let mut actions = kind.default_actions();
    for action in declared {
        if !actions.contains(&action) {
            actions.push(action);
        }
    }
    let title = if title.is_empty() {
        default_title(&kind)
    } else {
        title
    };
    Surface {
        id,
        title,
        weight: Weight::parse(map.get("weight")),
        source: Source::parse(map.get("source")),
        actions,
        kind,
        arrived_ms: now_ms,
    }
}

/// A title for a payload that named none. Never "Untitled": a kind and a count
/// is more use on a rail than a word that says the agent forgot.
fn default_title(kind: &Kind) -> String {
    match kind {
        Kind::Artifact(a) => short_href(&a.href),
        Kind::Markdown(_) => "note".into(),
        Kind::Table(t) => format!("table of {}", t.rows.len()),
        Kind::Architecture(a) => format!("{} nodes", a.nodes.len()),
        Kind::Changeset(c) => format!("{} hunks", c.hunks.len()),
        Kind::Decision(d) => d.question.chars().take(TITLE_MAX_CHARS).collect(),
        Kind::Unclassified(_) => "unclassified".into(),
    }
}

/// A parse failure that knows how to be both strict and lenient.
struct KindError {
    strict: String,
}

fn parse_kind(name: &str, model: Option<&Value>) -> Result<Kind, KindError> {
    let empty = Map::new();
    let m = model.and_then(Value::as_object).unwrap_or(&empty);
    let text = |key: &str| m.get(key).and_then(Value::as_str).map(str::to_string);
    let err = |msg: String| KindError { strict: msg };

    Ok(match name {
        "artifact" => {
            let href = text("href").ok_or_else(|| {
                err("an artifact needs a `href` — an absolute path or a full URL".into())
            })?;
            check_href(&href).map_err(err)?;
            Kind::Artifact(Artifact {
                href,
                mime: text("mime"),
                summary: text("summary"),
            })
        }
        "markdown" => Kind::Markdown(Markdown {
            body: text("body").ok_or_else(|| err("markdown needs a `body` string".into()))?,
        }),
        "table" => {
            let columns: Vec<String> = m
                .get("columns")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|v| v.as_str().unwrap_or("").to_string())
                        .collect()
                })
                .ok_or_else(|| err("a table needs `columns`, an array of strings".into()))?;
            let rows = m
                .get("rows")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|row| {
                            row.as_array()
                                .map(|cells| cells.iter().map(cell_text).collect())
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            Kind::Table(Table { columns, rows })
        }
        "architecture" => {
            let nodes: Vec<Node> = m
                .get("nodes")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(parse_node).collect())
                .ok_or_else(|| {
                    err("architecture needs `nodes`, each with an `id` and a `label`".into())
                })?;
            let declared: Vec<Edge> = m
                .get("edges")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(parse_edge).collect())
                .unwrap_or_default();
            let known: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
            let (edges, dangling): (Vec<Edge>, Vec<Edge>) = declared
                .into_iter()
                .partition(|e| known.contains(&e.from.as_str()) && known.contains(&e.to.as_str()));
            Kind::Architecture(Architecture {
                nodes,
                edges,
                dangling,
            })
        }
        "changeset" => {
            let hunks: Vec<Hunk> = m
                .get("hunks")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .enumerate()
                        .filter_map(|(i, v)| parse_hunk(i, v))
                        .collect()
                })
                .ok_or_else(|| {
                    err("a changeset needs `hunks`, each with a `file` and a `patch`".into())
                })?;
            Kind::Changeset(Changeset {
                repository: text("repository"),
                hunks,
            })
        }
        "decision" => {
            let question =
                text("question").ok_or_else(|| err("a decision needs a `question`".into()))?;
            let mut options: Vec<Choice> = m
                .get("options")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .enumerate()
                        .filter_map(|(i, v)| parse_option(i, v))
                        .collect()
                })
                .unwrap_or_default();
            // Exactly one recommendation, or none. Two is a schema error the
            // brief format already enforces by hand, and the hand slips.
            let mut seen = false;
            for option in options.iter_mut() {
                if option.recommended {
                    if seen {
                        option.recommended = false;
                    }
                    seen = true;
                }
            }
            Kind::Decision(Decision {
                question,
                options,
                consequences: m
                    .get("consequences")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        }
        other => {
            return Err(err(format!(
                "unknown kind {other:?} — this build renders {}",
                catalogue_names().join(", ")
            )))
        }
    })
}

/// A cell that is genuinely absent stays absent. `null` means the agent looked
/// and had nothing; a missing entry means the row was short.
fn cell_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

fn parse_node(v: &Value) -> Option<Node> {
    let m = v.as_object()?;
    let id = m.get("id").and_then(Value::as_str)?.to_string();
    Some(Node {
        label: m
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string(),
        id,
        state: m.get("state").and_then(Value::as_str).map(str::to_string),
        group: m.get("group").and_then(Value::as_str).map(str::to_string),
    })
}

fn parse_edge(v: &Value) -> Option<Edge> {
    let m = v.as_object()?;
    Some(Edge {
        from: m.get("from").and_then(Value::as_str)?.to_string(),
        to: m.get("to").and_then(Value::as_str)?.to_string(),
        label: m.get("label").and_then(Value::as_str).map(str::to_string),
    })
}

fn parse_hunk(index: usize, v: &Value) -> Option<Hunk> {
    let m = v.as_object()?;
    let file = m.get("file").and_then(Value::as_str)?.to_string();
    let patch = m
        .get("patch")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    // Counted here rather than trusted: an agent's own +/− tally is a number it
    // can get wrong, and the patch is right there.
    let (added, removed) = patch.lines().fold((0, 0), |(a, r), line| {
        if line.starts_with("+++") || line.starts_with("---") {
            (a, r)
        } else if line.starts_with('+') {
            (a + 1, r)
        } else if line.starts_with('-') {
            (a, r + 1)
        } else {
            (a, r)
        }
    });
    Some(Hunk {
        id: m
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{file}#hunk-{}", index + 1)),
        file,
        patch,
        added,
        removed,
        verdict: Verdict::Undecided,
    })
}

fn parse_option(index: usize, v: &Value) -> Option<Choice> {
    let m = v.as_object()?;
    let name = m.get("name").and_then(Value::as_str)?.to_string();
    Some(Choice {
        id: m
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("option-{}", index + 1)),
        name,
        case: m.get("case").and_then(Value::as_str).map(str::to_string),
        cost: m.get("cost").and_then(Value::as_str).map(str::to_string),
        recommended: m
            .get("recommended")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Refuse what would appear to work and be wrong.
fn check_href(href: &str) -> Result<(), String> {
    if href.starts_with("http://") || href.starts_with("https://") || href.starts_with("file://") {
        return Ok(());
    }
    if href.starts_with("javascript:") || href.starts_with("data:") {
        return Err(format!(
            "{href:?} is not a document — the workbench opens files and web pages, nothing else"
        ));
    }
    if href.starts_with('/') {
        return Ok(());
    }
    Err(format!(
        "{href:?} is relative — it would resolve against the TERMINAL's directory, \
         not yours. Send an absolute path or a full URL."
    ))
}

fn check_version(raw: &str) -> Result<(), String> {
    let major = |s: &str| s.split('.').next().unwrap_or("").parse::<u32>().ok();
    match (major(raw), major(TDSP_VERSION)) {
        (Some(theirs), Some(ours)) if theirs <= ours => Ok(()),
        (Some(theirs), Some(ours)) => Err(format!(
            "payload says TDSP {theirs}.x and this build speaks {ours}.x — \
             a newer major may have re-cut the envelope, so it is refused rather than guessed at"
        )),
        _ => Err(format!(
            "`td` should be a version like {TDSP_VERSION:?} — got {raw:?}"
        )),
    }
}

/// The last path segment, or the host — what a person calls the thing.
fn short_href(href: &str) -> String {
    href.rsplit('/')
        .find(|s| !s.is_empty())
        .unwrap_or(href)
        .chars()
        .take(TITLE_MAX_CHARS)
        .collect()
}

/// Keep enough of a rejected payload to see what was sent, and not so much that
/// one bad file fills the pane.
fn truncate_raw(value: &Value) -> String {
    let text = serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string());
    if text.len() <= 4096 {
        text
    } else {
        let mut cut = 4096;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!("{}\n… {} more bytes", &text[..cut], text.len() - cut)
    }
}

// ---------------------------------------------------------------------------
// what this build can render, said out loud
// ---------------------------------------------------------------------------

pub fn catalogue_names() -> Vec<&'static str> {
    vec![
        "artifact",
        "markdown",
        "table",
        "architecture",
        "changeset",
        "decision",
    ]
}

/// The machine-readable catalogue, for a capabilities handshake.
///
/// A2UI's host-advertised catalogue, in one function. An agent asks what this
/// client can render and is told, rather than guessing and having its work
/// quietly downgraded to unclassified.
pub fn catalogue() -> Value {
    json!({
        "td": TDSP_VERSION,
        "type": "capabilities",
        "kinds": catalogue_names().iter().map(|k| json!({
            "kind": k,
            "version": 1,
        })).collect::<Vec<_>>(),
        "actions": ["open", "comment", "approve", "reject",
                    "accept_part", "reject_part", "ask_agent", "open_source"],
        "weights": {
            "effort": ["small", "medium", "large", "epic"],
            "complexity": ["trivial", "moderate", "involved", "hairy"],
            "depth": ["leaf", "component", "subsystem", "bedrock"],
            "confidence": ["measured", "inferred", "hunch", "unknown"],
        },
        "transports": ["mcp:present_surface", "file", "transcript-fence"],
        "unknown_kind": "rendered as unclassified, never dropped",
    })
}

/// The briefing Terminal Delight hands an agent it starts.
///
/// This is the other half of the launcher: a pane that TD opened knows what it
/// launched, so it can tell the thing it launched what this window can do. An
/// agent started any other way reads the same text from
/// `terminal-delight surface --catalogue`.
///
/// Kept short deliberately. A launch prompt competes with the user's actual
/// first instruction, and a page of protocol documentation ahead of "fix the
/// login bug" is a page the agent reads instead of the bug.
pub fn launch_briefing(dir: &str) -> String {
    format!(
        "You are running inside Terminal Delight, which can render your work as a native \
         surface beside this terminal — a WORKBENCH face on this pane, toggled from its header.\n\
         \n\
         Alongside your normal reply, present each finished work object as one JSON document. \
         Write it to a new file in {dir} (any filename ending .json), or call the \
         `present_surface` MCP verb, or print it in your reply inside a fenced ```td block.\n\
         \n\
         {{\"td\":\"{TDSP_VERSION}\",\"kind\":\"<kind>\",\"title\":\"<short name>\",\
         \"model\":{{...}},\"weight\":{{\"effort\":\"medium\",\"complexity\":\"moderate\",\
         \"confidence\":\"inferred\",\"foundation\":{{\"system\":\"<what it touches>\",\
         \"depth\":\"component\"}}}}}}\n\
         \n\
         Kinds: {kinds}. Describe MEANING, never layout — no widths, no colours, no components. \
         Terminal Delight owns how each kind looks. An unknown kind is shown as unclassified \
         rather than dropped, so it is always safe to send.\n\
         \n\
         A person acting on a surface answers you here, in this terminal, as a line beginning \
         [workbench].",
        kinds = catalogue_names().join(", "),
    )
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_758_000_000_000;

    fn post(value: Value) -> Post {
        parse(&value, NOW).expect("valid payload")
    }

    fn surface(value: Value) -> Surface {
        post(value).surface.expect("a surface")
    }

    #[test]
    fn a_minimal_artifact_is_accepted_and_titled() {
        let s = surface(json!({
            "td": "0.1",
            "kind": "artifact",
            "model": { "href": "/home/parker/report.html" }
        }));
        assert_eq!(s.kind.id(), "artifact");
        assert_eq!(
            s.title, "report.html",
            "an untitled artifact wears its filename"
        );
        assert!(s.actions.contains(&Action::Open));
    }

    #[test]
    fn a_relative_href_is_refused_with_the_reason() {
        let err = parse(
            &json!({ "td": "0.1", "kind": "artifact", "model": { "href": "report.html" } }),
            NOW,
        )
        .expect_err("relative paths cannot be resolved");
        assert!(err.contains("relative"), "{err}");
        assert!(
            err.contains("TERMINAL"),
            "the reason names whose directory it would use: {err}"
        );
    }

    #[test]
    fn javascript_and_data_urls_are_not_documents() {
        for href in ["javascript:alert(1)", "data:text/html,hi"] {
            let err = parse(
                &json!({ "td": "0.1", "kind": "artifact", "model": { "href": href } }),
                NOW,
            )
            .expect_err("not a document");
            assert!(err.contains("not a document"), "{href}: {err}");
        }
    }

    #[test]
    fn an_unknown_kind_is_kept_as_unclassified_not_dropped() {
        let p = parse_lenient(
            &json!({ "td": "0.1", "kind": "hologram", "title": "Spinning thing" }),
            NOW,
            "dropped.json",
        );
        let s = p.surface.expect("kept");
        match &s.kind {
            Kind::Unclassified(u) => {
                assert!(u.reason.contains("hologram"), "{}", u.reason);
                assert!(
                    u.reason.contains("architecture"),
                    "it lists what IS renderable"
                );
                assert!(u.raw.contains("Spinning thing"), "the payload survives");
            }
            other => panic!("expected unclassified, got {}", other.id()),
        }
        assert_eq!(s.title, "Spinning thing");
    }

    #[test]
    fn a_payload_that_is_not_even_json_shaped_still_lands() {
        let p = parse_lenient(&json!([1, 2, 3]), NOW, "junk.json");
        let s = p.surface.expect("kept");
        assert!(matches!(s.kind, Kind::Unclassified(_)));
        assert_eq!(s.title, "junk.json", "the filename is the fallback title");
    }

    #[test]
    fn a_newer_major_is_refused_and_a_newer_minor_is_not() {
        assert!(parse(
            &json!({ "td": "1.0", "kind": "markdown", "model": {"body":"x"} }),
            NOW
        )
        .is_err());
        assert!(parse(
            &json!({ "td": "0.9", "kind": "markdown", "model": {"body":"x"} }),
            NOW
        )
        .is_ok());
    }

    #[test]
    fn dangling_edges_are_kept_apart_rather_than_dropped() {
        let s = surface(json!({
            "td": "0.1", "kind": "architecture",
            "model": {
                "nodes": [{"id":"a","label":"Agent"},{"id":"b","label":"Queue"}],
                "edges": [{"from":"a","to":"b"},{"from":"a","to":"ghost"}]
            }
        }));
        match &s.kind {
            Kind::Architecture(a) => {
                assert_eq!(a.edges.len(), 1, "only the resolvable edge is a real edge");
                assert_eq!(a.dangling.len(), 1, "the arrow to nowhere is kept");
                assert_eq!(a.dangling[0].to, "ghost");
            }
            other => panic!("{}", other.id()),
        }
        assert!(s.subtitle().contains("dangling"), "{}", s.subtitle());
    }

    #[test]
    fn hunk_counts_are_taken_from_the_patch_not_from_the_agent() {
        let s = surface(json!({
            "td": "0.1", "kind": "changeset",
            "model": { "hunks": [{
                "file": "src/surface.rs",
                "added": 999, "removed": 999,
                "patch": "--- a/x\n+++ b/x\n@@\n+one\n+two\n-gone\n context"
            }]}
        }));
        match &s.kind {
            Kind::Changeset(c) => {
                assert_eq!(c.hunks[0].added, 2, "the +++ header is not an addition");
                assert_eq!(c.hunks[0].removed, 1, "the --- header is not a removal");
                assert_eq!(c.hunks[0].verdict, Verdict::Undecided);
                assert_eq!(
                    c.hunks[0].id, "src/surface.rs#hunk-1",
                    "an unnamed hunk gets a stable id"
                );
            }
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn a_second_recommendation_is_demoted_because_two_is_none() {
        let s = surface(json!({
            "td": "0.1", "kind": "decision",
            "model": { "question": "Which way?", "options": [
                {"name":"A","recommended":true},
                {"name":"B","recommended":true}
            ]}
        }));
        match &s.kind {
            Kind::Decision(d) => {
                assert!(d.options[0].recommended);
                assert!(!d.options[1].recommended, "the second claim is dropped");
            }
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn a_null_cell_is_absent_and_an_empty_string_is_not() {
        let s = surface(json!({
            "td": "0.1", "kind": "table",
            "model": { "columns": ["a","b"], "rows": [["x", null], ["", "y"]] }
        }));
        match &s.kind {
            Kind::Table(t) => {
                assert_eq!(t.rows[0][1], None, "null is unavailable");
                assert_eq!(
                    t.rows[1][0],
                    Some(String::new()),
                    "empty is a filled-in blank"
                );
            }
            other => panic!("{}", other.id()),
        }
    }

    #[test]
    fn weights_left_out_stay_out() {
        let s = surface(json!({ "td": "0.1", "kind": "markdown", "model": {"body": "hi"} }));
        assert!(
            s.weight.is_silent(),
            "nothing is invented for an unweighed surface"
        );
        assert_eq!(s.weight.effort, None);
        assert_eq!(s.weight.confidence, None);
    }

    #[test]
    fn declared_unknown_and_undeclared_are_different_values() {
        let said = surface(json!({
            "td": "0.1", "kind": "markdown", "model": {"body": "hi"},
            "weight": { "confidence": "unknown" }
        }));
        let silent = surface(json!({ "td": "0.1", "kind": "markdown", "model": {"body": "hi"} }));
        assert_eq!(said.weight.confidence, Some(Confidence::Unknown));
        assert_eq!(silent.weight.confidence, None);
        assert_ne!(said.weight.confidence, silent.weight.confidence);
    }

    #[test]
    fn the_same_anonymous_payload_twice_is_one_row() {
        let a = post(
            json!({ "td": "0.1", "kind": "markdown", "title": "Same", "model": {"body":"1"} }),
        );
        let b = post(
            json!({ "td": "0.1", "kind": "markdown", "title": "Same", "model": {"body":"2"} }),
        );
        assert_eq!(a.id, b.id, "a retry updates rather than stacks");
    }

    #[test]
    fn a_retire_needs_an_id_and_carries_no_surface() {
        let p = post(json!({ "td": "0.1", "op": "retire", "id": "srf-1" }));
        assert_eq!(p.op, Op::Retire);
        assert!(p.surface.is_none());
        assert!(parse(&json!({ "td": "0.1", "op": "retire" }), NOW).is_err());
    }

    #[test]
    fn an_update_leaves_unmentioned_fields_alone() {
        let mut first = surface(json!({
            "td": "0.1", "kind": "markdown", "title": "Draft", "model": {"body":"one"},
            "weight": {"effort": "large"}
        }));
        let second = surface(json!({
            "td": "0.1", "kind": "markdown", "title": "Final", "model": {"body":"two"}
        }));
        first.merge(second);
        assert_eq!(first.title, "Final");
        assert_eq!(
            first.weight.effort,
            Some(Effort::Large),
            "a silent update does not erase a stated weight"
        );
    }

    #[test]
    fn declared_actions_extend_the_kinds_own_and_never_duplicate() {
        let s = surface(json!({
            "td": "0.1", "kind": "artifact",
            "model": { "href": "/tmp/a.pdf" },
            "actions": ["open", "publish"]
        }));
        assert_eq!(
            s.actions.iter().filter(|a| **a == Action::Open).count(),
            1,
            "open was already a default and is not listed twice"
        );
        assert!(s.actions.contains(&Action::Custom("publish".into())));
    }

    #[test]
    fn every_kind_files_under_exactly_one_shelf() {
        let kinds = [
            Kind::Artifact(Artifact {
                href: "/x".into(),
                mime: None,
                summary: None,
            }),
            Kind::Markdown(Markdown {
                body: String::new(),
            }),
            Kind::Table(Table {
                columns: vec![],
                rows: vec![],
            }),
            Kind::Architecture(Architecture {
                nodes: vec![],
                edges: vec![],
                dangling: vec![],
            }),
            Kind::Changeset(Changeset {
                repository: None,
                hunks: vec![],
            }),
            Kind::Decision(Decision {
                question: "?".into(),
                options: vec![],
                consequences: vec![],
            }),
            Kind::Unclassified(Unclassified {
                reason: String::new(),
                raw: String::new(),
            }),
        ];
        for kind in &kinds {
            let shelf = kind.shelf();
            assert!(Shelf::ALL.contains(&shelf), "{} has no shelf", kind.id());
            assert!(
                !kind.default_actions().is_empty(),
                "{} offers nothing",
                kind.id()
            );
        }
    }

    #[test]
    fn the_catalogue_and_the_parser_cannot_disagree() {
        // The announcement is what agents are told; the parser is what actually
        // happens. A kind in one and not the other is a promise this build does
        // not keep, which is the failure a capabilities handshake exists to end.
        for name in catalogue_names() {
            let probe = json!({ "td": "0.1", "kind": name });
            let refused_for_kind_reasons = parse(&probe, NOW)
                .err()
                .is_some_and(|e| e.contains("unknown kind"));
            assert!(
                !refused_for_kind_reasons,
                "{name} is advertised but the parser has never heard of it"
            );
        }
    }

    #[test]
    fn an_action_report_reads_as_english_in_a_terminal() {
        let report = ActionReport {
            surface: SurfaceId("change-847".into()),
            action: Action::RejectPart,
            target: Some("src/surface.rs#hunk-4".into()),
            comment: Some("Tube geometry shouldn't depend on terminal state.".into()),
        };
        let line = report.to_prompt();
        assert!(
            line.starts_with("[workbench] reject_part on surface change-847"),
            "{line}"
        );
        assert!(line.contains("hunk-4"), "{line}");
        assert!(line.contains("Tube geometry"), "{line}");
        assert!(
            !line.contains('\n'),
            "one line, or it is several prompts: {line}"
        );
        assert_eq!(report.to_json()["action"], json!("reject_part"));
    }

    #[test]
    fn a_comment_with_newlines_still_arrives_as_one_line() {
        let report = ActionReport {
            surface: SurfaceId("s".into()),
            action: Action::Comment,
            target: None,
            comment: Some("first\nsecond".into()),
        };
        assert!(!report.to_prompt().contains('\n'));
    }

    #[test]
    fn local_actions_are_the_ones_the_window_can_do_itself() {
        assert!(Action::Open.is_local());
        assert!(Action::OpenSource.is_local());
        assert!(!Action::Approve.is_local(), "approval belongs to the agent");
        assert!(
            !Action::Custom("deploy".into()).is_local(),
            "unknown verbs go to the agent"
        );
    }

    #[test]
    fn the_briefing_names_every_kind_and_the_file_drop() {
        let text = launch_briefing("/run/user/1000/terminal-delight/surfaces/7");
        for name in catalogue_names() {
            assert!(text.contains(name), "briefing omits {name}");
        }
        assert!(text.contains("/run/user/1000/terminal-delight/surfaces/7"));
        assert!(text.contains("present_surface"));
        assert!(text.contains("```td"));
        assert!(
            text.contains("[workbench]"),
            "it says how an answer will arrive"
        );
    }

    #[test]
    fn a_title_longer_than_the_rail_is_cut_not_wrapped() {
        let long = "x".repeat(400);
        let s = surface(json!({
            "td": "0.1", "kind": "markdown", "title": long, "model": {"body":"b"}
        }));
        assert_eq!(s.title.chars().count(), TITLE_MAX_CHARS);
    }

    #[test]
    fn an_id_with_a_slash_in_it_cannot_escape_a_directory() {
        let p = post(json!({
            "td": "0.1", "kind": "markdown", "id": "../../etc/passwd", "model": {"body":"b"}
        }));
        assert!(!p.id.as_str().contains('/'), "{}", p.id.as_str());
        assert_eq!(p.id.as_str(), "....etcpasswd");
    }
}
