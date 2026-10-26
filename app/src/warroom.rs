//! The War Room's rules: which agent may reach which pane, and how far.
//!
//! A person gives each pane a position (open, read-only, off-limits) that holds
//! for every agent, seats agent panes in the War Room, and draws lines between
//! seats. A line says how far its sender may reach the pane it points at: see
//! it in the census, read its screen and tool calls, or ask it a question. Only
//! a person draws a line, and nothing here is drawn on screen — this module is
//! the rule book every caller asks, written without gpui so that each rule is
//! a test. The plan and its four approved gates are in
//! `docs/plans/pane-agent-access/`.
//!
//! Three things the rules are careful about, each because the earlier briefs
//! found the cheap version wrong:
//!
//! - **A pane is never reachable by default once it is seated.** A seat with no
//!   line from the caller is hidden from that caller. A pane nobody has seated
//!   keeps today's behaviour (readable), and is never asked: no prompt travels
//!   without a line.
//! - **A word this build cannot read grants nothing.** A rung written by a
//!   later build is kept as written so a save does not erase it, and it opens
//!   nothing; an `agents` word on a saved pane that this build does not know
//!   reads as off-limits.
//! - **Seats are not pane numbers.** Pane ids come from the session host's
//!   counter, which starts again at 1 when the host restarts, so a line saved
//!   against a number could wake up pointing at a stranger. A seat id is minted
//!   when a person seats the pane and is saved with that pane.

use serde::{Deserialize, Serialize};

use crate::mcp::{Expose, McpConfig};

/// A seat's name: minted once, when a person seats a pane, and saved on that
/// pane's leaf so it outlives the pane's number.
///
/// Kept inside `i64` because the saved window is TOML, whose integers are
/// signed: a `u64` above `i64::MAX` would not save at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeatId(pub u64);

impl SeatId {
    /// A fresh, effectively unique id. `RandomState` is seeded from the OS per
    /// instance, so no random-number crate is needed for 63 bits of noise.
    pub fn mint() -> Self {
        use std::hash::{BuildHasher, Hasher};
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        h.write_u128(now);
        h.write_u32(std::process::id());
        let id = h.finish() & (i64::MAX as u64);
        SeatId(id.max(1))
    }
}

/// A saved org identity, independent of its editable label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrgId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Org {
    pub id: OrgId,
    /// Legacy charts have no declared label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default)]
    pub seats: Vec<SeatId>,
}

/// Grouping only: every permission still comes from `Chart`.
#[derive(Clone, Debug, Default)]
pub struct Orgs {
    orgs: Vec<Org>,
}

impl Orgs {
    pub fn restore(saved: Option<Vec<Org>>, legacy: &[SeatId]) -> Self {
        match saved {
            Some(orgs) => Self { orgs },
            None if legacy.is_empty() => Self::default(),
            None => Self {
                orgs: vec![Org {
                    id: OrgId(SeatId::mint().0),
                    name: None,
                    seats: legacy
                        .iter()
                        .copied()
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect(),
                }],
            },
        }
    }

    pub fn saved(&self) -> &[Org] {
        &self.orgs
    }
    pub fn get(&self, id: OrgId) -> Option<&Org> {
        self.orgs.iter().find(|o| o.id == id)
    }

    /// Ambiguous or absent membership stays unknown.
    pub fn owner(&self, seat: SeatId) -> Option<OrgId> {
        let mut owners = self.orgs.iter().filter(|o| o.seats.contains(&seat));
        let owner = owners.next()?.id;
        owners.next().is_none().then_some(owner)
    }

    pub fn create(&mut self, name: &str) -> Option<OrgId> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        let id = OrgId(SeatId::mint().0);
        self.orgs.push(Org {
            id,
            name: Some(name.to_string()),
            seats: vec![],
        });
        Some(id)
    }

    pub fn rename(&mut self, id: OrgId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(org) = self.orgs.iter_mut().find(|o| o.id == id) else {
            return false;
        };
        org.name = Some(name.to_string());
        true
    }

    pub fn assign(&mut self, seat: SeatId, org: OrgId) -> bool {
        if self.get(org).is_none() {
            return false;
        }
        self.remove(seat);
        self.orgs
            .iter_mut()
            .find(|o| o.id == org)
            .unwrap()
            .seats
            .push(seat);
        true
    }

    pub fn remove(&mut self, seat: SeatId) {
        for org in &mut self.orgs {
            org.seats.retain(|s| *s != seat);
        }
    }
}

/// An unfinished relationship grants nothing until both endpoints and a rung
/// are explicitly selected and the person saves it.
#[derive(Clone, Debug, Default)]
pub struct RelationDraft {
    pub from: Option<SeatId>,
    pub to: Option<SeatId>,
    pub rung: Option<Rung>,
}

impl RelationDraft {
    pub fn ready(&self, orgs: &Orgs) -> Option<(SeatId, SeatId, Rung)> {
        let (from, to, rung) = (self.from?, self.to?, self.rung?);
        (from != to && orgs.owner(from).is_some() && orgs.owner(from) == orgs.owner(to))
            .then_some((from, to, rung))
    }
}

/// A person's choice for one pane, for every agent. Set only by a person: the
/// pane's right-click menu or a Yes on the attention rail.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AgentAccess {
    /// Agents may watch the pane and, with writes on, write to it.
    #[default]
    Open,
    /// Agents may watch the pane and ask it; they may not write to it or type
    /// into its bench.
    ReadOnly,
    /// No agent may see the pane at all, by any line.
    OffLimits,
}

/// The word a read-only pane is saved with, beside the older `off_limits` key.
pub const READ_ONLY_WORD: &str = "read-only";

impl AgentAccess {
    /// From a saved leaf. `off_limits = true` wins over any word; a word this
    /// build does not know reads as off-limits, never as open.
    pub fn from_saved(off_limits: bool, agents: Option<&str>) -> Self {
        if off_limits {
            return Self::OffLimits;
        }
        match agents {
            None => Self::Open,
            Some(READ_ONLY_WORD) => Self::ReadOnly,
            Some(_) => Self::OffLimits,
        }
    }

    /// For a saved leaf: the `off_limits` flag and the `agents` word.
    pub fn to_saved(self) -> (bool, Option<String>) {
        match self {
            Self::Open => (false, None),
            Self::ReadOnly => (false, Some(READ_ONLY_WORD.to_string())),
            Self::OffLimits => (true, None),
        }
    }

    /// Hidden from every agent but its own.
    pub fn is_off_limits(self) -> bool {
        self == Self::OffLimits
    }

    /// Another agent may write to it: notes, appearance, surfaces, bench
    /// input, document actions.
    pub fn takes_writes(self) -> bool {
        self == Self::Open
    }
}

/// How far a line reaches. The first step stops at `Ask`; assigning work
/// arrives later, together with budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rung {
    See,
    Read,
    Ask,
}

/// A rung as saved. A word this build does not know is kept as written, so a
/// save does not erase what a later build wrote, and it grants nothing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RungWord {
    Known(Rung),
    Unknown(String),
}

impl RungWord {
    pub fn known(&self) -> Option<Rung> {
        match self {
            Self::Known(r) => Some(*r),
            Self::Unknown(_) => None,
        }
    }
}

/// One line a person drew, or a bypass a person granted on the rail.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub from: SeatId,
    pub to: SeatId,
    pub rung: RungWord,
    /// `Some(n)`: a bypass with `n` prompts left. `None`: a drawn line, which is
    /// unlimited — absent here means "no count applies", never zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompts_left: Option<u8>,
}

