//! When an idle agent rests, when it sleeps, and why it refuses to.
//!
//! Parker, 2026-10-26: *"Idle Agents take up a lot of memory … sometimes it's
//! upwards of a week for me."* Measured that afternoon: six Claude agents
//! untouched for four to seven days still held 1.6 GB between them, and an
//! idle Claude is never asleep — it wakes about once a second and pulls its
//! own pages back out of swap, so the kernel cannot keep it out of RAM alone.
//!
//! Two states, both approved at Gate 1 of `docs/plans/idle-agents/`:
//!
//! - **Rest** freezes the pane's memory group and pushes its memory out. The
//!   process is still there, so nothing is lost; a keystroke shows within a
//!   fifth of a second (191–212 ms measured after 314 MB went to swap).
//! - **Sleep** ends the agent with SIGTERM and keeps its conversation; a key in
//!   the pane brings it back with the same launch line plus `--resume` for
//!   Claude, `resume <id>` for Codex (1.4 s to a prompt for a 20 MB Claude
//!   conversation). It loses what lives only in the process — background
//!   shells, session grants, a self-paced loop — so it refuses wherever one of
//!   those might exist. A Codex sleeps only when it runs its own server: one
//!   attached to a shared Codex service would free almost nothing.
//!
//! This module is the part with no I/O: the thresholds, the reasons to refuse,
//! the slider's scale, and the launch line a sleeper wakes with. Every input is
//! an `Option` where it can be unknown, and **unknown is never idle**: a fact
//! the window could not read refuses rest as surely as a busy agent does.

use std::time::Duration;

/// How long a pane must be untouched and quiet before it rests, and how long it
/// must have rested before it sleeps. Gate 1: ten minutes; two hours of rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thresholds {
    pub rest_after: Duration,
    pub sleep_after_rest: Duration,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            rest_after: Duration::from_secs(10 * 60),
            sleep_after_rest: Duration::from_secs(2 * 60 * 60),
        }
    }
}

/// Which program the pane is running, as far as rest and sleep care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

/// What Claude Code writes about itself in `~/.claude/sessions/<pid>.json`.
/// The status words seen on 2026-10-26: `idle`, `busy`, `waiting`, `shell`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SelfReport {
    pub status: Option<String>,
    /// When the status last changed, in Unix milliseconds.
    pub since_ms: Option<u64>,
    pub session_id: Option<String>,
}

/// Parse a Claude Code session registry file.
pub fn parse_self_report(text: &str, pid: i32) -> Option<SelfReport> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    // The file is named by pid; a file that names another pid is stale.
    if v.get("pid").and_then(|p| p.as_i64()) != Some(i64::from(pid)) {
        return None;
    }
    Some(SelfReport {
        status: v.get("status").and_then(|s| s.as_str()).map(str::to_owned),
        since_ms: v.get("statusUpdatedAt").and_then(|s| s.as_u64()),
        session_id: v
            .get("sessionId")
            .and_then(|s| s.as_str())
            .filter(|s| crate::session::safe_resume_id(s))
            .map(str::to_owned),
    })
}

/// Read the self-report of the Claude Code process `pid`.
pub fn self_report(pid: i32) -> Option<SelfReport> {
    let home = std::env::var_os("HOME")?;
    let path = std::path::Path::new(&home)
        .join(".claude/sessions")
        .join(format!("{pid}.json"));
    parse_self_report(&std::fs::read_to_string(path).ok()?, pid)
}

impl SelfReport {
    /// How long the agent has said it is idle, at `now_ms`. Any status other
    /// than `idle`, or no timestamp, is `None` — not idle, or not known to be.
    pub fn idle_for(&self, now_ms: u64) -> Option<Duration> {
        (self.status.as_deref() == Some("idle"))
            .then_some(self.since_ms?)
            .map(|since| Duration::from_millis(now_ms.saturating_sub(since)))
    }
}

/// A process in a pane's group that is not the agent, one of its helpers, or
/// the pane's shell — the thing rest would freeze along with the agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreign {
    /// A command the agent is running, e.g. a background Bash shell.
    Command { pid: i32, name: String },
    /// A process serving a network port, which freezing would hang.
    Listener { pid: i32, name: String },
    /// A job of the pane's shell that the agent did not start.
    ShellJob { pid: i32, name: String },
}

/// One process of a pane's group, as [`foreign`] needs to see it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub pid: i32,
    pub ppid: i32,
    pub argv: Vec<String>,
    /// Whether it holds a listening TCP socket.
    pub listens: bool,
    /// Its session id (`/proc/<pid>/stat` field 6), `None` when unreadable.
    pub session: Option<i32>,
}

/// Is `argv` one of Claude Code's Bash tool shells? They run as
/// `bash -c source ~/.claude/shell-snapshots/<…> && …`.
fn is_tool_shell(argv: &[String]) -> bool {
    argv.iter().any(|a| a.contains("/.claude/shell-snapshots/"))
}

/// Did `p` start a session of its own? An agent's commands do and its helpers
/// do not. Measured 2026-10-27 on every agent pane in the live window: each MCP
/// helper, Claude's and Codex's, shared its agent's session and terminal, while
/// Claude's Bash tool shells and the command behind a Codex "background
/// terminal" (one Codex keeps running after its turn ends, a direct child of
/// the Codex window) each led a new session with no terminal. A session that
/// cannot be read counts as one: unknown is never idle.
fn leads_own_session(p: &Proc) -> bool {
    p.session.is_none_or(|s| s == p.pid)
}

/// Where one process of a pane's group stands for rest.
enum Kind {
    /// The pane's shell or its agent: what rest freezes on purpose.
    Itself,
    /// One of the agent's helpers, frozen along with it.
    Helper,
    Foreign(Foreign),
}

/// Classify `p`. `shell` is the pane's shell, `agent` the agent it runs.
/// Helpers are the agent's descendants that are not commands (MCP servers and
/// what they start). Anything else is foreign: a command the agent ran (a tool
/// shell, or anything that leads its own session, and their children), a job
/// of the shell, any process holding a listening port.
fn kind(procs: &[Proc], shell: i32, agent: i32, p: &Proc) -> Kind {
    let parent = |pid: i32| procs.iter().find(|q| q.pid == pid).map(|q| q.ppid);
    let by_pid = |pid: i32| procs.iter().find(|q| q.pid == pid);
    let name = |p: &Proc| {
        p.argv
            .first()
            .map(|a| a.rsplit('/').next().unwrap_or(a).to_owned())
            .unwrap_or_default()
    };
    let (pid, name) = (p.pid, name(p));
    if p.pid == shell || p.pid == agent {
        if p.listens && p.pid == shell {
            return Kind::Foreign(Foreign::Listener { pid, name });
        }
        return Kind::Itself;
    }
    if p.listens {
        return Kind::Foreign(Foreign::Listener { pid, name });
    }
    // Walk up to the agent, the shell, or the edge of the group.
    let mut cur = p.pid;
    let mut under_agent = false;
    let mut a_command = false;
    for _ in 0..64 {
        if let Some(q) = by_pid(cur) {
            a_command |= is_tool_shell(&q.argv) || leads_own_session(q);
        }
        match parent(cur) {
            Some(pp) if pp == agent => {
                under_agent = true;
                break;
            }
            Some(pp) if pp == shell => break,
            Some(pp) => cur = pp,
            None => break,
        }
    }
    if !under_agent {
        Kind::Foreign(Foreign::ShellJob { pid, name })
    } else if a_command {
        Kind::Foreign(Foreign::Command { pid, name })
    } else {
        Kind::Helper
    }
}

/// The processes of a pane's group that rest must not freeze; see [`kind`].
pub fn foreign(procs: &[Proc], shell: i32, agent: i32) -> Vec<Foreign> {
    procs
        .iter()
        .filter_map(|p| match kind(procs, shell, agent, p) {
            Kind::Foreign(f) => Some(f),
            Kind::Itself | Kind::Helper => None,
        })
        .collect()
}

