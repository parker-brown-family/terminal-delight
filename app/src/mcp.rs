//! Read-only MCP control surface for terminal-delight.
//!
//! terminal-delight already knows, per pane, *who* is running (claude / codex /
//! shell), *where* (cwd), and — for an agent — *which conversation* (a resumable
//! session id, which is also the on-disk transcript where structured tool-call
//! events live). That makes it a natural **read-only control surface for
//! agents**: an orchestrator could watch a wall of panes, see when an agent
//! finishes or calls a given tool, and act — without ever touching a keyboard.
//!
//! This module is the *safe foundation* for that: the operator-managed policy
//! (the mother-bar robot panel edits it) plus the exact, tested shape of the
//! snapshot the server would expose. The hard line: **nothing here can WRITE to
//! a PTY.** Sending bytes into a shell is arbitrary code execution; the entire
//! first cut is observe-only, and the policy below defaults to *exposing
//! nothing* until an operator opts in.
//!
//! The live stdio / JSON-RPC transport is a deliberately separate increment —
//! what lands here is the config, the identity model, and the policy that
//! decides which panes an agent may ever see.

use serde::{Deserialize, Serialize};

/// What the read-only MCP server is allowed to expose. Safe by default:
/// disabled entirely, and when enabled, only conversational-agent panes
/// (claude / codex) — never a plain root shell — so flipping it on can't
/// accidentally leak an arbitrary shell's cwd/scrollback to a connected agent.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct McpConfig {
    /// Master switch. Off by default: the server reports nothing until asked.
    pub enabled: bool,
    /// Which panes the server may report (see [`Expose`]).
    pub expose: Expose,
    /// Stream structured tool-call events by tailing each agent pane's *own*
    /// transcript (the claude/codex JSONL), rather than scraping the rendered
    /// screen — the reliable, structured event source.
    pub events: bool,
    /// Allow the config-write tools (`set_pane_config`) to *mutate* a pane's
    /// appearance. A separate, second opt-in beyond [`Self::enabled`]: turning
    /// the server on makes it a read-only *watch* surface; flipping this makes
    /// it a *remote-control* surface (an agent can dim/recolour your terminals).
    /// Off by default — that escalation must be a deliberate choice, and the
    /// `TD_MCP_WRITE` env var or the robot panel's "writes" toggle sets it.
    pub writable: bool,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            expose: Expose::AgentsOnly,
            events: true,
            writable: false,
        }
    }
}

/// The exposure policy — how wide the read-only window is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Expose {
    /// Only claude / codex agent panes. The safe default.
    #[default]
    AgentsOnly,
    /// Every pane, including plain shells. Broader, and riskier.
    All,
}

impl Expose {
    /// Cycle the policy for the panel toggle.
    pub fn next(self) -> Self {
        match self {
            Expose::AgentsOnly => Expose::All,
            Expose::All => Expose::AgentsOnly,
        }
    }
}

/// Which Terminal Delight answered.
///
/// Several instances run on one box — one window per workspace, each with its
/// own session host — and until this existed nothing on the wire said which of
/// them a reply came from. An agent's relay resolves a window from its own
/// environment, so pointing it at the wrong instance produced an answer that
/// was correct in shape, plausible in content, and about somebody else's
/// terminals. There was no field to check, so there was no way to notice.
///
/// Stamped onto every tool result by [`tools_call`] — the one place every verb
/// passes through — so a tool added later cannot ship without saying where its
/// answer came from.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Instance {
    /// The session id this window holds (`1`, `tdclip`, …) — the same name the
    /// host socket and `$TD_SESSION` carry.
    pub session: String,
    /// This window's own pid, which is also the `ctl-<pid>.sock` the caller
    /// reached it through.
    pub window: u32,
    /// The build this window is running, taken from its executable's own file
    /// name (`td-<sha>-<label>` for an installed one). Two windows on one box
    /// are routinely different builds, and the version string cannot tell them
    /// apart because it is the same in both. `None` only when the executable
    /// path could not be read at all.
    pub build: Option<String>,
}

impl Instance {
    /// One line naming this instance, for the text half of a tool result — what
    /// a model actually reads.
    fn line(&self) -> String {
        format!(
            "— terminal-delight · session {} · window {} · {}",
            self.session,
            self.window,
            self.build.as_deref().unwrap_or("build unreadable")
        )
    }
}

/// One pane as the read-only server would report it — the *identity* an
/// orchestrator binds a watch rule to.
///
/// The durable key is [`PaneInfo::session`] (the agent conversation id), NOT
/// [`PaneInfo::pid`]: a pid recycles, and terminal-delight's own resume flow
/// restarts the agent under a fresh pid for the *same* conversation. A watch
/// keyed on the session survives a crash/resume; one keyed on the pid does not.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct PaneInfo {
    /// Index of the tab this pane lives in.
    pub tab: usize,
    /// Display name (user-set name, else OSC title, else the mode label).
    pub title: String,
    /// Foreground process class: SHELL | CLAUDE | CODEX | REMOTE | <program>.
    pub mode: String,
    /// True for a conversational agent (claude / codex).
    pub is_agent: bool,
    /// The pane's shell pid (ephemeral — see the struct note).
    pub pid: u32,
    /// The host's id for this pane — the same number the host stamps into the
    /// pane's environment as `$TD_PANE_ID`, which is how a process running
    /// inside a terminal can say which pane it is in without walking `/proc`
    /// and guessing.
    ///
    /// Deliberately serialised even when absent. `None` means this window owns
    /// the pane directly (no host), which is a different fact from "the id was
    /// not reported", and an omitted key leaves a hole a reader fills in.
    pub pane_id: Option<u64>,
    /// Foreground process cwd (falls back to the shell's), if readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Resumable agent command, e.g. `claude --resume <uuid>` — the stable
    /// identity AND the pointer to where this agent's tool-call transcript
    /// lives on disk. `None` for non-agent panes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// The tool this pane is holding right now — the same resolution its LOGO
    /// wears (see [`crate::toolprop`]), so `None` here and a tool plate on
    /// screen can never both be true. Present so that "is the pane wearing the
    /// right thing?" is a question something can ANSWER, rather than one only a
    /// photograph of the screen could settle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The sticky note on this pane's glass, if one is posted — title, text
    /// and pin, exactly what a human sees on the paper. Present so an agent can
    /// READ the fridge door (its own note included) as well as write on it
    /// with `leave_note`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<NoteReport>,
    /// Whether this pane would be exposed under the current policy.
    pub exposed: bool,
    /// The pane's *effective* grade — what it actually renders with (its own
    /// override, else the inherited outer) — in the config API's uniform
    /// `0..100` percents. `#[serde(skip)]` so `list_panes` stays an
    /// identity-only listing; `get_pane_config` serialises it explicitly.
    #[serde(skip)]
    pub grade: GradeReport,
}

/// The grade group exactly as the config API reads and writes it: every channel
/// a `0..=100` percent (see [`crate::theme::GradeKey::to_percent`]), uniform
/// across channels so an agent never reasons about a channel's stored range.
/// This is the **GET** shape — a full report of a scope's current grade.
///
/// `tracking` is deliberately absent: it is a theme-authored roll-bar dial, not
/// a user-facing OSD slider, so v1 of the config API leaves it alone.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize, Default)]
pub struct GradeReport {
    pub brightness: f32,
    pub contrast: f32,
    pub colour: f32,
    pub text: f32,
    pub background: f32,
    pub gamma: f32,
    /// Menu-bar / chrome size (the `Scale` channel).
    pub menu_bar: f32,
    /// Terminal grid text size.
    pub text_size: f32,
    /// Workbench type size. Reported as the size the bench is ACTUALLY
    /// drawn at, which is `text_size` on a pane whose two faces nobody has
    /// split — reading it back and posting it is what splits them.
    pub bench_size: f32,
    /// CRT barrel-warp amount.
    pub warp: f32,
    /// Star-Wars text-crawl toggle (a bool, not a percent).
    pub crawl: bool,
    pub crawl_angle: f32,
    pub crawl_depth: f32,
    /// The CRT master switch (a bool). Off = a flat screen: `warp` and the
    /// roll bar keep their stored values but draw nothing until it is on.
    pub crt: bool,
}

/// The **POST** shape: a *partial* grade. Every field is optional and an absent
/// field is left **unchanged** (PATCH, not PUT) — so "dim the brightness" never
/// silently resets the theme, the warp, or any other channel. Percents are
/// `0..=100`; out-of-range values clamp (the API is "dumb" — it stores the
/// number it is given, it does not interpret "20% lower"; the agent does that
/// math and posts the resulting absolute value). `deny_unknown_fields` makes a
/// typo'd channel a loud error rather than a silent no-op.
#[derive(Clone, PartialEq, Debug, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ConfigPatch {
    pub brightness: Option<f32>,
    pub contrast: Option<f32>,
    pub colour: Option<f32>,
    pub text: Option<f32>,
    pub background: Option<f32>,
    pub gamma: Option<f32>,
    pub menu_bar: Option<f32>,
    pub text_size: Option<f32>,
    pub bench_size: Option<f32>,
    pub warp: Option<f32>,
    pub crawl: Option<bool>,
    pub crawl_angle: Option<f32>,
    pub crawl_depth: Option<f32>,
    pub crt: Option<bool>,
    /// Attach (or clear) the pane's card LOGO: an absolute path to an image file
    /// (png/jpg/jpeg/svg/webp). An empty string clears it. Lets an agent brand the
    /// terminal it's working in — shown as the card portrait on the agent wall.
    pub logo: Option<String>,
    /// Post, replace, or peel the pane's sticky note. Filled in by the
    /// `leave_note` tool (already validated); rides the config pipeline so a
    /// note write shares the one writes gate, the one pane lookup, and the one
    /// GUI-thread apply with every other pane mutation. Pane targets only.
    pub note: Option<NoteChange>,
    /// Declare (or withdraw) what this turn produced — the one artifact a person
    /// is meant to open. Filled in by `declare_deliverable`, already validated;
    /// rides the same pipeline as the note for the same reasons. Pane targets
    /// only.
    pub deliverable: Option<DeliverableChange>,
    /// A work object for the pane's WORKBENCH, already parsed and validated by
    /// [`crate::surface::parse`]. Filled in by `present_surface`; rides the
    /// same pipeline as the note and the deliverable, so a surface write
    /// shares the one writes gate, the one pane lookup and the one GUI-thread
    /// apply with every other pane mutation.
    ///
    /// `skip`, not a wire field: this is the only member of the patch never
    /// deserialised from an agent's JSON, because the agent's JSON is the
    /// *payload* and the verb parses it into this.
    #[serde(skip)]
    pub surface: Option<crate::surface::Post>,
}

/// A validated deliverable declaration on its way to the GUI thread.
#[derive(Clone, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableChange {
    /// Withdraw it. What a turn produced stops being true when the agent says
    /// it does, not when somebody guesses it has gone stale.
    Clear,
    Declare {
        label: String,
        href: String,
    },
}

/// The longest label a 300-pixel queue row can carry.
pub const DELIVERABLE_LABEL_MAX_CHARS: usize = 48;

/// Check a declared deliverable, and say why not in words the agent can act on.
///
/// **The target must be absolute.** A relative path is resolved against
/// whatever the OPENER's working directory happens to be, which is this window's
/// and not the agent's — so `report.html` would open a different file, or
/// nothing, and the row would have lied about what a click does. An agent that
/// knows its own cwd can make it absolute; this cannot do it for them without
/// guessing, and guessing is the thing the whole surface refuses.
///
/// Schemes are allowed through unexamined beyond the list: the desktop's own
/// handler decides what opens a `file://` or an `https://`, and a terminal that
/// second-guessed the MIME database would override a choice the person already
/// made. What is refused is the shapes that are not targets at all — a
/// `javascript:` URL is not a document, and `data:` is a payload pretending to
/// be one.
pub fn validate_deliverable(
    label: Option<&str>,
    href: Option<&str>,
    clear: bool,
) -> Result<DeliverableChange, String> {
    let label = label.map(str::trim).filter(|t| !t.is_empty());
    let href = href.map(str::trim).filter(|t| !t.is_empty());
    if clear || (label.is_none() && href.is_none()) {
        return Ok(DeliverableChange::Clear);
    }
    let Some(href) = href else {
        return Err("a deliverable needs an `href` — a label with nothing to \
                    open is a row that lies about what a click does"
            .to_string());
    };
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("data:") {
        return Err(format!(
            "{href:?} is not a document. A deliverable is a thing a person \
             opens and reads — a file, a page, a pull request."
        ));
    }
    let absolute = href.starts_with('/')
        || lower.starts_with("file://")
        || lower.starts_with("http://")
        || lower.starts_with("https://");
    if !absolute {
        return Err(format!(
            "{href:?} is relative. It would be resolved against the TERMINAL's \
             working directory rather than yours, so give an absolute path or a \
             full URL."
        ));
    }
    // A label is optional and defaults to the target's own last segment, which
    // is what the person would have called it anyway.
    let label = label
        .map(str::to_string)
        .unwrap_or_else(|| deliverable_fallback_label(href));
    let chars = label.chars().count();
    if chars > DELIVERABLE_LABEL_MAX_CHARS {
        return Err(format!(
            "label is {chars} characters; keep it under \
             {DELIVERABLE_LABEL_MAX_CHARS} — it sits on a narrow row beside the \
             reason, so name the thing rather than describing it"
        ));
    }
    Ok(DeliverableChange::Declare {
        label,
        href: href.to_string(),
    })
}

/// What to call a deliverable nobody labelled: the target's last segment.
///
/// Derived rather than left blank, because a row with a link and no name is a
/// row that cannot be read out loud. It is not a guess about CONTENT — the agent
/// still chose the target — only about what to call the thing it already named.
pub fn deliverable_fallback_label(href: &str) -> String {
    let path = href.split(['?', '#']).next().unwrap_or(href);
    let seg = path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path);
    if seg.is_empty() {
        href.to_string()
    } else {
        seg.chars().take(DELIVERABLE_LABEL_MAX_CHARS).collect()
    }
}

/// A validated note write on its way to the GUI thread.
#[derive(Clone, PartialEq, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteChange {
    /// Peel the note off (a no-op on a bare pane).
    Clear,
    /// Put this paper up, replacing whatever note the pane carries.
    Post {
        title: Option<String>,
        text: String,
        pin: bool,
    },
}

/// A pane's posted note as `list_panes` reports it — so an agent can read the
/// fridge door as well as write on it.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct NoteReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub text: String,
    pub pinned: bool,
}

/// The `leave_note` contract: at most this many words of body. A note is what
/// you read from across the room on the way past; past ten words it is a
/// message that wants the transcript.
pub const NOTE_MAX_WORDS: usize = 10;

/// And at most this many characters of headline — "GET MILK!" sized, not a
/// sentence wearing a hat.
pub const NOTE_TITLE_MAX_CHARS: usize = 40;

/// Validate a `leave_note` request into the change to apply. Pure, so the
/// tool's contract is unit-tested without a window. Empty title AND empty text
/// (or `clear`) peel the note; otherwise the body must hold at most
/// [`NOTE_MAX_WORDS`] words, the title at most [`NOTE_TITLE_MAX_CHARS`]
/// characters, and the whole note must fit the paper.
pub fn validate_note(
    title: Option<&str>,
    text: Option<&str>,
    pin: bool,
    clear: bool,
) -> Result<NoteChange, String> {
    let title = title.map(str::trim).filter(|t| !t.is_empty());
    let text = text.map(str::trim).filter(|t| !t.is_empty());
    if clear || (title.is_none() && text.is_none()) {
        return Ok(NoteChange::Clear);
    }
    if let Some(t) = title {
        let chars = t.chars().count();
        if chars > NOTE_TITLE_MAX_CHARS {
            return Err(format!(
                "title is {chars} characters; keep it under {NOTE_TITLE_MAX_CHARS} — \
                 a headline, not a sentence (\"GET MILK!\")"
            ));
        }
    }
    let body = text.unwrap_or("");
    let words = body.split_whitespace().count();
    if words > NOTE_MAX_WORDS {
        return Err(format!(
            "note is {words} words; a sticky holds {NOTE_MAX_WORDS} or fewer — \
             say what happened or what you need, not how it went"
        ));
    }
    let total = title.map(|t| t.chars().count() + 1).unwrap_or(0) + body.chars().count();
    if total > crate::sticky::MAX_CHARS {
        return Err(format!(
            "note is {total} characters; the paper holds {}",
            crate::sticky::MAX_CHARS
        ));
    }
    Ok(NoteChange::Post {
        title: title.map(str::to_string),
        text: body.to_string(),
        pin,
    })
}

impl ConfigPatch {
    /// True when the patch carries no field — a request that would change
    /// nothing, which we reject so a caller learns their `config` was empty
    /// (e.g. a misspelled wrapper key) instead of silently succeeding.
    pub fn is_empty(&self) -> bool {
        *self == ConfigPatch::default()
    }
}

/// What a single get/set addresses: one pane (by pid — the live, ephemeral
/// handle from `list_panes`) or the window-level `outer` scope that every
/// inheriting pane follows. Setting `outer` is the "every terminal at once"
/// lever; setting a pid pins that one pane's grade group.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Target {
    Pane(u32),
    Outer,
}

impl Target {
    /// Parse a JSON target: the string `"outer"` or an integer pid.
    pub fn parse(v: &Value) -> Result<Self, String> {
        if let Some(s) = v.as_str() {
            if s.eq_ignore_ascii_case("outer") {
                return Ok(Target::Outer);
            }
            return Err(format!("unknown target \"{s}\" (want a pid or \"outer\")"));
        }
        if let Some(pid) = v.as_u64() {
            return Ok(Target::Pane(pid as u32));
        }
        Err("target must be a pid (integer) or the string \"outer\"".to_string())
    }

    /// The JSON form echoed back in results (pid as a number, `outer` as a
    /// string) so a caller can correlate without re-parsing the label.
    pub fn to_json(&self) -> Value {
        match self {
            Target::Pane(pid) => json!(pid),
            Target::Outer => json!("outer"),
        }
    }
}

/// One parsed `set_pane_config` update: a target and the partial grade to apply.
pub type ConfigUpdate = (Target, ConfigPatch);

/// The per-target outcome the GUI-thread `apply` closure returns: the resulting
/// effective grade on success, or a human-readable reason it was refused.
pub type ApplyOutcome = (Target, Result<GradeReport, String>);

/// The single policy gate: would a pane with this agent-ness be exposed under
/// `cfg`? Pure, so the safety rule is unit-tested in isolation.
///
/// Disabled ⇒ nothing. `AgentsOnly` ⇒ agents only. `All` ⇒ every pane — but
/// still only while the master switch is on.
pub fn should_expose(cfg: &McpConfig, is_agent: bool) -> bool {
    cfg.enabled && (matches!(cfg.expose, Expose::All) || is_agent)
}

// ===========================================================================
// JSON-RPC 2.0 — the read-only MCP server's wire protocol.
//
// This half is pure: it turns one request *line* into one response *line* given
// a live [`Snapshot`] and a `tail` callback (the file IO that resolves an
// agent's transcript events lives in the caller, so the dispatch stays testable
// with a fake closure). The transport (`mcp_transport`) owns stdio + the gpui
// bridge that produces the snapshot; it never decides protocol shape — that is
// all here, behind one entry point: [`handle_line`].
// ===========================================================================

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// MCP revision we advertise (we echo the client's if it sends one).
pub const PROTOCOL_VERSION: &str = "2025-06-18";
/// Server identity reported in `initialize`.
pub const SERVER_NAME: &str = "terminal-delight";