impl Line {
    pub fn is_bypass(&self) -> bool {
        self.prompts_left.is_some()
    }
}

/// Every line in one window. At most one line runs from a seat to another; a
/// new one replaces the old.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chart {
    lines: Vec<Line>,
}

/// A seat's place in the chart, read from its drawn lines (bypasses never
/// count).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Draws lines to others and has none drawn to it.
    Orchestrator,
    /// Has lines drawn to it and draws lines to others.
    Manager,
    /// Has lines drawn to it and draws none.
    Worker,
    /// Seated, with no drawn line either way.
    Alone,
}

impl Chart {
    pub fn from_saved(lines: Vec<Line>) -> Self {
        let mut chart = Self::default();
        for l in lines {
            chart.put(l);
        }
        chart
    }

    pub fn saved(&self) -> &[Line] {
        &self.lines
    }

    pub fn line(&self, from: SeatId, to: SeatId) -> Option<&Line> {
        self.lines.iter().find(|l| l.from == from && l.to == to)
    }

    fn put(&mut self, line: Line) {
        if line.from == line.to {
            return;
        }
        self.lines
            .retain(|l| !(l.from == line.from && l.to == line.to));
        self.lines.push(line);
    }

    /// A person drew (or changed) a line. Callers: the War Room panel only.
    pub fn draw(&mut self, from: SeatId, to: SeatId, rung: Rung) {
        self.put(Line {
            from,
            to,
            rung: RungWord::Known(rung),
            prompts_left: None,
        });
    }

    /// A person pressed Yes on a bypass ask. Callers: the rail only. A bypass
    /// never shortens a drawn line that already reaches as far.
    pub fn grant_bypass(&mut self, from: SeatId, to: SeatId, prompts: u8) {
        if let Some(l) = self.line(from, to) {
            if !l.is_bypass() && l.rung.known() >= Some(Rung::Ask) {
                return;
            }
        }
        self.put(Line {
            from,
            to,
            rung: RungWord::Known(Rung::Ask),
            prompts_left: Some(prompts),
        });
    }

    pub fn erase(&mut self, from: SeatId, to: SeatId) {
        self.lines.retain(|l| !(l.from == from && l.to == to));
    }

    /// A seat was taken off the chart: every line to or from it goes.
    pub fn forget_seat(&mut self, seat: SeatId) {
        self.lines.retain(|l| l.from != seat && l.to != seat);
    }

    /// Spend one prompt of a bypass. A drawn line spends nothing. At zero the
    /// bypass disappears. Returns what is left, `None` for a drawn line.
    pub fn spend(&mut self, from: SeatId, to: SeatId) -> Option<u8> {
        let i = self
            .lines
            .iter()
            .position(|l| l.from == from && l.to == to)?;
        let left = self.lines[i].prompts_left?;
        let left = left.saturating_sub(1);
        if left == 0 {
            self.lines.remove(i);
        } else {
            self.lines[i].prompts_left = Some(left);
        }
        Some(left)
    }

    fn drawn(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter().filter(|l| !l.is_bypass())
    }

    pub fn role_of(&self, seat: SeatId) -> Role {
        let has_in = self.drawn().any(|l| l.to == seat);
        let has_out = self.drawn().any(|l| l.from == seat);
        match (has_in, has_out) {
            (false, true) => Role::Orchestrator,
            (true, true) => Role::Manager,
            (true, false) => Role::Worker,
            (false, false) => Role::Alone,
        }
    }

    /// The seat this one is drawn under: the oldest drawn line into it.
    pub fn manager_of(&self, seat: SeatId) -> Option<SeatId> {
        self.drawn().find(|l| l.to == seat).map(|l| l.from)
    }
}

/// Where a seat is drawn in the War Room: its row (how deep it sits under
/// managers) and its place in that row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub seat: SeatId,
    pub row: usize,
    pub col: usize,
}

/// Lay seats out as an org chart. A seat sits one row under the seat it is
/// drawn under ([`Chart::manager_of`], the oldest drawn line into it), so a
/// seat with no manager on the chart is in the top row. Within a row, seats
/// are ordered by project and then title, so one project's seats sit together
/// (Gate 1, question 7). A loop of lines is cut where it is first met rather
/// than followed forever.
pub fn layout(seats: &[(SeatId, String, String)], chart: &Chart) -> Vec<Placed> {
    use std::collections::{HashMap, HashSet};
    let here: HashSet<SeatId> = seats.iter().map(|(s, _, _)| *s).collect();
    let depth_of = |start: SeatId| -> usize {
        let mut seen = HashSet::from([start]);
        let mut at = start;
        let mut depth = 0;
        while let Some(up) = chart.manager_of(at).filter(|m| here.contains(m)) {
            if !seen.insert(up) {
                break;
            }
            depth += 1;
            at = up;
        }
        depth
    };
    let mut rows: HashMap<usize, Vec<&(SeatId, String, String)>> = HashMap::new();
    for s in seats {
        rows.entry(depth_of(s.0)).or_default().push(s);
    }
    let mut out = Vec::new();
    let mut depths: Vec<usize> = rows.keys().copied().collect();
    depths.sort_unstable();
    for (row, depth) in depths.iter().enumerate() {
        let mut members = rows.remove(depth).unwrap_or_default();
        members.sort_by(|a, b| (&a.1, &a.2, a.0).cmp(&(&b.1, &b.2, b.0)));
        for (col, s) in members.iter().enumerate() {
            out.push(Placed {
                seat: s.0,
                row,
                col,
            });
        }
    }
    out
}

/// What one agent gets of one pane, after every ceiling. Ordered, so "at least
/// read" is a comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reach {
    Hidden,
    Seen,
    Read,
    Ask,
}

impl From<Rung> for Reach {
    fn from(r: Rung) -> Self {
        match r {
            Rung::See => Self::Seen,
            Rung::Read => Self::Read,
            Rung::Ask => Self::Ask,
        }
    }
}

/// The largest copy of an agent's conversation that starts without a
/// person's press, unless the person set their own. Zero: every copy asks
/// first (Parker, on the War Room's build page, 2026-10-22). Gate 1 drew
/// 400,000, and the median Claude pane measured that day was about 360,000,
/// so most copies would have started unasked.
pub const ASK_PRESS_TOKENS: u64 = 0;

/// The rail's colour for a copy, by what it reads (Gate 2, figure 6): up to
/// this many tokens it is a caution.
pub const COPY_CAUTION_TOKENS: u64 = 600_000;

/// Up to this many tokens a copy is a warning; past it, an alert.
pub const COPY_WARNING_TOKENS: u64 = 1_000_000;

/// One side of a reach question: a pane as the rules see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Party {
    pub seat: Option<SeatId>,
    pub is_agent: bool,
    pub access: AgentAccess,
}

/// The window's switches alone: MCP on, and a plain shell only when the
/// window shows every pane.
pub fn policy_admits(cfg: &McpConfig, is_agent: bool) -> bool {
    cfg.enabled && (matches!(cfg.expose, Expose::All) || is_agent)
}

/// The most a pane's position lets any line reach.
fn ceiling(access: AgentAccess) -> Reach {
    match access {
        AgentAccess::Open | AgentAccess::ReadOnly => Reach::Ask,
        AgentAccess::OffLimits => Reach::Hidden,
    }
}