/// How many helpers the agent has of its own. A Codex window running its own
/// server has its MCP helpers as children; one attached to a shared Codex
/// service has none, because the service starts them.
pub fn helpers(procs: &[Proc], shell: i32, agent: i32) -> usize {
    procs
        .iter()
        .filter(|p| matches!(kind(procs, shell, agent, p), Kind::Helper))
        .count()
}

/// Everything rest and sleep decide on, read in one look at a pane.
#[derive(Debug, Clone, Default)]
pub struct Look {
    pub enabled: bool,
    pub agent: Option<Agent>,
    /// The pane holds the keyboard focus.
    pub focused: bool,
    /// The person pinned this pane awake.
    pub keep_awake: bool,
    /// Since the person last typed, clicked or focused this pane.
    pub since_touch: Option<Duration>,
    /// Since the pane last printed anything.
    pub since_output: Option<Duration>,
    /// Claude: how long it has reported itself idle (see [`SelfReport::idle_for`]).
    pub said_idle_for: Option<Duration>,
    /// Whether the screen shows the agent at its prompt: `Some(false)` when it
    /// shows it working, asking or failing; `None` when the screen could not be
    /// read. Codex rests on this alone; for Claude it can only refuse.
    pub screen_at_prompt: Option<bool>,
    /// Processes in the pane's group that freezing would catch.
    pub foreign: Option<Vec<Foreign>>,
    /// Whether the agent has helpers of its own (see [`helpers`]). For Codex
    /// this says it runs its own server, so ending it frees that server too.
    pub owns_helpers: Option<bool>,
    /// The group's CPU over the last rest window, as a share of one core.
    pub cpu_share: Option<f64>,
}

/// Why a pane did not rest or sleep. Every refusal is counted and shown, so it
/// can be seen and argued with.
#[derive(Debug, Clone, PartialEq)]
pub enum Refusal {
    Off,
    NotAnAgent,
    Focused,
    KeptAwake,
    Touched,
    Printing,
    /// The agent says it is not idle (its own word), or has said so too briefly.
    AgentBusy(Option<String>),
    /// The screen does not show the agent at its prompt.
    NotAtPrompt,
    Running(Foreign),
    /// The group used more CPU than an idle agent does.
    UsingCpu,
    /// A fact rest depends on could not be read.
    Unknown(&'static str),
    /// Sleep only: not rested long enough yet.
    NotRestedLongEnough,
    /// Sleep only: this Codex window is attached to a shared Codex service,
    /// which holds its helpers and its work; ending the window would save
    /// almost nothing. A Codex running its own server may sleep.
    CodexKeepsItsHelpers,
    /// Sleep only: the agent's conversation id is not known for certain, or
    /// Claude has not saved that conversation, so `--resume` could not open it.
    NoConversation,
    /// Sleep only: the launch line cannot be rebuilt exactly.
    LaunchLine(String),
}

/// The kinds of refusal a person is shown, counted on the Agents page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefusalClass {
    /// Working, asking, printing, or using CPU.
    Busy,
    /// Touched or looked at inside the window.
    Touched,
    /// Running a command or serving a port under the agent.
    Running,
    KeptAwake,
    /// A fact could not be read, so it was left alone.
    Unknown,
    Off,
}

impl Refusal {
    /// The kind this refusal is counted as; `None` for the ones that are not
    /// a refusal to a person (a shell pane, a sleep that is simply not due).
    pub fn class(&self) -> Option<RefusalClass> {
        Some(match self {
            Refusal::Off => RefusalClass::Off,
            Refusal::Focused | Refusal::Touched => RefusalClass::Touched,
            Refusal::KeptAwake => RefusalClass::KeptAwake,
            Refusal::Printing
            | Refusal::AgentBusy(_)
            | Refusal::NotAtPrompt
            | Refusal::UsingCpu => RefusalClass::Busy,
            Refusal::Running(_) => RefusalClass::Running,
            Refusal::Unknown(_) | Refusal::NoConversation | Refusal::LaunchLine(_) => {
                RefusalClass::Unknown
            }
            Refusal::NotAnAgent | Refusal::NotRestedLongEnough | Refusal::CodexKeepsItsHelpers => {
                return None;
            }
        })
    }
}

/// An idle Claude uses 0.5–1.2% of a core (90 s samples of ten agents); a
/// group above this share is doing something besides waiting.
pub const IDLE_CPU_CEILING: f64 = 0.03;

/// May this pane rest now?
pub fn may_rest(look: &Look, t: &Thresholds) -> Result<(), Refusal> {
    if !look.enabled {
        return Err(Refusal::Off);
    }
    let agent = look.agent.ok_or(Refusal::NotAnAgent)?;
    if look.focused {
        return Err(Refusal::Focused);
    }
    if look.keep_awake {
        return Err(Refusal::KeptAwake);
    }
    let quiet = |d: Option<Duration>, what: &'static str, short: Refusal| match d {
        None => Err(Refusal::Unknown(what)),
        Some(d) if d < t.rest_after => Err(short),
        Some(_) => Ok(()),
    };
    quiet(look.since_touch, "last touch", Refusal::Touched)?;
    quiet(look.since_output, "last output", Refusal::Printing)?;
    match agent {
        // Claude's own word decides, and a screen that contradicts it (working,
        // asking, an error) refuses: two witnesses, either can say no.
        Agent::Claude => {
            if look.screen_at_prompt == Some(false) {
                return Err(Refusal::NotAtPrompt);
            }
            match look.said_idle_for {
                Some(d) if d >= t.rest_after => {}
                _ => return Err(Refusal::AgentBusy(None)),
            }
        }
        Agent::Codex => match look.screen_at_prompt {
            Some(true) => {}
            Some(false) => return Err(Refusal::NotAtPrompt),
            None => return Err(Refusal::Unknown("screen")),
        },
    }
    match &look.foreign {
        None => return Err(Refusal::Unknown("processes")),
        Some(list) => {
            if let Some(first) = list.first() {
                return Err(Refusal::Running(first.clone()));
            }
        }
    }
    match look.cpu_share {
        None => Err(Refusal::Unknown("cpu")),
        Some(share) if share > IDLE_CPU_CEILING => Err(Refusal::UsingCpu),
        Some(_) => Ok(()),
    }
}

/// May a pane that has rested for `rested` now sleep? `wake` is the launch line
/// it would wake with, or the reason there is none.
pub fn may_sleep(
    look: &Look,
    rested: Duration,
    t: &Thresholds,
    wake: &Result<Vec<String>, String>,
) -> Result<(), Refusal> {
    match look.agent {
        Some(Agent::Claude) => {}
        // Running its own server (its helpers are its children), ending it
        // frees all of it. Attached to a shared service, nothing comes back.
        Some(Agent::Codex) if look.owns_helpers == Some(true) => {}
        Some(Agent::Codex) => return Err(Refusal::CodexKeepsItsHelpers),
        None => return Err(Refusal::NotAnAgent),
    }
    if rested < t.sleep_after_rest {
        return Err(Refusal::NotRestedLongEnough);
    }
    // Everything that kept it resting must still hold.
    let mut look = look.clone();
    look.focused = false;
    may_rest(&look, t)?;
    match wake {
        Ok(_) => Ok(()),
        Err(why) => Err(Refusal::LaunchLine(why.clone())),
    }
}

// ---------------------------------------------------------------------------
// What a pane carries.

/// A sleeping agent: what it was, and how it comes back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Asleep {
    /// When it fell asleep, in Unix milliseconds.
    pub since_ms: u64,
    /// The conversation it will resume, as the agent itself reported it.
    pub session: String,
    /// The script holding its exact launch line plus `--resume`.
    pub wake_file: std::path::PathBuf,
    /// The model it was launched with, when its launch line named one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Which harness it ran, `CLAUDE` or `CODEX`, read from its launch line
    /// when it fell asleep. `None` for a sleep recorded before this was kept,
    /// or a launch line naming neither: the Floor then says AGENT, not a guess.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
}

