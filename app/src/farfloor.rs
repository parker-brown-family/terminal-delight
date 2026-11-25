//! The Threshing Floor across machines: which agents on another machine this
//! one may see and reach, and which of this machine's agents another may.
//!
//! On one machine a person seats agents and draws lines between them, and the
//! window polices every call along those lines (`threshing_floor`). Two
//! machines keep that shape and split it down the middle:
//!
//! - **The sending machine holds the line.** A person there draws a line from
//!   one of their own seated agents to an agent on another machine, at a rung.
//!   Their window checks it before anything leaves ([`FarLines`]).
//! - **The receiving machine holds a ceiling.** A person there says how far
//!   each other machine's agents may reach into theirs at all: nothing, see,
//!   read, ask or prompt ([`Sharing`]). Nothing is the default, so a machine
//!   that has never been told about another shows it no agent.
//!
//! What a far agent gets is the least of three things: the line, the ceiling,
//! and the target pane's own position (open, read-only, off-limits), which
//! holds for every agent wherever it runs.
//!
//! T3 Code's peer links are the specimen (`docs/internals/remote.md` there):
//! one environment signs in to another with an access level the receiving side
//! lists and revokes, and "the link is routing, not isolation". The same is
//! true here and worth saying plainly. The road between machines is ssh as the
//! same person, which already lets a program on one machine do anything that
//! person can do on the other. The ceiling decides what Terminal Delight will
//! carry for an agent. It is not a wall around a program that goes round
//! Terminal Delight with ssh of its own.
//!
//! Pure: the rules, the saved shapes and the messages. The window asks the far
//! machine and draws the answers (`far_floor_view.rs`).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::mcp::McpConfig;
use crate::threshing_floor::{policy_admits, AgentAccess, Party, Reach, Rung, RungWord, SeatId};

// ---- what this machine is called ----

/// The name this machine gives itself when it speaks to another: its
/// hostname, unless `TD_MACHINE_NAME` says otherwise (two machines that share
/// a hostname, or a rig running both on one box).
pub fn machine_name() -> String {
    machine_name_from(
        std::env::var("TD_MACHINE_NAME").ok().as_deref(),
        std::fs::read_to_string("/proc/sys/kernel/hostname")
            .ok()
            .as_deref(),
    )
}

/// [`machine_name`] with both sources passed in.
pub fn machine_name_from(named: Option<&str>, hostname: Option<&str>) -> String {
    let clean = |s: &str| {
        let s: String = s
            .trim()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "._-".contains(*c))
            .take(64)
            .collect();
        (!s.is_empty()).then_some(s)
    };
    named
        .and_then(clean)
        .or_else(|| hostname.and_then(clean))
        .unwrap_or_else(|| "unnamed".into())
}

// ---- the receiving side: a ceiling per machine ----

/// How far each other machine's agents may reach into this one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sharing {
    entries: Vec<Shared>,
}

/// One machine's ceiling, as saved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shared {
    /// The name that machine gives itself ([`machine_name`] there).
    pub host: String,
    /// A word this build cannot read is kept as written and grants nothing.
    pub reach: RungWord,
}

#[derive(Serialize, Deserialize, Default)]
struct SharingFile {
    #[serde(default)]
    machine: Vec<Shared>,
}

/// Where the ceilings are kept.
pub fn sharing_path(config: &Path) -> PathBuf {
    config.join("sharing.toml")
}

const SHARING_HEADER: &str =
    "# How far other machines' agents may reach into this one, through their\n\
# Threshing Floor: see, read, ask or prompt. A machine not listed reaches nothing.\n\
# `host` is the name that machine gives itself. Settings > Machines edits this.\n";

impl Sharing {
    pub fn parse(text: &str) -> Result<Sharing, String> {
        let file: SharingFile = toml::from_str(text).map_err(|e| e.to_string())?;
        Ok(Sharing {
            entries: file
                .machine
                .into_iter()
                .filter(|m| !m.host.trim().is_empty())
                .collect(),
        })
    }