/// How far `caller` reaches `target`, in order: MCP off, then an off-limits
/// pane, then a shell the window does not show — each hidden. A pane nobody
/// seated is read and never asked. A seated pane is the caller's line to it,
/// capped by its position; no line, an unknown caller, or a rung word this
/// build cannot read is hidden. `own` (the caller IS the target) skips the
/// seat step so an agent never vanishes from its own census.
pub fn reach(
    cfg: &McpConfig,
    chart: &Chart,
    caller: Option<&Party>,
    target: &Party,
    own: bool,
) -> Reach {
    if !cfg.enabled || target.access.is_off_limits() || !policy_admits(cfg, target.is_agent) {
        return Reach::Hidden;
    }
    let Some(seat) = target.seat else {
        return Reach::Read;
    };
    if own {
        return Reach::Read;
    }
    let line = caller
        .and_then(|c| c.seat)
        .and_then(|from| chart.line(from, seat));
    match line.and_then(|l| l.rung.known()) {
        Some(rung) => Reach::from(rung).min(ceiling(target.access)),
        None => Reach::Hidden,
    }
}

/// What the rail's colour for an ask is read from (Gate 2, figure 6).
pub enum RiskFacts {
    /// A window-wide or pane change an agent asked for.
    Pane(crate::access::Change),
    /// A bypass line: to another project, into a read-only pane, and for how
    /// many prompts.
    Bypass {
        across_projects: bool,
        into_read_only: bool,
        prompts: u8,
    },
    /// A read-only copy that would read `reads` tokens (`None`: unknown).
    /// The colour is the size alone: the press limit decides whether a copy
    /// waits, not how serious it looks.
    Copy { reads: Option<u64> },
}

/// How serious an ask is, on the coordination panel's four-step scale: the
/// colour says how much could go wrong, not what kind of ask it is. Narrowing
/// access is clear; reading a little is caution; wider or costlier is a
/// warning; writes, or a copy reading more than [`COPY_WARNING_TOKENS`],
/// alert. An unknown size is never cheaper than a warning.
pub fn risk(facts: &RiskFacts) -> crate::coordination::Severity {
    use crate::access::Change;
    use crate::coordination::Severity::*;
    match facts {
        RiskFacts::Pane(change) => match change {
            Change::ClosePane => Clear,
            Change::EventsOn | Change::AskLine => Caution,
            Change::OpenPane | Change::ExposeShells => Warning,
            Change::WritesOn => Alert,
            // A copy is weighed by what it reads (`RiskFacts::Copy`); named
            // without a size it is never cheaper than an unknown one.
            Change::Copy => Warning,
        },
        RiskFacts::Bypass {
            across_projects,
            into_read_only,
            prompts,
        } => {
            if *across_projects || *into_read_only || *prompts > crate::access::BYPASS_PROMPTS {
                Warning
            } else {
                Caution
            }
        }
        RiskFacts::Copy { reads } => match reads {
            None => Warning,
            Some(n) if *n <= COPY_CAUTION_TOKENS => Caution,
            Some(n) if *n <= COPY_WARNING_TOKENS => Warning,
            Some(_) => Alert,
        },
    }
}

/// One ask's name in the record and, for a copy, in `ask_answer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AskId(pub u64);

/// Every way an ask is turned down, by name, so the asking agent and the
/// record both say why rather than "failed".
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Refusal {
    /// The window's MCP switch is off.
    McpOff,
    /// No line from the asker's seat reaches as far as ask — including an
    /// asker or a target nobody seated (no prompt without a line).
    NoLine,
    /// The target pane is off-limits to every agent.
    OffLimits,
    /// The target is not an agent: there is nothing to ask.
    NotAnAgent,
    /// The asker named itself.
    OwnPane,
    /// The question is empty or longer than [`QUESTION_MAX_CHARS`].
    BadQuestion,
    /// The asker or the target is not in this window.
    Gone,
    /// A copy was asked of a pane whose agent is not Claude or Codex.
    CannotCopy,
    /// The asker's earlier copy is still running or waiting for a person.
    Busy,
    /// The pane's agent has not named its conversation, so which one a copy
    /// would fork is a guess, and no copy is made of a guess.
    ConversationUnnamed,
    /// Another pane names the same conversation.
    ConversationShared,
    /// The copy came up with a tool beyond reading, or a tool server, or
    /// called one, and was stopped.
    ToolsWider,
    /// This version of the agent program has not been checked for read-only
    /// copies.
    UnprovenProgram,
    /// `ask_answer` named no copy ask this pane made.
    NoSuchAsk,
}

impl Refusal {
    /// The sentence the asking agent reads.
    pub fn why(&self) -> &'static str {
        match self {
            Self::McpOff => "MCP is switched off in this window",
            Self::NoLine => "no line from your seat reaches this pane as far as ask; only a person draws one, in the War Room, or grants a bypass on the rail",
            Self::OffLimits => "that pane is off-limits to agents",
            Self::NotAnAgent => "that pane is not running an agent",
            Self::OwnPane => "that is your own pane",
            Self::BadQuestion => "the question is empty or longer than 4,000 characters",
            Self::Gone => "the asking pane or the pane asked is not in this window",
            Self::CannotCopy => "a read-only copy can be made of a Claude or Codex agent only",
            Self::Busy => "your earlier copy is still running or waiting for a person; read it with ask_answer first",
            Self::ConversationUnnamed => "the agent in that pane has not named its conversation, so a copy would be of a guess",
            Self::ConversationShared => "another pane names the same conversation, so a copy could be of the wrong agent",
            Self::ToolsWider => "the copy came up able to do more than read, and was stopped before it answered",
            Self::UnprovenProgram => "this version of the agent program has not been checked for read-only copies",
            Self::NoSuchAsk => "no copy ask by that id was made from your pane",
        }
    }
}

/// Whether an ask goes through, and along what — `"line"` or `"bypass"` —
/// or which refusal stops it, checked in this order: the asker named itself,
/// a question that is empty or too long, MCP off, an off-limits pane, a pane
/// that is not an agent, and no line from the asker's seat reaching ask.
pub fn decide_ask(
    cfg: &McpConfig,
    chart: &Chart,
    caller: &Party,
    target: &Party,
    same_pane: bool,
    question_chars: usize,
) -> Result<&'static str, Refusal> {
    if same_pane {
        return Err(Refusal::OwnPane);
    }
    if question_chars == 0 || question_chars > QUESTION_MAX_CHARS {
        return Err(Refusal::BadQuestion);
    }
    if !cfg.enabled {
        return Err(Refusal::McpOff);
    }
    if target.access.is_off_limits() {
        return Err(Refusal::OffLimits);
    }
    if !target.is_agent {
        return Err(Refusal::NotAnAgent);
    }
    if reach(cfg, chart, Some(caller), target, false) < Reach::Ask {
        return Err(Refusal::NoLine);
    }
    let line = caller
        .seat
        .zip(target.seat)
        .and_then(|(f, t)| chart.line(f, t));
    Ok(if line.is_some_and(Line::is_bypass) {
        "bypass"
    } else {
        "line"
    })
}

/// The longest question an ask carries.
pub const QUESTION_MAX_CHARS: usize = 4_000;

/// What a person calls a pane: where its tab sits on the left bar, and the
/// title on the pane's own header. The header alone cannot do it — until an
/// agent names itself, every header reads "shell", and a list of twenty
/// agents reads as twenty identical rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PaneName {
    /// The left bar's levels above the tab, outermost first, joined by its
    /// own `›`: "TERMINAL DELIGHT", or "SERVER › auth work". Empty for an
    /// unfiled tab.
    pub outer: String,
    /// The tab's own name, as its row on the left bar reads.
    pub tab: String,
    /// The pane header's title. Empty when the header has none.
    pub header: String,
}