/// An agent the sleep signalled that had not exited by the sleep's own
/// deadline. The sleep cannot wait longer without holding up every other
/// pane's look, and an agent that ends a moment later has still been ended by
/// it: before this, that agent was gone and its pane was never marked asleep,
/// a plain shell nothing would resume (rig 5, 2026-10-27, issue 1174). So the
/// pane keeps the sleep it will become, and each look asks again.
#[derive(Debug, Clone, PartialEq)]
pub struct Stopping {
    pub pid: i32,
    /// The process's start time in clock ticks (`/proc/<pid>/stat` field
    /// 22), read before the signal: with the pid, the process's identity, so
    /// a pid the kernel has since handed to somebody else is not waited on.
    pub start: Option<u64>,
    pub asleep: Asleep,
    /// When the agent was signalled.
    pub since: std::time::Instant,
}

/// How long a signalled agent may take to go before the sleep gives up on it
/// and the pane is awake again. An agent still running a minute after SIGTERM
/// has chosen not to stop, and is never killed harder (a SIGKILL could cut a
/// transcript write).
pub const STOP_GIVE_UP: std::time::Duration = std::time::Duration::from_secs(60);

/// What a look at a signalled agent decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopLook {
    /// It has gone: the pane is asleep.
    Asleep,
    /// It has gone, and another agent already runs in the pane's shell,
    /// started by a person in the moment between: that pane is awake, and is
    /// never marked asleep over the new agent.
    Replaced,
    /// Still going; look again next time.
    Stopping,
    /// Still running past [`STOP_GIVE_UP`]: it is awake.
    GaveUp,
}

pub fn stop_look(exited: bool, replaced: bool, since_signal: std::time::Duration) -> StopLook {
    if exited && replaced {
        StopLook::Replaced
    } else if exited {
        StopLook::Asleep
    } else if since_signal >= STOP_GIVE_UP {
        StopLook::GaveUp
    } else {
        StopLook::Stopping
    }
}

/// Where a pane is between awake and asleep.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Dormancy {
    #[default]
    Awake,
    /// Frozen by this window's gate, its memory pushed out.
    Resting { since: std::time::Instant },
    /// The agent has ended; the pane waits for a key to bring it back.
    Asleep(Asleep),
}

impl Dormancy {
    pub fn is_resting(&self) -> bool {
        matches!(self, Dormancy::Resting { .. })
    }
    pub fn asleep(&self) -> Option<&Asleep> {
        match self {
            Dormancy::Asleep(a) => Some(a),
            _ => None,
        }
    }
}

/// Everything a pane holds for rest and sleep. Grouped in one field so the
/// pane view gains one member, not a dozen.
#[derive(Debug)]
pub struct PaneRest {
    /// Shared with the pane's byte stream: every write thaws through it.
    pub gate: std::sync::Arc<crate::panegroup::Gate>,
    /// The pane's own memory group, once adopted.
    pub group: Option<crate::panegroup::PaneGroup>,
    /// Why adoption failed, when it did — shown, never treated as zero.
    pub group_error: Option<String>,
    /// The last time the person typed, pasted, clicked or focused here. Starts
    /// at the pane's creation: a window that has just arrived has not seen a
    /// touch, and assuming one only delays rest.
    pub last_touch: std::time::Instant,
    /// The last time the screen changed.
    pub last_output: std::time::Instant,
    pub seen_generation: u64,
    /// The gate's write count at the last look; a change is a touch.
    pub seen_writes: u64,
    pub dormancy: Dormancy,
    /// The person pinned this pane awake.
    pub keep_awake: bool,
    /// The person asked for this agent to sleep now: the next tick skips the
    /// clocks, never the refusals.
    pub sleep_now: bool,
    /// Why the last look did not rest or sleep it.
    pub refusal: Option<Refusal>,
    /// An agent the sleep signalled that has not gone yet.
    pub stopping: Option<Stopping>,
    /// The group's memory at the last look.
    pub usage: crate::panegroup::Usage,
    /// What its processes held in RAM when it last went to rest.
    pub rest_from: Option<u64>,
    /// The previous CPU reading, for a share over the window.
    pub cpu_prev: Option<(std::time::Instant, u64)>,
}

impl PaneRest {
    pub fn new(now: std::time::Instant) -> Self {
        Self {
            gate: Default::default(),
            group: None,
            group_error: None,
            last_touch: now,
            last_output: now,
            seen_generation: 0,
            seen_writes: 0,
            dormancy: Dormancy::Awake,
            keep_awake: false,
            sleep_now: false,
            refusal: None,
            stopping: None,
            usage: Default::default(),
            rest_from: None,
            cpu_prev: None,
        }
    }

    /// A person did something to this pane.
    pub fn touch(&mut self, now: std::time::Instant) {
        self.last_touch = now;
        if self.dormancy.is_resting() {
            self.gate.release();
            self.dormancy = Dormancy::Awake;
        }
    }

    /// Note the screen's generation; a change is output.
    pub fn saw_generation(&mut self, generation: u64, now: std::time::Instant) {
        if generation != self.seen_generation {
            self.seen_generation = generation;
            self.last_output = now;
        }
    }

    /// Anything written to the pane — a paste, a mouse report, a bench line —
    /// counts as a touch. Some writes are the program's own (a colour reply),
    /// and counting those can only delay rest, never cause it.
    pub fn saw_writes(&mut self, now: std::time::Instant) {
        let writes = self.gate.writes();
        if writes != self.seen_writes {
            self.seen_writes = writes;
            self.last_touch = now;
        }
    }

    /// Observed RAM decrease from this pane's last before-rest reading.
    /// Sleeping keeps the same shell and comparison; waking a new pane starts
    /// with no baseline. This is a measured decrease, not a counterfactual.
    pub fn given_back(&self) -> Option<u64> {
        if matches!(self.dormancy, Dormancy::Awake) {
            return None;
        }
        Some(self.rest_from?.saturating_sub(self.usage.resident?))
    }

    /// Input thawed the pane on the pump thread; make the pane say so.
    pub fn absorb_wake(&mut self, now: std::time::Instant) -> bool {
        if self.gate.take_woke() {
            self.last_touch = now;
            if self.dormancy.is_resting() {
                self.dormancy = Dormancy::Awake;
            }
            return true;
        }
        false
    }
}

/// A subtotal paired with its coverage. `None` means no value was measured;
/// a partial subtotal never stands alone as the whole population's memory.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MemoryTotal {
    pub bytes: Option<u64>,
    pub measured: u32,
    pub panes: u32,
}

impl MemoryTotal {
    pub fn add(&mut self, value: Option<u64>) {
        self.panes += 1;
        if let Some(value) = value {
            self.bytes = if self.measured == 0 {
                Some(value)
            } else {
                self.bytes.and_then(|sum| sum.checked_add(value))
            };
            self.measured += 1;
        }
    }