    /// Read the file. A missing file is nobody shared with, which is the
    /// default and not an error. A file that cannot be read is an error, and
    /// the caller shares with nobody rather than with whoever half of it named.
    pub fn load(config: &Path) -> Result<Sharing, String> {
        match std::fs::read_to_string(sharing_path(config)) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Sharing::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn to_toml(&self) -> String {
        let body = toml::to_string(&SharingFile {
            machine: self.entries.clone(),
        })
        .unwrap_or_default();
        format!("{SHARING_HEADER}\n{body}")
    }

    /// Write the file whole. The temporary file it is written to first is
    /// this process's own, named by pid and a counter and created new, so two
    /// windows saving at once never write into one another's copy. A change
    /// a person makes goes through [`Sharing::update`], which holds the lock
    /// around the read as well; this is the write alone.
    pub fn save(&self, config: &Path) -> Result<(), String> {
        use std::io::Write;
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(config).map_err(|e| e.to_string())?;
        let path = sharing_path(config);
        let tmp = config.join(format!(
            ".sharing.toml.{}.{}.tmp",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let written = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .and_then(|mut f| f.write_all(self.to_toml().as_bytes()));
        if let Err(e) = written {
            let _ = std::fs::remove_file(&tmp);
            return Err(e.to_string());
        }
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })
    }

    /// Change one machine's ceiling in the file, read and written under one
    /// lock (`sharing.toml.lock`, beside it) that every window on this
    /// machine takes, so a window holding an older copy cannot write back a
    /// permission another window has just revoked. Returns the file as it now
    /// stands. A file that cannot be read is an error and is left as it is,
    /// rather than replaced by the one change made over an empty list.
    pub fn update(config: &Path, host: &str, reach: Option<Rung>) -> Result<Sharing, String> {
        use std::os::fd::AsRawFd;
        std::fs::create_dir_all(config).map_err(|e| e.to_string())?;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(config.join("sharing.toml.lock"))
            .map_err(|e| e.to_string())?;
        loop {
            // SAFETY: flock on a descriptor this function owns; it is
            // released when `lock` is dropped at the end of the function.
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) } == 0 {
                break;
            }
            let err = std::io::Error::last_os_error();
            if err.kind() != std::io::ErrorKind::Interrupted {
                return Err(err.to_string());
            }
        }
        let mut sharing = Sharing::load(config)?;
        if sharing.set(host, reach) {
            sharing.save(config)?;
        }
        Ok(sharing)
    }

    /// The most `host`'s agents may do here. `None`: nothing, whether the
    /// machine was never listed or its word is one this build cannot read.
    ///
    /// The file is a person's to edit, so it can name one machine twice
    /// (`legion` and `Legion`). Every entry for it counts, and the least of
    /// them holds: a duplicate never widens what another entry allows, and an
    /// entry this build cannot read closes the machine as it would alone.
    pub fn ceiling(&self, host: &str) -> Option<Rung> {
        let mut least: Option<Rung> = None;
        for m in self
            .entries
            .iter()
            .filter(|m| m.host.eq_ignore_ascii_case(host))
        {
            let r = m.reach.known()?;
            least = Some(least.map_or(r, |l| l.min(r)));
        }
        least
    }

    /// Set or clear a machine's ceiling. Says whether anything changed.
    /// Every entry naming the machine is replaced, so a revocation cannot
    /// leave behind a second entry for the same machine that still grants.
    pub fn set(&mut self, host: &str, reach: Option<Rung>) -> bool {
        let host = host.trim();
        if host.is_empty() {
            return false;
        }
        let at = self
            .entries
            .iter()
            .position(|m| m.host.eq_ignore_ascii_case(host));
        let before = self.entries.len();
        if let Some(i) = at {
            let mut n = 0;
            self.entries.retain(|m| {
                n += 1;
                n - 1 == i || !m.host.eq_ignore_ascii_case(host)
            });
        }
        let dropped = self.entries.len() != before;
        match (at, reach) {
            (Some(i), None) => {
                self.entries.remove(i);
                true
            }
            (Some(i), Some(r)) => {
                let word = RungWord::Known(r);
                let changed = dropped || self.entries[i].reach != word;
                self.entries[i].reach = word;
                changed
            }
            (None, Some(r)) => {
                self.entries.push(Shared {
                    host: host.to_string(),
                    reach: RungWord::Known(r),
                });
                true
            }
            (None, None) => false,
        }
    }

    pub fn hosts(&self) -> impl Iterator<Item = &Shared> {
        self.entries.iter()
    }
}