/// One structured tool-call event, tailed from an agent pane's *own* transcript
/// (claude/codex JSONL) — never the rendered screen.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct ToolEvent {
    /// ISO timestamp from the transcript line (empty if absent).
    pub ts: String,
    /// Tool name the agent invoked (e.g. `Bash`, `Edit`).
    pub tool: String,
    /// A short, single-line gist of the tool input (path / command / first arg).
    pub summary: String,
}

/// One `grep` hit: the scrollback line index, the column of the first match, and
/// the full line text (so an agent gets the context, not just a coordinate).
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct GrepMatch {
    pub line: i32,
    pub col: usize,
    pub text: String,
}

/// All `grep` hits in one exposed pane, with the pane's identity echoed (the same
/// fields `list_panes` reports) so a caller can correlate without a second call.
#[derive(Clone, PartialEq, Debug, Serialize)]
pub struct PaneMatches {
    pub pid: u32,
    pub tab: usize,
    pub title: String,
    pub mode: String,
    pub matches: Vec<GrepMatch>,
}

/// The live data one request is answered from — built fresh per request on the
/// gpui main thread: the operator policy plus the current pane snapshot.
pub struct Snapshot {
    pub config: McpConfig,
    pub panes: Vec<PaneInfo>,
    /// The window-level outer grade (the scope panes inherit from), reported in
    /// the same `0..100` percents — the `outer` target of `get_pane_config`.
    pub outer_grade: GradeReport,
    /// Which window built this snapshot. `None` only where there is no window to
    /// ask — a snapshot-independent method, or a UI that has gone away — and it
    /// stays `Option` to the wire, because a window that cannot name itself is a
    /// fact a caller needs and silence would read as agreement.
    pub instance: Option<Instance>,
    /// Who asked, when they were able to say. Per-request rather than per-window,
    /// which is why it lives on the snapshot: the snapshot is the live data ONE
    /// request is answered from.
    pub caller: Option<Caller>,
    /// The engineering state of every project the rail has read — checkouts,
    /// drift, what it would take to land everything. The `engineering_state`
    /// tool's whole answer; empty until the rail's first scan lands.
    pub engineering: Vec<crate::engstate::Report>,
    /// Every document open in this window, floating or on a pane of its own,
    /// with what its notes layer shows. What `document_notes` looks through
    /// for the one beside its caller.
    pub documents: Vec<DocInfo>,
}

/// Where an open document is drawn: floating over a pane's terminal, or on a
/// pane of its own beside it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DocPlace {
    Float,
    Split,
}

/// One document open in this window, as `document_notes` sees it.
///
/// Built on the main thread with the rest of the snapshot, so the verb stays
/// pure over data and its one rule — which document counts as beside whom —
/// is tested without a window. See [`document_beside`].
#[derive(Clone, PartialEq, Debug)]
pub struct DocInfo {
    /// Index of the tab the document is in.
    pub tab: usize,
    /// The shell pid of the pane it is drawn on: the pane a floating square
    /// floats over, or the split's own pane.
    pub pane: u32,
    pub place: DocPlace,
    /// For a split, the host id of the pane it was opened beside. `None` for a
    /// float, whose pane is [`Self::pane`], and for a split whose opening this
    /// window did not see — one restored from a saved layout — which is still
    /// found by its tab.
    pub opened_by: Option<u64>,
    pub path: String,
    /// What the notes layer reports: the same JSON `ctl doc notes` prints,
    /// map included. `Err` says why there is none — a document that is not a
    /// brief, or a brief not laid out yet.
    pub notes: Result<Value, String>,
}

/// How the document beside a caller was found, which the answer says out
/// loud so the agent knows how sure to be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Beside {
    /// A floating square over the caller's own pane.
    FloatOverYou,
    /// A pane in the caller's tab that was opened beside the caller.
    YourSplit,
    /// A document pane in the caller's tab that this window did not see the
    /// caller open. `of` counts such panes; the first in the tab's order is
    /// the one answered.
    InYourTab { of: usize },
}

/// The document beside `caller`: a floating square over its own pane, else a
/// document pane in its tab that it opened, else a document pane in its tab.
///
/// Never a document in another tab, and never a square floating over another
/// pane: those are beside somebody else, and a person's notes on them are not
/// this caller's to read. The tab is checked on every rule, including the one
/// matching on who opened the split, because a pane can be dragged into
/// another tab and take its opener's id with it.
pub fn document_beside<'a>(
    docs: &'a [DocInfo],
    caller: &PaneInfo,
) -> Option<(&'a DocInfo, Beside)> {
    let here = |d: &&DocInfo| d.tab == caller.tab;
    if let Some(d) = docs
        .iter()
        .filter(here)
        .find(|d| d.place == DocPlace::Float && d.pane == caller.pid)
    {
        return Some((d, Beside::FloatOverYou));
    }
    let splits: Vec<&DocInfo> = docs
        .iter()
        .filter(here)
        .filter(|d| d.place == DocPlace::Split)
        .collect();
    // An opener nobody recorded is not the caller. `None == None` would say it
    // is, and hand a pane with no host id every split restored from a layout.
    if let Some(d) = splits
        .iter()
        .find(|d| d.opened_by.is_some() && d.opened_by == caller.pane_id)
    {
        return Some((d, Beside::YourSplit));
    }
    splits
        .first()
        .map(|d| (*d, Beside::InYourTab { of: splits.len() }))
}

/// Who is asking, as derived from their own process tree by the relay.
///
/// The host that forked a pane's shell is an ancestor of everything running in
/// it, so a process inside a pane can find out which session and which pane it
/// is in by walking its own parents and asking the authority — no environment
/// variable, and nothing a wrapper could strip. See `ctl::locate`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Caller {
    /// The session the caller believes it is in. The window compares this
    /// against its own before answering anything.
    pub session: String,
    /// The pane the caller is in, when the host could name one. `None` is a
    /// real answer and never means pane zero.
    pub pane: Option<u64>,
}

impl Snapshot {
    /// Panes the policy currently permits a connected agent to see.
    fn exposed(&self) -> Vec<&PaneInfo> {
        self.panes.iter().filter(|p| p.exposed).collect()
    }

    /// Which pane a pane-scoped verb should act on: the one the caller named,
    /// else the one the caller is sitting in.
    ///
    /// The default is the whole point. `declare_deliverable` asked for a `pid`
    /// "from list_panes" and the caller's only way to find itself in that
    /// listing was its title and working directory — which two agents on one
    /// project share, so the rule was "guess, and if you guess wrong your turn's
    /// deliverable appears on somebody else's row". The pane id the host put in
    /// the caller's environment settles it without asking anyone to match
    /// strings.
    ///
    /// The error says which of the two ways to name a pane were available, so a
    /// caller whose chain the host could not read is told that rather than
    /// being told to try harder.
    fn target_pane(&self, named: Option<u32>) -> Result<u32, String> {
        if let Some(pid) = named {
            return Ok(pid);
        }
        let Some(caller) = &self.caller else {
            return Err("no `pid`, and this connection did not say who is calling \
                        — pass a pid from list_panes"
                .to_string());
        };
        let Some(pane) = caller.pane else {
            return Err("no `pid`, and the host could not say which pane you are \
                        in (a pane it did not start, or a process detached from \
                        its own parents) — pass a pid from list_panes"
                .to_string());
        };
        self.panes
            .iter()
            .find(|p| p.pane_id == Some(pane))
            .map(|p| p.pid)
            .ok_or_else(|| {
                format!(
                    "no `pid`, and pane {pane} — the pane you are calling from — \
                     is not in this window's listing"
                )
            })
    }

    /// The pane the caller is sitting in, for a verb that answers for that
    /// pane and no other.
    ///
    /// [`Self::target_pane`]'s three ways of not knowing, in sentences that do
    /// not send the caller off to find a pid: a verb scoped to its caller takes
    /// none, so "pass a pid" would be advice it refuses.
    fn caller_pane(&self, verb: &str) -> Result<&PaneInfo, String> {
        let Some(caller) = &self.caller else {
            return Err(format!(
                "this connection did not say who is calling, and {verb} answers \
                 only for the pane it is called from — call it from inside a \
                 Terminal Delight pane"
            ));
        };
        let Some(pane) = caller.pane else {
            return Err(format!(
                "the host could not say which pane you are in (a pane it did not \
                 start, or a process detached from its own parents), and {verb} \
                 answers only for the pane it is called from"
            ));
        };
        self.panes
            .iter()
            .find(|p| p.pane_id == Some(pane))
            .ok_or_else(|| {
                format!(
                    "pane {pane} — the pane you are calling from — is not in this \
                     window's listing"
                )
            })
    }

    /// A snapshot that exposes nothing — used to answer snapshot-independent
    /// methods (initialize / tools/list / ping …) without a main-thread
    /// round-trip, and as the safe fallback when the UI has gone away.
    pub fn empty() -> Self {
        Self {
            config: McpConfig::default(),
            panes: vec![],
            outer_grade: GradeReport::default(),
            instance: None,
            caller: None,
            engineering: vec![],
            documents: vec![],
        }
    }
}

/// A parsed request line. `id` absent ⇒ a notification (gets no reply).
struct Req {
    id: Option<Value>,
    method: String,
    params: Value,
}

fn parse_req(line: &str) -> Option<Req> {
    let v: Value = serde_json::from_str(line).ok()?;
    Some(Req {
        id: v.get("id").cloned(),
        method: v.get("method")?.as_str()?.to_string(),
        params: v.get("params").cloned().unwrap_or(Value::Null),
    })
}

/// Handle one JSON-RPC line. Returns the response line to write to stdout, or
/// `None` for a notification or an unparseable line (both get no reply). `tail`
/// resolves recent tool events for a pane.
pub fn handle_line<F>(line: &str, snap: &Snapshot, tail: F) -> Option<String>
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
{
    handle_line_with(line, snap, tail, no_apply, no_search)
}

/// Like [`handle_line`] but with a *write* capability. `apply` performs the
/// `set_pane_config` mutation — in the live server that is a round-trip onto the
/// gpui main thread (see `mcp_transport`) — and returns the per-target outcome.
/// The read-only [`handle_line`] supplies [`no_apply`], which refuses every
/// write, so a connection that never wires a real `apply` cannot mutate anything
/// regardless of policy. Keeping the effect behind a closure mirrors the `tail`
/// pattern: all protocol shape stays here, all IO/GUI lives in the caller.
pub fn handle_line_with<F, G, H>(
    line: &str,
    snap: &Snapshot,
    tail: F,
    apply: G,
    search: H,
) -> Option<String>
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
    H: Fn(&str, usize) -> Vec<PaneMatches>,
{
    handle_line_full(line, snap, tail, apply, search, no_open)
}

/// Like [`handle_line_with`], with the one capability that changes the layout:
/// `open`, which opens a document in the CALLER's pane or tab for
/// `open_document`. A separate closure rather than a field of the config patch,
/// because what comes back is the router's own sentence — focused, split,
/// floated at four panes — and not a grade.
pub fn handle_line_full<F, G, H, O>(
    line: &str,
    snap: &Snapshot,
    tail: F,
    apply: G,
    search: H,
    open: O,
) -> Option<String>
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
    H: Fn(&str, usize) -> Vec<PaneMatches>,
    O: Fn(&OpenRequest) -> OpenOutcome,
{
    let req = parse_req(line)?;
    // A notification (no id) is fire-and-forget — never answer it, even on error.
    let id = req.id.clone()?;
    Some(match dispatch(&req, snap, &tail, &apply, &search, &open) {
        Ok(result) => encode_ok(id, result),
        Err((code, msg)) => encode_err(id, code, msg),
    })
}

/// The read-only `search`: finds nothing. Used by the bare [`handle_line`] and any
/// transport that does not wire pane-content search, so `grep` is inert there.
pub fn no_search(_needle: &str, _cap: usize) -> Vec<PaneMatches> {
    Vec::new()
}

/// Where `open_document` puts a document.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Placement {
    /// A pane of its own beside the caller, as Ctrl+Alt+click on its path
    /// opens one — or, at four panes, floating over the caller instead.
    Beside,
    /// Floating over the caller's own pane, as Alt+click on its path opens it.
    Here,
}

impl Placement {
    fn as_str(self) -> &'static str {
        match self {
            Placement::Beside => "beside",
            Placement::Here => "here",
        }
    }
}

/// A validated `open_document` on its way to the GUI thread.
///
/// `pid` is always the caller's own pane. There is no way to build one naming
/// another: the verb takes no pane, and fills this in from who called.
#[derive(Clone, PartialEq, Debug)]
pub struct OpenRequest {
    pub pid: u32,
    pub target: crate::docopen::DocTarget,
    pub placement: Placement,
}

/// What the window did with an `open_document`: the router's own sentence, or
/// why nothing opened.
pub type OpenOutcome = Result<String, String>;

/// The `open` of a connection that cannot change the layout.
pub fn no_open(_: &OpenRequest) -> OpenOutcome {
    Err("this connection cannot open documents".to_string())
}

/// The router's reply, as `ctl doc beside` prints it, turned into an outcome:
/// `ok …` opened or focused, `desktop …` went to the desktop because TD has no
/// engine to draw it, and anything else is the sentence that refused it.
pub fn open_outcome(reply: &str) -> OpenOutcome {
    if let Some(said) = reply.strip_prefix("ok ") {
        return Ok(said.to_string());
    }
    if let Some(why) = reply.strip_prefix("desktop ") {
        return Ok(format!("handed to the desktop instead: {why}"));
    }
    Err(reply.strip_prefix("err ").unwrap_or(reply).to_string())
}

/// Check an `open_document` path, and say why not in words an agent can act
/// on. Absolute, as a path or a `file://` URL — a relative path would resolve
/// against the TERMINAL's directory rather than the agent's, the reason
/// `declare_deliverable` refuses one — and a file TD can draw: Markdown, HTML,
/// an image or a video. Reads the file's first sixteen bytes and nothing else.
pub fn validate_open_path(path: &str) -> Result<crate::docopen::DocTarget, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("open_document needs a `path`: the absolute path of the file to open".into());
    }
    let lower = path.to_ascii_lowercase();
    let file = if lower.starts_with("file:") {
        match crate::docview::resolve_link(std::path::Path::new("/"), path) {
            crate::docview::LinkTarget::File { path, .. } => path,
            _ => {
                return Err(format!(
                    "{path:?} is not a file on this machine — a file:// URL must name one"
                ))
            }
        }
    } else if path.starts_with('/') {
        std::path::PathBuf::from(path)
    } else if lower.starts_with("http://") || lower.starts_with("https://") {
        return Err(format!(
            "{path:?} is a web address; open_document opens files TD can draw. \
             Declare it with declare_deliverable and the rail opens it with the desktop."
        ));
    } else {
        return Err(format!(
            "{path:?} is relative. It would be resolved against the TERMINAL's \
             working directory rather than yours, so give an absolute path."
        ));
    };
    crate::docopen::drawable_document(&file).ok_or_else(|| {
        format!(
            "{} is not a file TD can draw — Markdown, HTML, an image or a video, and it \
             has to exist",
            file.display()
        )
    })
}

/// The read-only `apply`: refuses every update with a clear reason. Used by the
/// bare [`handle_line`] and by any transport that does not offer writes.
pub fn no_apply(updates: &[ConfigUpdate]) -> Vec<ApplyOutcome> {
    updates
        .iter()
        .map(|(t, _)| (t.clone(), Err("this connection is read-only".to_string())))
        .collect()
}

fn dispatch<F, G, H, O>(
    req: &Req,
    snap: &Snapshot,
    tail: &F,
    apply: &G,
    search: &H,
    open: &O,
) -> Result<Value, (i64, String)>
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
    H: Fn(&str, usize) -> Vec<PaneMatches>,
    O: Fn(&OpenRequest) -> OpenOutcome,
{
    match req.method.as_str() {
        "initialize" => Ok(initialize_result(&req.params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_defs() })),
        "tools/call" => tools_call(&req.params, snap, tail, apply, search, open),
        // We hold no resources/prompts — answer empty so discovery doesn't error.
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        // The client sets a logging verbosity; we acknowledge. The transport
        // separately reads the level (see `log_level_from`) to gate the push
        // feed — the dispatch stays pure and just confirms receipt.
        "logging/setLevel" => Ok(json!({})),
        m => Err((-32601, format!("method not found: {m}"))),
    }
}

fn initialize_result(params: &Value) -> Value {
    let pv = params
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or(PROTOCOL_VERSION);
    json!({
        "protocolVersion": pv,
        // `logging`: we push `notifications/message` as agents act (see Watcher).
        "capabilities": { "tools": {}, "logging": {} },
        "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
        "instructions":
            "Watch and configure terminal-delight's panes. SEVERAL TERMINAL \
             DELIGHTS RUN ON ONE MACHINE — one window per workspace, each with \
             its own panes — so every tool result ends with the instance that \
             answered it (session, window pid, build) and carries the same under \
             `structuredContent.instance`. Check it before believing a listing \
             is yours. To find your OWN pane in a listing, match the `pane <n>` \
             field against your `$TD_PANE_ID`; titles and directories are not \
             unique. `list_panes` reports \
             who is running where (mode, cwd, agent session); `pane_events` tails \
             an agent pane's own transcript for recent tool calls. With logging \
             enabled the server also pushes `notifications/message` as agents \
             appear, vanish, and call tools, so you can react without polling. \
             `get_pane_config` / `set_pane_config` read and change a pane's (or \
             the window-level `outer`) appearance — brightness, contrast, colour, \
             warp, text size, crawl, the CRT switch — in uniform 0..100 percents \
             and two booleans; writes need the \
             server's opt-in writes toggle. The config API is dumb: it stores the \
             absolute number you give it, so compute relative changes (\"20% \
             lower\") yourself from a get_pane_config read. `document_notes` \
             hands you the notes a person left on the brief open beside you — \
             the map alone, not the page; `open_document` opens a document you \
             made beside you, in your own tab. Appearance and layout are all you \
             can change: nothing here can write bytes to a terminal/PTY."
    })
}