    /// Coverage travels with the number, including a measured zero. An empty
    /// population is a dash; a population with no readings is unmeasured.
    pub fn label(&self, unmeasured: &str) -> String {
        if self.panes == 0 {
            return "—".into();
        }
        let value = self
            .bytes
            .map(short_bytes)
            .unwrap_or_else(|| unmeasured.into());
        format!("{value} ({}/{})", self.measured, self.panes)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryRow {
    pub ram: MemoryTotal,
    pub swap: MemoryTotal,
}

impl MemoryRow {
    fn add(&mut self, usage: crate::panegroup::Usage) {
        self.ram.add(usage.resident);
        self.swap.add(usage.swapped);
    }

    pub fn panes(&self) -> u32 {
        self.ram.panes
    }
}

/// Cached pane measurements grouped by dormancy. Summing this structure does
/// no I/O and cannot thaw a pane. Shared services outside pane scopes are not
/// assigned to individual agents.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryCensus {
    pub awake: MemoryRow,
    pub resting: MemoryRow,
    pub asleep: MemoryRow,
    pub held: MemoryTotal,
    pub decrease: MemoryTotal,
}

impl MemoryCensus {
    pub fn add(&mut self, rest: &PaneRest) {
        let row = match rest.dormancy {
            Dormancy::Awake => &mut self.awake,
            Dormancy::Resting { .. } => &mut self.resting,
            Dormancy::Asleep(_) => &mut self.asleep,
        };
        row.add(rest.usage);
        self.held.add(
            rest.usage
                .resident
                .zip(rest.usage.swapped)
                .and_then(|(ram, swap)| ram.checked_add(swap)),
        );
        if !matches!(rest.dormancy, Dormancy::Awake) {
            self.decrease.add(rest.given_back());
        }
    }
}

/// Binary memory units, matching the kernel's KiB-based readings.
pub fn short_bytes(b: u64) -> String {
    const K: f64 = 1024.0;
    let n = b as f64;
    if b < 1024 {
        format!("{b} B")
    } else if n < K * K {
        format!("{:.0} KiB", n / K)
    } else if n < 10. * K * K {
        format!("{:.1} MiB", n / K / K)
    } else if n < K * K * K {
        format!("{:.0} MiB", n / K / K)
    } else {
        format!("{:.1} GiB", n / K / K / K)
    }
}

/// How long, as a header reads it: `40s`, `14m`, `2h`, `3d`.
pub fn short_age(d: Duration) -> String {
    let s = d.as_secs();
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

/// The CPU share of one core between two `usage_usec` readings.
pub fn cpu_share(prev: (std::time::Instant, u64), now: (std::time::Instant, u64)) -> Option<f64> {
    let wall = now.0.checked_duration_since(prev.0)?.as_secs_f64();
    if wall < 1.0 {
        return None;
    }
    Some(now.1.saturating_sub(prev.1) as f64 / 1e6 / wall)
}

/// The kill switch: `TD_AGENT_REST=0` turns the whole feature off, groups
/// included.
pub fn enabled_by_env() -> bool {
    std::env::var("TD_AGENT_REST").map_or(true, |v| v != "0")
}

// ---------------------------------------------------------------------------
// The launch line a sleeper wakes with.

/// Claude Code 2.1.296 options that take a value (`claude --help`).
const VALUE_FLAGS: &[&str] = &[
    "--agent",
    "--agents",
    "--append-system-prompt",
    "--append-system-prompt-file",
    "--autocompact",
    "--debug-file",
    "--effort",
    "--environment",
    "--fallback-model",
    "--input-format",
    "--json-schema",
    "--max-budget-usd",
    "--model",
    "-n",
    "--name",
    "--output-format",
    "--permission-mode",
    "--permission-prompts",
    "--permission-prompt-tool",
    "--plugin-dir",
    "--plugin-url",
    "--remote-control-session-name-prefix",
    "--setting-sources",
    "--settings",
    "--system-prompt",
    "--system-prompt-snapshot",
];
/// Options that take every following word up to the next option.
const LIST_FLAGS: &[&str] = &[
    "--add-dir",
    "--allowedTools",
    "--allowed-tools",
    "--betas",
    "--disallowedTools",
    "--disallowed-tools",
    "--file",
    "--mcp-config",
    "--tools",
];
/// Options whose value is optional.
const OPTIONAL_VALUE_FLAGS: &[&str] = &[
    "--cloud",
    "-d",
    "--debug",
    "--prompt-suggestions",
    "--remote-control",
];
/// Plain switches.
const SWITCHES: &[&str] = &[
    "--allow-dangerously-skip-permissions",
    "--ax-screen-reader",
    "--bare",
    "--brief",
    "--chrome",
    "--dangerously-skip-permissions",
    "--desktop",
    "--disable-slash-commands",
    "--exclude-dynamic-system-prompt-sections",
    "--forward-subagent-text",
    "--ide",
    "--include-hook-events",
    "--include-partial-messages",
    "--no-chrome",
    "--replay-user-messages",
    "--restricted",
    "--safe-mode",
    "--strict-mcp-config",
    "--tmux",
];
/// Options naming which conversation to open; the wake line replaces them.
const RESUME_SWITCHES: &[&str] = &["-c", "--continue", "--fork-session"];
const RESUME_OPTIONAL: &[&str] = &["-r", "--resume", "--from-pr", "--teleport"];
const RESUME_VALUE: &[&str] = &["--session-id"];
/// Options that make a run something other than an interactive pane.
const NOT_INTERACTIVE: &[&str] = &["-p", "--print", "--bg", "--background", "-h", "--help"];

/// The launch line a sleeping Claude wakes with: its own argv, every option it
/// was started with kept word for word, any option naming a conversation
/// removed, and `--resume <session>` added. Refused, with the reason, when the
/// line holds an initial prompt (it would be sent again), an option this
/// version does not know, or a non-interactive mode.
pub fn wake_argv(argv: &[String], session: &str) -> Result<Vec<String>, String> {
    if !crate::session::safe_resume_id(session) {
        return Err("conversation id is not a session id".into());
    }
    let (program, rest) = argv.split_first().ok_or("no program")?;
    let mut out = vec![program.clone()];
    let mut i = 0;
    let is_option = |w: &str| w.starts_with('-') && w.len() > 1;
    while i < rest.len() {
        let word = rest[i].as_str();
        let (flag, inline) = match word.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v)),
            _ => (word, None),
        };
        if !is_option(flag) {
            return Err(format!(
                "an initial prompt would be sent again: {:.40}",
                word
            ));
        }
        if NOT_INTERACTIVE.contains(&flag) {
            return Err(format!("{flag} is not an interactive pane"));
        }
        if RESUME_SWITCHES.contains(&flag) {
            i += 1;
        } else if RESUME_OPTIONAL.contains(&flag) {
            i += 1;
            if inline.is_none() && rest.get(i).is_some_and(|w| !is_option(w)) {
                i += 1;
            }
        } else if RESUME_VALUE.contains(&flag) {
            i += if inline.is_some() { 1 } else { 2 };
        } else if VALUE_FLAGS.contains(&flag) {
            if inline.is_some() {
                out.push(word.to_owned());
                i += 1;
            } else {
                let value = rest.get(i + 1).ok_or(format!("{flag} has no value"))?;
                out.extend([word.to_owned(), value.clone()]);
                i += 2;
            }
        } else if LIST_FLAGS.contains(&flag) {
            out.push(word.to_owned());
            i += 1;
            while inline.is_none() && rest.get(i).is_some_and(|w| !is_option(w)) {
                out.push(rest[i].clone());
                i += 1;
            }
        } else if OPTIONAL_VALUE_FLAGS.contains(&flag) {
            out.push(word.to_owned());
            i += 1;
            if inline.is_none() && rest.get(i).is_some_and(|w| !is_option(w)) {
                out.push(rest[i].clone());
                i += 1;
            }
        } else if SWITCHES.contains(&flag) {
            out.push(word.to_owned());
            i += 1;
        } else {
            return Err(format!("{flag} is an option this build does not know"));
        }
    }
    out.extend(["--resume".to_owned(), session.to_owned()]);
    Ok(out)
}

