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

// Seating, drawing lines, roles and the press limit are called by the War Room
// panel and by asks, which land in later slices of the same pull request
// (docs/plans/pane-agent-access/04-slices.md). This allowance goes with them;
// the slice that wires the last caller deletes it.
#![cfg_attr(not(test), allow(dead_code))]

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

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
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

/// The size above which a copy of an agent's conversation waits for a
/// person's press, unless the person set their own (Gate 1, question 3).
pub const ASK_PRESS_TOKENS: u64 = 400_000;

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

/// The position-only half of [`may_write`], for the window's apply path, which
/// re-checks a write after the protocol layer has already checked the reach.
pub fn position_takes_writes(cfg: &McpConfig, target: &Party, own: bool) -> bool {
    if own {
        return policy_admits(cfg, target.is_agent);
    }
    policy_admits(cfg, target.is_agent) && target.access.takes_writes()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(
                position_takes_writes(&cfg, &target, false),
                other,
                "{access:?}: apply path"
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
        assert!(chart.is_empty());
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
}