/// How far an agent on another machine reaches `target` here: the window's
/// switches and the pane's position, exactly as for a local caller, and then
/// that machine's ceiling in place of a line. A seated pane and an unseated
/// one are the same to it: a machine with no ceiling sees neither.
pub fn inbound_reach(cfg: &McpConfig, ceiling: Option<Rung>, target: &Party) -> Reach {
    // MCP switched off is refused by `policy_admits`, which begins with
    // `cfg.enabled`; the separate check this line once had was the same test
    // twice, and the mutation run could not tell them apart (issue #1472).
    if target.access.is_off_limits() || !policy_admits(cfg, target.is_agent) {
        return Reach::Hidden;
    }
    let Some(ceiling) = ceiling else {
        return Reach::Hidden;
    };
    let position = match target.access {
        AgentAccess::Open => Reach::Prompt,
        AgentAccess::ReadOnly => Reach::Ask,
        AgentAccess::OffLimits => Reach::Hidden,
    };
    Reach::from(ceiling).min(position)
}

// ---- how a far agent is named ----

/// How one machine names an agent on another: by its conversation when the
/// far window can prove one, else by the seat a person there gave it. Never a
/// pane number, which starts again at 1 when a session host restarts and would
/// point a saved line at a stranger.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FarAddr {
    Conversation(String),
    Seat(SeatId),
}

impl FarAddr {
    pub fn parse(text: &str) -> Option<FarAddr> {
        let (kind, rest) = text.split_once(':')?;
        match kind {
            "conv"
                if !rest.is_empty() && rest.len() <= 200 && !rest.contains(char::is_whitespace) =>
            {
                Some(FarAddr::Conversation(rest.to_string()))
            }
            "seat" => rest.parse().ok().map(|n| FarAddr::Seat(SeatId(n))),
            _ => None,
        }
    }
}

impl std::fmt::Display for FarAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FarAddr::Conversation(c) => write!(f, "conv:{c}"),
            FarAddr::Seat(s) => write!(f, "seat:{}", s.0),
        }
    }
}

// ---- the sending side: lines to far agents ----

/// One line a person drew from a seat here to an agent on another machine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FarLine {
    pub from: SeatId,
    /// The machine, by its name in `machines.toml`.
    pub machine: String,
    /// The far agent ([`FarAddr`], as text).
    pub to: String,
    pub rung: RungWord,
    /// What the far agent was called when the line was drawn, so the line can
    /// still be read while that machine is unreachable.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// Every far line this window holds. Saved with the session, beside the
/// chart's own lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FarLines {
    lines: Vec<FarLine>,
}

impl FarLines {
    pub fn from_saved(lines: Vec<FarLine>) -> Self {
        Self { lines }
    }

    pub fn saved(&self) -> &[FarLine] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn at(&self, from: SeatId, machine: &str, to: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| l.from == from && l.machine == machine && l.to == to)
    }

    /// The rung of the line from `from` to that far agent, when one is drawn
    /// and this build can read it.
    pub fn rung(&self, from: SeatId, machine: &str, to: &str) -> Option<Rung> {
        self.at(from, machine, to)
            .and_then(|i| self.lines[i].rung.known())
    }

    /// Draw, redraw or erase. `None` erases.
    pub fn set(&mut self, from: SeatId, machine: &str, to: &str, name: &str, rung: Option<Rung>) {
        match (self.at(from, machine, to), rung) {
            (Some(i), None) => {
                self.lines.remove(i);
            }
            (Some(i), Some(r)) => {
                self.lines[i].rung = RungWord::Known(r);
                self.lines[i].name = name.to_string();
            }
            (None, Some(r)) => self.lines.push(FarLine {
                from,
                machine: machine.to_string(),
                to: to.to_string(),
                rung: RungWord::Known(r),
                name: name.to_string(),
            }),
            (None, None) => {}
        }
    }

    /// One press on a line's chip: nothing, see, read, ask, prompt, nothing.
    /// Returns the rung it lands on.
    pub fn cycle(&mut self, from: SeatId, machine: &str, to: &str, name: &str) -> Option<Rung> {
        let next = next_rung(self.rung(from, machine, to));
        self.set(from, machine, to, name, next);
        next
    }

    /// A seat was unseated: its far lines go with it, as its local ones do.
    pub fn forget_seat(&mut self, seat: SeatId) {
        self.lines.retain(|l| l.from != seat);
    }

    /// The lines out of one seat.
    pub fn lines_of(&self, seat: SeatId) -> impl Iterator<Item = &FarLine> {
        self.lines.iter().filter(move |l| l.from == seat)
    }

    /// The machines any line points at.
    pub fn machines(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for l in &self.lines {
            if !out.contains(&l.machine.as_str()) {
                out.push(&l.machine);
            }
        }
        out
    }
}