/// codex-cli 0.162.1 options that take a value (`codex --help`, `codex resume
/// --help`; the two accept the same ones).
const CODEX_VALUE_FLAGS: &[&str] = &[
    "-a",
    "--ask-for-approval",
    "--add-dir",
    "-c",
    "--cd",
    "-C",
    "--config",
    "--disable",
    "--enable",
    "--local-provider",
    "-m",
    "--model",
    "-p",
    "--profile",
    "--remote",
    "--remote-auth-token-env",
    "-s",
    "--sandbox",
];
/// Plain switches.
const CODEX_SWITCHES: &[&str] = &[
    "--approve-for-me",
    "--dangerously-bypass-approvals-and-sandbox",
    "--dangerously-bypass-hook-trust",
    "--no-alt-screen",
    "--no-daemon",
    "--oss",
    "--search",
    "--strict-config",
];
/// `resume`'s and `fork`'s own options choosing a conversation; the wake line
/// names its conversation instead.
const CODEX_PICKERS: &[&str] = &["--all", "--include-non-interactive", "--last"];
/// The subcommands that open a conversation in the pane; the wake line
/// replaces them, and the conversation they named, with `resume <id>`.
const CODEX_CONVERSATION_COMMANDS: &[&str] = &["fork", "resume"];
/// Every other subcommand: none of them is an interactive pane to wake.
const CODEX_OTHER_COMMANDS: &[&str] = &[
    "a",
    "agents",
    "app-server",
    "apply",
    "archive",
    "cloud",
    "completion",
    "debug",
    "delete",
    "doctor",
    "e",
    "exec",
    "exec-server",
    "features",
    "help",
    "login",
    "logout",
    "mcp",
    "mcp-server",
    "migrate-rollouts",
    "plugin",
    "queue",
    "remote-control",
    "review",
    "sandbox",
    "unarchive",
    "update",
];

/// The launch line a sleeping Codex wakes with: its own argv, every option it
/// was started with kept word for word, the conversation it named (by
/// `resume`, `fork`, `--last` or an id) and its opening prompt removed, and
/// `resume <session>` added.
///
/// The opening prompt is dropped where Claude's refuses, because Codex's
/// `resume` takes none: the prompt is already the first message of the
/// conversation being resumed, and Terminal Delight's own launcher always
/// passes one, so refusing would keep every launcher-started Codex awake.
/// Refused, with the reason, for an option this version does not know, a
/// subcommand that is not an interactive pane, initial images (they belong to
/// that first message), and `--worktree` (it would open a new checkout).
pub fn codex_wake_argv(argv: &[String], session: &str) -> Result<Vec<String>, String> {
    if !crate::session::safe_resume_id(session) {
        return Err("conversation id is not a session id".into());
    }
    let (program, rest) = argv.split_first().ok_or("no program")?;
    let mut out = vec![program.clone()];
    let is_option = |w: &str| w.starts_with('-') && w.len() > 1;
    let mut command_seen = false;
    let mut prompt_seen = false;
    let mut named = 0;
    let mut i = 0;
    while i < rest.len() {
        let word = rest[i].as_str();
        let (flag, inline) = match word.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v)),
            _ => (word, None),
        };
        if !is_option(flag) {
            if command_seen {
                // `resume`'s conversation, then its prompt: both replaced.
                named += 1;
                if named > 2 {
                    return Err(format!("an unexpected word after the command: {word:.40}"));
                }
            } else if prompt_seen {
                return Err(format!("an unexpected word after the prompt: {word:.40}"));
            } else if CODEX_CONVERSATION_COMMANDS.contains(&word) {
                command_seen = true;
            } else if CODEX_OTHER_COMMANDS.contains(&word) {
                return Err(format!("codex {word} is not an interactive pane"));
            } else {
                prompt_seen = true;
            }
            i += 1;
            continue;
        }
        match flag {
            "-h" | "--help" | "-V" | "--version" => {
                return Err(format!("{flag} is not an interactive pane"));
            }
            "-i" | "--image" => {
                return Err("an initial image would be attached again".into());
            }
            "--worktree" => return Err("--worktree would open a new checkout".into()),
            _ => {}
        }
        if CODEX_PICKERS.contains(&flag) {
            i += 1;
        } else if CODEX_VALUE_FLAGS.contains(&flag) {
            if inline.is_some() {
                out.push(word.to_owned());
                i += 1;
            } else {
                let value = rest.get(i + 1).ok_or(format!("{flag} has no value"))?;
                out.extend([word.to_owned(), value.clone()]);
                i += 2;
            }
        } else if CODEX_SWITCHES.contains(&flag) {
            out.push(word.to_owned());
            i += 1;
        } else {
            return Err(format!("{flag} is an option this build does not know"));
        }
    }
    out.extend(["resume".to_owned(), session.to_owned()]);
    Ok(out)
}

/// One word, quoted for a POSIX shell.
pub fn shell_quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./=:@%+,".contains(c))
    {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

// ---------------------------------------------------------------------------
// The threshold slider: smooth to slide, large targets to click.

/// A threshold a click lands on, and the span of values whose click lands on
/// it. Parker's, for sleep: 20 min (13–29), 45 (30–59), 90 (60–120), 4 h
/// (121 min–8 h), 24 h (above 8 h).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detent {
    pub minutes: u32,
    pub from: u32,
    pub to: u32,
}

/// The sleep-after slider.
pub const SLEEP_DETENTS: &[Detent] = &[
    Detent {
        minutes: 20,
        from: 0,
        to: 29,
    },
    Detent {
        minutes: 45,
        from: 30,
        to: 59,
    },
    Detent {
        minutes: 90,
        from: 60,
        to: 120,
    },
    Detent {
        minutes: 240,
        from: 121,
        to: 480,
    },
    Detent {
        minutes: 1440,
        from: 481,
        to: u32::MAX,
    },
];
/// The rest-after slider. Not specified by Parker; the same shape, scaled to
/// minutes: 5, 10 (the default), 20, 30, 60.
pub const REST_DETENTS: &[Detent] = &[
    Detent {
        minutes: 5,
        from: 0,
        to: 7,
    },
    Detent {
        minutes: 10,
        from: 8,
        to: 14,
    },
    Detent {
        minutes: 20,
        from: 15,
        to: 24,
    },
    Detent {
        minutes: 30,
        from: 25,
        to: 44,
    },
    Detent {
        minutes: 60,
        from: 45,
        to: u32::MAX,
    },
];

/// The span a slider covers, in minutes, on a logarithmic track so that the
/// short end, where a minute matters, gets the room.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub min: f64,
    pub max: f64,
}
pub const SLEEP_SCALE: Scale = Scale {
    min: 13.0,
    max: 2880.0,
};
pub const REST_SCALE: Scale = Scale {
    min: 2.0,
    max: 120.0,
};

impl Scale {
    /// Minutes at a position along the track, `0.0..=1.0`.
    pub fn minutes_at(&self, pos: f64) -> f64 {
        let pos = pos.clamp(0.0, 1.0);
        (self.min.ln() + pos * (self.max.ln() - self.min.ln())).exp()
    }
    /// Where `minutes` sits along the track.
    pub fn position_of(&self, minutes: f64) -> f64 {
        let m = minutes.clamp(self.min, self.max);
        (m.ln() - self.min.ln()) / (self.max.ln() - self.min.ln())
    }
}

/// Where a click lands: the detent whose span holds the value under the
/// pointer. A drag never snaps; it takes [`Scale::minutes_at`] rounded to the
/// minute.
pub fn click(detents: &[Detent], minutes: f64) -> u32 {
    let m = minutes.round().max(0.0) as u32;
    detents
        .iter()
        .find(|d| m >= d.from && m <= d.to)
        .or(detents.last())
        .map(|d| d.minutes)
        .unwrap_or(m)
}

/// A drag's value: smooth, to the minute.
pub fn drag(scale: &Scale, pos: f64) -> u32 {
    scale.minutes_at(pos).round() as u32
}

#[cfg(test)]
mod tests {