fn tool_defs() -> Value {
    json!([
        {
            "name": "list_panes",
            "description":
                "List the terminal-delight panes currently exposed to MCP — \
                 mode (claude/codex/shell), cwd, and resumable agent session. \
                 Read-only.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "pane_events",
            "description":
                "Recent structured tool-call events for one agent pane, tailed \
                 from its own transcript (not the screen). Read-only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "description": "pid of the pane, from list_panes" },
                    "limit": { "type": "integer", "description": "max events to return (default 20, max 200)" }
                },
                "required": ["pid"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_pane_config",
            "description":
                "Read the appearance (monitor grade) of one or more panes, or the \
                 window-level `outer` scope. Every channel is reported as a \
                 0..100 percent (brightness, contrast, colour, text, background, \
                 gamma, menu_bar, text_size, bench_size, warp, crawl_angle, crawl_depth) plus \
                 a `crawl` boolean and a `crt` boolean (off = a flat screen; \
                 warp and the roll bar keep their values but draw nothing). \
                 Omit `targets` to report every exposed pane \
                 plus `outer`. Read-only. To change a value, GET it, compute the \
                 new absolute number yourself, then POST it with set_pane_config.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "targets": {
                        "type": "array",
                        "description": "pids (from list_panes) and/or the string \"outer\"; omit for all",
                        "items": { "type": ["integer", "string"] }
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "set_pane_config",
            "description":
                "Set the appearance of one or more panes (by pid) or the `outer` \
                 scope. Each update carries a partial `config`: only the channels \
                 you include change (0..100 percents; out-of-range values clamp), \
                 everything else is left untouched. The API is deliberately dumb \
                 — it stores the absolute number you give it and does NOT \
                 interpret relative asks like \"20% lower\"; read the current \
                 value with get_pane_config, do that math yourself, and post the \
                 result. Setting `outer` re-grades every pane that inherits it — \
                 the one-shot way to change every terminal at once. A pane update \
                 may ALSO carry `logo` — an absolute path to an image (png/jpg/\
                 jpeg/svg/webp) — to BRAND the terminal it targets: the image shows \
                 as that pane's card portrait on the agent wall (\"\" clears it). \
                 That lets an agent attach a logo to the terminal it's working in. \
                 Requires the server's writes toggle (TD_MCP_WRITE).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "updates": {
                        "type": "array",
                        "description": "one entry per target to change",
                        "items": {
                            "type": "object",
                            "properties": {
                                "target": {
                                    "type": ["integer", "string"],
                                    "description": "a pid, or \"outer\""
                                },
                                "config": {
                                    "type": "object",
                                    "description": "partial grade — any of brightness/contrast/colour/text/background/gamma/menu_bar/text_size/bench_size/warp/crawl_angle/crawl_depth (0..100) and crawl/crt (bool) — AND `logo`: an absolute path to an image file (png/jpg/jpeg/svg/webp) to ATTACH as this pane's card logo (the agent wall portrait); an empty string clears it. Pane targets only."
                                }
                            },
                            "required": ["target", "config"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["updates"],
                "additionalProperties": false
            }
        },
        {
            "name": "leave_note",
            "description":
                "Stick a handwritten note on a pane's glass — the fridge-door \
                 message a returning human reads FIRST, before any transcript. \
                 A shouty little `title` (\"GET MILK!\", \"NEEDS EYES\", \
                 \"DONE ✓\") plus at most TEN words of `text` saying what \
                 happened or what you need (\"home at 7pm\", \"tests green, \
                 deploy paused, waiting on DNS\"). Leave one when you finish a \
                 run, hit a wall, or hand work back; post again to change it \
                 (same paper, new words), and `clear` (or an empty note) peels \
                 it off. `pin: true` sticks a pushpin through it that surfaces \
                 on the pane's TAB in the mother bar — use it when the note \
                 needs attention rather than merely records. The note lands on \
                 the same paper a human alt+s note uses: they can peel it, edit \
                 it, or pin it, and it survives a restart. list_panes shows \
                 each pane's current note. Requires the writes toggle \
                 (TD_MCP_WRITE).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "description": "pid of the pane to note, from list_panes. OMIT IT to note your own pane — the window works out which one you are calling from, which is more reliable than matching yourself in a listing." },
                    "title": { "type": "string", "description": "optional headline, up to 40 characters, drawn bigger and bolder (\"GET MILK!\")" },
                    "text": { "type": "string", "description": "the note itself — TEN words or fewer; what happened, or why this pane needs attention" },
                    "pin": { "type": "boolean", "description": "stick a pushpin through it (surfaces on the pane's tab); default false" },
                    "clear": { "type": "boolean", "description": "peel the note off instead of posting one" }
                },
                "required": [],
                "additionalProperties": false
            }
        },
        {
            "name": "declare_deliverable",
            "description":
                "Name the ONE artifact this turn produced — the thing a person \
                 is meant to open and read — so it appears as a click on your \
                 pane's row in the attention rail. A report, a page, a plan, a \
                 pull request: `label` is what to call it (a name, not a \
                 sentence) and `href` is an absolute path or a full URL. It is \
                 opened by the desktop's own handler, so a Markdown file goes \
                 wherever this machine sends Markdown. Declare it when you \
                 finish something worth opening; it is DECLARED and never \
                 inferred, which is why nothing appears unless you say so. Call \
                 again to replace it, `clear: true` to withdraw it. Point it at \
                 what the reader can open AND understand — a page explaining \
                 what you did beats the source file you changed. Requires the \
                 writes toggle (TD_MCP_WRITE).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "description": "pid of the pane that produced it, from list_panes. OMIT IT for your own pane — the usual case, and the window resolves it from who is calling rather than leaving you to match a title." },
                    "label": { "type": "string", "description": "what to call it, up to 48 characters (\"Slice ledger\", \"PR 460\"); defaults to the target's filename" },
                    "href": { "type": "string", "description": "absolute path (/home/you/report.html) or full URL (https://…). Relative paths are refused: they would resolve against the terminal's directory, not yours." },
                    "clear": { "type": "boolean", "description": "withdraw the current declaration instead of making one" }
                },
                "required": [],
                "additionalProperties": false
            }
        },
        {
            "name": "present_surface",
            "description":
                "Put a WORK OBJECT on this pane's workbench — the pane's second \
                 face, toggled from its header beside the terminal. Describe what \
                 the thing MEANS and Terminal Delight renders it natively. END \
                 EVERY TURN with a `response`: the workbench's OVERVIEW is a feed \
                 of these and shows nothing else. A response is a `layman` \
                 (required) — the whole reply in plain English, written for the \
                 person and not for yourself, and the register the card opens on \
                 — and a `brief`, the bare minimum in at most two sentences and \
                 under fifty words, serious and direct and never a metaphor, \
                 drawn last as the reading a person drops to \
                 — plus registers a person unfolds by name: `technical`, \
                 `evidence`, `asks`, `next`, and `doubts`, where you are not \
                 sure, each a `claim` with a `why` and a `confidence`. Any other \
                 key becomes a section labelled by its key; a string is prose, an \
                 array a list, an object facts. Every register is a PEER and the \
                 reader lights the one they are in, so do not try to emphasise \
                 one. The one thing that interrupts is \
                 `escalation`: {\"level\":\"blocking|wanted|none\", \"why\":\"…\", \
                 \"items\":[{\"ask\":\"…\",\"answered\":false}]}. Declare `none` \
                 when you need nothing — leaving it out means you never said, \
                 which is a different answer. The other kinds are what you MADE \
                 and what you are ASKING: decision, changeset, architecture, \
                 table, markdown, artifact, question. Never describe layout — no \
                 widths, no colours, no components; the window owns all of that. \
                 `surface` is one TDSP document: \
                 {\"td\":\"0.4\", \"kind\":\"response\", \"title\":\"…\", \
                 \"model\":{\"layman\":\"…\",\"brief\":\"…\",\"technical\":\"…\",\
                 \"doubts\":[…]}}. \
                 Send the same `id` again to update it in place, or op \"retire\" \
                 to take it off the bench. An unknown kind is shown as \
                 unclassified rather than dropped, so it is always safe to send. \
                 When a person acts on it you receive a line beginning [workbench] \
                 in this terminal. Call surface_catalogue for the kinds and their \
                 models. Requires the writes toggle (TD_MCP_WRITE).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pid": { "type": "integer", "description": "pid of the pane whose bench this belongs on, from list_panes — usually your own" },
                    "surface": { "type": "object", "description": "one TDSP document: td, kind, title, model, and optionally id, op, weight, source, actions" }
                },
                "required": ["pid", "surface"],
                "additionalProperties": false
            }
        },
        {
            "name": "engineering_state",
            "description":
                "The engineering state of every project this window has read — what a \
                 coordinating agent needs before landing work in flight. Per project: the \
                 checkouts its panes are in (branch, dirty files and ±lines, ahead/behind \
                 main, upstream and unpushed commits, the files the line touches, and \
                 whether it would merge into main today — a merge-tree dry run naming the \
                 conflicting files), the idle worktrees on disk that nobody is in, panes \
                 filed here but working in another repository and vice versa, pairs of \
                 lines converging on the same files, stashes, and the LANDING list: the \
                 ordered things that must become true for everything to be on main, each \
                 naming its line. Read-only; the rail moves nothing. A null field was not \
                 measured — it is never zero. Empty until the rail's first scan lands.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "document_notes",
            "description":
                "The notes a person left on the document open BESIDE YOU — a \
                 brief or a Markdown file floating over your own pane, else a \
                 document pane in your tab that you opened, else a document pane \
                 in your tab. Answers the notes map (anchor, heading, the notes \
                 under it), which is the text the bar's \"copy map\" gives, so \
                 you read 80 words of notes instead of re-reading the whole page. \
                 A brief's map names element ids; a Markdown file's names the \
                 line each block starts on, as [L42]. Notes added and not yet \
                 saved into the file are included, and the answer counts them. \
                 Takes no arguments: it answers for the pane you call it \
                 from and never reaches a document in another tab. Read-only; it \
                 writes nothing, anywhere.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "open_document",
            "description":
                "Open a document IN TERMINAL DELIGHT, beside you — the page, \
                 brief, picture or clip you just made, so the person reads it next to \
                 your prompt instead of going to look for it. `path` is an \
                 absolute path or a file:// URL to Markdown, HTML, an image or a video. \
                 `placement` \"beside\" (the default) opens it in a pane of its \
                 own to the right of yours, exactly as Ctrl+Alt+click on the \
                 path does: a pane in your tab already showing the file is \
                 focused instead, and at four panes it floats over your pane. \
                 \"here\" floats it over your own pane, as Alt+click does. It \
                 only ever opens in your own pane or tab, and takes no pid. Call \
                 it after declare_deliverable, for an HTML or Markdown \
                 deliverable. Requires the writes toggle (TD_MCP_WRITE).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "absolute path (/home/you/report.html) or file:// URL of a Markdown, HTML, image or video file" },
                    "placement": { "type": "string", "enum": ["beside", "here"], "description": "\"beside\" (default): a pane of its own beside yours; \"here\": floating over your own pane" }
                },
                "required": ["path"],
                "additionalProperties": false
            }
        },
        {
            "name": "surface_catalogue",
            "description": "What kinds of work object this build of Terminal Delight can render, the actions a person can take on them, and the weights a surface may carry. Read-only. Ask before presenting if you are unsure a kind exists — an unknown kind still lands, but as unclassified.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "grep",
            "description": "Search the recent scrollback of every EXPOSED pane for an exact, case-insensitive substring. Returns, per matching pane, its identity (pid/tab/title/mode) and the matching lines with the match column. Read-only — it reads on-screen text, never writes. Use it to find where something is across the whole window (an error, a path, a TODO, a value).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Exact substring to find (case-insensitive)."
                    },
                    "scrollback": {
                        "type": "integer",
                        "description": "How many recent lines per pane to search (default 2000, max 50000)."
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }
        }
    ])
}

fn tools_call<F, G, H, O>(
    params: &Value,
    snap: &Snapshot,
    tail: &F,
    apply: &G,
    search: &H,
    open: &O,
) -> Result<Value, (i64, String)>
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
    H: Fn(&str, usize) -> Vec<PaneMatches>,
    O: Fn(&OpenRequest) -> OpenOutcome,
{
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "tools/call requires a `name`".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    let out = match name {
        "list_panes" => list_panes(snap),
        "engineering_state" => engineering_state(snap),
        "pane_events" => pane_events(&args, snap, tail),
        "get_pane_config" => get_pane_config(&args, snap),
        "set_pane_config" => set_pane_config(&args, snap, apply),
        "leave_note" => leave_note(&args, snap, apply),
        "declare_deliverable" => declare_deliverable(&args, snap, apply),
        "present_surface" => present_surface(&args, snap, apply),
        "surface_catalogue" => surface_catalogue(),
        "document_notes" => document_notes(&args, snap),
        "open_document" => open_document(&args, snap, open),
        "grep" => grep(&args, snap, search),
        other => return Err((-32602, format!("unknown tool: {other}"))),
    };
    Ok(stamp(out, snap))
}

/// Stamp the answering instance onto a tool result.
///
/// Both halves, because they are read by different things: the text is what a
/// model sees, the structured half is what a program joins on. Errors are
/// stamped too — a refusal from the wrong window ("MCP exposure is disabled")
/// is exactly the answer somebody would act on without ever asking which
/// terminal refused.
///
/// Done here rather than inside each verb on purpose. Nine tools would be
/// nine chances to forget, and the tenth would ship without it.
fn stamp(mut v: Value, snap: &Snapshot) -> Value {
    let line = match &snap.instance {
        Some(i) => i.line(),
        // Not silence. A window that cannot name itself is a finding, and an
        // unstamped answer beside stamped ones would read as "same as before".
        None => "— terminal-delight · instance unknown: this answer names no window".to_string(),
    };
    if let Some(content) = v.get_mut("content").and_then(Value::as_array_mut) {
        content.push(json!({ "type": "text", "text": line }));
    }
    let who = serde_json::to_value(&snap.instance).unwrap_or(Value::Null);
    match v
        .get_mut("structuredContent")
        .and_then(Value::as_object_mut)
    {
        Some(obj) => {
            obj.insert("instance".into(), who);
        }
        // A refusal carries no structured half of its own. It gets one here
        // anyway, because a caller that reads `structuredContent.instance` to
        // decide whether an answer is its own needs that to work on the answers
        // it is most likely to act on blind — "exposure is disabled", "unknown
        // pid" — and those are exactly the errors a wrong window produces.
        None => {
            v["structuredContent"] = json!({ "instance": who });
        }
    }
    v
}

/// `grep` — search every exposed pane's scrollback for an exact, case-insensitive
/// substring (the `search` closure does the per-pane work on the GUI thread; here
/// we just shape the request and the response). Read-only; gated by the same
/// expose policy as `list_panes`, so disclosure of on-screen text follows it.
fn grep<H>(args: &Value, snap: &Snapshot, search: &H) -> Value
where
    H: Fn(&str, usize) -> Vec<PaneMatches>,
{
    if !snap.config.enabled {
        return tool_err(
            "MCP exposure is disabled. Enable it in terminal-delight's MCP \
             CONTROL panel (the robot button on the mother bar).",
        );
    }
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    if query.is_empty() {
        return tool_err("grep needs a non-empty \"query\" string");
    }
    let cap = args
        .get("scrollback")
        .and_then(Value::as_u64)
        .unwrap_or(2000)
        .clamp(1, 50_000) as usize;
    // Per-pane match cap so a noisy pane can't flood one response.
    const PER_PANE: usize = 50;
    let mut panes = search(query, cap);
    let mut truncated = false;
    let mut total = 0usize;
    for p in &mut panes {
        if p.matches.len() > PER_PANE {
            p.matches.truncate(PER_PANE);
            truncated = true;
        }
        total += p.matches.len();
    }
    let text = if panes.is_empty() {
        format!("no matches for {query:?} in any exposed pane")
    } else {
        format!(
            "{total} match{} for {query:?} across {} pane{}{}",
            if total == 1 { "" } else { "es" },
            panes.len(),
            if panes.len() == 1 { "" } else { "s" },
            if truncated {
                format!(" (capped at {PER_PANE} per pane)")
            } else {
                String::new()
            }
        )
    };
    tool_ok(text, json!({ "query": query, "panes": panes }))
}