impl PaneName {
    /// From the left bar's levels and the header's title. A level with no
    /// words is left out rather than drawn as an empty `›`, and a header that
    /// only repeats the tab's name is not said twice.
    pub fn new(spine: &[String], header: &str) -> Self {
        let mut levels: Vec<&str> = spine
            .iter()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();
        let tab = levels.pop().unwrap_or_default();
        let header = header.trim();
        PaneName {
            outer: levels.join(" \u{203a} "),
            tab: tab.to_string(),
            header: if header == tab { "" } else { header }.to_string(),
        }
    }

    /// The whole place: "TERMINAL DELIGHT › MCP work".
    pub fn place(&self) -> String {
        match (self.outer.is_empty(), self.tab.is_empty()) {
            (false, false) => format!("{} \u{203a} {}", self.outer, self.tab),
            (false, true) => self.outer.clone(),
            (true, _) => self.tab.clone(),
        }
    }

    /// The place and the header on one line, for a row with room for one.
    pub fn full(&self) -> String {
        let place = self.place();
        match (place.is_empty(), self.header.is_empty()) {
            (false, false) => format!("{place} · {}", self.header),
            (false, true) => place,
            (true, _) => self.header.clone(),
        }
    }
}

/// What a seat shows, and what the cheap answer carries about it. Unknown is
/// `None` all the way to the wire — never a 0 size or an empty model.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SeatView {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// The agent counter's word for it: working, needs you, done, idle, or
    /// unreadable.
    pub state: String,
    pub model: Option<String>,
    /// The agent's current context in tokens: also what one copy of it would
    /// read on every model call.
    pub size: Option<u64>,
}

/// The agent's own latest summary for a person: its plain reading, and the
/// two-sentence brief when it wrote one.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Summary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
    pub layman: String,
}

/// The cheap answer: what the window can already show about the agent asked,
/// before anyone starts a copy of it (Gate 1, question 2). The protocol layer
/// adds the recent tool calls.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CheapAnswer {
    pub ask_id: AskId,
    pub seat: SeatView,
    pub summary: Option<Summary>,
    pub note: Option<String>,
    /// `line` or `bypass`: what let the ask through.
    pub via: String,
}

/// The window-side filter for a walk that cannot know who is asking (the
/// grep walk, the agent wall's count): the pane's position and the window's
/// switches only. The protocol layer narrows it further by the caller's lines.
pub fn visible_to_some_agent(cfg: &McpConfig, target: &Party) -> bool {
    !target.access.is_off_limits() && policy_admits(cfg, target.is_agent)
}

/// How far an agent nobody can name reaches `target`: what a feed serving
/// every connection at once (pushed events, the coordination view) may show.
/// A seated pane is hidden from it — lines name their callers, and this one
/// has none.
pub fn anonymous_reach(cfg: &McpConfig, target: &Party) -> Reach {
    reach(cfg, &Chart::default(), None, target, false)
}

/// Whether one of TD's own write verbs (a note, an appearance change, a
/// surface, bench input, a document action) may land on `target`. Another
/// agent needs at least read and an open position; the agent inside the pane
/// keeps its own verbs at every position, as long as the window admits it.
/// The writes switch is checked where it always was (`apply_permission`).
pub fn may_write(cfg: &McpConfig, reach: Reach, target: &Party, own: bool) -> bool {
    if own {
        return policy_admits(cfg, target.is_agent);
    }
    reach >= Reach::Read && target.access.takes_writes()
}

/// The effect-time write check, run on the UI thread against the LIVE chart.
///
/// A write that `write_gate` admitted lands later, on the ticker, by which time
/// the chart may have changed. This recomputes the decision from the sender's
/// *current* seat and the target's *current* party, so a line revoked while the
/// write was queued, or a reseat under the sender, stops the write. The sender is
/// identified live by its pane id (resolved to `caller_seat` at the effect); if
/// that pane is gone or unseated, `caller_seat` is `None` and reach collapses to
/// `Hidden` unless the write is to the sender's own pane (`own`). Authority in the
/// chart is per-pane-seat, so re-resolving the seat live is the whole fix:
/// nothing captured at request time is trusted but the pane identity.
pub fn may_write_live(
    cfg: &McpConfig,
    chart: &Chart,
    caller_seat: Option<SeatId>,
    target: &Party,
    own: bool,
) -> bool {
    let caller = caller_seat.map(|seat| Party {
        seat: Some(seat),
        is_agent: true,
        access: target.access,
    });
    let r = reach(cfg, chart, caller.as_ref(), target, own);
    may_write(cfg, r, target, own)
}

/// Whether a copy waits for a person's press on the rail: when it would read
/// more than their limit, or nobody knows how much it would read. A size of
/// zero is not a size, since every copy reads at least its own instructions,
/// so it waits like an unknown one, and a limit of zero means every copy asks.
pub fn copy_waits_for_press(reads: Option<u64>, limit: u64) -> bool {
    !matches!(reads, Some(n) if n > 0 && n <= limit)
}

/// How far a copy ask has got, on the wire as `stage` with what each stage
/// carries beside it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum CopyStage {
    /// Over the press limit, or of unknown size: waiting on the rail.
    AwaitingPress,
    Running,
    /// The copy's last word, and what answering read and cost as the agent
    /// program reported them (`None`: not reported, not zero).
    Answered {
        answer: String,
        tokens_read: Option<u64>,
        cost_usd: Option<f64>,
    },
    /// A person said no on the rail.
    Declined,
    /// Stopped by a check, before the copy could act.
    Refused {
        refusal: Refusal,
    },
    /// The program failed or ran out of time.
    Failed {
        why: String,
    },
}

impl CopyStage {
    /// Still waiting on a press or a copy.
    pub fn is_open(&self) -> bool {
        matches!(self, Self::AwaitingPress | Self::Running)
    }
}

/// What `ask_agent` with `copy` and `ask_answer` hand back.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CopyStatus {
    pub ask_id: AskId,
    /// What one copy of the agent reads per model call, in tokens: its
    /// current context. `None` is unknown.
    pub reads: Option<u64>,
    /// The person's press limit: a copy reading more, or an unknown amount,
    /// waits for their Yes.
    pub press_limit: u64,
    #[serde(flatten)]
    pub stage: CopyStage,
}

/// One copy ask, held by the window from the ask to its answer.
#[derive(Clone, Debug, PartialEq)]
pub struct CopyAsk {
    /// The asking pane's host pane id: the only caller `ask_answer` answers.
    pub asker_pane: u64,
    pub from: Option<SeatId>,
    pub to: Option<SeatId>,
    pub target_pid: u32,
    /// `line` or `bypass`.
    pub via: &'static str,
    pub reads: Option<u64>,
    pub model: Option<String>,
    /// Kept in memory until the copy is done with it, then dropped. Never
    /// written anywhere: the record keeps its length and fingerprint.
    pub question: String,
    pub question_len: usize,
    pub question_sha256: String,
    pub program: crate::askcopy::Program,
    /// The conversation it was priced on. A copy forks this one or none: if
    /// the pane holds another by the time it runs, it fails rather than read
    /// something nobody priced.
    pub conversation: String,
    pub stage: CopyStage,
}

/// How many copy asks the window remembers. Open ones are never let go.
pub const BOOK_KEEPS: usize = 32;