    /// A signalled agent is asleep the moment it is seen gone, however late,
    /// and given up on only after a full minute still running.
    #[test]
    fn a_signalled_agent_is_asleep_once_gone_and_awake_after_a_minute() {
        use std::time::Duration;
        assert_eq!(stop_look(true, false, Duration::ZERO), StopLook::Asleep);
        assert_eq!(
            stop_look(true, false, Duration::from_secs(3600)),
            StopLook::Asleep
        );
        assert_eq!(
            stop_look(false, false, Duration::from_secs(6)),
            StopLook::Stopping
        );
        assert_eq!(
            stop_look(false, false, STOP_GIVE_UP - Duration::from_millis(1)),
            StopLook::Stopping
        );
        assert_eq!(stop_look(false, false, STOP_GIVE_UP), StopLook::GaveUp);
        // A person started another agent in the shell before the sweep: that
        // pane is awake, however the old one went.
        assert_eq!(
            stop_look(true, true, Duration::from_secs(6)),
            StopLook::Replaced
        );
    }

    use super::*;

    #[test]
    fn memory_totals_keep_empty_unknown_partial_and_measured_zero_distinct() {
        let mut total = MemoryTotal::default();
        assert_eq!(total.label("unmeasured"), "—");
        total.add(None);
        assert_eq!(total.label("unmeasured"), "unmeasured (0/1)");
        total.add(Some(0));
        assert_eq!(total.label("unmeasured"), "0 B (1/2)");
        total.add(Some(1024));
        assert_eq!(total.label("unmeasured"), "1 KiB (2/3)");
    }

    #[test]
    fn memory_census_groups_states_and_retains_comparison_coverage() {
        use crate::panegroup::Usage;
        let now = std::time::Instant::now();
        let mut rest = PaneRest::new(now);
        let mut census = MemoryCensus::default();
        census.add(&rest); // Awake, no reading yet.
        rest.dormancy = Dormancy::Resting { since: now };
        rest.rest_from = Some(256);
        rest.usage = Usage {
            resident: Some(32),
            swapped: Some(64),
            ..Default::default()
        };
        census.add(&rest);
        rest.rest_from = None; // A resting pane inherited from another window.
        rest.usage = Usage {
            resident: Some(0),
            swapped: None,
            ..Default::default()
        };
        census.add(&rest);
        rest.dormancy = Dormancy::Asleep(Asleep {
            since_ms: 1,
            session: "fixture".into(),
            wake_file: "/unused".into(),
            model: None,
            program: None,
        });
        rest.rest_from = Some(128);
        rest.usage = Usage {
            resident: Some(4),
            swapped: Some(0),
            ..Default::default()
        };
        census.add(&rest);
        assert_eq!(
            (
                census.awake.panes(),
                census.resting.panes(),
                census.asleep.panes()
            ),
            (1, 2, 1)
        );
        assert_eq!(
            census.resting.ram,
            MemoryTotal {
                bytes: Some(32),
                measured: 2,
                panes: 2
            }
        );
        assert_eq!(
            census.resting.swap,
            MemoryTotal {
                bytes: Some(64),
                measured: 1,
                panes: 2
            }
        );
        assert_eq!(
            census.held,
            MemoryTotal {
                bytes: Some(100),
                measured: 2,
                panes: 4
            }
        );
        assert_eq!(
            census.decrease,
            MemoryTotal {
                bytes: Some(348),
                measured: 2,
                panes: 3
            }
        );
        rest.dormancy = Dormancy::Awake;
        assert_eq!(rest.given_back(), None, "awake panes cannot retain savings");
        assert_eq!(
            PaneRest::new(now).rest_from,
            None,
            "a new pane has no earlier reading"
        );
    }

    #[test]
    fn memory_units_match_the_binary_kernel_measurements() {
        assert_eq!(short_bytes(0), "0 B");
        assert_eq!(short_bytes(1024), "1 KiB");
        assert_eq!(short_bytes(5446 * 1024), "5.3 MiB");
        assert_eq!(short_bytes(1332 * 1024 * 1024), "1.3 GiB");
    }

    fn s(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }
    const SID: &str = "80915041-fe1f-413a-9f48-95217cb9ec6b";
    const CODEX_SID: &str = "01a17e9d-7d73-7160-a209-cfd369b05db8";

    fn idle_claude() -> Look {
        let long = Some(Duration::from_secs(3600));
        Look {
            enabled: true,
            agent: Some(Agent::Claude),
            since_touch: long,
            since_output: long,
            said_idle_for: long,
            foreign: Some(Vec::new()),
            cpu_share: Some(0.008),
            ..Look::default()
        }
    }

    #[test]
    fn an_idle_claude_untouched_and_quiet_may_rest() {
        assert_eq!(may_rest(&idle_claude(), &Thresholds::default()), Ok(()));
    }

    #[test]
    fn every_unknown_refuses_rest() {
        let t = Thresholds::default();
        let mut l = idle_claude();
        l.since_touch = None;
        assert_eq!(may_rest(&l, &t), Err(Refusal::Unknown("last touch")));
        let mut l = idle_claude();
        l.since_output = None;
        assert_eq!(may_rest(&l, &t), Err(Refusal::Unknown("last output")));
        let mut l = idle_claude();
        l.said_idle_for = None;
        assert_eq!(may_rest(&l, &t), Err(Refusal::AgentBusy(None)));
        let mut l = idle_claude();
        l.foreign = None;
        assert_eq!(may_rest(&l, &t), Err(Refusal::Unknown("processes")));
        let mut l = idle_claude();
        l.cpu_share = None;
        assert_eq!(may_rest(&l, &t), Err(Refusal::Unknown("cpu")));
        let mut l = idle_claude();
        l.agent = None;
        assert_eq!(may_rest(&l, &t), Err(Refusal::NotAnAgent));
    }

    #[test]
    fn a_pane_touched_or_printing_inside_the_window_does_not_rest() {
        let t = Thresholds::default();
        let mut l = idle_claude();
        l.since_touch = Some(Duration::from_secs(9 * 60));
        assert_eq!(may_rest(&l, &t), Err(Refusal::Touched));
        let mut l = idle_claude();
        l.since_output = Some(Duration::from_secs(30));
        assert_eq!(may_rest(&l, &t), Err(Refusal::Printing));
        let mut l = idle_claude();
        l.focused = true;
        assert_eq!(may_rest(&l, &t), Err(Refusal::Focused));
        let mut l = idle_claude();
        l.keep_awake = true;
        assert_eq!(may_rest(&l, &t), Err(Refusal::KeptAwake));
        let mut l = idle_claude();
        l.enabled = false;
        assert_eq!(may_rest(&l, &t), Err(Refusal::Off));
    }

    #[test]
    fn a_running_command_or_a_busy_group_refuses_rest() {
        let t = Thresholds::default();
        let mut l = idle_claude();
        let job = Foreign::Command {
            pid: 9,
            name: "bash".into(),
        };
        l.foreign = Some(vec![job.clone()]);
        assert_eq!(may_rest(&l, &t), Err(Refusal::Running(job)));
        let mut l = idle_claude();
        l.cpu_share = Some(0.2);
        assert_eq!(may_rest(&l, &t), Err(Refusal::UsingCpu));
    }

    #[test]
    fn a_claude_whose_screen_shows_it_working_does_not_rest_whatever_it_says() {
        let t = Thresholds::default();
        let mut l = idle_claude();
        l.screen_at_prompt = Some(false);
        assert_eq!(may_rest(&l, &t), Err(Refusal::NotAtPrompt));
        l.screen_at_prompt = None;
        assert_eq!(
            may_rest(&l, &t),
            Ok(()),
            "an unreadable screen leaves it to Claude's word"
        );
    }

    #[test]
    fn a_codex_rests_only_when_its_screen_shows_the_prompt() {
        let t = Thresholds::default();
        let mut l = idle_claude();
        l.agent = Some(Agent::Codex);
        l.said_idle_for = None;
        assert_eq!(may_rest(&l, &t), Err(Refusal::Unknown("screen")));
        l.screen_at_prompt = Some(false);
        assert_eq!(may_rest(&l, &t), Err(Refusal::NotAtPrompt));
        l.screen_at_prompt = Some(true);
        assert_eq!(may_rest(&l, &t), Ok(()));
    }