/// The step after `rung` in the order a chip cycles through.
pub fn next_rung(rung: Option<Rung>) -> Option<Rung> {
    match rung {
        None => Some(Rung::See),
        Some(Rung::See) => Some(Rung::Read),
        Some(Rung::Read) => Some(Rung::Ask),
        Some(Rung::Ask) => Some(Rung::Prompt),
        Some(Rung::Prompt) => None,
    }
}

/// How far a seat here actually reaches a far agent: its line, capped by what
/// the far machine said that agent allows this machine. No line is hidden,
/// and so is an agent the far machine did not list.
pub fn outbound_reach(line: Option<Rung>, far_allows: Option<Reach>) -> Reach {
    match (line, far_allows) {
        (Some(rung), Some(allowed)) => Reach::from(rung).min(allowed),
        _ => Reach::Hidden,
    }
}

// ---- the messages ----

/// One agent, as a machine reports it to another. What it carries depends on
/// how far the asking machine reaches it: a name and a state at `seen`, the
/// agent's own note and summary from `ask` up.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FarAgent {
    /// How to name it in a later call ([`FarAddr`], as text).
    pub addr: String,
    /// The seat it also answers to, when `addr` names its conversation and a
    /// person has seated it too. A line saved before its conversation was
    /// known names the seat, and still reaches it by this. `None`: no second
    /// name was given (not seated, or a build that does not send one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub also: Option<String>,
    /// What a person there calls it: where its tab sits, then its header.
    pub name: String,
    /// The program: `CLAUDE`, `CODEX`.
    pub harness: String,
    /// Its state, in English: working, needs you, done, idle, unknown.
    pub state: String,
    #[serde(default)]
    pub model: Option<String>,
    /// Its context in tokens. `None`: not measured.
    #[serde(default)]
    pub size: Option<u64>,
    /// The most the asking machine may do with it.
    pub reach: Reach,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brief: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layman: Option<String>,
}

/// One window's answer to "what may my machine see here".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Census {
    /// The name the answering machine gives itself.
    pub machine: String,
    pub session: String,
    pub window: u32,
    /// Whether that window's MCP switch is on. Off shares nothing.
    pub mcp: bool,
    /// The ceiling it holds for the asking machine. `None`: nothing.
    #[serde(default)]
    pub ceiling: Option<Rung>,
    #[serde(default)]
    pub agents: Vec<FarAgent>,
    /// Agents the asking machine may see that have neither a proven
    /// conversation nor a seat, so nothing durable names them. Counted, so a
    /// short list is not taken for everyone.
    #[serde(default)]
    pub unnamed: u64,
}

/// A prompt one machine hands another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PromptIn {
    /// The sending machine's own name.
    pub from: String,
    /// Who sent it, as a person there would say it.
    pub sender: String,
    /// The agent it is for.
    pub to: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// A prompt key as it leaves for another machine: the sending agent's own.
/// The far window sees every agent on this machine as one sender, so the key
/// carries which agent chose it: its seat, which survives a wake, or, for an
/// agent with no seat, its pane.
pub fn scoped_key(seat: Option<SeatId>, caller_pane: u64, key: &str) -> String {
    match seat {
        Some(s) => format!("seat{}/{key}", s.0),
        None => format!("pane{caller_pane}/{key}"),
    }
}

/// A JSON-RPC line for `method` with `params`.
pub fn request(method: &str, params: serde_json::Value) -> String {
    serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }).to_string()
}

pub const M_CENSUS: &str = "far/census";
pub const M_PROMPT: &str = "far/prompt";
pub const M_STATUS: &str = "far/prompt_status";

/// Runs one JSON-RPC line, read from stdin, against the far machine's windows.
/// `$1` is `all` or a window's pid. Prints what `terminal-delight ctl` prints:
/// one line per window, its pid, a tab, and its answer.
///
/// The line travels on stdin because it carries a person's words, and a
/// prompt's worth of text has no business being quoted through two shells.
pub fn rpc_script() -> String {
    r#"td=$(command -v terminal-delight 2>/dev/null)
if [ -z "$td" ] && [ -x "$HOME/.local/bin/terminal-delight" ]; then td="$HOME/.local/bin/terminal-delight"; fi
if [ -z "$td" ]; then echo "no-terminal-delight"; exit 0; fi
p=$(cat)
case $1 in
  all) "$td" ctl --all mcp rpc "$p" ;;
  *[!0-9]*|'') echo "bad-window"; exit 0 ;;
  *) "$td" ctl --pid "$1" mcp rpc "$p" ;;
esac
exit 0
"#
    .to_string()
}