/// The window's copy asks, by id.
#[derive(Clone, Debug, Default)]
pub struct AskBook {
    asks: Vec<(AskId, CopyAsk)>,
}

impl AskBook {
    pub fn remove(&mut self, id: AskId) -> Option<CopyAsk> {
        let index = self.asks.iter().position(|(i, _)| *i == id)?;
        Some(self.asks.remove(index).1)
    }

    /// An explicit change to this directed relationship cancels its old asks.
    pub fn forget_pair(&mut self, from: SeatId, to: SeatId) {
        self.asks
            .retain(|(_, a)| a.from != Some(from) || a.to != Some(to));
    }

    /// Removal revokes undelivered answers as well as waiting copy requests.
    /// A running copy may finish, but its result can no longer enter the book.
    pub fn forget_seat(&mut self, seat: SeatId) {
        self.asks
            .retain(|(_, a)| a.from != Some(seat) && a.to != Some(seat));
    }

    pub fn open(&mut self, id: AskId, ask: CopyAsk) {
        self.asks.push((id, ask));
        while self.asks.len() > BOOK_KEEPS {
            match self.asks.iter().position(|(_, a)| !a.stage.is_open()) {
                Some(i) => {
                    self.asks.remove(i);
                }
                None => break,
            }
        }
    }

    /// Whether `asker_pane` already has a copy running or waiting.
    pub fn busy(&self, asker_pane: u64) -> bool {
        self.asks
            .iter()
            .any(|(_, a)| a.asker_pane == asker_pane && a.stage.is_open())
    }

    pub fn get(&self, id: AskId) -> Option<&CopyAsk> {
        self.asks.iter().find(|(i, _)| *i == id).map(|(_, a)| a)
    }

    /// The ask `id`, for the pane that made it and nobody else.
    pub fn of_asker(&self, id: AskId, asker_pane: u64) -> Option<&CopyAsk> {
        self.get(id).filter(|a| a.asker_pane == asker_pane)
    }