    #[test]
    fn only_the_word_idle_counts_as_idle() {
        let now = 10_000_000;
        let report = |status: &str| SelfReport {
            status: Some(status.into()),
            since_ms: Some(now - 60_000),
            session_id: None,
        };
        assert_eq!(report("idle").idle_for(now), Some(Duration::from_secs(60)));
        for other in ["busy", "waiting", "shell", "blocked", "running", ""] {
            assert_eq!(report(other).idle_for(now), None, "{other}");
        }
        assert_eq!(SelfReport::default().idle_for(now), None);
    }

    #[test]
    fn a_self_report_for_another_pid_is_ignored() {
        let text = format!(
            r#"{{"pid":2113529,"sessionId":"{SID}","status":"idle","statusUpdatedAt":1792480845491}}"#
        );
        let r = parse_self_report(&text, 2113529).unwrap();
        assert_eq!(r.status.as_deref(), Some("idle"));
        assert_eq!(r.session_id.as_deref(), Some(SID));
        assert_eq!(r.since_ms, Some(1_792_480_845_491));
        assert_eq!(parse_self_report(&text, 4), None);
    }

    #[test]
    fn helpers_are_allowed_and_commands_jobs_and_listeners_are_not() {
        // Everything in the pane shell's session (1) unless it started its own.
        let p = |pid, ppid, argv: &[&str], listens| Proc {
            pid,
            ppid,
            argv: s(argv),
            listens,
            session: Some(1),
        };
        let procs = vec![
            p(1, 0, &["bash"], false),
            p(2, 1, &["/home/parker/.local/bin/claude"], false),
            p(3, 2, &["/home/parker/.local/bin/lean-ctx"], false),
            p(4, 3, &["node", "helper.js"], false),
            // Claude's Bash tool shell leads a session of its own.
            Proc {
                session: Some(5),
                ..p(
                    5,
                    2,
                    &[
                        "/usr/bin/bash",
                        "-c",
                        "source /home/parker/.claude/shell-snapshots/x.sh && cargo build",
                    ],
                    false,
                )
            },
            Proc {
                session: Some(5),
                ..p(6, 5, &["cargo", "build"], false)
            },
            p(7, 1, &["python3", "-m", "http.server"], true),
            p(8, 2, &["terminal-delight", "mcp"], false),
            p(9, 2, &["devserver"], true),
        ];
        let f = foreign(&procs, 1, 2);
        assert_eq!(
            f,
            vec![
                Foreign::Command {
                    pid: 5,
                    name: "bash".into()
                },
                Foreign::Command {
                    pid: 6,
                    name: "cargo".into()
                },
                Foreign::Listener {
                    pid: 7,
                    name: "python3".into()
                },
                Foreign::Listener {
                    pid: 9,
                    name: "devserver".into()
                },
            ]
        );
        let quiet = vec![
            procs[0].clone(),
            procs[1].clone(),
            procs[2].clone(),
            procs[3].clone(),
            procs[7].clone(),
        ];
        assert!(foreign(&quiet, 1, 2).is_empty());
        let job = vec![
            procs[0].clone(),
            procs[1].clone(),
            p(10, 1, &["sleep", "600"], false),
        ];
        assert_eq!(
            foreign(&job, 1, 2),
            vec![Foreign::ShellJob {
                pid: 10,
                name: "sleep".into()
            }]
        );
        assert_eq!(helpers(&procs, 1, 2), 3, "lean-ctx, its node, and the mcp");
    }

    /// The process shapes measured on 2026-10-27 under an in-process Codex
    /// (codex-cli 0.162.1, `daemon_auto_start = false`): seven MCP helpers in
    /// the pane shell's session, and the `sleep 240` a "background terminal"
    /// kept running after the turn ended, a direct child of the Codex window
    /// leading a session of its own with no terminal.
    fn codex_pane(background: bool) -> Vec<Proc> {
        let p = |pid, ppid, argv: &[&str], session| Proc {
            pid,
            ppid,
            argv: s(argv),
            listens: false,
            session: Some(session),
        };
        let mut procs = vec![
            p(100, 1, &["/usr/bin/bash"], 100),
            p(
                101,
                100,
                &["/home/parker/.local/share/mise/installs/codex/latest/bin/codex"],
                100,
            ),
            p(
                102,
                101,
                &["/home/parker/.local/bin/terminal-delight", "mcp"],
                100,
            ),
            p(
                103,
                101,
                &["/usr/lib/chatgpt/resources/cua_node/bin/node", "cua-repl"],
                100,
            ),
            p(
                104,
                103,
                &["/usr/lib/chatgpt/resources/cua_node/bin/node_repl"],
                100,
            ),
            p(
                105,
                101,
                &[
                    "/usr/bin/bun",
                    "run",
                    "/home/parker/ai-garrison/src/cli.ts",
                    "mcp",
                ],
                100,
            ),
            p(106, 101, &["cdx-mcp"], 100),
            p(
                107,
                101,
                &["/home/parker/.local/bin/workspace-mcp", "--single-user"],
                100,
            ),
            p(108, 101, &["/home/parker/.local/bin/lean-ctx", "mcp"], 100),
        ];
        if background {
            procs.push(p(110, 101, &["sleep", "240"], 110));
        }
        procs
    }

    #[test]
    fn a_codex_background_terminal_is_a_command_and_its_mcp_helpers_are_not() {
        assert_eq!(
            foreign(&codex_pane(true), 100, 101),
            vec![Foreign::Command {
                pid: 110,
                name: "sleep".into()
            }]
        );
        assert!(foreign(&codex_pane(false), 100, 101).is_empty());
        assert_eq!(helpers(&codex_pane(true), 100, 101), 7);
        // A Codex window attached to a shared service has no children: the
        // service starts its helpers.
        let attached = codex_pane(false)[..2].to_vec();
        assert_eq!(helpers(&attached, 100, 101), 0);
    }

    #[test]
    fn a_process_whose_session_cannot_be_read_counts_as_a_command() {
        let mut procs = codex_pane(false);
        procs[2].session = None;
        assert_eq!(
            foreign(&procs, 100, 101),
            vec![Foreign::Command {
                pid: 102,
                name: "terminal-delight".into()
            }]
        );
    }

    #[test]
    fn a_codex_sleeps_only_when_it_runs_its_own_server() {
        let t = Thresholds::default();
        let rested = Duration::from_secs(7200);
        let wake = Ok(s(&["codex", "resume", CODEX_SID]));
        let mut own = idle_claude();
        own.agent = Some(Agent::Codex);
        own.said_idle_for = None;
        own.screen_at_prompt = Some(true);
        own.owns_helpers = Some(true);
        assert_eq!(may_sleep(&own, rested, &t, &wake), Ok(()));
        for attached in [Some(false), None] {
            let mut l = own.clone();
            l.owns_helpers = attached;
            assert_eq!(
                may_sleep(&l, rested, &t, &wake),
                Err(Refusal::CodexKeepsItsHelpers)
            );
        }
        // A background terminal still running keeps it awake, as for rest.
        let mut working = own.clone();
        working.foreign = Some(foreign(&codex_pane(true), 100, 101));
        assert!(matches!(
            may_sleep(&working, rested, &t, &wake),
            Err(Refusal::Running(Foreign::Command { pid: 110, .. }))
        ));
    }

    #[test]
    fn sleep_waits_for_rest_and_keeps_every_rest_refusal() {
        let t = Thresholds::default();
        let wake = Ok(s(&["claude", "--resume", SID]));
        let l = idle_claude();
        assert_eq!(
            may_sleep(&l, Duration::from_secs(3600), &t, &wake),
            Err(Refusal::NotRestedLongEnough)
        );
        assert_eq!(may_sleep(&l, Duration::from_secs(7200), &t, &wake), Ok(()));
        let mut busy = l.clone();
        busy.cpu_share = Some(0.5);
        assert_eq!(
            may_sleep(&busy, Duration::from_secs(7200), &t, &wake),
            Err(Refusal::UsingCpu)
        );
        let mut codex = l.clone();
        codex.agent = Some(Agent::Codex);
        assert_eq!(
            may_sleep(&codex, Duration::from_secs(7200), &t, &wake),
            Err(Refusal::CodexKeepsItsHelpers)
        );
        let bad = Err("an initial prompt would be sent again".to_string());
        assert!(matches!(
            may_sleep(&l, Duration::from_secs(7200), &t, &bad),
            Err(Refusal::LaunchLine(_))
        ));
    }