/// One window's reply line, split into its pid and the JSON-RPC result.
/// An `err …` line, or a JSON-RPC error, is `Err` with the far side's words.
pub fn parse_reply_line(line: &str) -> Option<(u32, Result<serde_json::Value, String>)> {
    let (pid, rest) = line.split_once('\t')?;
    let pid: u32 = pid.trim().parse().ok()?;
    let rest = rest.trim();
    if rest.starts_with("err") {
        return Some((pid, Err(rest.to_string())));
    }
    let value: serde_json::Value = match serde_json::from_str(rest) {
        Ok(v) => v,
        Err(_) => {
            return Some((
                pid,
                Err(format!(
                    "an answer this build cannot read: {}",
                    clip(rest, 120)
                )),
            ))
        }
    };
    if let Some(err) = value.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("the far window refused")
            .to_string();
        return Some((pid, Err(msg)));
    }
    match value.get("result") {
        Some(result) => Some((pid, Ok(result.clone()))),
        None => Some((pid, Err("an answer with no result".into()))),
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}

/// What a far machine's windows said to a census, put together.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MachineCensus {
    /// One per window that answered in a shape this build reads.
    pub windows: Vec<Census>,
    /// Why a window's answer could not be used. An older build that does not
    /// know the question lands here, by its own words.
    pub troubles: Vec<String>,
}

impl MachineCensus {
    /// Every agent across that machine's windows, with the window that holds it.
    pub fn agents(&self) -> impl Iterator<Item = (&Census, &FarAgent)> {
        self.windows
            .iter()
            .flat_map(|w| w.agents.iter().map(move |a| (w, a)))
    }

    /// The agent `addr` names: by the name the census gives it, or by the
    /// seat it also answers to.
    pub fn find(&self, addr: &str) -> Option<(&Census, &FarAgent)> {
        self.agents()
            .find(|(_, a)| a.addr == addr || a.also.as_deref() == Some(addr))
    }

    /// Whether any window there has been told to share with this machine.
    pub fn shares(&self) -> bool {
        self.windows.iter().any(|w| w.mcp && w.ceiling.is_some())
    }
}

/// Read what [`rpc_script`] printed for a census.
pub fn parse_census(out: &str) -> Result<MachineCensus, String> {
    let out = out.trim();
    if out == "no-terminal-delight" {
        return Err("Terminal Delight is not installed there".into());
    }
    let mut census = MachineCensus::default();
    for line in out.lines().filter(|l| !l.trim().is_empty()) {
        match parse_reply_line(line) {
            Some((_, Ok(result))) => match serde_json::from_value::<Census>(
                // A tool result's data is its structured half.
                result.get("structuredContent").cloned().unwrap_or(result),
            ) {
                Ok(c) => census.windows.push(c),
                Err(e) => census
                    .troubles
                    .push(format!("an answer this build cannot read ({e})")),
            },
            Some((_, Err(why))) => census.troubles.push(why),
            None => census.troubles.push(clip(line, 160)),
        }
    }
    if census.windows.is_empty() && census.troubles.is_empty() {
        return Err("no Terminal Delight window is open there".into());
    }
    Ok(census)
}

/// The stages of a prompt sent to another machine, as this one knows them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum FarStage {
    /// On its way to the far machine.
    Sending,
    /// The far window has it. `far` is that window's own account of it, as
    /// its `prompt_status` words it.
    Held { far: String },
    /// Ended there. `reply` is the worker's last message when the far window
    /// could hand one back; `unknown` is its reason when it could not.
    Finished {
        reply: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        unknown: Option<String>,
    },
    /// It did not arrive, or ended without finishing. In the far side's words.
    Failed { why: String },
}

impl FarStage {
    pub fn is_open(&self) -> bool {
        matches!(self, FarStage::Sending | FarStage::Held { .. })
    }
}

/// One prompt this window sent to another machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FarSent {
    pub id: u64,
    pub sender_pane: u64,
    pub machine: String,
    pub to: String,
    /// The far window that took it and the number it gave it there.
    pub far: Option<(u32, u64)>,
    pub sent_ms: u64,
    pub stage: FarStage,
    /// How many times in a row the far window that took it could not be
    /// asked, while its machine could. Not a count of anything else.
    pub misses: u8,
    /// The sender's key, as it left ([`scoped_key`]). A retry that carries
    /// it is answered with this prompt, so a lost first answer is not a lost
    /// prompt. `None`: sent without one.
    pub key: Option<String>,
}

/// How many such misses end a prompt. The far window has closed, and what
/// became of the prompt it was holding is not known here.
pub const WINDOW_GONE_MISSES: u8 = 5;