/// The rail's reading of every project, for a program.
///
/// The text half is the thing an agent can act on without parsing: one block
/// per project with its badge, its sentence and its landing list, the active
/// project first. The structured half is the whole reading, every `Option`
/// still an `Option`.
fn engineering_state(snap: &Snapshot) -> Value {
    if !snap.config.enabled {
        return tool_err(
            "MCP exposure is disabled. Enable it in terminal-delight's MCP \
             CONTROL panel (the robot button on the mother bar).",
        );
    }
    let mut projects: Vec<&crate::engstate::Report> = snap.engineering.iter().collect();
    projects.sort_by_key(|r| !r.active);
    let text = if projects.is_empty() {
        "the rail has not read any project yet — no scan has landed".to_string()
    } else {
        projects
            .iter()
            .map(|r| {
                let mut s = format!(
                    "{}{}  [{}]  read {}s ago",
                    r.name.as_deref().unwrap_or("(unfiled)"),
                    if r.active { " (active)" } else { "" },
                    r.badge.join(" \u{00b7} "),
                    r.read_secs_ago
                );
                if let Some(sentence) = &r.sentence {
                    s.push_str(&format!("\n  {sentence}"));
                }
                for c in r.checkouts.iter().chain(r.idle.iter()) {
                    let who: Vec<String> = c
                        .writers
                        .iter()
                        .map(|w| {
                            format!("{} \u{2018}{}\u{2019}", w.label.to_lowercase(), w.tab_name)
                        })
                        .collect();
                    s.push_str(&format!(
                        "\n  {} {}  dirty={} ahead={} behind={} unpushed={} merge={}  {}",
                        if c.idle {
                            "idle"
                        } else if c.shared {
                            "SHARED"
                        } else {
                            "wt"
                        },
                        c.line,
                        c.dirty.map_or("?".into(), |v| v.to_string()),
                        c.ahead.map_or("?".into(), |v| v.to_string()),
                        c.behind.map_or("?".into(), |v| v.to_string()),
                        c.unpushed.map_or("?".into(), |v| v.to_string()),
                        c.merge.as_deref().unwrap_or("?"),
                        if who.is_empty() {
                            c.root.clone()
                        } else {
                            who.join(", ")
                        }
                    ));
                }
                for f in &r.foreign {
                    s.push_str(&format!(
                        "\n  \u{26a0} \u{2018}{}\u{2019} is filed here but working in {}",
                        f.writer.tab_name, f.repo
                    ));
                }
                for v in &r.visitors {
                    s.push_str(&format!(
                        "\n  \u{25cc} \u{2018}{}\u{2019} is working here but filed under {}",
                        v.writer.tab_name,
                        v.filed_under.as_deref().unwrap_or("nothing")
                    ));
                }
                if !r.landing.is_empty() {
                    s.push_str("\n  to land everything:");
                    for (i, item) in r.landing.iter().enumerate() {
                        s.push_str(&format!("\n    {}. {}", i + 1, item.text));
                    }
                }
                for e in &r.afterglow {
                    s.push_str(&format!("\n  \u{25c6} {e}"));
                }
                s
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    tool_ok(
        text,
        json!({ "projects": serde_json::to_value(&snap.engineering).unwrap_or(Value::Null) }),
    )
}

fn list_panes(snap: &Snapshot) -> Value {
    if !snap.config.enabled {
        return tool_err(
            "MCP exposure is disabled. Enable it in terminal-delight's MCP \
             CONTROL panel (the robot button on the mother bar).",
        );
    }
    let exposed = snap.exposed();
    let text = if exposed.is_empty() {
        "no panes are currently exposed under the active policy".to_string()
    } else {
        exposed
            .iter()
            .map(|p| {
                let cwd = p.cwd.as_deref().unwrap_or("?");
                let sess = p.session.as_deref().unwrap_or("-");
                // The tool in flight rides the LINE, not just the structured
                // half — an agent reading this listing sees what each pane is
                // doing, and so does anyone checking that a pane is wearing the
                // right face without photographing the screen.
                let tool = p
                    .tool
                    .as_deref()
                    .map(|t| format!(" · \u{2692} {t}"))
                    .unwrap_or_default();
                // The note rides the line too — the fridge door is for reading.
                let note = p
                    .note
                    .as_ref()
                    .map(|n| match n.title.as_deref() {
                        Some(t) if !n.text.is_empty() => format!(" · \u{1f4dd} {t} — {}", n.text),
                        Some(t) => format!(" · \u{1f4dd} {t}"),
                        None => format!(" · \u{1f4dd} {}", n.text),
                    })
                    .unwrap_or_default();
                // The pane id rides the line beside the tab, because it is how
                // a caller finds ITSELF in this listing: the host stamps the
                // same number into the pane as `$TD_PANE_ID`. Matching on title
                // and cwd was the only way before, and in a window holding two
                // agents in one directory it picks the wrong one.
                let pane = p
                    .pane_id
                    .map(|id| format!("pane {id}"))
                    .unwrap_or_else(|| "pane —".to_string());
                format!(
                    "tab {} · {} · {} · {} · {} · {}{}{}",
                    p.tab, pane, p.title, p.mode, cwd, sess, tool, note
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    tool_ok(text, json!({ "panes": exposed }))
}

fn pane_events<F>(args: &Value, snap: &Snapshot, tail: &F) -> Value
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
{
    if !snap.config.enabled {
        return tool_err("MCP exposure is disabled.");
    }
    if !snap.config.events {
        return tool_err("Event tailing is off — enable Events in the MCP CONTROL panel.");
    }
    let Some(pid) = args.get("pid").and_then(Value::as_u64) else {
        return tool_err("pane_events requires an integer `pid` (see list_panes).");
    };
    let pid = pid as u32;
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(20)
        .clamp(1, 200) as usize;
    let Some(pane) = snap.panes.iter().find(|p| p.pid == pid && p.exposed) else {
        return tool_err(&format!("no exposed pane with pid {pid}"));
    };
    if !pane.is_agent {
        return tool_err(&format!(
            "pane {pid} is not an agent — no transcript to tail"
        ));
    }
    let events = tail(pane, limit);
    let text = if events.is_empty() {
        format!("no recent tool-call events for pid {pid}")
    } else {
        events
            .iter()
            .map(|e| format!("{} {} — {}", e.ts, e.tool, e.summary))
            .collect::<Vec<_>>()
            .join("\n")
    };
    tool_ok(text, json!({ "pid": pid, "events": events }))
}

/// GET the appearance of the requested targets (or every exposed pane + outer).
/// Read-only and gated only by the master switch — reading a look is as safe as
/// `list_panes`. A bad/hidden pid surfaces as a per-target `error` row rather
/// than failing the whole call, so a batch GET is robust to a recycled pid.
fn get_pane_config(args: &Value, snap: &Snapshot) -> Value {
    if !snap.config.enabled {
        return tool_err(
            "MCP exposure is disabled. Enable it in terminal-delight's MCP \
             CONTROL panel (the robot button on the mother bar).",
        );
    }
    let targets = match args.get("targets") {
        None | Some(Value::Null) => {
            let mut ts: Vec<Target> = snap.exposed().iter().map(|p| Target::Pane(p.pid)).collect();
            ts.push(Target::Outer);
            ts
        }
        Some(Value::Array(a)) => {
            let mut ts = Vec::with_capacity(a.len());
            for v in a {
                match Target::parse(v) {
                    Ok(t) => ts.push(t),
                    Err(e) => return tool_err(&e),
                }
            }
            ts
        }
        Some(_) => {
            return tool_err("`targets` must be an array of pids and/or \"outer\"");
        }
    };
    let configs: Vec<Value> = targets.iter().map(|t| resolve_get(t, snap)).collect();
    let text = configs
        .iter()
        .map(summarise_config)
        .collect::<Vec<_>>()
        .join("\n");
    tool_ok(text, json!({ "configs": configs }))
}

/// Build the GET row for one target: identity + the effective grade percents, or
/// a `{ target, error }` row if the pid is unknown / not exposed.
fn resolve_get(t: &Target, snap: &Snapshot) -> Value {
    match t {
        Target::Outer => json!({
            "target": t.to_json(),
            "scope": "outer",
            "grade": snap.outer_grade,
        }),
        Target::Pane(pid) => match snap.panes.iter().find(|p| p.pid == *pid && p.exposed) {
            Some(p) => json!({
                "target": t.to_json(),
                "scope": "pane",
                "tab": p.tab,
                "title": p.title,
                "mode": p.mode,
                "grade": p.grade,
            }),
            None => json!({
                "target": t.to_json(),
                "error": format!("no exposed pane with pid {pid}"),
            }),
        },
    }
}

/// A one-line human gist of a GET row (the structured `grade` is the real data).
fn summarise_config(c: &Value) -> String {
    let label = c
        .get("target")
        .map(|t| t.to_string().trim_matches('"').to_string())
        .unwrap_or_default();
    if let Some(err) = c.get("error").and_then(Value::as_str) {
        return format!("{label}: {err}");
    }
    let g = &c["grade"];
    let pct = |k: &str| g.get(k).and_then(Value::as_f64).unwrap_or(0.0).round() as i64;
    format!(
        "{label}: brightness {} · contrast {} · text {} · text-size {} · bench-size {} · warp {}{}{}",
        pct("brightness"),
        pct("contrast"),
        pct("text"),
        pct("text_size"),
        pct("bench_size"),
        pct("warp"),
        if g.get("crawl").and_then(Value::as_bool).unwrap_or(false) {
            " · crawl on"
        } else {
            ""
        },
        // Only an explicit `false` is worth a word: a report with no `crt`
        // key came from a build before the switch, where the tube was on.
        if g.get("crt").and_then(Value::as_bool) == Some(false) {
            " · crt off"
        } else {
            ""
        },
    )
}

/// POST a partial grade to one or more targets. Gated by BOTH the master switch
/// and the separate `writable` opt-in (a read→write escalation). The whole batch
/// is parsed and validated *before* any mutation, so a malformed update never
/// leaves a half-applied batch; the actual change is delegated to `apply` (the
/// gpui-thread round-trip), which returns a per-target outcome so one bad pid
/// doesn't sink the rest.
fn set_pane_config<G>(args: &Value, snap: &Snapshot, apply: &G) -> Value
where
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
{
    if !snap.config.enabled {
        return tool_err("MCP exposure is disabled. Enable it in the MCP CONTROL panel.");
    }
    if !snap.config.writable {
        return tool_err(
            "MCP writes are disabled. This server is a read-only watch surface \
             until you opt in: enable \"writes\" in the MCP CONTROL panel (or set \
             TD_MCP_WRITE=1) to let an agent change pane appearance.",
        );
    }
    let Some(updates_v) = args.get("updates").and_then(Value::as_array) else {
        return tool_err("set_pane_config requires an `updates` array of { target, config }.");
    };
    if updates_v.is_empty() {
        return tool_err("`updates` is empty — nothing to set.");
    }
    let mut updates: Vec<ConfigUpdate> = Vec::with_capacity(updates_v.len());
    for (i, u) in updates_v.iter().enumerate() {
        let target = match u.get("target") {
            Some(tv) => match Target::parse(tv) {
                Ok(t) => t,
                Err(e) => return tool_err(&format!("updates[{i}]: {e}")),
            },
            None => return tool_err(&format!("updates[{i}] is missing `target`")),
        };
        let cfg_v = u.get("config").cloned().unwrap_or(Value::Null);
        let patch: ConfigPatch = match serde_json::from_value(cfg_v) {
            Ok(p) => p,
            Err(e) => return tool_err(&format!("updates[{i}].config is invalid: {e}")),
        };
        if patch.is_empty() {
            return tool_err(&format!(
                "updates[{i}].config has no recognised channels to change"
            ));
        }
        updates.push((target, patch));
    }
    let outcomes = apply(&updates);
    let results: Vec<Value> = outcomes
        .iter()
        .map(|(t, r)| match r {
            Ok(g) => json!({ "target": t.to_json(), "ok": true, "grade": g }),
            Err(e) => json!({ "target": t.to_json(), "ok": false, "error": e }),
        })
        .collect();
    let ok = outcomes.iter().filter(|(_, r)| r.is_ok()).count();
    let failed = outcomes.len() - ok;
    let text = if failed == 0 {
        format!("applied {ok} update(s)")
    } else {
        format!("applied {ok} update(s), {failed} failed")
    };
    tool_ok(text, json!({ "results": results }))
}

/// `leave_note` — write on the fridge door.
///
/// The agent-facing half of sticky notes: a headline plus at most ten words,
/// posted onto a pane's glass exactly where a human `alt+s` note goes, so a
/// returning operator reads the wall instead of scrolling twenty transcripts.
/// Validation is [`validate_note`]; the write itself rides the same
/// gate/lookup/apply pipeline as `set_pane_config`.
fn leave_note<G>(args: &Value, snap: &Snapshot, apply: &G) -> Value
where
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
{
    if !snap.config.enabled {
        return tool_err("MCP exposure is disabled. Enable it in the MCP CONTROL panel.");
    }
    if !snap.config.writable {
        return tool_err(
            "MCP writes are disabled. This server is a read-only watch surface \
             until you opt in: enable \"writes\" in the MCP CONTROL panel (or set \
             TD_MCP_WRITE=1) to let an agent leave a note.",
        );
    }
    let pid = match snap.target_pane(args.get("pid").and_then(Value::as_u64).map(|p| p as u32)) {
        Ok(pid) => u64::from(pid),
        Err(why) => return tool_err(&why),
    };
    let title = args.get("title").and_then(Value::as_str);
    let text = args.get("text").and_then(Value::as_str);
    let pin = args.get("pin").and_then(Value::as_bool).unwrap_or(false);
    let clear = args.get("clear").and_then(Value::as_bool).unwrap_or(false);
    let change = match validate_note(title, text, pin, clear) {
        Ok(c) => c,
        Err(e) => return tool_err(&e),
    };
    let cleared = matches!(change, NoteChange::Clear);
    let patch = ConfigPatch {
        note: Some(change),
        ..Default::default()
    };
    let outcomes = apply(&[(Target::Pane(pid as u32), patch)]);
    match outcomes.into_iter().next() {
        Some((_, Ok(_))) => {
            let line = if cleared {
                format!("peeled the note off pane {pid}")
            } else {
                format!("posted — the note is on pane {pid}'s glass")
            };
            tool_ok(line, json!({ "pid": pid, "posted": !cleared }))
        }
        Some((_, Err(e))) => tool_err(&e),
        None => tool_err("the window did not answer the note write"),
    }
}

/// `declare_deliverable` — record what a turn produced, so the rail can offer it
/// as a click instead of the person going to look.
///
/// The same gate, lookup and apply as `leave_note`, deliberately: a declaration
/// is a pane mutation and there is one pipeline for those. What differs is the
/// meaning — a note is a message to a human, and this is a pointer to an
/// artifact — so the two are separate verbs rather than one overloaded one.
fn declare_deliverable<G>(args: &Value, snap: &Snapshot, apply: &G) -> Value
where
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
{
    if !snap.config.enabled {
        return tool_err("MCP exposure is disabled. Enable it in the MCP CONTROL panel.");
    }
    if !snap.config.writable {
        return tool_err(
            "MCP writes are disabled. This server is a read-only watch surface \
             until you opt in: enable \"writes\" in the MCP CONTROL panel (or set \
             TD_MCP_WRITE=1) to let an agent declare a deliverable.",
        );
    }
    let pid = match snap.target_pane(args.get("pid").and_then(Value::as_u64).map(|p| p as u32)) {
        Ok(pid) => u64::from(pid),
        Err(why) => return tool_err(&why),
    };
    let change = match validate_deliverable(
        args.get("label").and_then(Value::as_str),
        args.get("href").and_then(Value::as_str),
        args.get("clear").and_then(Value::as_bool).unwrap_or(false),
    ) {
        Ok(c) => c,
        Err(e) => return tool_err(&e),
    };
    let cleared = matches!(change, DeliverableChange::Clear);
    let said = match &change {
        DeliverableChange::Declare { label, .. } => label.clone(),
        DeliverableChange::Clear => String::new(),
    };
    let patch = ConfigPatch {
        deliverable: Some(change),
        ..Default::default()
    };
    match apply(&[(Target::Pane(pid as u32), patch)])
        .into_iter()
        .next()
    {
        Some((_, Ok(_))) => {
            let line = if cleared {
                format!("withdrawn — pane {pid} declares no deliverable")
            } else {
                format!("declared — {said:?} is on pane {pid}'s row in the rail")
            };
            tool_ok(line, json!({ "pid": pid, "declared": !cleared }))
        }
        Some((_, Err(e))) => tool_err(&e),
        None => tool_err("the window did not answer the deliverable write"),
    }
}

/// `present_surface` — put a work object on a pane's workbench.
///
/// The strict half of the protocol. A file dropped in the watched directory is
/// parsed leniently, because nobody is there to be told what was wrong with
/// it; a verb call has an agent on the other end that can read a sentence and
/// try again, so this one refuses and says why.
fn present_surface<G>(args: &Value, snap: &Snapshot, apply: &G) -> Value
where
    G: Fn(&[ConfigUpdate]) -> Vec<ApplyOutcome>,
{
    if !snap.config.enabled {
        return tool_err("MCP exposure is disabled. Enable it in the MCP CONTROL panel.");
    }
    if !snap.config.writable {
        return tool_err(
            "MCP writes are disabled. This server is a read-only watch surface \
             until you opt in: enable \"writes\" in the MCP CONTROL panel (or set \
             TD_MCP_WRITE=1) to let an agent present a surface.",
        );
    }
    let Some(pid) = args.get("pid").and_then(Value::as_u64) else {
        return tool_err("present_surface requires a `pid` (an integer, from list_panes).");
    };
    // `surface` is where the document belongs, but an agent that sent the
    // envelope at the top level meant the same thing and being pedantic about
    // it would cost a round trip to say so.
    let doc = match args.get("surface") {
        Some(v) if v.is_object() => v.clone(),
        Some(_) => return tool_err("`surface` must be a TDSP document (a JSON object)."),
        None => {
            let mut inline = args.clone();
            if let Some(map) = inline.as_object_mut() {
                map.remove("pid");
            }
            if inline.get("td").is_none() {
                return tool_err(
                    "present_surface requires a `surface` — one TDSP document, \
                     starting {\"td\":\"0.1\",\"kind\":…}. Call surface_catalogue \
                     for the kinds this build renders.",
                );
            }
            inline
        }
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default();
    let mut post = match crate::surface::parse(&doc, now) {
        Ok(p) => p,
        Err(why) => return tool_err(&why),
    };
    // Who is asking. This runs in the relay the agent spawned over stdio, so
    // the parent is the agent — or whatever else opened the pipe. Whether that
    // pid sits under the pane being presented to is the window's to decide,
    // when the surface lands (`TerminalView::present`).
    if let Some(s) = post.surface.as_mut() {
        s.origin = crate::surface::Origin::Mcp {
            pid: std::os::unix::process::parent_id(),
            own: None,
        };
    }
    let said = match (&post.op, post.surface.as_ref()) {
        (crate::surface::Op::Retire, _) => {
            format!("retired — {} is off the bench", post.id.as_str())
        }
        (_, Some(s)) => format!(
            "on the bench — {:?} ({}) is on pane {pid}'s WORKBENCH face",
            s.title,
            s.kind.id()
        ),
        (_, None) => "nothing to draw".to_string(),
    };
    let kind = post
        .surface
        .as_ref()
        .map(|s| s.kind.id().to_string())
        .unwrap_or_else(|| "retired".into());
    let id = post.id.as_str().to_string();
    // The op is echoed back because `present` on an id that already exists is
    // a replacement rather than an addition, and an agent that meant to update
    // should be able to see from the reply which of the two it just did.
    let op = post.op.as_str();

    // PERSIST ON THE WAY PAST, so the bench survives a restart.
    //
    // This module used to hand the surface to the live window and stop there,
    // while `surfacefeed`'s own header drew all three transports converging on
    // `surfaces/<session>/<pane>/*.json` and promised the directory is re-read
    // when a window opens. The file drop kept that promise; the verb did not,
    // so everything an agent sent through MCP died with the window — a pane
    // that had presented four surfaces had no directory at all. Filed as #567.
    //
    // Written before the apply rather than after: the window is the thing that
    // can fail, and a surface a person can see but that is not on disk is the
    // failure being fixed here. `drop_surface` names the file by surface id and
    // writes-then-renames, so a re-send updates the same row instead of
    // stacking, and the watcher re-reading its own file is a no-op the origin
    // rule in `Workbench::apply` already absorbs.
    //
    // `pane_id: None` is a window-owned pane with no host — it has no
    // directory, and inventing one would write into a path nothing watches.
    let pane_dir = snap
        .panes
        .iter()
        .find(|p| p.pid == pid as u32)
        .and_then(|p| p.pane_id)
        .zip(snap.instance.as_ref())
        .map(|(pane, inst)| crate::surfacefeed::pane_dir(&inst.session, pane));
    if let Some(dir) = pane_dir.as_ref() {
        match post.op {
            crate::surface::Op::Retire => {
                crate::surfacefeed::retire_surface(dir, post.id.as_str());
            }
            // A failure here is not worth refusing the call over: the surface
            // still reaches the bench, and the person sees it. It costs the
            // restart, which is what the log line is for.
            _ => {
                if let Err(err) = crate::surfacefeed::drop_surface(dir, post.id.as_str(), &doc) {
                    eprintln!(
                        "terminal-delight mcp: presented {:?} but could not persist it into {}: {err}",
                        post.id.as_str(),
                        dir.display()
                    );
                }
            }
        }
    }

    let patch = ConfigPatch {
        surface: Some(post),
        ..Default::default()
    };
    match apply(&[(Target::Pane(pid as u32), patch)])
        .into_iter()
        .next()
    {
        Some((_, Ok(_))) => tool_ok(
            said,
            json!({ "pid": pid, "surface": id, "kind": kind, "op": op }),
        ),
        Some((_, Err(e))) => tool_err(&e),
        None => tool_err("the window did not answer the surface write"),
    }
}

/// `surface_catalogue` — what this build can draw, said out loud.
///
/// A2UI's host-advertised catalogue. An agent that asks is told, rather than
/// guessing and having its work quietly downgraded to unclassified.
fn surface_catalogue() -> Value {
    let cat = crate::surface::catalogue();
    tool_ok(
        format!(
            "This build renders: {}. Unknown kinds are shown as unclassified, never dropped.",
            crate::surface::catalogue_names().join(", ")
        ),
        cat,
    )
}

/// `document_notes` — the notes on the document beside the agent that asks.
///
/// The other half of a brief: an agent writes one, a person reads it beside
/// the agent and leaves notes on it, and until this the agent's way back was
/// "read my notes in <file>" — the whole page re-read to find the eighty words
/// that were new. This hands over the map alone, the text the brief's own copy
/// map gives, taken from the same notes layer `ctl doc notes` reads.
///
/// Scoped to the caller with no way to name anything else: no `pid`, no path.
/// Which document counts as beside the caller is [`document_beside`]. It reads
/// and writes nothing — the snapshot already holds the report, built on the
/// main thread with the rest of it.
fn document_notes(args: &Value, snap: &Snapshot) -> Value {
    if !snap.config.enabled {
        return tool_err(
            "MCP exposure is disabled. Enable it in terminal-delight's MCP \
             CONTROL panel (the robot button on the mother bar).",
        );
    }
    if args.as_object().is_some_and(|a| !a.is_empty()) {
        return tool_err(
            "document_notes takes no arguments: it answers for the pane you call \
             it from, and only that one.",
        );
    }
    let me = match snap.caller_pane("document_notes") {
        Ok(p) => p,
        Err(why) => return tool_err(&why),
    };
    if !me.exposed {
        return tool_err(&format!(
            "the pane you are calling from ({}) is not exposed under the current \
             policy, so nothing about it is answered",
            me.pid
        ));
    }
    let Some((doc, how)) = document_beside(&snap.documents, me) else {
        return tool_ok(
            "nothing is open beside you — no brief floats over your pane, and no \
             pane in your tab shows a document"
                .to_string(),
            json!({
                "path": null, "place": null, "pane": null, "found": null,
                "notes": null, "concurs": null, "unsaved": null, "map": null,
            }),
        );
    };
    let (found, whereabouts) = match how {
        Beside::FloatOverYou => ("float-over-you", "floating over your pane".to_string()),
        Beside::YourSplit => (
            "your-split",
            "in the pane you opened it in, beside you".to_string(),
        ),
        Beside::InYourTab { of: 1 } => ("in-your-tab", "in a pane in your tab".to_string()),
        Beside::InYourTab { of } => (
            "in-your-tab",
            format!("in a pane in your tab — the first of {of} documents open there"),
        ),
    };
    let report = doc.notes.as_ref().ok();
    let field = |k: &str| {
        report
            .and_then(|r| r.get(k))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let mut structured = json!({
        "path": doc.path,
        "place": doc.place,
        "pane": doc.pane,
        "found": found,
        "notes": field("notes"),
        "concurs": field("concurs"),
        "unsaved": field("unsaved"),
        "map": field("map"),
        "state": field("state"),
        "read_only": field("read_only"),
    });
    let head = format!("{} — {whereabouts}", doc.path);
    let text = match (&doc.notes, report.and_then(|r| r["map"].as_str())) {
        (Err(why), _) => {
            structured["why"] = json!(why);
            format!("{head}\nno notes to read: {why}")
        }
        (Ok(r), Some(map)) => {
            let n = r["notes"].as_u64();
            let mut counts = n.map_or_else(
                || "notes uncounted".to_string(),
                |n| format!("{n} {}", if n == 1 { "note" } else { "notes" }),
            );
            if let Some(c) = r["concurs"].as_u64() {
                counts.push_str(&format!(
                    " · {c} {}",
                    if c == 1 { "concur" } else { "concurs" }
                ));
            }
            if let Some(u) = r["unsaved"].as_u64().filter(|u| *u > 0) {
                // A Markdown file's notes are kept by TD, never in the file.
                let into = match r["kept"].as_str() {
                    Some("store") => "not kept yet",
                    _ => "not saved into the file yet",
                };
                counts.push_str(&format!(
                    " · {u} {} {into}, included below",
                    if u == 1 { "edit" } else { "edits" }
                ));
            }
            format!("{head} · {counts}\n\n{map}")
        }
        (Ok(r), None) => {
            let why = r["read_only"].as_str().unwrap_or("it shows no notes");
            format!("{head}\nno notes to read: {why}")
        }
    };
    tool_ok(text, structured)
}

/// `open_document` — an agent opens what it made beside itself.
///
/// The other half of a deliverable. `declare_deliverable` puts a click on the
/// rail; this puts the document on screen beside the agent, so the person who
/// asked for a brief is reading it the moment it exists. It changes the layout
/// and writes nothing to any terminal: a split's new pane gets a fresh shell
/// with no command line, as Ctrl+Alt+click's does.
///
/// Scoped to the caller with no way out: there is no pane argument, the pid in
/// the request is filled in from who called, and the router it goes through
/// only ever splits the tab the pane that asked is in. A `pid`, or any key but
/// the two the schema names, is refused rather than ignored, so an agent that
/// meant to open something in another pane is told that it cannot.
fn open_document<O>(args: &Value, snap: &Snapshot, open: &O) -> Value
where
    O: Fn(&OpenRequest) -> OpenOutcome,
{
    if !snap.config.enabled {
        return tool_err("MCP exposure is disabled. Enable it in the MCP CONTROL panel.");
    }
    if !snap.config.writable {
        return tool_err(
            "MCP writes are disabled. This server is a read-only watch surface \
             until you opt in: enable \"writes\" in the MCP CONTROL panel (or set \
             TD_MCP_WRITE=1) to let an agent open a document.",
        );
    }
    if let Some(stray) = args.as_object().and_then(|a| {
        a.keys()
            .find(|k| !matches!(k.as_str(), "path" | "placement"))
    }) {
        return tool_err(&format!(
            "open_document takes `path` and `placement` only, not `{stray}`: it \
             opens in your own pane or tab, never in one you name"
        ));
    }
    let me = match snap.caller_pane("open_document") {
        Ok(p) => p,
        Err(why) => return tool_err(&why),
    };
    if !me.exposed {
        return tool_err(&format!(
            "the pane you are calling from ({}) is not exposed under the current \
             policy, so nothing opens in it",
            me.pid
        ));
    }
    let placement = match args.get("placement").and_then(Value::as_str) {
        None | Some("beside") => Placement::Beside,
        Some("here") => Placement::Here,
        Some(other) => {
            return tool_err(&format!(
                "placement {other:?} is not one of \"beside\" or \"here\""
            ))
        }
    };
    let path = args.get("path").and_then(Value::as_str).unwrap_or("");
    let target = match validate_open_path(path) {
        Ok(t) => t,
        Err(why) => return tool_err(&why),
    };
    let shown = target.path.display().to_string();
    let request = OpenRequest {
        pid: me.pid,
        target,
        placement,
    };
    match open(&request) {
        Ok(said) => tool_ok(
            format!("{shown} — {said}"),
            json!({
                "path": shown,
                "placement": placement.as_str(),
                "pane": me.pid,
                "said": said,
            }),
        ),
        Err(why) => tool_err(&why),
    }
}

fn tool_ok(text: String, structured: Value) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": structured })
}

fn tool_err(text: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": true })
}

fn encode_ok(id: Value, result: Value) -> String {
    serde_json::to_string(&json!({ "jsonrpc": "2.0", "id": id, "result": result }))
        .unwrap_or_default()
}

fn encode_err(id: Value, code: i64, msg: String) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg }
    }))
    .unwrap_or_default()
}

// ---- transport helpers (let the stdio layer route without re-parsing logic) ----

/// Does this request need a live pane snapshot? Only `tools/call` reads panes;
/// `initialize` / `tools/list` / `ping` / `logging/setLevel` etc. are answered
/// from static data, so the transport can reply instantly without a main-thread
/// round-trip (and the handshake never blocks on the GUI being ready).
pub fn requires_snapshot(line: &str) -> bool {
    parse_req(line)
        .map(|r| r.method == "tools/call")
        .unwrap_or(false)
}

/// True for the `initialize` request — the transport flips the push feed live
/// once the client has handshaken.
pub fn is_initialize(line: &str) -> bool {
    parse_req(line)
        .map(|r| r.method == "initialize")
        .unwrap_or(false)
}

/// If this is `logging/setLevel`, the requested level's severity (see
/// [`log_severity`]); otherwise `None`. Lets the transport gate the push feed.
pub fn log_level_from(line: &str) -> Option<u8> {
    let r = parse_req(line)?;
    if r.method != "logging/setLevel" {
        return None;
    }
    r.params
        .get("level")
        .and_then(Value::as_str)
        .map(log_severity)
}

/// MCP syslog-style level → severity rank (higher = more severe). We push at
/// `info`, so a client that raises the level to `warning`+ silences the feed.
pub fn log_severity(name: &str) -> u8 {
    match name {
        "debug" => 0,
        "info" => 1,
        "notice" => 2,
        "warning" => 3,
        "error" => 4,
        "critical" => 5,
        "alert" => 6,
        "emergency" => 7,
        _ => 1,
    }
}

/// Build a JSON-RPC error response for a request line (used when the UI can't
/// produce a snapshot in time). `None` for a notification — those get no reply.
pub fn error_response(line: &str, code: i64, msg: &str) -> Option<String> {
    let id = parse_req(line)?.id?;
    Some(encode_err(id, code, msg.to_string()))
}

// ===========================================================================
// Push feed — turn the pull tools into a live stream.
//
// An orchestrator's real need is "tell me the moment an agent acts", not "let
// me poll". The [`Watcher`] is the pure brain of that: fed a fresh exposed
// snapshot plus freshly-tailed events per agent pane, it diffs against what it
// last saw and returns the notifications to push. No IO, no gpui — so every
// rule (don't flood history on first sight, fire on new tool calls, announce
// appear/vanish) is unit-tested. The transport just does the tailing IO and
// ships whatever this returns as `notifications/message`.
// ===========================================================================

/// One thing worth telling the client about, at a syslog level.
#[derive(Clone, PartialEq, Debug)]
pub struct Notification {
    pub level: &'static str,
    pub data: Value,
}

/// Stateful change-detector across snapshots. Cheap: a last-seen event
/// signature + the set of known agent pids.
#[derive(Default)]
pub struct Watcher {
    /// Signature of the newest tool event already emitted, per pane pid.
    seen: HashMap<u32, String>,
    /// Exposed agent pids known as of the last diff (for appear/vanish).
    known: HashSet<u32>,
}

/// Identity of a tool event for "have I already pushed this?" — ts+tool+summary
/// is stable across re-tails of the same transcript.
fn event_sig(e: &ToolEvent) -> String {
    format!("{}|{}|{}", e.ts, e.tool, e.summary)
}

impl Watcher {
    /// Diff a fresh exposed snapshot + freshly-tailed events (pid → recent
    /// events, oldest-first) into the notifications to push. Mutates the
    /// last-seen state. Rules:
    ///   • agent appears (new exposed agent pid) → `agent_appeared`
    ///   • agent vanishes (known pid gone) → `agent_vanished`
    ///   • on FIRST sight of a pane we record its latest event but emit NOTHING
    ///     for it — we never replay the backlog when an orchestrator connects
    ///     mid-conversation; only events that happen *after* we start watching.
    ///   • thereafter, every event newer than the last we emitted → `tool_call`.
    pub fn diff(
        &mut self,
        panes: &[PaneInfo],
        tailed: &HashMap<u32, Vec<ToolEvent>>,
    ) -> Vec<Notification> {
        let agents: Vec<&PaneInfo> = panes.iter().filter(|p| p.exposed && p.is_agent).collect();
        let now: HashSet<u32> = agents.iter().map(|p| p.pid).collect();
        let mut out = vec![];

        for p in &agents {
            if !self.known.contains(&p.pid) {
                out.push(Notification {
                    level: "info",
                    data: json!({
                        "event": "agent_appeared", "pid": p.pid, "title": p.title,
                        "mode": p.mode, "cwd": p.cwd, "session": p.session,
                    }),
                });
            }
        }
        for pid in &self.known {
            if !now.contains(pid) {
                out.push(Notification {
                    level: "info",
                    data: json!({ "event": "agent_vanished", "pid": pid }),
                });
            }
        }

        for p in &agents {
            let Some(events) = tailed.get(&p.pid) else {
                continue;
            };
            if events.is_empty() {
                continue;
            }
            let fresh: Vec<&ToolEvent> = match self.seen.get(&p.pid) {
                // First time we look at this pane: emit nothing, just bookmark.
                None => vec![],
                Some(sig) => match events.iter().rposition(|e| &event_sig(e) == sig) {
                    // Everything after the bookmark is new.
                    Some(i) => events[i + 1..].iter().collect(),
                    // Bookmark fell out of the tail window (rotation/burst):
                    // emit only the newest so we don't replay a whole file.
                    None => events.last().into_iter().collect(),
                },
            };
            for e in fresh {
                out.push(Notification {
                    level: "info",
                    data: json!({
                        "event": "tool_call", "pid": p.pid, "title": p.title,
                        "tool": e.tool, "summary": e.summary, "ts": e.ts,
                    }),
                });
            }
            if let Some(latest) = events.last() {
                self.seen.insert(p.pid, event_sig(latest));
            }
        }

        // Drop bookmarks for panes that are gone, so a recycled pid starts fresh.
        self.seen.retain(|pid, _| now.contains(pid));
        self.known = now;
        out
    }
}

/// Encode a [`Notification`] as an MCP `notifications/message` line.
pub fn encode_notification(n: &Notification) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "method": "notifications/message",
        "params": { "level": n.level, "logger": SERVER_NAME, "data": n.data },
    }))
    .unwrap_or_default()
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_locked_down() {
        let c = McpConfig::default();
        assert!(!c.enabled, "MCP must ship disabled");
        assert_eq!(c.expose, Expose::AgentsOnly, "default policy = safest");
        assert!(c.events);
    }

    #[test]
    fn disabled_exposes_nothing() {
        let c = McpConfig::default(); // enabled = false
        assert!(!should_expose(&c, true));
        assert!(!should_expose(&c, false));
    }

    #[test]
    fn agents_only_excludes_shells() {
        let c = McpConfig {
            enabled: true,
            expose: Expose::AgentsOnly,
            events: true,
            writable: false,
        };
        assert!(should_expose(&c, true), "agent pane exposed");
        assert!(!should_expose(&c, false), "plain shell NOT exposed");
    }

    #[test]
    fn all_includes_shells_when_enabled() {
        let c = McpConfig {
            enabled: true,
            expose: Expose::All,
            events: false,
            writable: false,
        };
        assert!(should_expose(&c, true));
        assert!(should_expose(&c, false), "All exposes shells too");
    }

    #[test]
    fn config_survives_a_toml_round_trip() {
        let c = McpConfig {
            enabled: true,
            expose: Expose::All,
            events: false,
            writable: true,
        };
        let body = toml::to_string(&c).unwrap();
        // kebab-case on the enum: "all", not "All".
        assert!(body.contains("expose = \"all\""), "got: {body}");
        assert!(body.contains("writable = true"), "got: {body}");
        let back: McpConfig = toml::from_str(&body).unwrap();
        assert_eq!(c, back);
        // an older state.toml without `writable` still loads (serde default off).
        let legacy: McpConfig =
            toml::from_str("enabled = true\nevents = true\nexpose = \"all\"\n").unwrap();
        assert!(!legacy.writable, "missing writable defaults to read-only");
    }

    #[test]
    fn expose_toggle_cycles() {
        assert_eq!(Expose::AgentsOnly.next(), Expose::All);
        assert_eq!(Expose::All.next(), Expose::AgentsOnly);
    }

    // ---- JSON-RPC protocol ----

    fn agent_pane(pid: u32, exposed: bool) -> PaneInfo {
        PaneInfo {
            tab: 0,
            title: "work".into(),
            mode: "CLAUDE".into(),
            is_agent: true,
            pid,
            pane_id: Some(u64::from(pid % 97)),
            cwd: Some("/work/x".into()),
            session: Some("claude --resume abc".into()),
            tool: None,
            note: None,
            exposed,
            grade: GradeReport::default(),
        }
    }

    /// A landing list an agent can act on rides the TEXT, numbered, with the
    /// active project first; the structured half carries the kind a program
    /// switches on. And nothing is answered while exposure is off.
    #[test]
    fn engineering_state_hands_an_agent_the_landing_list() {
        use crate::engstate::{CheckoutReport, LandingReport, Report};
        let report = |name: &str, active: bool| Report {
            project: Some(1),
            name: Some(name.into()),
            active,
            read_secs_ago: 3,
            took_ms: 900,
            main: Some("main".into()),
            badge: vec!["2 WORKTREES".into(), "1 SHARED".into()],
            sentence: Some("2 lines of work, one shared".into()),
            repos: vec![],
            checkouts: vec![CheckoutReport {
                root: "/w/feature".into(),
                repo: "o/r".into(),
                line: "feature".into(),
                branch: Some("feature".into()),
                head: Some("abc1234".into()),
                idle: false,
                shared: false,
                writers: vec![],
                dirty: Some(0),
                untracked: None,
                added: None,
                removed: None,
                ahead: Some(2),
                behind: Some(1),
                last_commit: None,
                upstream: Some("origin/feature".into()),
                unpushed: Some(1),
                touched: None,
                merge: Some("conflicts".into()),
                conflict_files: vec!["a.txt".into()],
            }],
            idle: vec![],
            no_git: vec![],
            foreign: vec![],
            visitors: vec![],
            collisions: vec![],
            landing: vec![
                LandingReport {
                    line: "feature".into(),
                    idle: false,
                    kind: "push".into(),
                    text: "push 1 commit on feature".into(),
                },
                LandingReport {
                    line: "feature".into(),
                    idle: false,
                    kind: "conflict".into(),
                    text: "merge feature into main (2 commits) — would conflict on a.txt".into(),
                },
            ],
            afterglow: vec!["a commit landed on feature".into()],
        };
        let mut s = snap(true, false, vec![]);
        s.engineering = vec![report("BFS", false), report("TERMINAL DELIGHT", true)];
        let out = engineering_state(&s);
        let text = out["content"][0]["text"].as_str().unwrap();
        assert!(
            text.starts_with("TERMINAL DELIGHT (active)  [2 WORKTREES \u{00b7} 1 SHARED]"),
            "the active project leads: {text}"
        );
        assert!(text.contains(
            "to land everything:\n    1. push 1 commit on feature\n    2. merge feature"
        ));
        assert!(text.contains("wt feature  dirty=0 ahead=2 behind=1 unpushed=1 merge=conflicts"));
        assert!(text.contains("\u{25c6} a commit landed on feature"));
        let kinds: Vec<&str> = out["structuredContent"]["projects"][1]["landing"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, vec!["push", "conflict"]);
        assert_eq!(
            out["structuredContent"]["projects"][1]["checkouts"][0]["untracked"],
            Value::Null,
            "unmeasured stays null on the wire"
        );

        let empty = engineering_state(&snap(true, false, vec![]));
        assert!(empty["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no scan has landed"));
        let off = engineering_state(&snap(false, false, vec![]));
        assert_eq!(
            off["isError"], true,
            "exposure off is a refusal, not an empty list"
        );
    }

    fn snap(enabled: bool, events: bool, panes: Vec<PaneInfo>) -> Snapshot {
        Snapshot {
            config: McpConfig {
                enabled,
                expose: Expose::AgentsOnly,
                events,
                writable: false,
            },
            panes,
            outer_grade: GradeReport::default(),
            instance: Some(Instance {
                session: "tdclip".into(),
                window: 4242,
                build: Some("td-abc1234-under-test".into()),
            }),
            caller: None,
            engineering: vec![],
            documents: vec![],
        }
    }

    /// A snapshot like [`snap`] but with the write opt-in flipped on, and an
    /// `outer` grade set so reads have something non-trivial to report.
    fn snap_writable(panes: Vec<PaneInfo>) -> Snapshot {
        let mut s = snap(true, true, panes);
        s.config.writable = true;
        s.outer_grade = GradeReport {
            brightness: 40.0,
            text_size: 50.0,
            bench_size: 50.0,
            ..GradeReport::default()
        };
        s
    }

    /// A relative target is refused, and the message says WHY rather than just
    /// no — the agent can act on "give me an absolute path" and cannot act on
    /// "invalid".
    ///
    /// This is the one that would otherwise ship working and be wrong: a
    /// relative href resolves against whatever directory the OPENER is in,
    /// which is this window's, so the click would open a different file or
    /// nothing at all while the row confidently claimed otherwise.
    #[test]
    fn a_relative_target_is_refused_with_the_reason() {
        let err = validate_deliverable(Some("report"), Some("report.html"), false)
            .expect_err("relative must not be accepted");
        assert!(err.contains("relative"), "{err}");
        assert!(
            err.contains("working directory"),
            "say what would go wrong: {err}"
        );
        // …and each absolute shape is accepted.
        for ok in [
            "/home/parker/report.html",
            "file:///home/parker/report.html",
            "https://github.com/x/y/pull/1",
        ] {
            assert!(
                validate_deliverable(Some("r"), Some(ok), false).is_ok(),
                "{ok} should be accepted"
            );
        }
    }

    /// Things that are not documents are refused. A `javascript:` URL is not an
    /// artifact and `data:` is a payload pretending to be one; neither is
    /// something a person opens and reads.
    #[test]
    fn a_target_that_is_not_a_document_is_refused() {
        for bad in ["javascript:alert(1)", "data:text/html,<b>x"] {
            assert!(
                validate_deliverable(Some("x"), Some(bad), false).is_err(),
                "{bad} should be refused"
            );
        }
    }

    /// A label is optional; a target is not. A row with a name and nothing to
    /// open lies about what a click does.
    #[test]
    fn a_label_without_a_target_is_refused_and_a_target_alone_is_named() {
        assert!(validate_deliverable(Some("just a name"), None, false).is_err());
        let d = validate_deliverable(None, Some("/home/parker/2026-09-16-ledger.html"), false)
            .expect("a bare target is fine");
        assert_eq!(
            d,
            DeliverableChange::Declare {
                label: "2026-09-16-ledger.html".into(),
                href: "/home/parker/2026-09-16-ledger.html".into(),
            },
            "named by its own last segment rather than left blank"
        );
    }

    /// Nothing at all, or an explicit clear, withdraws the declaration.
    #[test]
    fn nothing_said_is_a_withdrawal_rather_than_an_error() {
        assert_eq!(
            validate_deliverable(None, None, false).unwrap(),
            DeliverableChange::Clear
        );
        assert_eq!(
            validate_deliverable(Some("r"), Some("/tmp/r.html"), true).unwrap(),
            DeliverableChange::Clear,
            "clear wins over a declaration in the same call"
        );
    }

    /// The label is capped, because it shares a 300-pixel row with the reason.
    #[test]
    fn an_essay_is_not_a_label() {
        let long = "a".repeat(DELIVERABLE_LABEL_MAX_CHARS + 1);
        assert!(validate_deliverable(Some(&long), Some("/tmp/x.html"), false).is_err());
    }

    /// The tool is refused while writes are off, in the same words as every
    /// other write — one gate, not a second one somebody has to find.
    #[test]
    fn declaring_needs_the_writes_toggle() {
        let readonly = snap(true, false, vec![agent_pane(1, true)]);
        let refuse = |_: &[ConfigUpdate]| -> Vec<ApplyOutcome> {
            panic!("must not reach the window with writes off")
        };
        let out = declare_deliverable(
            &json!({ "pid": 1, "label": "r", "href": "/tmp/r.html" }),
            &readonly,
            &refuse,
        );
        assert_eq!(out.get("isError").and_then(Value::as_bool), Some(true));
        assert!(
            out["content"][0]["text"]
                .as_str()
                .unwrap_or_default()
                .contains("TD_MCP_WRITE"),
            "name the toggle, so the reader can turn it on"
        );
    }

    /// It is listed, so an agent can find it without being told it exists.
    #[test]
    fn the_verb_is_advertised_in_the_tool_list() {
        let defs = tool_defs();
        let names: Vec<&str> = defs
            .as_array()
            .expect("tools")
            .iter()
            .filter_map(|t| t.get("name").and_then(Value::as_str))
            .collect();
        assert!(names.contains(&"declare_deliverable"), "{names:?}");
    }

    fn no_tail(_: &PaneInfo, _: usize) -> Vec<ToolEvent> {
        vec![]
    }

    /// Parse a response line back to a Value for assertions.
    fn resp(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
    }

    #[test]
    fn grep_returns_matches_and_gates_on_enabled() {
        let search = |q: &str, _cap: usize| -> Vec<PaneMatches> {
            vec![PaneMatches {
                pid: 42,
                tab: 0,
                title: "work".into(),
                mode: "CLAUDE".into(),
                matches: vec![
                    GrepMatch {
                        line: -3,
                        col: 4,
                        text: format!("found {q} here"),
                    },
                    GrepMatch {
                        line: 0,
                        col: 0,
                        text: format!("{q} again"),
                    },
                ],
            }]
        };
        let line = json!({ "id": 1, "method": "tools/call",
            "params": { "name": "grep", "arguments": { "query": "boom" } } })
        .to_string();
        let on = snap(true, false, vec![agent_pane(42, true)]);
        let r = resp(&handle_line_with(&line, &on, no_tail, no_apply, search).unwrap())["result"]
            .clone();
        assert!(r.get("isError").is_none(), "enabled grep is not an error");
        let panes = r["structuredContent"]["panes"].as_array().unwrap();
        assert_eq!(panes[0]["pid"], 42);
        assert_eq!(panes[0]["matches"].as_array().unwrap().len(), 2);
        assert_eq!(panes[0]["matches"][0]["col"], 4);

        // exposure disabled → error result (search closure never consulted).
        let off = snap(false, false, vec![agent_pane(42, true)]);
        let r = resp(&handle_line_with(&line, &off, no_tail, no_apply, search).unwrap())["result"]
            .clone();
        assert_eq!(r["isError"], true);
    }

    #[test]
    fn initialize_echoes_version_and_names_the_server() {
        let s = snap(false, true, vec![]);
        let line = handle_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
            &s,
            no_tail,
        )
        .unwrap();
        let v = resp(&line);
        assert_eq!(
            v["result"]["protocolVersion"], "2024-11-05",
            "echoes client's"
        );
        assert_eq!(v["result"]["serverInfo"]["name"], SERVER_NAME);
        assert!(v["result"]["capabilities"]["tools"].is_object());
    }

    #[test]
    fn notifications_get_no_reply() {
        let s = snap(true, true, vec![]);
        assert!(
            handle_line(
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                &s,
                no_tail
            )
            .is_none(),
            "a no-id notification must never be answered"
        );
    }

    #[test]
    fn garbage_line_is_silently_dropped() {
        let s = snap(true, true, vec![]);
        assert!(handle_line("not json at all", &s, no_tail).is_none());
    }

    #[test]
    fn tools_list_advertises_all_tools() {
        let s = snap(true, true, vec![]);
        let v = resp(&handle_line(r#"{"id":2,"method":"tools/list"}"#, &s, no_tail).unwrap());
        let names: Vec<&str> = v["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"list_panes"));
        assert!(names.contains(&"pane_events"));
        assert!(names.contains(&"get_pane_config"), "config GET advertised");
        assert!(names.contains(&"set_pane_config"), "config SET advertised");
        assert!(names.contains(&"grep"), "grep advertised");
        assert!(
            names.contains(&"document_notes"),
            "document_notes advertised"
        );
        assert!(names.contains(&"open_document"), "open_document advertised");
    }

    /// The `present_surface` blurb is the ONLY text about this protocol that
    /// reaches an agent without it asking for anything — `surface_catalogue`
    /// is a verb somebody has to call, and the machine-global AGENTS.md is
    /// read once at session start and so misses every agent already running.
    /// It shipped naming `"td":"0.1"` and six kinds, none of them the one an
    /// agent is supposed to send every turn, which is how a protocol version
    /// and the sentence describing it drift apart.
    #[test]
    fn the_present_surface_blurb_names_the_current_version_and_the_response_kind() {
        let s = snap(true, true, vec![]);
        let v = resp(&handle_line(r#"{"id":2,"method":"tools/list"}"#, &s, no_tail).unwrap());
        let blurb = v["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "present_surface")
            .expect("present_surface advertised")["description"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            blurb.contains(&format!("\"td\":\"{}\"", crate::surface::TDSP_VERSION)),
            "the example names a version this build does not speak: {blurb}"
        );
        for word in ["response", "brief", "fifty words", "layman", "doubts"] {
            assert!(
                blurb.contains(word),
                "the blurb never says {word:?}: {blurb}"
            );
        }
        // The same gate the launch briefing carries: an agent must not be asked
        // over MCP for a register the window retired. The two texts drifted
        // apart once already — the briefing is in `surface`, the blurb is here,
        // and nothing but a test joins them.
        for dead in ["tldr", "tl;dr", "eli5", "ELI5"] {
            assert!(
                !blurb.contains(dead),
                "the blurb still asks for {dead:?}: {blurb}"
            );
        }
    }

    // ---- config API: GET ----

    /// Call a tool through the read-only path (no write capability wired).
    fn call(snap: &Snapshot, name: &str, args: Value) -> Value {
        let line = json!({ "id": 9, "method": "tools/call",
            "params": { "name": name, "arguments": args } })
        .to_string();
        resp(&handle_line(&line, snap, no_tail).unwrap())["result"].clone()
    }

    /// A tool call with a write capability that succeeds on any pane, so a test
    /// about WHICH pane was chosen is not also a test of the apply pipeline.
    fn call_writing(snap: &Snapshot, name: &str, args: Value) -> Value {
        let apply = |ups: &[ConfigUpdate]| -> Vec<ApplyOutcome> {
            ups.iter()
                .map(|(t, _)| (t.clone(), Ok(GradeReport::default())))
                .collect()
        };
        let line = json!({ "id": 9, "method": "tools/call",
            "params": { "name": name, "arguments": args } })
        .to_string();
        resp(&handle_line_with(&line, snap, no_tail, apply, no_search).unwrap())["result"].clone()
    }

    /// Every block of text in a result, joined — the stamp is its own block, so
    /// an assertion that only read `content[0]` would pass whatever we did.
    fn text_of(result: &Value) -> String {
        result["content"]
            .as_array()
            .expect("a result always carries content")
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n")
    }

    // ---- which instance answered ----

    /// EVERY tool names the window that answered it, reads and writes alike.
    ///
    /// The whole defect this exists for is that a reply from the wrong instance
    /// was byte-identical in shape to a reply from the right one. The list is
    /// spelled out rather than derived from `tool_defs`, so adding a tool
    /// without a stamp fails here instead of quietly inheriting a pass.
    #[test]
    fn every_tool_result_names_the_instance_that_answered() {
        let snap = snap_writable(vec![agent_pane(100, true)]);
        for (name, args) in [
            ("list_panes", json!({})),
            ("pane_events", json!({ "pid": 100 })),
            ("get_pane_config", json!({})),
            ("set_pane_config", json!({ "pid": 100, "brightness": 40 })),
            ("leave_note", json!({ "pid": 100, "text": "hi" })),
            (
                "declare_deliverable",
                json!({ "pid": 100, "href": "/tmp/x.html" }),
            ),
            ("grep", json!({ "query": "x" })),
            ("document_notes", json!({})),
            ("open_document", json!({ "path": "/nowhere.md" })),
        ] {
            let out = call(&snap, name, args);
            let text = text_of(&out);
            assert!(
                text.contains("session tdclip") && text.contains("window 4242"),
                "{name} answered without naming its instance: {text}"
            );
            assert_eq!(
                out["structuredContent"]["instance"]["session"], "tdclip",
                "{name} left the structured half unstamped"
            );
            assert_eq!(
                out["structuredContent"]["instance"]["build"], "td-abc1234-under-test",
                "{name} named no build, and two windows on one box are routinely different ones"
            );
        }
    }

    /// A refusal is stamped too. "MCP exposure is disabled" from the window you
    /// did not mean to reach is the answer most likely to be acted on blind.
    #[test]
    fn a_refusal_names_the_window_that_refused() {
        let out = call(
            &snap(false, true, vec![agent_pane(100, true)]),
            "list_panes",
            json!({}),
        );
        assert_eq!(out["isError"], true, "a disabled server must refuse");
        assert!(
            text_of(&out).contains("window 4242"),
            "the refusal did not say which window refused: {}",
            text_of(&out)
        );
    }

    /// A window that cannot name itself says so, out loud, in the same slot.
    /// Silence there would be read as agreement with whatever the caller assumed.
    #[test]
    fn a_window_that_cannot_name_itself_says_so_rather_than_going_quiet() {
        let mut s = snap(true, true, vec![agent_pane(100, true)]);
        s.instance = None;
        let out = call(&s, "list_panes", json!({}));
        let text = text_of(&out);
        assert!(
            text.contains("instance unknown"),
            "an unnamed instance went quiet instead of saying so: {text}"
        );
        assert!(
            out["structuredContent"]["instance"].is_null(),
            "unknown must reach the wire as null, never as a blank name"
        );
    }

    // ---- which pane is the caller ----

    /// The listing carries the host's pane id, which is the number the caller
    /// already holds in `$TD_PANE_ID` — the join that lets an agent find itself
    /// without matching on a title two panes share.
    #[test]
    fn list_panes_carries_the_pane_id_an_agent_can_match_on() {
        let mut p = agent_pane(100, true);
        p.pane_id = Some(9);
        let out = call(&snap(true, true, vec![p]), "list_panes", json!({}));
        assert!(
            text_of(&out).contains("pane 9"),
            "the pane id is missing from the line a model reads: {}",
            text_of(&out)
        );
        assert_eq!(out["structuredContent"]["panes"][0]["pane_id"], 9);
    }

    /// A caller that names itself does not have to name its own pane: the
    /// declaration lands on the pane it called from.
    ///
    /// This is the defect the whole caller-identity path exists for. The old
    /// rule sent the agent to `list_panes` to find its own pid, and the only
    /// keys it could match on were the title and the directory — which two
    /// agents working on one project share, so the deliverable went to
    /// whichever of them the agent picked.
    #[test]
    fn a_deliverable_with_no_pid_lands_on_the_pane_that_declared_it() {
        let mut mine = agent_pane(100, true);
        mine.pane_id = Some(9);
        let mut sibling = agent_pane(200, true);
        sibling.pane_id = Some(4);
        // Indistinguishable by every key the agent could have matched on.
        assert_eq!(mine.title, sibling.title);
        assert_eq!(mine.cwd, sibling.cwd);

        let mut s = snap_writable(vec![sibling, mine]);
        s.caller = Some(Caller {
            session: "tdclip".into(),
            pane: Some(9),
        });
        let out = call_writing(&s, "declare_deliverable", json!({ "href": "/tmp/r.html" }));
        assert_ne!(out["isError"], true, "{}", text_of(&out));
        assert_eq!(
            out["structuredContent"]["pid"],
            100,
            "the deliverable landed on the wrong pane: {}",
            text_of(&out)
        );
    }

    /// The same default for a note, and it reaches the pane through the one
    /// pipeline every pane mutation uses.
    #[test]
    fn a_note_with_no_pid_lands_on_the_pane_that_left_it() {
        let mut mine = agent_pane(100, true);
        mine.pane_id = Some(9);
        let mut s = snap_writable(vec![mine]);
        s.caller = Some(Caller {
            session: "tdclip".into(),
            pane: Some(9),
        });
        let out = call_writing(&s, "leave_note", json!({ "text": "back in ten" }));
        assert_ne!(out["isError"], true, "{}", text_of(&out));
        assert_eq!(out["structuredContent"]["pid"], 100);
    }

    /// An explicit pid still wins. Supervising another pane is a real use, and
    /// the default is a convenience for the common case, not a confinement.
    #[test]
    fn an_explicit_pid_still_beats_the_caller_default() {
        let mut mine = agent_pane(100, true);
        mine.pane_id = Some(9);
        let mut other = agent_pane(200, true);
        other.pane_id = Some(4);
        let mut s = snap_writable(vec![mine, other]);
        s.caller = Some(Caller {
            session: "tdclip".into(),
            pane: Some(9),
        });
        let out = call_writing(&s, "leave_note", json!({ "pid": 200, "text": "look here" }));
        assert_eq!(out["structuredContent"]["pid"], 200);
    }

    /// A caller the host could not place is told THAT, rather than being told
    /// to pass a pid with no hint of why the default did not work. The three
    /// ways this fails are three different sentences on purpose: an anonymous
    /// connection, a located caller in no known pane, and a pane this window is
    /// not showing.
    #[test]
    fn each_way_of_not_knowing_the_caller_says_which_one_it_was() {
        let mut p = agent_pane(100, true);
        p.pane_id = Some(9);

        let anon = snap_writable(vec![p.clone()]);
        let e = text_of(&call(&anon, "leave_note", json!({ "text": "x" })));
        assert!(e.contains("did not say who is calling"), "{e}");

        let mut no_pane = snap_writable(vec![p.clone()]);
        no_pane.caller = Some(Caller {
            session: "tdclip".into(),
            pane: None,
        });
        let e = text_of(&call(&no_pane, "leave_note", json!({ "text": "x" })));
        assert!(e.contains("could not say which pane"), "{e}");

        let mut stranger = snap_writable(vec![p]);
        stranger.caller = Some(Caller {
            session: "tdclip".into(),
            pane: Some(77),
        });
        let e = text_of(&call(&stranger, "leave_note", json!({ "text": "x" })));
        assert!(e.contains("pane 77"), "{e}");
    }

    /// A pane with no host id reports `null` rather than dropping the key.
    /// A window-owned pane genuinely has no id; an omitted field would leave a
    /// hole the reader fills in with a guess.
    #[test]
    fn a_pane_with_no_host_id_reports_null_rather_than_omitting_it() {
        let mut p = agent_pane(100, true);
        p.pane_id = None;
        let out = call(&snap(true, true, vec![p]), "list_panes", json!({}));
        let pane = &out["structuredContent"]["panes"][0];
        assert!(
            pane.get("pane_id").is_some(),
            "pane_id was omitted; absence must be visible: {pane}"
        );
        assert!(pane["pane_id"].is_null());
        assert!(
            text_of(&out).contains("pane —"),
            "the line hid the missing id instead of drawing it: {}",
            text_of(&out)
        );
    }

    // ---- document_notes: the brief beside the caller ----

    /// A notes report the way the notes layer writes one, with `map` as given.
    fn report(map: &str, notes: u64, unsaved: u64) -> Value {
        json!({
            "state": "notes", "label": "brief.html", "notes": notes,
            "concurs": 0, "anchors": 4, "read_only": null, "writable": null,
            "refusal": null, "unsaved": unsaved, "saving": false, "gone": false,
            "said": null, "map": map, "open": null,
        })
    }

    fn doc(tab: usize, pane: u32, place: DocPlace, opened_by: Option<u64>, path: &str) -> DocInfo {
        DocInfo {
            tab,
            pane,
            place,
            opened_by,
            path: path.into(),
            notes: Ok(report(&format!("NOTES — {path}\n"), 1, 0)),
        }
    }

    /// A window: the caller is pane id 9 (pid 100) in tab 0, a shell sits
    /// beside it in tab 0 (pid 300), and tab 1 holds another agent (pid 200,
    /// pane id 4) with a shell of its own (pid 400).
    fn window_with(documents: Vec<DocInfo>) -> Snapshot {
        let mut me = agent_pane(100, true);
        me.pane_id = Some(9);
        let mut shell = agent_pane(300, false);
        shell.pane_id = Some(12);
        shell.is_agent = false;
        let mut other = agent_pane(200, true);
        other.pane_id = Some(4);
        other.tab = 1;
        let mut other_shell = agent_pane(400, false);
        other_shell.pane_id = Some(13);
        other_shell.tab = 1;
        let mut s = snap(true, true, vec![me, shell, other, other_shell]);
        s.caller = Some(Caller {
            session: "tdclip".into(),
            pane: Some(9),
        });
        s.documents = documents;
        s
    }

    fn notes_of(s: &Snapshot) -> Value {
        call(s, "document_notes", json!({}))
    }

    /// The square floating over the caller's own pane is the one it hears
    /// about, map and counts, and the map rides the text a model reads.
    #[test]
    fn document_notes_answers_the_float_over_the_callers_own_pane() {
        let s = window_with(vec![
            doc(0, 300, DocPlace::Split, None, "/r/other.html"),
            doc(0, 100, DocPlace::Float, None, "/r/brief.html"),
        ]);
        let out = notes_of(&s);
        assert_ne!(out["isError"], true, "{}", text_of(&out));
        let got = &out["structuredContent"];
        assert_eq!(
            got["path"], "/r/brief.html",
            "the float wins over a split: {got}"
        );
        assert_eq!(got["place"], "float");
        assert_eq!(got["found"], "float-over-you");
        assert_eq!(got["map"], "NOTES — /r/brief.html\n");
        assert_eq!(got["notes"], 1);
        assert!(
            text_of(&out).contains("NOTES — /r/brief.html"),
            "{}",
            text_of(&out)
        );
    }

    /// With no square, the split the caller opened is answered — ahead of a
    /// document pane in the same tab that somebody else opened.
    #[test]
    fn document_notes_answers_the_split_the_caller_opened() {
        let s = window_with(vec![
            doc(0, 300, DocPlace::Split, Some(12), "/r/theirs.html"),
            doc(0, 301, DocPlace::Split, Some(9), "/r/mine.html"),
        ]);
        let got = notes_of(&s)["structuredContent"].clone();
        assert_eq!(got["path"], "/r/mine.html", "{got}");
        assert_eq!(got["found"], "your-split");
        assert_eq!(got["map"], "NOTES — /r/mine.html\n");
        // No split recorded as the caller's: the first document pane in the
        // tab, and the answer says how many there were to choose from.
        let s = window_with(vec![
            doc(0, 300, DocPlace::Split, None, "/r/a.html"),
            doc(0, 301, DocPlace::Split, Some(12), "/r/b.html"),
        ]);
        let out = notes_of(&s);
        assert_eq!(out["structuredContent"]["path"], "/r/a.html");
        assert_eq!(out["structuredContent"]["found"], "in-your-tab");
        assert!(
            text_of(&out).contains("the first of 2"),
            "{}",
            text_of(&out)
        );
    }

    /// Nothing beside the caller is an answer, not an error, and every field
    /// is there and null — absence visible, never a zero count.
    #[test]
    fn document_notes_answers_none_when_nothing_is_beside_the_caller() {
        let out = notes_of(&window_with(vec![]));
        assert_ne!(out["isError"], true, "{}", text_of(&out));
        let got = &out["structuredContent"];
        for key in ["path", "notes", "concurs", "unsaved", "map"] {
            assert!(
                got.get(key).is_some_and(Value::is_null),
                "{key} should be present and null: {got}"
            );
        }
        assert!(text_of(&out).contains("nothing is open beside you"));
    }

    /// The load-bearing one. Another tab's documents are never the caller's,
    /// whatever they look like: a square over a pane there, a split there
    /// recorded as opened by the caller's own id (a pane dragged across tabs
    /// takes its opener with it), and a square over ANOTHER pane in the
    /// caller's own tab. Every one of those is beside somebody else.
    ///
    /// Mutation-tested: dropping the tab filter from [`document_beside`], or
    /// matching a float on the tab alone, makes this fail.
    #[test]
    fn document_notes_never_reaches_another_tabs_document() {
        let s = window_with(vec![
            doc(1, 200, DocPlace::Float, None, "/r/their-float.html"),
            doc(1, 400, DocPlace::Split, Some(9), "/r/moved-split.html"),
            doc(1, 400, DocPlace::Split, Some(4), "/r/their-split.html"),
            doc(
                0,
                300,
                DocPlace::Float,
                None,
                "/r/float-over-the-shell.html",
            ),
        ]);
        let out = notes_of(&s);
        assert_ne!(out["isError"], true, "{}", text_of(&out));
        assert!(
            out["structuredContent"]["path"].is_null(),
            "the caller was handed a document that is not beside it: {}",
            text_of(&out)
        );
        assert!(
            !text_of(&out).contains("NOTES"),
            "another tab's notes reached the text: {}",
            text_of(&out)
        );
        // And the pure rule agrees, for a caller in each tab.
        let me = &s.panes[0];
        assert!(document_beside(&s.documents, me).is_none());
        let them = &s.panes[2];
        let (d, how) = document_beside(&s.documents, them).expect("tab 1 has its own");
        assert_eq!(
            (d.path.as_str(), how),
            ("/r/their-float.html", Beside::FloatOverYou)
        );
    }

    /// Nobody recorded as the opener is not the caller. A caller with no host
    /// id must not be handed a split restored from a layout as "yours" — the
    /// `None == None` that an unguarded comparison would call a match.
    #[test]
    fn an_unrecorded_opener_is_not_the_caller() {
        let mut me = agent_pane(100, true);
        me.pane_id = None;
        let docs = vec![doc(0, 300, DocPlace::Split, None, "/r/restored.html")];
        let (_, how) = document_beside(&docs, &me).expect("still in the tab");
        assert_eq!(how, Beside::InYourTab { of: 1 });
    }

    /// Scoped to the caller with no way out of it: a `pid` is refused rather
    /// than honoured, and a caller that cannot be placed is told which way it
    /// could not be.
    #[test]
    fn document_notes_names_nothing_but_its_caller() {
        let s = window_with(vec![doc(
            1,
            200,
            DocPlace::Float,
            None,
            "/r/their-float.html",
        )]);
        let out = call(&s, "document_notes", json!({ "pid": 200 }));
        assert_eq!(
            out["isError"],
            true,
            "a pid was honoured: {}",
            text_of(&out)
        );
        assert!(!text_of(&out).contains("NOTES"));

        let mut anon = window_with(vec![]);
        anon.caller = None;
        let e = text_of(&notes_of(&anon));
        assert!(e.contains("did not say who is calling"), "{e}");
        assert!(!e.contains("pass a pid"), "advice the verb refuses: {e}");

        let mut off = window_with(vec![]);
        off.config.enabled = false;
        assert_eq!(notes_of(&off)["isError"], true);
    }

    /// Unsaved notes are in the map, as the brief's own copy map includes
    /// them, and the answer says how many — never silently.
    #[test]
    fn document_notes_says_how_much_of_the_map_is_unsaved() {
        let mut d = doc(0, 100, DocPlace::Float, None, "/r/brief.html");
        d.notes = Ok(report("NOTES — brief.html\n\n[a] A\n  - new\n", 3, 2));
        let out = notes_of(&window_with(vec![d]));
        assert_eq!(out["structuredContent"]["unsaved"], 2);
        assert!(
            text_of(&out).contains("2 edits not saved into the file yet"),
            "{}",
            text_of(&out)
        );
        // A document with no notes layer says why, and every count is null.
        let mut md = doc(0, 100, DocPlace::Float, None, "/r/notes.md");
        md.notes = Err("the Markdown document is not read yet".into());
        let out = notes_of(&window_with(vec![md]));
        assert!(out["structuredContent"]["notes"].is_null());
        assert!(text_of(&out).contains("the Markdown document is not read yet"));
    }

    /// It writes nothing: through a connection that CAN write, the apply
    /// capability is never reached.
    #[test]
    fn document_notes_writes_nothing() {
        let mut s = window_with(vec![doc(0, 100, DocPlace::Float, None, "/r/brief.html")]);
        s.config.writable = true;
        let applied = std::cell::Cell::new(0);
        let apply = |ups: &[ConfigUpdate]| -> Vec<ApplyOutcome> {
            applied.set(applied.get() + 1);
            ups.iter()
                .map(|(t, _)| (t.clone(), Ok(GradeReport::default())))
                .collect()
        };
        let line = json!({ "id": 9, "method": "tools/call",
            "params": { "name": "document_notes", "arguments": {} } })
        .to_string();
        let out = resp(&handle_line_with(&line, &s, no_tail, apply, no_search).unwrap());
        assert_eq!(out["result"]["structuredContent"]["path"], "/r/brief.html");
        assert_eq!(applied.get(), 0, "document_notes reached the write path");
    }

    // ---- open_document: the agent's own deliverable, beside it ----

    /// A file on disk for the verb to find, removed when dropped.
    struct TempDoc(std::path::PathBuf);
    impl TempDoc {
        fn new(name: &str, body: &str) -> TempDoc {
            let dir = std::env::temp_dir().join(format!("td-open-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join(name);
            std::fs::write(&path, body).unwrap();
            TempDoc(path)
        }
        fn path(&self) -> String {
            self.0.display().to_string()
        }
    }
    impl Drop for TempDoc {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    /// Call `open_document` on a writable window as the pane with id 9 (pid
    /// 100), with an `open` that records every request it is handed.
    fn open_as_caller(args: Value) -> (Value, Vec<OpenRequest>) {
        let mut s = window_with(vec![]);
        s.config.writable = true;
        let seen = std::cell::RefCell::new(Vec::new());
        let open = |r: &OpenRequest| -> OpenOutcome {
            seen.borrow_mut().push(r.clone());
            Ok(match r.placement {
                Placement::Beside => "beside pane 9 — opened in pane 12".into(),
                Placement::Here => "float pane 9".into(),
            })
        };
        let line = json!({ "id": 9, "method": "tools/call",
            "params": { "name": "open_document", "arguments": args } })
        .to_string();
        let out = resp(&handle_line_full(&line, &s, no_tail, no_apply, no_search, open).unwrap())
            ["result"]
            .clone();
        (out, seen.into_inner())
    }

    /// The load-bearing one. `open_document` opens in the caller's own pane
    /// or tab and nowhere else: a `pid` naming another pane is refused, not
    /// honoured and not ignored, and so is any key the schema does not name;
    /// and the one request that does reach the window names the caller.
    ///
    /// Mutation-tested: resolving the pane from an argument instead of the
    /// caller, and dropping the refusal of stray keys, each fail this.
    #[test]
    fn open_document_refuses_a_pane_that_is_not_the_callers() {
        let md = TempDoc::new("theirs.md", "# a page\n");
        for stray in [
            json!({ "pid": 200 }),
            json!({ "pane": 4 }),
            json!({ "tab": 1 }),
        ] {
            let mut args = stray.clone();
            args["path"] = json!(md.path());
            let (out, seen) = open_as_caller(args);
            assert_eq!(
                out["isError"],
                true,
                "{stray} was honoured: {}",
                text_of(&out)
            );
            assert!(seen.is_empty(), "{stray} reached the window: {seen:?}");
        }
        let (out, seen) = open_as_caller(json!({ "path": md.path() }));
        assert_ne!(out["isError"], true, "{}", text_of(&out));
        assert_eq!(seen.len(), 1);
        assert_eq!(
            seen[0].pid, 100,
            "opened for a pane that is not the caller's"
        );
        // No caller, no pane: it does not fall back to any pane at all.
        let mut anon = window_with(vec![]);
        anon.config.writable = true;
        anon.caller = None;
        let line = json!({ "id": 9, "method": "tools/call",
            "params": { "name": "open_document", "arguments": { "path": md.path() } } })
        .to_string();
        let refused = resp(
            &handle_line_full(
                &line,
                &anon,
                no_tail,
                no_apply,
                no_search,
                |_: &OpenRequest| panic!("an anonymous caller reached the window"),
            )
            .unwrap(),
        );
        assert_eq!(refused["result"]["isError"], true);
    }

    /// Absolute paths only, and only files TD can draw — each refusal in words
    /// the agent can act on, and none of them reaching the window.
    #[test]
    fn open_document_refuses_a_relative_path_and_a_file_td_cannot_draw() {
        let txt = TempDoc::new("notes.txt", "plain text\n");
        for (path, says) in [
            ("report.html", "relative"),
            ("./report.html", "relative"),
            ("https://example.com/r.html", "web address"),
            ("/nowhere/at/all/r.html", "not a file TD can draw"),
            (txt.path().as_str(), "not a file TD can draw"),
            ("", "needs a `path`"),
        ] {
            let (out, seen) = open_as_caller(json!({ "path": path }));
            assert_eq!(out["isError"], true, "{path:?} was accepted");
            assert!(text_of(&out).contains(says), "{path:?}: {}", text_of(&out));
            assert!(seen.is_empty(), "{path:?} reached the window");
        }
        let (out, _) = open_as_caller(json!({ "path": "/r.html", "placement": "elsewhere" }));
        assert_eq!(out["isError"], true);
    }

    /// "beside" is the default and asks for a pane beside the caller; "here"
    /// floats over it. A file:// URL is a path, decoded.
    #[test]
    fn open_document_lands_beside_the_caller_or_over_it() {
        let md = TempDoc::new("my report.md", "# made this turn\n");
        let (out, seen) = open_as_caller(json!({ "path": md.path() }));
        assert_eq!(seen[0].placement, Placement::Beside);
        assert_eq!(seen[0].target.kind, crate::docopen::DocKind::Markdown);
        assert_eq!(out["structuredContent"]["placement"], "beside");
        assert!(
            text_of(&out).contains("opened in pane 12"),
            "{}",
            text_of(&out)
        );

        let url = format!("file://{}", md.path().replace(' ', "%20"));
        let (out, seen) = open_as_caller(json!({ "path": url, "placement": "here" }));
        assert_ne!(out["isError"], true, "{}", text_of(&out));
        assert_eq!(seen[0].placement, Placement::Here);
        assert_eq!(
            seen[0].target.path, md.0,
            "the URL was not decoded to the file"
        );
    }

    /// Changing the layout is a write: it needs the writes toggle, as a note
    /// does.
    #[test]
    fn open_document_needs_the_writes_toggle() {
        let md = TempDoc::new("gate.md", "# gate\n");
        let s = window_with(vec![]);
        let line = json!({ "id": 9, "method": "tools/call",
            "params": { "name": "open_document", "arguments": { "path": md.path() } } })
        .to_string();
        let out = resp(
            &handle_line_full(
                &line,
                &s,
                no_tail,
                no_apply,
                no_search,
                |_: &OpenRequest| panic!("opened with writes off"),
            )
            .unwrap(),
        );
        assert_eq!(out["result"]["isError"], true);
        assert!(text_of(&out["result"]).contains("writes"));
    }

    /// The router's own lines, as `ctl doc beside` prints them, read back.
    #[test]
    fn the_routers_reply_becomes_the_answer() {
        assert_eq!(
            open_outcome("ok beside pane 9 — opened in pane 12"),
            Ok("beside pane 9 — opened in pane 12".into())
        );
        assert_eq!(
            open_outcome("ok float pane 9 — the tab has four panes, so it floats instead"),
            Ok("float pane 9 — the tab has four panes, so it floats instead".into())
        );
        assert!(open_outcome("desktop no browser")
            .unwrap()
            .contains("desktop"));
        assert_eq!(
            open_outcome("err the pane that asked is in no tab"),
            Err("the pane that asked is in no tab".into())
        );
    }

    /// The map `document_notes` hands over is byte for byte the map `ctl doc
    /// notes` prints, on every written case of the decision-brief skill's
    /// shared fixtures — and both are the skill's own expected-map.txt.
    ///
    /// `ctl doc notes` prints the notes layer's report (`DocumentView::
    /// notes_report` → `NotesLayer::report`); the snapshot carries that same
    /// report. So this builds the layer from each fixture exactly as the page
    /// does, takes its report as `ctl doc notes` would, and puts the report
    /// through the verb.
    #[test]
    fn document_notes_map_is_ctl_doc_notes_map_on_the_shared_fixtures() {
        use crate::docview::engine::{Anchor, ConcurSupport, RectCss};
        use crate::docview::{notes, notes_ui::NotesLayer};
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/decision-brief/notes-format/cases");
        let read_json = |p: std::path::PathBuf| -> Value {
            serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
        };
        let mut cases: Vec<_> = std::fs::read_dir(&fixtures).unwrap().flatten().collect();
        cases.sort_by_key(|e| e.file_name());
        let mut checked = 0;
        for case in cases {
            let dir = case.path();
            let (Ok(bytes), Ok(want)) = (
                std::fs::read(dir.join("expected.html")),
                std::fs::read_to_string(dir.join("expected-map.txt")),
            ) else {
                continue;
            };
            let expect = read_json(dir.join("expect.json"));
            let support = if expect["concur_support"] == "supported" {
                ConcurSupport::Supported
            } else {
                ConcurSupport::NotSupported
            };
            let rect = RectCss {
                x: 0.0,
                y: 0.0,
                w: 700.0,
                h: 200.0,
            };
            let anchors: Vec<Anchor> = read_json(dir.join("anchors.json"))
                .as_array()
                .unwrap()
                .iter()
                .map(|a| Anchor {
                    nid: a["nid"].as_str().unwrap().into(),
                    title: a["title"].as_str().unwrap().into(),
                    tag: "div".into(),
                    dialog: None,
                    rect: Some(rect),
                    button: None,
                    concur_zone: a["concurrable"].as_bool().unwrap().then_some(rect),
                    has_note: None,
                    has_concur: None,
                    line: None,
                })
                .collect();
            let layer = NotesLayer::new(
                notes::read(&bytes),
                expect["notes_file"].as_str().map(str::to_string),
                support,
                anchors.len() as u32,
                &dir.join("brief.html"),
            );
            let ctl = layer.report(&anchors);
            let mut d = doc(0, 100, DocPlace::Float, None, "/r/brief.html");
            d.notes = Ok(ctl.clone());
            let out = notes_of(&window_with(vec![d]));
            let name = case.file_name();
            let got = out["structuredContent"]["map"]
                .as_str()
                .unwrap_or_else(|| panic!("{name:?}: no map: {}", text_of(&out)));
            assert_eq!(
                Some(got),
                ctl["map"].as_str(),
                "{name:?}: not ctl doc notes' map"
            );
            assert_eq!(got, want, "{name:?}: not the skill's expected-map.txt");
            assert!(
                text_of(&out).contains(&want),
                "{name:?}: the text a model reads lost bytes of the map"
            );
            checked += 1;
        }
        assert_eq!(checked, 11, "every written case of the fixtures has a map");
    }

    /// Call set_pane_config with a fake gpui-thread `apply`: pid 100 succeeds
    /// (echoing the patched brightness), every other pid fails, outer succeeds.
    fn call_set(snap: &Snapshot, updates: Value) -> Value {
        let apply = |ups: &[ConfigUpdate]| -> Vec<ApplyOutcome> {
            ups.iter()
                .map(|(t, patch)| match t {
                    Target::Outer => (
                        t.clone(),
                        Ok(GradeReport {
                            brightness: patch.brightness.unwrap_or(0.0),
                            ..GradeReport::default()
                        }),
                    ),
                    Target::Pane(100) => (
                        t.clone(),
                        Ok(GradeReport {
                            brightness: patch.brightness.unwrap_or(0.0),
                            ..GradeReport::default()
                        }),
                    ),
                    Target::Pane(other) => {
                        (t.clone(), Err(format!("no exposed pane with pid {other}")))
                    }
                })
                .collect()
        };
        let line = json!({ "id": 9, "method": "tools/call",
            "params": { "name": "set_pane_config", "arguments": { "updates": updates } } })
        .to_string();
        resp(&handle_line_with(&line, snap, no_tail, apply, no_search).unwrap())["result"].clone()
    }

    #[test]
    fn set_pane_config_carries_a_logo_attach() {
        // a logo-only config is NOT empty and parses the path.
        let patch: ConfigPatch =
            serde_json::from_value(json!({ "logo": "/tmp/brand.png" })).unwrap();
        assert!(
            !patch.is_empty(),
            "a logo-only config has something to change"
        );
        assert_eq!(patch.logo.as_deref(), Some("/tmp/brand.png"));

        // end-to-end: the parsed logo path reaches the `apply` closure intact, so an
        // agent can attach a logo to the terminal it's working in.
        let seen: std::sync::Arc<std::sync::Mutex<Option<String>>> = Default::default();
        let seen2 = seen.clone();
        let apply = move |ups: &[ConfigUpdate]| -> Vec<ApplyOutcome> {
            *seen2.lock().unwrap() = ups[0].1.logo.clone();
            vec![(ups[0].0.clone(), Ok(GradeReport::default()))]
        };
        let s = snap_writable(vec![agent_pane(100, true)]);
        let line = json!({ "id": 9, "method": "tools/call", "params": {
            "name": "set_pane_config",
            "arguments": { "updates": [{ "target": 100, "config": { "logo": "/tmp/brand.png" } }] }
        }})
        .to_string();
        let _ = handle_line_with(&line, &s, no_tail, apply, no_search);
        assert_eq!(seen.lock().unwrap().as_deref(), Some("/tmp/brand.png"));
    }

    /// THE contract of the fridge door: ten words go on a sticky, eleven go in
    /// a transcript. The refusal must carry the count, so the agent's retry is
    /// an edit rather than a guess.
    #[test]
    fn a_note_holds_ten_words_and_not_eleven() {
        let ten = "tests green deploy paused waiting on DNS back at seven";
        assert!(matches!(
            validate_note(None, Some(ten), false, false),
            Ok(NoteChange::Post { .. })
        ));
        let eleven = "tests green deploy paused waiting on DNS and back at seven";
        let err = validate_note(None, Some(eleven), false, false).unwrap_err();
        assert!(
            err.contains("11 words"),
            "the refusal names the count: {err}"
        );
    }

    /// A title is "GET MILK!" sized. Forty characters pass; past that the
    /// refusal points at the headline, not the body.
    #[test]
    fn a_title_is_a_headline_not_a_sentence() {
        let forty = "A".repeat(40);
        assert!(validate_note(Some(&forty), Some("done"), false, false).is_ok());
        let long = "A".repeat(41);
        let err = validate_note(Some(&long), Some("done"), false, false).unwrap_err();
        assert!(err.contains("title"), "the refusal blames the title: {err}");
    }

    /// A title alone is a note ("GET MILK!"); nothing at all is a peel, and so
    /// is `clear` regardless of what else came along.
    #[test]
    fn an_empty_note_is_a_peel_and_a_title_alone_is_a_note() {
        assert_eq!(
            validate_note(None, None, false, false),
            Ok(NoteChange::Clear)
        );
        assert_eq!(
            validate_note(Some("  "), Some(" "), true, false),
            Ok(NoteChange::Clear)
        );
        assert_eq!(
            validate_note(Some("ignored"), Some("ignored"), false, true),
            Ok(NoteChange::Clear)
        );
        assert_eq!(
            validate_note(Some("GET MILK!"), None, false, false),
            Ok(NoteChange::Post {
                title: Some("GET MILK!".into()),
                text: String::new(),
                pin: false,
            })
        );
    }

    /// End-to-end: the tool call reaches the apply closure as a validated
    /// `NoteChange::Post` on the right pane, pin included.
    #[test]
    fn leave_note_reaches_the_apply_closure() {
        let seen: std::sync::Arc<std::sync::Mutex<Option<ConfigUpdate>>> = Default::default();
        let seen2 = seen.clone();
        let apply = move |ups: &[ConfigUpdate]| -> Vec<ApplyOutcome> {
            *seen2.lock().unwrap() = Some(ups[0].clone());
            vec![(ups[0].0.clone(), Ok(GradeReport::default()))]
        };
        let s = snap_writable(vec![agent_pane(100, true)]);
        let line = json!({ "id": 4, "method": "tools/call", "params": {
            "name": "leave_note",
            "arguments": { "pid": 100, "title": "NEEDS EYES", "text": "deploy paused, waiting on DNS", "pin": true }
        }})
        .to_string();
        let reply = handle_line_with(&line, &s, no_tail, apply, no_search).unwrap();
        assert!(!reply.contains("isError"), "the post succeeds: {reply}");
        let (target, patch) = seen.lock().unwrap().clone().expect("apply was called");
        assert_eq!(target, Target::Pane(100));
        assert_eq!(
            patch.note,
            Some(NoteChange::Post {
                title: Some("NEEDS EYES".into()),
                text: "deploy paused, waiting on DNS".into(),
                pin: true,
            })
        );
    }

    /// The same second opt-in as every other write: no TD_MCP_WRITE, no note.
    #[test]
    fn leave_note_refused_when_writes_are_off() {
        let mut s = snap_writable(vec![agent_pane(100, true)]);
        s.config.writable = false;
        let line = json!({ "id": 5, "method": "tools/call", "params": {
            "name": "leave_note",
            "arguments": { "pid": 100, "text": "hello" }
        }})
        .to_string();
        let reply = handle_line_with(&line, &s, no_tail, no_apply, no_search).unwrap();
        assert!(reply.contains("isError"), "refused: {reply}");
        assert!(reply.contains("read-only watch surface"));
    }

    /// The fridge door is for reading too: a pane's posted note rides
    /// `list_panes`, title on the line where a passing agent will see it.
    #[test]
    fn list_panes_reads_the_fridge_door() {
        let mut pane = agent_pane(100, true);
        pane.note = Some(NoteReport {
            title: Some("GET MILK!".into()),
            text: "home at 7pm".into(),
            pinned: false,
        });
        let s = snap(true, true, vec![pane]);
        let line = json!({ "id": 6, "method": "tools/call", "params": { "name": "list_panes" } })
            .to_string();
        let reply = handle_line_with(&line, &s, no_tail, no_apply, no_search).unwrap();
        assert!(
            reply.contains("GET MILK!"),
            "the title is on the line: {reply}"
        );
        assert!(reply.contains("home at 7pm"), "and so is the note: {reply}");
    }

    #[test]
    fn get_pane_config_defaults_to_every_exposed_pane_plus_outer() {
        let s = snap_writable(vec![agent_pane(100, true), agent_pane(200, false)]);
        let r = call(&s, "get_pane_config", json!({}));
        let configs = r["structuredContent"]["configs"].as_array().unwrap();
        // exposed pane 100 + outer, but NOT the unexposed pane 200.
        let scopes: Vec<&str> = configs
            .iter()
            .map(|c| c["scope"].as_str().unwrap())
            .collect();
        assert_eq!(configs.len(), 2, "one exposed pane + outer");
        assert!(scopes.contains(&"outer"));
        assert!(scopes.contains(&"pane"));
        let outer = configs.iter().find(|c| c["scope"] == "outer").unwrap();
        assert_eq!(
            outer["grade"]["text_size"], 50.0,
            "outer grade reported in percents"
        );
    }

    #[test]
    fn get_pane_config_unknown_pid_is_a_per_target_error_not_a_failure() {
        let s = snap_writable(vec![agent_pane(100, true)]);
        let r = call(
            &s,
            "get_pane_config",
            json!({ "targets": [100, 999, "outer"] }),
        );
        assert!(
            r.get("isError").is_none(),
            "a bad pid does not fail the batch"
        );
        let configs = r["structuredContent"]["configs"].as_array().unwrap();
        assert_eq!(configs.len(), 3);
        let bad = configs.iter().find(|c| c["target"] == 999).unwrap();
        assert!(bad["error"].as_str().unwrap().contains("999"));
    }

    #[test]
    fn get_pane_config_locked_when_disabled() {
        let s = snap(false, true, vec![agent_pane(100, true)]);
        let r = call(&s, "get_pane_config", json!({}));
        assert_eq!(r["isError"], true);
    }

    // ---- config API: SET (gating + parsing + result shape) ----

    #[test]
    fn set_pane_config_refused_when_writes_are_off() {
        // enabled, but the second opt-in is off ⇒ refused, and it says why.
        let s = snap(true, true, vec![agent_pane(100, true)]);
        let r = call_set(
            &s,
            json!([{ "target": 100, "config": { "brightness": 30 } }]),
        );
        assert_eq!(r["isError"], true);
        assert!(r["content"][0]["text"].as_str().unwrap().contains("writes"));
    }

    #[test]
    fn set_pane_config_refused_when_disabled() {
        let mut s = snap(false, true, vec![agent_pane(100, true)]);
        s.config.writable = true; // writable but master switch off ⇒ still refused
        let r = call_set(
            &s,
            json!([{ "target": 100, "config": { "brightness": 30 } }]),
        );
        assert_eq!(r["isError"], true);
    }

    #[test]
    fn set_pane_config_applies_and_echoes_the_resulting_grade() {
        let s = snap_writable(vec![agent_pane(100, true)]);
        let r = call_set(
            &s,
            json!([{ "target": 100, "config": { "brightness": 30 } }]),
        );
        assert!(r.get("isError").is_none());
        let results = r["structuredContent"]["results"].as_array().unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["ok"], true);
        assert_eq!(results[0]["target"], 100);
        assert_eq!(
            results[0]["grade"]["brightness"], 30.0,
            "the dumb store echoes the value"
        );
    }

    #[test]
    fn set_pane_config_partial_batch_reports_per_target() {
        // one good pid, one bad pid, plus outer — the bad one fails alone.
        let s = snap_writable(vec![agent_pane(100, true)]);
        let r = call_set(
            &s,
            json!([
                { "target": 100, "config": { "brightness": 20 } },
                { "target": 777, "config": { "brightness": 20 } },
                { "target": "outer", "config": { "brightness": 10 } }
            ]),
        );
        let results = r["structuredContent"]["results"].as_array().unwrap();
        assert_eq!(results.len(), 3);
        let ok = results.iter().filter(|x| x["ok"] == true).count();
        assert_eq!(ok, 2, "100 and outer applied; 777 failed");
        assert!(r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("1 failed"));
    }

    #[test]
    fn set_pane_config_rejects_malformed_requests_before_mutating() {
        let s = snap_writable(vec![agent_pane(100, true)]);
        // missing `updates`
        let line = json!({ "id": 1, "method": "tools/call",
            "params": { "name": "set_pane_config", "arguments": {} } })
        .to_string();
        let r = resp(&handle_line_with(&line, &s, no_tail, no_apply, no_search).unwrap())["result"]
            .clone();
        assert_eq!(r["isError"], true);
        // empty updates
        assert_eq!(call_set(&s, json!([]))["isError"], true);
        // empty config (no channels)
        assert_eq!(
            call_set(&s, json!([{ "target": 100, "config": {} }]))["isError"],
            true
        );
        // unknown channel ⇒ loud error (deny_unknown_fields)
        assert_eq!(
            call_set(&s, json!([{ "target": 100, "config": { "britness": 30 } }]))["isError"],
            true
        );
        // bad target string
        assert_eq!(
            call_set(
                &s,
                json!([{ "target": "everything", "config": { "brightness": 30 } }])
            )["isError"],
            true
        );
    }

    #[test]
    fn config_patch_patch_semantics_only_touch_named_channels() {
        // A patch with one channel deserialises to exactly one Some(..).
        let p: ConfigPatch = serde_json::from_value(json!({ "brightness": 42 })).unwrap();
        assert_eq!(p.brightness, Some(42.0));
        assert!(p.contrast.is_none() && p.warp.is_none() && p.crawl.is_none());
        assert!(!p.is_empty());
        assert!(ConfigPatch::default().is_empty());
    }

    #[test]
    fn read_only_handle_line_cannot_write_even_if_policy_allows() {
        // Through the bare read-only `handle_line`, set_pane_config reaches
        // `no_apply` and every target is refused — the capability, not just the
        // policy, gates writes.
        let s = snap_writable(vec![agent_pane(100, true)]);
        let line = json!({ "id": 1, "method": "tools/call",
            "params": { "name": "set_pane_config",
                "arguments": { "updates": [{ "target": 100, "config": { "brightness": 30 } }] } } })
        .to_string();
        let r = resp(&handle_line(&line, &s, no_tail).unwrap())["result"].clone();
        let results = r["structuredContent"]["results"].as_array().unwrap();
        assert_eq!(results[0]["ok"], false);
        assert!(results[0]["error"].as_str().unwrap().contains("read-only"));
    }

    #[test]
    fn list_panes_locked_when_disabled() {
        // master switch off ⇒ no pane leaks, even if a pane is marked exposed.
        let s = snap(false, true, vec![agent_pane(100, true)]);
        let v = resp(
            &handle_line(
                r#"{"id":3,"method":"tools/call","params":{"name":"list_panes"}}"#,
                &s,
                no_tail,
            )
            .unwrap(),
        );
        assert_eq!(v["result"]["isError"], true);
        assert!(v["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("disabled"));
    }

    #[test]
    fn list_panes_reports_only_exposed() {
        let s = snap(
            true,
            true,
            vec![agent_pane(100, true), agent_pane(200, false)],
        );
        let v = resp(
            &handle_line(
                r#"{"id":4,"method":"tools/call","params":{"name":"list_panes"}}"#,
                &s,
                no_tail,
            )
            .unwrap(),
        );
        let panes = v["result"]["structuredContent"]["panes"]
            .as_array()
            .unwrap();
        assert_eq!(panes.len(), 1, "the non-exposed pane is filtered out");
        assert_eq!(panes[0]["pid"], 100);
    }

    #[test]
    fn pane_events_honours_the_events_switch() {
        let s = snap(true, false, vec![agent_pane(100, true)]);
        let v = resp(
            &handle_line(
                r#"{"id":5,"method":"tools/call","params":{"name":"pane_events","arguments":{"pid":100}}}"#,
                &s,
                |_, _| vec![ToolEvent { ts: "t".into(), tool: "Bash".into(), summary: "ls".into() }],
            )
            .unwrap(),
        );
        assert_eq!(v["result"]["isError"], true, "events off ⇒ refused");
    }

    #[test]
    fn pane_events_returns_tailed_events_for_an_exposed_agent() {
        let s = snap(true, true, vec![agent_pane(100, true)]);
        let v = resp(
            &handle_line(
                r#"{"id":6,"method":"tools/call","params":{"name":"pane_events","arguments":{"pid":100,"limit":5}}}"#,
                &s,
                |p, limit| {
                    assert_eq!(p.pid, 100);
                    assert_eq!(limit, 5);
                    vec![ToolEvent { ts: "2026".into(), tool: "Edit".into(), summary: "main.rs".into() }]
                },
            )
            .unwrap(),
        );
        let events = v["result"]["structuredContent"]["events"]
            .as_array()
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["tool"], "Edit");
    }

    #[test]
    fn pane_events_refuses_an_unexposed_or_unknown_pid() {
        let s = snap(true, true, vec![agent_pane(100, false)]);
        let v = resp(
            &handle_line(
                r#"{"id":7,"method":"tools/call","params":{"name":"pane_events","arguments":{"pid":100}}}"#,
                &s,
                no_tail,
            )
            .unwrap(),
        );
        assert_eq!(v["result"]["isError"], true);
    }

    #[test]
    fn unknown_method_is_a_jsonrpc_error() {
        let s = snap(true, true, vec![]);
        let v = resp(&handle_line(r#"{"id":8,"method":"do/stuff"}"#, &s, no_tail).unwrap());
        assert_eq!(v["error"]["code"], -32601);
    }

    // ---- transport routing helpers ----

    #[test]
    fn only_tools_call_requires_a_snapshot() {
        assert!(requires_snapshot(
            r#"{"id":1,"method":"tools/call","params":{"name":"list_panes"}}"#
        ));
        for m in [
            r#"{"id":1,"method":"initialize"}"#,
            r#"{"id":1,"method":"tools/list"}"#,
            r#"{"id":1,"method":"ping"}"#,
            r#"{"id":1,"method":"logging/setLevel","params":{"level":"warning"}}"#,
            r#"{"method":"notifications/initialized"}"#,
            "garbage",
        ] {
            assert!(!requires_snapshot(m), "should be static: {m}");
        }
    }

    #[test]
    fn log_level_parsing_and_severity_order() {
        assert_eq!(
            log_level_from(r#"{"id":1,"method":"logging/setLevel","params":{"level":"warning"}}"#),
            Some(log_severity("warning"))
        );
        assert_eq!(log_level_from(r#"{"id":1,"method":"ping"}"#), None);
        assert!(log_severity("debug") < log_severity("info"));
        assert!(log_severity("info") < log_severity("warning"));
        assert_eq!(
            log_severity("nonsense"),
            log_severity("info"),
            "unknown ⇒ info"
        );
    }

    #[test]
    fn error_response_targets_the_id_and_skips_notifications() {
        let e =
            error_response(r#"{"id":9,"method":"tools/call"}"#, -32000, "ui not ready").unwrap();
        let v = resp(&e);
        assert_eq!(v["id"], 9);
        assert_eq!(v["error"]["code"], -32000);
        assert_eq!(v["error"]["message"], "ui not ready");
        assert!(
            error_response(r#"{"method":"notifications/initialized"}"#, -1, "x").is_none(),
            "a notification gets no error reply"
        );
    }

    #[test]
    fn initialize_advertises_logging_capability() {
        let s = Snapshot::empty();
        let v = resp(
            &handle_line(r#"{"id":1,"method":"initialize","params":{}}"#, &s, no_tail).unwrap(),
        );
        assert!(v["result"]["capabilities"]["logging"].is_object());
    }

    // ---- Watcher (push feed brain) ----

    fn tailed(pid: u32, events: &[(&str, &str)]) -> HashMap<u32, Vec<ToolEvent>> {
        let v = events
            .iter()
            .map(|(tool, sum)| ToolEvent {
                ts: format!("t-{sum}"),
                tool: (*tool).into(),
                summary: (*sum).into(),
            })
            .collect();
        HashMap::from([(pid, v)])
    }

    #[test]
    fn first_sight_announces_the_agent_but_replays_no_history() {
        let mut w = Watcher::default();
        let panes = vec![agent_pane(100, true)];
        let n = w.diff(&panes, &tailed(100, &[("Bash", "old1"), ("Edit", "old2")]));
        // exactly one notification: the agent appeared. No tool_call backlog.
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].data["event"], "agent_appeared");
        assert_eq!(n[0].data["pid"], 100);
    }

    #[test]
    fn new_tool_call_after_first_sight_is_pushed_once() {
        let mut w = Watcher::default();
        let panes = vec![agent_pane(100, true)];
        // first sight: bookmark old2, emit only appeared
        w.diff(&panes, &tailed(100, &[("Bash", "old1"), ("Edit", "old2")]));
        // a new event arrives
        let n = w.diff(
            &panes,
            &tailed(100, &[("Bash", "old1"), ("Edit", "old2"), ("Grep", "new1")]),
        );
        assert_eq!(n.len(), 1, "only the new event");
        assert_eq!(n[0].data["event"], "tool_call");
        assert_eq!(n[0].data["tool"], "Grep");
        assert_eq!(n[0].data["summary"], "new1");
        // idempotent: re-tailing the same file pushes nothing new
        let again = w.diff(&panes, &tailed(100, &[("Edit", "old2"), ("Grep", "new1")]));
        assert!(again.is_empty(), "no duplicate pushes");
    }

    #[test]
    fn rotated_transcript_emits_only_the_newest_not_a_flood() {
        let mut w = Watcher::default();
        let panes = vec![agent_pane(100, true)];
        w.diff(&panes, &tailed(100, &[("Bash", "a"), ("Edit", "b")])); // bookmark "b"
                                                                       // the tail window no longer contains "b" (rotated); 3 unseen events present
        let n = w.diff(&panes, &tailed(100, &[("X", "c"), ("Y", "d"), ("Z", "e")]));
        assert_eq!(n.len(), 1, "only newest, not all three");
        assert_eq!(n[0].data["summary"], "e");
    }

    #[test]
    fn vanished_agent_is_announced_and_state_forgotten() {
        let mut w = Watcher::default();
        let panes = vec![agent_pane(100, true)];
        w.diff(&panes, &tailed(100, &[("Bash", "x")]));
        let n = w.diff(&[], &HashMap::new()); // pane gone
        assert_eq!(n.len(), 1);
        assert_eq!(n[0].data["event"], "agent_vanished");
        assert_eq!(n[0].data["pid"], 100);
        // a brand-new pane reusing pid 100 is treated as fresh (appeared again)
        let again = w.diff(&panes, &tailed(100, &[("Bash", "x")]));
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].data["event"], "agent_appeared");
    }

    #[test]
    fn watcher_ignores_unexposed_and_non_agent_panes() {
        let mut w = Watcher::default();
        let shell = PaneInfo {
            tab: 0,
            title: "sh".into(),
            mode: "SHELL".into(),
            is_agent: false,
            pid: 7,
            pane_id: Some(7),
            cwd: None,
            session: None,
            tool: None,
            note: None,
            exposed: true,
            grade: GradeReport::default(),
        };
        let hidden_agent = agent_pane(8, false);
        let n = w.diff(&[shell, hidden_agent], &HashMap::new());
        assert!(n.is_empty(), "only exposed agents are watched");
    }

    #[test]
    fn notification_encodes_as_mcp_message() {
        let v = resp(&encode_notification(&Notification {
            level: "info",
            data: json!({ "event": "tool_call", "tool": "Bash" }),
        }));
        assert_eq!(v["method"], "notifications/message");
        assert!(v["id"].is_null(), "a notification carries no id");
        assert_eq!(v["params"]["level"], "info");
        assert_eq!(v["params"]["logger"], SERVER_NAME);
        assert_eq!(v["params"]["data"]["tool"], "Bash");
    }
}