    #[test]
    fn the_wake_line_keeps_every_option_and_replaces_the_conversation() {
        let argv = s(&[
            "/home/parker/.local/bin/claude",
            "--resume",
            "449de7c5-f158-452d-94be-2870b396ab8b",
            "--model",
            "opus",
            "--effort=max",
            "--append-system-prompt",
            "You are running inside Terminal Delight. Don't 'quote' me.",
            "--add-dir",
            "/a",
            "/b",
            "--dangerously-skip-permissions",
            "--fork-session",
        ]);
        assert_eq!(
            wake_argv(&argv, SID).unwrap(),
            s(&[
                "/home/parker/.local/bin/claude",
                "--model",
                "opus",
                "--effort=max",
                "--append-system-prompt",
                "You are running inside Terminal Delight. Don't 'quote' me.",
                "--add-dir",
                "/a",
                "/b",
                "--dangerously-skip-permissions",
                "--resume",
                SID,
            ])
        );
        assert_eq!(
            wake_argv(&s(&["claude", "-c"]), SID).unwrap(),
            s(&["claude", "--resume", SID])
        );
        assert_eq!(
            wake_argv(&s(&["claude", "--session-id", SID, "-r"]), SID).unwrap(),
            s(&["claude", "--resume", SID])
        );
    }

    #[test]
    fn a_wake_line_that_would_resend_a_prompt_or_guess_an_option_is_refused() {
        assert!(wake_argv(&s(&["claude", "fix the build"]), SID)
            .unwrap_err()
            .contains("initial prompt"));
        assert!(wake_argv(&s(&["claude", "--model", "opus", "go"]), SID).is_err());
        assert!(wake_argv(&s(&["claude", "--brand-new-flag"]), SID)
            .unwrap_err()
            .contains("does not know"));
        assert!(wake_argv(&s(&["claude", "-p", "hi"]), SID).is_err());
        assert!(wake_argv(&s(&["claude", "--model"]), SID).is_err());
        assert!(wake_argv(&s(&["claude"]), "not a session").is_err());
        assert!(wake_argv(&[], SID).is_err());
    }

    #[test]
    fn a_codex_wakes_with_every_option_and_without_its_opening_prompt() {
        let codex = "/home/parker/.local/share/mise/installs/codex/latest/bin/codex";
        // Terminal Delight's launcher, as pane 41 ran it on 2026-10-27.
        assert_eq!(
            codex_wake_argv(
                &s(&[
                    codex,
                    "-c",
                    "model_reasoning_effort=medium",
                    "You are running inside Terminal Delight, which can render your work",
                ]),
                CODEX_SID
            )
            .unwrap(),
            s(&[
                codex,
                "-c",
                "model_reasoning_effort=medium",
                "resume",
                CODEX_SID
            ])
        );
        // A resume line, by id or by --last: the conversation is replaced.
        for line in [
            &[codex, "resume", "01a164c0-27f1-7dc2-bcd4-f919cdf3e0ba"][..],
            &[codex, "resume", "--last"][..],
            &[codex, "fork", "--last"][..],
        ] {
            assert_eq!(
                codex_wake_argv(&s(line), CODEX_SID).unwrap(),
                s(&[codex, "resume", CODEX_SID])
            );
        }
        // Options after the command are kept, before the new `resume`.
        assert_eq!(
            codex_wake_argv(
                &s(&[
                    "codex",
                    "resume",
                    "-m",
                    "gpt-6-astra",
                    "--search",
                    "--config=model_reasoning_effort=xhigh",
                    "01a164c0-27f1-7dc2-bcd4-f919cdf3e0ba",
                    "carry on",
                ]),
                CODEX_SID
            )
            .unwrap(),
            s(&[
                "codex",
                "-m",
                "gpt-6-astra",
                "--search",
                "--config=model_reasoning_effort=xhigh",
                "resume",
                CODEX_SID,
            ])
        );
    }

    #[test]
    fn a_codex_wake_line_that_would_guess_is_refused() {
        let refused = |words: &[&str]| codex_wake_argv(&s(words), CODEX_SID).unwrap_err();
        assert!(refused(&["codex", "exec", "fix it"]).contains("not an interactive pane"));
        assert!(refused(&["codex", "review"]).contains("not an interactive pane"));
        assert!(refused(&["codex", "-i", "shot.png", "look"]).contains("image"));
        assert!(refused(&["codex", "--worktree"]).contains("new checkout"));
        assert!(refused(&["codex", "--brand-new-flag"]).contains("does not know"));
        assert!(refused(&["codex", "-m"]).contains("no value"));
        assert!(refused(&["codex", "--help"]).contains("not an interactive pane"));
        assert!(refused(&["codex", "one prompt", "and another"]).contains("unexpected"));
        assert!(refused(&["codex", "resume", "a", "b", "c"]).contains("unexpected"));
        assert!(codex_wake_argv(&s(&["codex"]), "not a session").is_err());
        assert!(codex_wake_argv(&[], CODEX_SID).is_err());
    }

    #[test]
    fn sizes_and_ages_read_short() {
        assert_eq!(short_bytes(980 * 1024), "980 KiB");
        assert_eq!(short_bytes(412 << 20), "412 MiB");
        assert_eq!(short_bytes((12 << 30) / 10), "1.2 GiB");
        assert_eq!(short_age(Duration::from_secs(40)), "40s");
        assert_eq!(short_age(Duration::from_secs(14 * 60)), "14m");
        assert_eq!(short_age(Duration::from_secs(2 * 3600 + 5)), "2h");
        assert_eq!(short_age(Duration::from_secs(3 * 86_400)), "3d");
    }

    #[test]
    fn a_quoted_word_survives_the_shell() {
        assert_eq!(shell_quote("opus"), "opus");
        assert_eq!(shell_quote("--effort=max"), "--effort=max");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("$HOME `x`"), "'$HOME `x`'");
    }

    #[test]
    fn a_click_lands_on_parkers_thresholds() {
        for (m, want) in [
            (13.0, 20),
            (29.0, 20),
            (30.0, 45),
            (59.0, 45),
            (60.0, 90),
            (120.0, 90),
            (121.0, 240),
            (480.0, 240),
            (481.0, 1440),
            (2880.0, 1440),
        ] {
            assert_eq!(click(SLEEP_DETENTS, m), want, "{m}");
        }
        assert_eq!(click(REST_DETENTS, 10.4), 10);
        assert_eq!(click(REST_DETENTS, 2.0), 5);
    }

    #[test]
    fn the_track_is_smooth_monotonic_and_round_trips() {
        let mut last = 0.0;
        for i in 0..=1000 {
            let m = SLEEP_SCALE.minutes_at(i as f64 / 1000.0);
            assert!(m > last);
            last = m;
        }
        assert!((SLEEP_SCALE.minutes_at(0.0) - 13.0).abs() < 1e-9);
        assert!((SLEEP_SCALE.minutes_at(1.0) - 2880.0).abs() < 1e-6);
        for m in [20.0, 45.0, 90.0, 120.0, 240.0, 1440.0] {
            assert!((SLEEP_SCALE.minutes_at(SLEEP_SCALE.position_of(m)) - m).abs() < 1e-6);
        }
        assert_eq!(drag(&REST_SCALE, REST_SCALE.position_of(10.0)), 10);
    }
}