    /// Move an ask to `stage`. Once it is done the question is let go.
    pub fn set(&mut self, id: AskId, stage: CopyStage) -> Option<&CopyAsk> {
        let (_, a) = self.asks.iter_mut().find(|(i, _)| *i == id)?;
        if !stage.is_open() {
            a.question = String::new();
        }
        a.stage = stage;
        Some(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_org_migration_keeps_labels_unknown_and_seats_stable() {
        let legacy = [SeatId(7), SeatId(9), SeatId(7)];
        let orgs = Orgs::restore(None, &legacy);
        assert_eq!(orgs.saved().len(), 1);
        let org = &orgs.saved()[0];
        assert_eq!(org.name, None);
        assert_eq!(org.seats, vec![SeatId(7), SeatId(9)]);
        let saved = toml::to_string(org).unwrap();
        let restored: Org = toml::from_str(&saved).unwrap();
        assert_eq!(&restored, org);
        assert_eq!(
            Orgs::restore(Some(vec![restored]), &[]).owner(SeatId(7)),
            Some(org.id)
        );
        assert!(
            Orgs::restore(Some(vec![]), &legacy).saved().is_empty(),
            "declared empty is not a legacy layout"
        );
        assert!(Orgs::restore(None, &[]).saved().is_empty());
    }

    #[test]
    fn org_names_and_membership_have_stable_identity_and_validate_the_destination() {
        let mut orgs = Orgs::default();
        assert_eq!(orgs.create("  "), None);
        let first = orgs.create("Cinema").unwrap();
        let second = orgs.create("BFS").unwrap();
        assert!(orgs.assign(SeatId(1), first));
        assert!(!orgs.assign(SeatId(1), OrgId(0)));
        assert_eq!(orgs.owner(SeatId(1)), Some(first));
        assert!(orgs.rename(first, "Production"));
        assert!(!orgs.rename(first, " "));
        assert_eq!(orgs.get(first).unwrap().name.as_deref(), Some("Production"));
        assert_eq!(orgs.owner(SeatId(1)), Some(first));
        assert!(orgs.assign(SeatId(1), second));
        assert!(orgs.get(first).unwrap().seats.is_empty());
        assert_eq!(orgs.get(second).unwrap().seats, vec![SeatId(1)]);
        orgs.remove(SeatId(1));
        assert_eq!(orgs.owner(SeatId(1)), None);
    }

    #[test]
    fn a_relationship_needs_explicit_permission_and_two_members_of_the_same_org() {
        let mut orgs = Orgs::default();
        let a = orgs.create("A").unwrap();
        let b = orgs.create("B").unwrap();
        orgs.assign(SeatId(1), a);
        orgs.assign(SeatId(2), a);
        orgs.assign(SeatId(3), b);
        let mut draft = RelationDraft::default();
        assert_eq!(draft.ready(&orgs), None);
        draft.from = Some(SeatId(1));
        draft.to = Some(SeatId(2));
        assert_eq!(draft.ready(&orgs), None, "a drop is not permission");
        draft.rung = Some(Rung::Read);
        assert_eq!(draft.ready(&orgs), Some((SeatId(1), SeatId(2), Rung::Read)));
        draft.to = Some(SeatId(1));
        assert_eq!(draft.ready(&orgs), None, "no self link");
        draft.to = Some(SeatId(3));
        assert_eq!(draft.ready(&orgs), None, "no invisible cross-org grant");
        draft.to = Some(SeatId(99));
        assert_eq!(draft.ready(&orgs), None, "missing member");
        let ambiguous = Orgs::restore(
            Some(vec![
                Org {
                    id: OrgId(4),
                    name: None,
                    seats: vec![SeatId(1), SeatId(2)],
                },
                Org {
                    id: OrgId(5),
                    name: None,
                    seats: vec![SeatId(1)],
                },
            ]),
            &[],
        );
        draft.to = Some(SeatId(2));
        assert_eq!(
            draft.ready(&ambiguous),
            None,
            "ambiguous membership is unknown"
        );
    }

    fn on() -> McpConfig {
        McpConfig {
            enabled: true,
            ..McpConfig::default()
        }
    }

    fn agent(seat: Option<u64>, access: AgentAccess) -> Party {
        Party {
            seat: seat.map(SeatId),
            is_agent: true,
            access,
        }
    }

    fn line(chart: &mut Chart, from: u64, to: u64, rung: Rung) {
        chart.draw(SeatId(from), SeatId(to), rung);
    }

    /// The effect-time recheck follows the CURRENT chart and the sender's CURRENT
    /// seat, not anything captured when the write was queued: a revoked line, and
    /// a sender that has left its seat, both stop the write; a present line and
    /// the sender's own pane both let it through. This is the TOCTOU guard
    /// (issue #1133): breaking it to read a stale decision fails here.
    #[test]
    fn may_write_live_rechecks_the_current_chart_and_seat() {
        let cfg = on();
        let target = agent(Some(2), AgentAccess::Open);
        let sender = Some(SeatId(1));

        let mut with_line = Chart::default();
        line(&mut with_line, 1, 2, Rung::Read);
        assert!(
            may_write_live(&cfg, &with_line, sender, &target, false),
            "a current read line admits the write"
        );

        let revoked = Chart::default();
        assert!(
            !may_write_live(&cfg, &revoked, sender, &target, false),
            "a line revoked while the write was queued stops it at the effect"
        );
        assert!(
            !may_write_live(&cfg, &with_line, None, &target, false),
            "a sender that no longer holds a seat is refused, even with the line still drawn"
        );
        assert!(
            may_write_live(&cfg, &revoked, None, &target, true),
            "the sender's own pane writes without a line, whatever the chart says"
        );
    }

    #[test]
    fn mcp_off_hides_every_pane() {
        let off = McpConfig::default();
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        let caller = agent(Some(1), AgentAccess::Open);
        for target in [
            agent(None, AgentAccess::Open),
            agent(Some(2), AgentAccess::Open),
        ] {
            assert_eq!(
                reach(&off, &chart, Some(&caller), &target, false),
                Reach::Hidden
            );
        }
    }

    #[test]
    fn an_off_limits_pane_is_hidden_from_every_line() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        let caller = agent(Some(1), AgentAccess::Open);
        let target = agent(Some(2), AgentAccess::OffLimits);
        assert_eq!(
            reach(&on(), &chart, Some(&caller), &target, false),
            Reach::Hidden
        );
        let unseated = agent(None, AgentAccess::OffLimits);
        assert_eq!(
            reach(&on(), &chart, Some(&caller), &unseated, false),
            Reach::Hidden
        );
        assert_eq!(
            reach(&on(), &chart, Some(&caller), &unseated, true),
            Reach::Hidden,
            "even its own agent's census"
        );
    }

    #[test]
    fn a_plain_shell_follows_the_window_switch() {
        let chart = Chart::default();
        let shell = Party {
            seat: None,
            is_agent: false,
            access: AgentAccess::Open,
        };
        assert_eq!(reach(&on(), &chart, None, &shell, false), Reach::Hidden);
        let all = McpConfig {
            expose: Expose::All,
            ..on()
        };
        assert_eq!(reach(&all, &chart, None, &shell, false), Reach::Read);
    }

    #[test]
    fn a_seated_pane_without_a_line_is_hidden() {
        let chart = Chart::default();
        let caller = agent(Some(1), AgentAccess::Open);
        let target = agent(Some(2), AgentAccess::Open);
        assert_eq!(
            reach(&on(), &chart, Some(&caller), &target, false),
            Reach::Hidden
        );
        let unseated_caller = agent(None, AgentAccess::Open);
        assert_eq!(
            reach(&on(), &chart, Some(&unseated_caller), &target, false),
            Reach::Hidden
        );
        assert_eq!(
            reach(&on(), &chart, None, &target, false),
            Reach::Hidden,
            "an unknown caller"
        );
    }

    #[test]
    fn a_see_line_shows_a_seated_pane_and_nothing_more() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::See);
        let caller = agent(Some(1), AgentAccess::Open);
        let target = agent(Some(2), AgentAccess::Open);
        let r = reach(&on(), &chart, Some(&caller), &target, false);
        assert_eq!(r, Reach::Seen);
        assert!(!may_write(&on(), r, &target, false));
        assert_eq!(
            reach(&on(), &chart, Some(&target), &caller, false),
            Reach::Hidden,
            "a line runs one way"
        );
    }

    /// Mutation-tested: returning `Reach::Ask` for an unseated pane fails this.
    #[test]
    fn an_unseated_pane_is_read_and_never_asked() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        let caller = agent(Some(1), AgentAccess::Open);
        let target = agent(None, AgentAccess::Open);
        let r = reach(&on(), &chart, Some(&caller), &target, false);
        assert_eq!(r, Reach::Read);
        assert!(
            r < Reach::Ask,
            "no prompt without a line, even to an open pane"
        );
    }

    #[test]
    fn a_read_only_pane_caps_a_line_at_ask() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        let caller = agent(Some(1), AgentAccess::Open);
        let target = agent(Some(2), AgentAccess::ReadOnly);
        assert_eq!(
            reach(&on(), &chart, Some(&caller), &target, false),
            Reach::Ask
        );
    }

    #[test]
    fn an_unknown_rung_word_grants_nothing_and_survives_a_save() {
        let saved = r#"lines = [{ from = 1, to = 2, rung = "assign" }]"#;
        #[derive(Deserialize, Serialize)]
        struct F {
            lines: Vec<Line>,
        }
        let f: F = toml::from_str(saved).unwrap();
        let chart = Chart::from_saved(f.lines);
        let caller = agent(Some(1), AgentAccess::Open);
        let target = agent(Some(2), AgentAccess::Open);
        assert_eq!(
            reach(&on(), &chart, Some(&caller), &target, false),
            Reach::Hidden
        );
        let again = toml::to_string(&F {
            lines: chart.saved().to_vec(),
        })
        .unwrap();
        assert!(
            again.contains("rung = \"assign\""),
            "the word is kept as written: {again}"
        );
    }

    #[test]
    fn the_own_pane_is_never_lost_from_its_own_census() {
        let chart = Chart::default();
        let me = agent(Some(7), AgentAccess::ReadOnly);
        assert_eq!(reach(&on(), &chart, Some(&me), &me, true), Reach::Read);
        assert!(
            may_write(&on(), Reach::Read, &me, true),
            "the agent inside keeps its own verbs"
        );
    }

    #[test]
    fn may_write_refuses_a_read_only_pane_for_every_write_verb() {
        let cfg = on();
        for (access, other, own) in [
            (AgentAccess::Open, true, true),
            (AgentAccess::ReadOnly, false, true),
            (AgentAccess::OffLimits, false, true),
        ] {
            let target = agent(None, access);
            let r = reach(&cfg, &Chart::default(), None, &target, false);
            assert_eq!(
                may_write(&cfg, r, &target, false),
                other,
                "{access:?}: another agent"
            );
            assert_eq!(
                may_write(&cfg, r, &target, true),
                own,
                "{access:?}: its own agent"
            );
        }
        let shell = Party {
            seat: None,
            is_agent: false,
            access: AgentAccess::OffLimits,
        };
        assert!(
            !may_write(&cfg, Reach::Read, &shell, true),
            "a shell the window does not show"
        );
        assert!(
            !may_write(
                &McpConfig::default(),
                Reach::Read,
                &agent(None, AgentAccess::Open),
                true
            ),
            "MCP off is off"
        );
    }

    #[test]
    fn roles_come_from_lines() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        line(&mut chart, 2, 3, Rung::Ask);
        chart.grant_bypass(SeatId(1), SeatId(4), 3);
        assert_eq!(chart.role_of(SeatId(1)), Role::Orchestrator);
        assert_eq!(chart.role_of(SeatId(2)), Role::Manager);
        assert_eq!(chart.role_of(SeatId(3)), Role::Worker);
        assert_eq!(
            chart.role_of(SeatId(4)),
            Role::Alone,
            "a bypass never counts toward a role"
        );
        assert_eq!(chart.manager_of(SeatId(3)), Some(SeatId(2)));
        assert_eq!(chart.manager_of(SeatId(4)), None);
    }

    #[test]
    fn a_bypass_spends_one_prompt_and_vanishes_at_zero() {
        let mut chart = Chart::default();
        chart.grant_bypass(SeatId(1), SeatId(2), 2);
        assert_eq!(chart.spend(SeatId(1), SeatId(2)), Some(1));
        assert_eq!(chart.spend(SeatId(1), SeatId(2)), Some(0));
        assert!(chart.line(SeatId(1), SeatId(2)).is_none());
        line(&mut chart, 1, 3, Rung::Ask);
        assert_eq!(
            chart.spend(SeatId(1), SeatId(3)),
            None,
            "a drawn line spends nothing"
        );
        assert!(chart.line(SeatId(1), SeatId(3)).is_some());
    }

    #[test]
    fn a_bypass_never_shortens_a_drawn_ask_line() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        chart.grant_bypass(SeatId(1), SeatId(2), 3);
        assert!(!chart.line(SeatId(1), SeatId(2)).unwrap().is_bypass());
    }

    #[test]
    fn a_seat_cannot_draw_a_line_to_itself() {
        let mut chart = Chart::default();
        line(&mut chart, 5, 5, Rung::Ask);
        assert!(chart.saved().is_empty());
    }

    #[test]
    fn forgetting_a_seat_erases_its_lines_both_ways() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        line(&mut chart, 2, 3, Rung::Read);
        line(&mut chart, 1, 3, Rung::See);
        chart.forget_seat(SeatId(2));
        assert_eq!(chart.saved().len(), 1);
    }

    #[test]
    fn the_saved_position_reads_back_and_unknown_is_off_limits() {
        for a in [
            AgentAccess::Open,
            AgentAccess::ReadOnly,
            AgentAccess::OffLimits,
        ] {
            let (off, word) = a.to_saved();
            assert_eq!(AgentAccess::from_saved(off, word.as_deref()), a);
        }
        assert_eq!(
            AgentAccess::from_saved(false, Some("assignable")),
            AgentAccess::OffLimits
        );
        assert_eq!(
            AgentAccess::from_saved(true, Some(READ_ONLY_WORD)),
            AgentAccess::OffLimits,
            "off_limits wins"
        );
    }

    #[test]
    fn layout_puts_each_seat_a_row_under_its_manager_and_groups_projects() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        line(&mut chart, 1, 3, Rung::Ask);
        line(&mut chart, 3, 4, Rung::Read);
        let seats = vec![
            (SeatId(4), "server".to_string(), "auth".to_string()),
            (SeatId(3), "server".to_string(), "backend lead".to_string()),
            (SeatId(2), "docsite".to_string(), "docs lead".to_string()),
            (SeatId(1), "server".to_string(), "release lead".to_string()),
            (SeatId(9), "aaa".to_string(), "alone".to_string()),
        ];
        let placed = layout(&seats, &chart);
        let at = |s: u64| {
            placed
                .iter()
                .find(|p| p.seat == SeatId(s))
                .map(|p| (p.row, p.col))
        };
        assert_eq!(
            at(9),
            Some((0, 0)),
            "a seat with no manager is in the top row, by project"
        );
        assert_eq!(at(1), Some((0, 1)));
        assert_eq!(
            at(2),
            Some((1, 0)),
            "docsite sorts before server in the second row"
        );
        assert_eq!(at(3), Some((1, 1)));
        assert_eq!(at(4), Some((2, 0)));
    }

    #[test]
    fn layout_cuts_a_loop_of_lines() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        line(&mut chart, 2, 1, Rung::Ask);
        let seats = vec![
            (SeatId(1), String::new(), "a".into()),
            (SeatId(2), String::new(), "b".into()),
        ];
        let placed = layout(&seats, &chart);
        assert_eq!(placed.len(), 2, "both seats are drawn");
    }

    #[test]
    fn an_ask_is_refused_by_name_in_order() {
        let mut chart = Chart::default();
        line(&mut chart, 1, 2, Rung::Ask);
        line(&mut chart, 1, 3, Rung::Read);
        chart.grant_bypass(SeatId(1), SeatId(4), 3);
        let me = agent(Some(1), AgentAccess::Open);
        let worker = agent(Some(2), AgentAccess::Open);
        let decide = |cfg: &McpConfig, t: &Party, same: bool, q: usize| {
            decide_ask(cfg, &chart, &me, t, same, q)
        };
        assert_eq!(decide(&on(), &worker, false, 12), Ok("line"));
        assert_eq!(
            decide(&on(), &agent(Some(4), AgentAccess::Open), false, 12),
            Ok("bypass")
        );
        assert_eq!(decide(&on(), &worker, true, 12), Err(Refusal::OwnPane));
        assert_eq!(decide(&on(), &worker, false, 0), Err(Refusal::BadQuestion));
        assert_eq!(
            decide(&on(), &worker, false, QUESTION_MAX_CHARS + 1),
            Err(Refusal::BadQuestion)
        );
        assert_eq!(
            decide(&McpConfig::default(), &worker, false, 12),
            Err(Refusal::McpOff)
        );
        assert_eq!(
            decide(&on(), &agent(Some(2), AgentAccess::OffLimits), false, 12),
            Err(Refusal::OffLimits)
        );
        let shell = Party {
            seat: Some(SeatId(2)),
            is_agent: false,
            access: AgentAccess::Open,
        };
        assert_eq!(decide(&on(), &shell, false, 12), Err(Refusal::NotAnAgent));
        assert_eq!(
            decide(&on(), &agent(Some(3), AgentAccess::Open), false, 12),
            Err(Refusal::NoLine),
            "a read line does not reach ask"
        );
        assert_eq!(
            decide(&on(), &agent(None, AgentAccess::Open), false, 12),
            Err(Refusal::NoLine),
            "no prompt without a line"
        );
        assert_eq!(
            decide(&on(), &agent(Some(9), AgentAccess::Open), false, 12),
            Err(Refusal::NoLine)
        );
        assert_eq!(
            decide(&on(), &agent(Some(2), AgentAccess::ReadOnly), false, 12),
            Ok("line"),
            "read-only caps at ask"
        );
    }

    /// Every row of the Gate 2 risk table, in the order it was drawn.
    #[test]
    fn risk_matches_the_rail_table() {
        use crate::access::Change;
        use crate::coordination::Severity::*;
        let bypass = |a, r, p| {
            risk(&RiskFacts::Bypass {
                across_projects: a,
                into_read_only: r,
                prompts: p,
            })
        };
        let copy = |reads| risk(&RiskFacts::Copy { reads });
        assert_eq!(
            risk(&RiskFacts::Pane(Change::ClosePane)),
            Clear,
            "to put a pane off-limits"
        );
        assert_eq!(
            risk(&RiskFacts::Pane(Change::EventsOn)),
            Caution,
            "to receive pushed events"
        );
        assert_eq!(
            bypass(false, false, 3),
            Caution,
            "a bypass, three prompts, same project"
        );
        assert_eq!(
            copy(Some(600_000)),
            Caution,
            "a copy reading up to 600,000 tokens"
        );
        assert_eq!(
            risk(&RiskFacts::Pane(Change::OpenPane)),
            Warning,
            "to see a hidden pane"
        );
        assert_eq!(
            risk(&RiskFacts::Pane(Change::ExposeShells)),
            Warning,
            "or every plain shell"
        );
        assert_eq!(
            bypass(true, false, 3),
            Warning,
            "a bypass into another project"
        );
        assert_eq!(bypass(false, true, 3), Warning, "or a read-only pane");
        assert_eq!(
            bypass(false, false, 10),
            Warning,
            "or for more than three prompts"
        );
        assert_eq!(
            copy(Some(1_000_000)),
            Warning,
            "a copy reading up to 1,000,000 tokens"
        );
        assert_eq!(
            risk(&RiskFacts::Pane(Change::WritesOn)),
            Alert,
            "to turn writes on"
        );
        assert_eq!(
            copy(Some(1_000_001)),
            Alert,
            "a copy reading more than 1,000,000"
        );
    }

    #[test]
    fn an_unknown_copy_size_is_never_cheaper_than_warning() {
        let r = risk(&RiskFacts::Copy { reads: None });
        assert!(r >= crate::coordination::Severity::Warning, "{r:?}");
    }

    #[test]
    fn a_minted_seat_saves_in_toml() {
        #[derive(Serialize, Deserialize)]
        struct S {
            seat: SeatId,
        }
        for _ in 0..64 {
            let seat = SeatId::mint();
            assert!(seat.0 >= 1 && seat.0 <= i64::MAX as u64);
            let text = toml::to_string(&S { seat }).unwrap();
            let back: S = toml::from_str(&text).unwrap();
            assert_eq!(back.seat, seat);
        }
        assert_ne!(SeatId::mint(), SeatId::mint());
    }

    fn copy_ask(asker_pane: u64, stage: CopyStage) -> CopyAsk {
        CopyAsk {
            asker_pane,
            from: Some(SeatId(1)),
            to: Some(SeatId(2)),
            target_pid: 200,
            via: "line",
            reads: Some(1),
            model: None,
            question: "what is left?".into(),
            question_len: 13,
            question_sha256: "f".into(),
            program: crate::askcopy::Program::Claude,
            conversation: "a".into(),
            stage,
        }
    }

    #[test]
    fn a_copy_over_the_limit_or_of_unknown_size_waits_for_a_press() {
        assert!(
            !copy_waits_for_press(Some(400_000), 400_000),
            "at the limit runs"
        );
        assert!(copy_waits_for_press(Some(400_001), 400_000));
        assert!(
            copy_waits_for_press(None, 400_000),
            "unknown never runs unpressed"
        );
        assert!(copy_waits_for_press(None, u64::MAX));
        assert!(
            copy_waits_for_press(Some(0), 400_000),
            "a zero is a misreading, and waits"
        );
    }

    /// Parker, looking at row after row of "shell · CLAUDE": the name should be
    /// the left bar plus the pane's title.
    #[test]
    fn a_pane_is_named_by_its_place_on_the_left_bar_then_its_header() {
        let spine = |l: &[&str]| l.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let n = PaneName::new(
            &spine(&["TERMINAL DELIGHT", "MCP work"]),
            "✳ Per-pane MCP least privilege",
        );
        assert_eq!(n.place(), "TERMINAL DELIGHT › MCP work");
        // The tab's own name is kept apart, so a narrow box can keep it whole
        // and let the levels above it give way.
        let deep = PaneName::new(&spine(&["SERVER", "auth work", "backend lead"]), "shell");
        assert_eq!(
            (deep.outer.as_str(), deep.tab.as_str()),
            ("SERVER › auth work", "backend lead")
        );
        assert_eq!(
            n.full(),
            "TERMINAL DELIGHT › MCP work · ✳ Per-pane MCP least privilege"
        );
        // Two headers that both say "shell" are told apart by where they sit.
        let a = PaneName::new(&spine(&["GLOBAL", "OPEN"]), "shell");
        let b = PaneName::new(&spine(&["JEV", "judgement surface"]), "shell");
        assert_ne!(a.full(), b.full());
        // A level with no words is not drawn as an empty `›`, and a part
        // that is missing leaves the other standing alone.
        let g = PaneName::new(&spine(&["", "  ", "X Files"]), "  ");
        assert_eq!(g.place(), "X Files");
        assert_eq!(g.outer, "");
        assert_eq!(g.full(), "X Files");
        assert_eq!(PaneName::new(&[], "shell").full(), "shell");
        // A pane named after its tab is not named twice.
        let r = PaneName::new(&spine(&["SERVER", "release lead"]), "release lead");
        assert_eq!(r.full(), "SERVER › release lead");
        assert_eq!(r.header, "");
    }

    #[test]
    fn by_default_every_copy_waits_for_a_press() {
        for reads in [None, Some(0), Some(1), Some(5_000), Some(400_000)] {
            assert!(
                copy_waits_for_press(reads, ASK_PRESS_TOKENS),
                "{reads:?} started unasked"
            );
        }
    }

    #[test]
    fn a_copys_colour_does_not_follow_the_press_limit() {
        use crate::coordination::Severity::*;
        assert_eq!(risk(&RiskFacts::Copy { reads: Some(1) }), Caution);
        assert_eq!(
            risk(&RiskFacts::Copy {
                reads: Some(600_000)
            }),
            Caution
        );
        assert_eq!(
            risk(&RiskFacts::Copy {
                reads: Some(600_001)
            }),
            Warning
        );
    }

    #[test]
    fn ask_answer_only_answers_the_asker() {
        let mut book = AskBook::default();
        book.open(AskId(1), copy_ask(10, CopyStage::Running));
        assert!(book.of_asker(AskId(1), 10).is_some());
        assert!(book.of_asker(AskId(1), 11).is_none(), "another pane's ask");
        assert!(book.of_asker(AskId(2), 10).is_none(), "no such ask");
    }

    #[test]
    fn removed_seats_cannot_deliver_waiting_running_or_finished_copies() {
        for seat in [SeatId(1), SeatId(2)] {
            let mut book = AskBook::default();
            for (id, stage) in [
                CopyStage::AwaitingPress,
                CopyStage::Running,
                CopyStage::Answered {
                    answer: "private".into(),
                    tokens_read: Some(1),
                    cost_usd: None,
                },
            ]
            .into_iter()
            .enumerate()
            {
                book.open(AskId(id as u64), copy_ask(10, stage));
            }
            let mut unrelated = copy_ask(20, CopyStage::Running);
            unrelated.from = Some(SeatId(3));
            unrelated.to = Some(SeatId(4));
            book.open(AskId(9), unrelated);
            book.forget_seat(seat);
            for id in 0..3 {
                assert!(book.of_asker(AskId(id), 10).is_none());
                assert!(book
                    .set(
                        AskId(id),
                        CopyStage::Answered {
                            answer: "late".into(),
                            tokens_read: None,
                            cost_usd: None
                        }
                    )
                    .is_none());
            }
            assert!(!book.busy(10));
            assert!(book.busy(20));
        }
    }

    #[test]
    fn one_copy_at_a_time_per_asker_and_the_question_goes_when_done() {
        let mut book = AskBook::default();
        book.open(AskId(1), copy_ask(10, CopyStage::AwaitingPress));
        assert!(book.busy(10) && !book.busy(11));
        book.set(AskId(1), CopyStage::Running);
        assert!(book.busy(10));
        assert_eq!(book.get(AskId(1)).unwrap().question, "what is left?");
        book.set(AskId(1), CopyStage::Declined);
        assert!(!book.busy(10));
        assert!(
            book.get(AskId(1)).unwrap().question.is_empty(),
            "the words are let go"
        );
    }

    #[test]
    fn the_book_forgets_old_answers_but_never_an_open_ask() {
        let mut book = AskBook::default();
        book.open(AskId(0), copy_ask(1, CopyStage::Running));
        for i in 1..=(BOOK_KEEPS as u64 + 5) {
            book.open(AskId(i), copy_ask(2, CopyStage::Declined));
        }
        assert!(book.get(AskId(0)).is_some(), "the open ask is kept");
        assert!(book.get(AskId(1)).is_none(), "the oldest answer went");
        assert!(book.asks.len() <= BOOK_KEEPS);
    }

    #[test]
    fn a_copy_status_says_its_stage_by_name_with_what_it_carries() {
        let s = CopyStatus {
            ask_id: AskId(9),
            reads: None,
            press_limit: ASK_PRESS_TOKENS,
            stage: CopyStage::Refused {
                refusal: Refusal::ConversationUnnamed,
            },
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["stage"], "refused");
        assert_eq!(v["refusal"], "conversation_unnamed");
        assert_eq!(v["reads"], serde_json::Value::Null, "unknown, not 0");
    }
}