/// Every prompt this window has sent to other machines.
#[derive(Clone, Debug, Default)]
pub struct FarBook {
    sent: Vec<FarSent>,
}

impl FarBook {
    pub fn open(&mut self, sent: FarSent) {
        // Keep the open ones and the newest fifty ended, like a book that is
        // read back but never grows without end.
        let ended = self.sent.iter().filter(|s| !s.stage.is_open()).count();
        if ended > 50 {
            if let Some(i) = self.sent.iter().position(|s| !s.stage.is_open()) {
                self.sent.remove(i);
            }
        }
        self.sent.push(sent);
    }

    pub fn of_sender(&self, id: u64, sender_pane: u64) -> Option<&FarSent> {
        self.sent
            .iter()
            .find(|s| s.id == id && s.sender_pane == sender_pane)
    }

    /// The prompt this window already sent to `machine` under `key`, open
    /// or ended. The key carries the sending agent ([`scoped_key`]), so this
    /// is that agent's own earlier attempt.
    pub fn by_key(&self, machine: &str, key: &str) -> Option<&FarSent> {
        self.sent
            .iter()
            .rev()
            .find(|s| s.machine == machine && s.key.as_deref() == Some(key))
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut FarSent> {
        self.sent.iter_mut().find(|s| s.id == id)
    }

    /// The prompts still open, for the poller.
    pub fn open_ones(&self) -> Vec<FarSent> {
        self.sent
            .iter()
            .filter(|s| s.stage.is_open())
            .cloned()
            .collect()
    }

    /// The far machine answered, and the window that took prompt `id` did
    /// not. After [`WINDOW_GONE_MISSES`] in a row the prompt is ended as not
    /// known, so its sender is not held waiting on a window that closed.
    /// Says whether this miss ended it.
    pub fn window_missed(&mut self, id: u64, why: &str) -> bool {
        let Some(s) = self
            .sent
            .iter_mut()
            .find(|s| s.id == id && s.stage.is_open())
        else {
            return false;
        };
        s.misses = s.misses.saturating_add(1);
        if s.misses < WINDOW_GONE_MISSES {
            return false;
        }
        s.stage = FarStage::Failed {
            why: format!(
                "the window that took it on the far machine stopped answering ({why}). Whether the agent finished is not known; ask it, or look at that machine."
            ),
        };
        true
    }

    /// A status arrived for prompt `id`. Says whether the stage changed.
    pub fn heard(&mut self, id: u64, stage: FarStage) -> bool {
        let Some(s) = self.sent.iter_mut().find(|s| s.id == id) else {
            return false;
        };
        s.misses = 0;
        if s.stage == stage {
            return false;
        }
        s.stage = stage;
        true
    }

    /// Whether `sender_pane` already has a prompt open to that agent: one at a
    /// time per pair, as on one machine.
    pub fn busy(&self, sender_pane: u64, machine: &str, to: &str) -> bool {
        self.sent.iter().any(|s| {
            s.sender_pane == sender_pane && s.machine == machine && s.to == to && s.stage.is_open()
        })
    }
}

/// Read the far window's answer to a prompt or a status question into a
/// stage. The far side answers in the shape its own `prompt_status` uses: a
/// `stage` word, and a `reply` once the worker has finished.
pub fn stage_of(result: &serde_json::Value) -> (Option<u64>, FarStage) {
    let s = result.get("structuredContent").unwrap_or(result);
    let id = s.get("prompt_id").and_then(|v| v.as_u64());
    if result.get("isError").and_then(|v| v.as_bool()) == Some(true) {
        let why = result
            .get("content")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("the far window refused")
            .to_string();
        return (id, FarStage::Failed { why });
    }
    let word = s.get("stage").and_then(|v| v.as_str()).unwrap_or("");
    let stage = match word {
        "finished" => FarStage::Finished {
            reply: s.get("reply").and_then(|v| v.as_str()).map(str::to_string),
            unknown: s
                .get("reply_unknown")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        },
        "expired" => FarStage::Failed {
            why: "its record expired on the far machine, so how it ended is not known".into(),
        },
        "failed" => FarStage::Failed {
            why: s
                .get("why")
                .and_then(|v| v.as_str())
                .unwrap_or("it failed there")
                .to_string(),
        },
        "" => FarStage::Failed {
            why: "the far window answered in a shape this build cannot read".into(),
        },
        other => FarStage::Held {
            far: other.to_string(),
        },
    };
    (id, stage)
}

#[cfg(test)]
mod tests;
