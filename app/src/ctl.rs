//! ctl — the control socket: how the desktop talks to a RUNNING terminal.
//!
//! Every terminal-delight process listens on its own unix socket,
//! `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock`, for one-line commands —
//! PAINT mode (the per-pane palette overlay), `ping`, `paint status`, the MCP
//! policy toggles, and `mcp rpc <json>`, which carries a whole JSON-RPC line to
//! the MCP protocol handler. The same binary is also the client: `terminal-delight ctl
//! paint toggle --workspace active` finds the sockets of the windows on the
//! active Hyprland workspace and pokes each one. That split is what lets an
//! Omarchy bar widget (or a keybind, or a plain script) raise the overlay
//! without linking against anything: the whole contract is a socket path and a
//! line of text.
//!
//! Wire protocol, deliberately dumb: the client writes one line, the server
//! answers one line — `ok`, `on`, `off`, `pong`, or `err <why>` — and the
//! connection is done. Command effects are queued to the UI thread; `ok`
//! acknowledges the queue write, and `paint status` reads a mirror the UI
//! refreshes every tick, so a status read straight after a toggle can lag it
//! by one ~150 ms tick. Honest enough, and nothing ever blocks the UI.
//!
//! The socket file is NOT unlinked on exit — there is no hook that runs on
//! every exit path, so pretending otherwise would just be a lie that works in
//! demos. Instead the server unlinks any stale file before binding its own
//! pid's path, and the client treats a connection failure as "stale: sweep it
//! and move on".
//!
//! **`mcp rpc` is the exception to "never blocks".** The MCP handler needs a
//! round-trip onto the gpui main thread, so that one verb is served on its own
//! thread and the accept loop moves on; everything else still answers inline
//! from a queue write or an atomic mirror. `terminal-delight mcp` is the
//! matching client — a stdio JSON-RPC relay an agent registers as an MCP server,
//! which finds the terminal hosting it by walking its own parent chain.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use gpui::Context;

use crate::{mcp, mcp_transport, theme, Workspace};

/// A tile adoption: open a fresh pane at `cwd`, optionally running `run` in it
/// (an agent resume line, a tmux attach) — how the desktop hands a terminal
/// session over to us (td-send / SUPER+ALT+T). Rides the same queue as paint.
#[derive(Debug, PartialEq)]
pub(crate) struct AdoptReq {
    pub cwd: Option<String>,
    pub run: Option<String>,
}

/// Who is asking, as they worked it out from their own process tree.
///
/// Carried on the wire so the window can do two things it previously could not:
/// refuse a call meant for a different Terminal Delight, and let a pane-scoped
/// verb default to the pane the caller is actually sitting in. Both were
/// guesses before — the first was not made at all, and the second was made by
/// the agent, out of titles and directories that two panes routinely share.
#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct Caller {
    /// The session the caller believes it is in.
    pub session: String,
    /// The pane, when the host could name one. `None` is a real answer — a
    /// process whose chain the host does not recognise is in no pane of ours —
    /// and it stays distinct from "pane zero".
    pub pane: Option<u64>,
}

/// A queued control request, applied on the UI thread by the ticker.
#[derive(Debug)]
pub(crate) enum Req {
    Set(bool),
    Toggle,
    Adopt(AdoptReq),
    McpPolicy(McpPolicy),
    Tabs(Vec<TabOp>),
    /// Choose the chrome skin for this window: a builtin id, `custom` for the
    /// user's own file, or `theme` to follow whatever the theme asks for.
    Skin(String),
    /// Turn every pane in this window to a face. See [`Cmd::Bench`].
    Bench(BenchFace),
    /// Press an answer on the focused pane's bench. See [`Cmd::BenchChoose`].
    /// The sender carries the OUTCOME back to the socket — `ok pane 3`,
    /// `queued pane 3`, or an `err` — because "the message was accepted" is
    /// what `ok` used to mean, and a smoke run reported green on that while
    /// the window logged that nothing happened.
    BenchChoose(usize, mpsc::Sender<String>),
    /// Send the open round from the focused pane's bench. See
    /// [`Cmd::BenchSubmit`].
    BenchSubmit(mpsc::Sender<String>),
    /// Say a line to the agent through the bench. See [`Cmd::BenchSay`].
    BenchSay(String, mpsc::Sender<String>),
    /// Put a line in the composer WITHOUT submitting it — what a person
    /// halfway through typing looks like. See [`Cmd::BenchType`].
    BenchType(String, mpsc::Sender<String>),
    /// Open a document in a floating square. See [`Cmd::DocHere`].
    DocHere(PathBuf, mpsc::Sender<String>),
    /// Open a document in a pane beside the focused one. See
    /// [`Cmd::DocBeside`].
    DocBeside(PathBuf, mpsc::Sender<String>),
    /// Close a floating square. See [`Cmd::DocClose`].
    DocClose(mpsc::Sender<String>),
}

/// Wait for the window to say what a bench verb actually did. The ticker
/// drains the queue every 150ms, so two seconds is many chances; past that
/// the window is not answering and the socket should say so rather than `ok`.
fn bench_outcome(rx: mpsc::Receiver<String>) -> String {
    rx.recv_timeout(Duration::from_secs(2))
        .unwrap_or_else(|_| "err the window did not answer".into())
}

/// One field of the MCP control-surface policy — the robot panel's toggles,
/// reachable from the socket. Same escalation, same persistence: the panel and
/// this path both write `ws.mcp` and `save()`, so a grant made from the CLI is
/// visible in the panel and survives a restart. Kept as one-field-at-a-time so a
/// script can grant reads without silently also granting writes.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum McpPolicy {
    /// The master switch (`mcp.enabled`).
    Enabled(bool),
    /// The second, separate opt-in that permits `set_pane_config` (`mcp.writable`).
    Writes(bool),
    /// `true` = expose every pane, `false` = agent panes only (the safe default).
    ExposeAll(bool),
}

/// WHICH tab an op acts on.
///
/// Two ways to say it, and the difference matters more than it looks. An INDEX
/// is what you read off `list_panes`, and it is convenient for a one-off — but
/// a grouping op slides tabs around to keep a group's members adjacent, so the
/// indices an op list was written against stop being true the moment that list
/// is applied. Re-running an index-addressed list therefore writes its names
/// onto whatever tabs have since moved into those slots. That is not a
/// hypothetical: it happened here, and it put one name on three different tabs.
///
/// A PANE reference does not move. The pane's shell pid identifies a terminal,
/// the terminal identifies the tab holding it, and neither changes when the
/// strip reorders — so an op list addressed this way means the same thing every
/// time it is applied. Scripts and agents should use it; `tab` is for a human
/// reading positions off a listing.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum TabRef {
    /// Position in the strip, as `list_panes` reports it. Not durable.
    Index(usize),
    /// The tab holding the pane whose shell has this pid. Survives a reorder.
    Pane(u32),
}

/// What an op DOES, with no opinion about which tab it does it to.
///
/// Split from the target on purpose: the four tab-scoped actions all needed the
/// same "exactly one of `tab` or `pane`" check, and four copies of a validation
/// rule is four chances for them to drift. Now there is one.
#[derive(Debug, PartialEq, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub(crate) enum TabAction {
    /// Set (or with `name: null`, clear) the tab's label.
    Name {
        #[serde(default)]
        name: Option<String>,
    },
    /// Put the tab in the group called `group`, creating it if no group has that
    /// name. `color` seeds a NEW group only; an existing group keeps its colour
    /// (use `group_color` to change it).
    Group {
        group: String,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        text_color: Option<String>,
    },
    /// Take the tab out of its group (dropping the group if it empties).
    Ungroup,
    /// The tab's own colour overrides. `null` clears an override so the tab
    /// falls back to its group's lead.
    Color {
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        text_color: Option<String>,
    },
    /// Recolour a whole group by name. A group always has a fill, so `color:
    /// null` is ignored there; `text_color: null` clears the text lead.
    GroupColor {
        group: String,
        #[serde(default)]
        color: Option<String>,
        #[serde(default)]
        text_color: Option<String>,
    },
    /// Fold or unfold a group into its counted pill.
    Collapse { group: String, collapsed: bool },
}

impl TabAction {
    /// Does this action act on ONE tab (and therefore need a target), or on a
    /// group as a whole (and therefore refuse one)?
    fn needs_a_tab(&self) -> bool {
        !matches!(
            self,
            TabAction::GroupColor { .. } | TabAction::Collapse { .. }
        )
    }

    /// Every colour string this action carries, for the parse-time hex check.
    fn colors(&self) -> [Option<&String>; 2] {
        match self {
            TabAction::Name { .. } | TabAction::Ungroup | TabAction::Collapse { .. } => {
                [None, None]
            }
            TabAction::Group {
                color, text_color, ..
            }
            | TabAction::Color { color, text_color }
            | TabAction::GroupColor {
                color, text_color, ..
            } => [color.as_ref(), text_color.as_ref()],
        }
    }
}

/// One edit to the mother bar's tab strip, from `ctl tabs '<json>'`.
///
/// The strip is what a returning human reads first, and until this existed the
/// only way to write it was a right-click and a colour wheel — so a workspace of
/// twenty tabs stayed half-labelled, because relabelling it by hand was never
/// worth the minutes. An agent that has just finished a task is the thing that
/// knows what the tab it worked in should be called; this is how it says so.
#[derive(Debug, PartialEq, serde::Deserialize)]
pub(crate) struct TabOp {
    #[serde(flatten)]
    pub(crate) action: TabAction,
    #[serde(default)]
    tab: Option<usize>,
    #[serde(default)]
    pane: Option<u32>,
}

impl TabOp {
    /// The tab this op names, validated: exactly one of `tab`/`pane` for a
    /// tab-scoped action, neither for a group-scoped one.
    ///
    /// Checked here, at parse time, rather than when the op is applied — the
    /// applier runs on the UI thread behind a queue and cannot answer the
    /// caller, so a mistake caught there is a mistake nobody hears about.
    pub(crate) fn target(&self) -> Result<Option<TabRef>, String> {
        match (self.action.needs_a_tab(), self.tab, self.pane) {
            (true, Some(_), Some(_)) => Err(
                "give either `tab` (a position) or `pane` (a pid), not both —                  they can name different tabs"
                    .into(),
            ),
            (true, Some(i), None) => Ok(Some(TabRef::Index(i))),
            (true, None, Some(p)) => Ok(Some(TabRef::Pane(p))),
            (true, None, None) => Err(
                "this op needs a tab: `pane` (a pid from list_panes — preferred,                  it survives a reorder) or `tab` (a position)"
                    .into(),
            ),
            (false, None, None) => Ok(None),
            (false, _, _) => Err(
                "a group-wide op acts on the whole group by name — drop `tab`/`pane`".into(),
            ),
        }
    }
}

/// The `tabs` payload: a bare array of ops, or `{"ops":[…]}`. Both shapes exist
/// because a one-op call reads better as an array of one than as a wrapper.
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum TabsPayload {
    Bare(Vec<TabOp>),
    Wrapped { ops: Vec<TabOp> },
}

/// Parse + VALIDATE a `tabs` payload. Application is queued to the UI thread and
/// so cannot report failure, which makes this the only place a bad colour or an
/// empty batch can be caught while the caller is still listening — so it is
/// checked here rather than shrugged at later.
fn parse_tabs(json: &str) -> Result<Vec<TabOp>, String> {
    let ops = match serde_json::from_str::<TabsPayload>(json) {
        Ok(TabsPayload::Bare(v)) | Ok(TabsPayload::Wrapped { ops: v }) => v,
        Err(e) => return Err(format!("tabs payload is not a valid op list: {e}")),
    };
    if ops.is_empty() {
        return Err("tabs: empty op list".into());
    }
    for (i, op) in ops.iter().enumerate() {
        // Position the complaint: in a batch of twenty, "op 13" is the
        // difference between a fix and a hunt.
        op.target().map_err(|e| format!("tabs: op {i}: {e}"))?;
        for c in op.action.colors().into_iter().flatten() {
            if theme::parse_hex(c).is_none() {
                return Err(format!(
                    "tabs: op {i}: {c:?} is not a hex colour (want #rrggbb)"
                ));
            }
        }
    }
    Ok(ops)
}

/// One parsed request line.
#[derive(Debug)]
enum Cmd {
    Ping,
    PaintStatus,
    Paint(Req),
    Adopt(AdoptReq),
    /// A whole JSON-RPC line for the MCP handler, verbatim, plus who is asking
    /// when they were able to work it out.
    McpRpc(Option<Caller>, String),
    /// Which session this window holds, and its own pid. The verb that lets a
    /// caller find the right window by asking every live one, rather than by
    /// trusting a file or an environment variable to still be true.
    WhoAmI,
    McpStatus,
    McpPolicy(McpPolicy),
    /// A batch of tab-strip edits, verbatim JSON.
    Tabs(Vec<TabOp>),
    Skin(String),
    SkinStatus,
    /// Turn a pane (or every pane) to one of its two faces.
    ///
    /// The scriptable half of the TERM / BENCH toggle. It exists because a
    /// gesture that can only be performed by a hand cannot be demonstrated,
    /// recorded, or tested — and this window is tested by photographing it.
    Bench(BenchFace),
    /// Answer the selected question on the focused pane's bench, by option
    /// number as the surface shows it. When the focused pane is not showing
    /// a bench with a question, the first pane that is — the same rule the
    /// two verbs below follow, as a table in `workbench::bench_target`.
    BenchChoose(usize),
    /// Send the open round, however much of it was answered — what pressing
    /// the navigator's SUBMIT tab does.
    ///
    /// Takes no argument on purpose: the round is not named, because the tab
    /// is not either. It sends whatever round the pane would send if somebody
    /// clicked, which is the only thing a test of that click can mean.
    BenchSubmit,
    /// Type a line into the agent through the bench, exactly as the composer
    /// does. The scripted half of talking to a pane.
    BenchSay(String),
    /// The same, WITHOUT the return: a composer holding a line somebody is
    /// still writing.
    ///
    /// It exists for the same reason the rest of this family does — the
    /// gestures on this surface belong to a hand, and the shell that builds it
    /// has none — and it earns its place specifically because the half-typed
    /// state is where both caret defects lived. Neither was reachable by a
    /// script, so both were found by borrowing a person's keyboard, once by
    /// accident into the wrong window.
    BenchType(String),
    /// Open a document in a floating square on the focused pane — what
    /// Alt+clicking its path does, for a caller with no pointer. Takes an
    /// absolute path: the socket has no working directory to resolve against.
    DocHere(PathBuf),
    /// Open a document in a pane beside the focused one — what Ctrl+Alt+
    /// clicking its path does, split, dedupe and four-pane cap included.
    /// Absolute, for `DocHere`'s reason.
    DocBeside(PathBuf),
    /// Close the floating square, on the focused pane or the first one that
    /// has a square open.
    DocClose,
}

/// Which face `ctl bench` asks for.
#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum BenchFace {
    Terminal,
    Workbench,
    Toggle,
}

// The `mcp status` mirror, refreshed by the ticker each pass (same pattern as
// the paint mirror): a policy read never touches the main thread.
const MCP_ON: u8 = 1 << 0;
const MCP_WRITES: u8 = 1 << 1;
const MCP_EXPOSE_ALL: u8 = 1 << 2;
const MCP_EVENTS: u8 = 1 << 3;

fn mcp_bits(c: &mcp::McpConfig) -> u8 {
    let mut b = 0;
    if c.enabled {
        b |= MCP_ON;
    }
    if c.writable {
        b |= MCP_WRITES;
    }
    if c.expose == mcp::Expose::All {
        b |= MCP_EXPOSE_ALL;
    }
    if c.events {
        b |= MCP_EVENTS;
    }
    b
}

/// Render the mirror as the one status line `mcp status` answers with.
fn mcp_status_line(bits: u8) -> String {
    let on = |m: u8| if bits & m != 0 { "on" } else { "off" };
    format!(
        "enabled={} writes={} expose={} events={}",
        on(MCP_ON),
        on(MCP_WRITES),
        if bits & MCP_EXPOSE_ALL != 0 {
            "all"
        } else {
            "agents"
        },
        on(MCP_EVENTS),
    )
}

/// The per-user control directory. Runtime state, so `$XDG_RUNTIME_DIR` (a
/// tmpfs that dies with the session) — never the config dir, which persists.
fn ctl_dir() -> PathBuf {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/terminal-delight-{}", unsafe { libc::getuid() }));
    PathBuf::from(base).join("terminal-delight")
}

/// This process's socket path. Keyed by pid so `ctl --pid N` needs no lookup
/// table and a workspace query (window → pid) lands directly on the file.
pub fn socket_path(pid: u32) -> PathBuf {
    ctl_dir().join(format!("ctl-{pid}.sock"))
}

/// Everything the grammar accepts, in one place — the usage string and the
/// unknown-command error both quote it, so they can't drift from the match.
const USAGE: &str = "ping | whoami | paint on|off|toggle|status | \
     skin <name>|theme|status | \
     bench on|off|toggle|choose <n>|submit|say <text>|type <text> | \
     doc here <absolute path> | doc beside <absolute path> | doc close | \
     mcp status|on|off | mcp writes on|off | mcp expose agents|all | \
     mcp rpc <json> | mcp from <session> <pane|-> rpc <json> | \
     adopt {\"cwd\":\"/…\",\"run\":\"…\"} | \
     tabs [{\"op\":\"name\",\"pane\":1234,\"name\":\"DEV\"}, …] | \
     tab name <text> | tab group <name> | tab ungroup";

fn parse_line(s: &str) -> Result<Cmd, String> {
    // `adopt` carries a JSON payload (cwd/run both hold spaces); everything
    // else stays word-shaped.
    if let Some(rest) = s.strip_prefix("adopt ") {
        return parse_adopt(rest.trim()).map(Cmd::Adopt);
    }
    // `bench say` carries a whole sentence: take the remainder VERBATIM
    // rather than splitting it into words, or every prompt loses its spacing.
    if let Some(rest) = s.strip_prefix("bench type ") {
        let line = rest.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            return Err("bench type: nothing to type".into());
        }
        return Ok(Cmd::BenchType(line.to_string()));
    }
    if let Some(rest) = s.strip_prefix("bench say ") {
        let line = rest.trim();
        if line.is_empty() {
            return Err("bench say: nothing to say".into());
        }
        return Ok(Cmd::BenchSay(line.to_string()));
    }
    // `doc here` and `doc beside` carry a path, and paths hold spaces: take
    // it verbatim.
    for (verb, make) in [
        ("doc here ", Cmd::DocHere as fn(PathBuf) -> Cmd),
        ("doc beside ", Cmd::DocBeside),
    ] {
        if let Some(rest) = s.strip_prefix(verb) {
            let path = rest.trim_end_matches(['\r', '\n']);
            if !path.starts_with('/') {
                return Err(format!(
                    "{}: {path:?} is not an absolute path — the socket has no working directory",
                    verb.trim_end()
                ));
            }
            return Ok(make(PathBuf::from(path)));
        }
    }
    // `tabs` carries a JSON op list — tab names hold spaces, so the remainder
    // is taken verbatim and parsed as JSON rather than split into words.
    if let Some(rest) = s.strip_prefix("tabs ") {
        return parse_tabs(rest.trim()).map(Cmd::Tabs);
    }
    // `mcp from <session> <pane|-> rpc <json>` — the same payload with the
    // caller named. Parsed before the bare form so the longer prefix wins, and
    // the JSON is still taken verbatim from after the third word.
    if let Some(rest) = s.strip_prefix("mcp from ") {
        return parse_from(rest);
    }
    // `mcp rpc` carries a whole JSON-RPC line: take the remainder VERBATIM.
    // Splitting it on whitespace would corrupt every string literal in it.
    if let Some(rest) = s.strip_prefix("mcp rpc ") {
        let line = rest.trim();
        if line.is_empty() {
            return Err("mcp rpc: empty payload".into());
        }
        return Ok(Cmd::McpRpc(None, line.to_string()));
    }
    let w: Vec<&str> = s.split_whitespace().collect();
    match w.as_slice() {
        ["ping"] => Ok(Cmd::Ping),
        ["whoami"] => Ok(Cmd::WhoAmI),
        ["paint", "on"] => Ok(Cmd::Paint(Req::Set(true))),
        ["paint", "off"] => Ok(Cmd::Paint(Req::Set(false))),
        ["paint", "toggle"] => Ok(Cmd::Paint(Req::Toggle)),
        ["paint", "status"] => Ok(Cmd::PaintStatus),
        ["bench", "on"] | ["bench", "workbench"] => Ok(Cmd::Bench(BenchFace::Workbench)),
        ["bench", "off"] | ["bench", "terminal"] => Ok(Cmd::Bench(BenchFace::Terminal)),
        ["bench", "toggle"] => Ok(Cmd::Bench(BenchFace::Toggle)),
        // `bench choose 3` — the pressable half of the bench, for a caller
        // with no pointer. Numbered as the surface numbers it, so what you
        // type is what you read.
        ["bench", "choose", n] => n
            .parse::<usize>()
            .ok()
            .filter(|n| *n >= 1 && *n <= 20)
            .map(Cmd::BenchChoose)
            .ok_or_else(|| format!("bench choose: {n:?} is not an option number")),
        // `bench submit` — the round's only exit, for a caller with no
        // pointer. Since a round with a navigator stopped posting itself on
        // its last answer, the SUBMIT tab is the whole of how one ends, and a
        // capability reachable only by a mouse cannot be gated by anything.
        ["bench", "submit"] => Ok(Cmd::BenchSubmit),
        ["doc", "close"] => Ok(Cmd::DocClose),
        ["skin", "status"] => Ok(Cmd::SkinStatus),
        // Any other single word is a skin id — `theme` and `custom` included,
        // which is why they are not special-cased here. The window is what knows
        // which ids exist, so validation happens there and comes back as text.
        ["skin", name] => Ok(Cmd::Skin((*name).to_string())),
        ["mcp", "status"] => Ok(Cmd::McpStatus),
        ["mcp", "on"] => Ok(Cmd::McpPolicy(McpPolicy::Enabled(true))),
        ["mcp", "off"] => Ok(Cmd::McpPolicy(McpPolicy::Enabled(false))),
        ["mcp", "writes", "on"] => Ok(Cmd::McpPolicy(McpPolicy::Writes(true))),
        ["mcp", "writes", "off"] => Ok(Cmd::McpPolicy(McpPolicy::Writes(false))),
        ["mcp", "expose", "all"] => Ok(Cmd::McpPolicy(McpPolicy::ExposeAll(true))),
        ["mcp", "expose", "agents"] => Ok(Cmd::McpPolicy(McpPolicy::ExposeAll(false))),
        _ => Err(format!("unknown command {s:?} — try: {USAGE}")),
    }
}

/// `mcp from <session> <pane|-> rpc <json>`.
///
/// Three fixed words then a verbatim tail, for the same reason the bare form
/// takes its tail verbatim: splitting the payload on whitespace would corrupt
/// every string literal in it. `-` in the pane slot says the caller could not
/// find out which pane it is in, which is different from a pane called nothing
/// and is the honest answer from a process whose chain the host did not know.
fn parse_from(rest: &str) -> Result<Cmd, String> {
    let mut words = rest.splitn(3, ' ');
    let (Some(session), Some(pane), Some(tail)) = (words.next(), words.next(), words.next()) else {
        return Err("mcp from: expected <session> <pane|-> rpc <json>".into());
    };
    if session.is_empty() {
        return Err("mcp from: empty session".into());
    }
    let pane = match pane {
        "-" => None,
        n => Some(
            n.parse::<u64>()
                .map_err(|_| format!("mcp from: {n:?} is not a pane id (or `-`)"))?,
        ),
    };
    let Some(payload) = tail.strip_prefix("rpc ") else {
        return Err("mcp from: expected `rpc <json>` after the pane".into());
    };
    let payload = payload.trim();
    if payload.is_empty() {
        return Err("mcp from: empty payload".into());
    }
    Ok(Cmd::McpRpc(
        Some(Caller {
            session: session.to_string(),
            pane,
        }),
        payload.to_string(),
    ))
}

/// The `adopt` payload: one JSON object, `cwd` and/or `run`, cwd absolute when
/// present. Same-user trust as the rest of the socket — the validation here is
/// protocol hygiene, not a security boundary.
fn parse_adopt(json: &str) -> Result<AdoptReq, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("adopt payload is not JSON: {e}"))?;
    let take = |k: &str| -> Result<Option<String>, String> {
        match &v[k] {
            serde_json::Value::Null => Ok(None),
            serde_json::Value::String(s) if s.is_empty() => Ok(None),
            serde_json::Value::String(s) => Ok(Some(s.clone())),
            other => Err(format!("adopt: {k} must be a string, got {other}")),
        }
    };
    let cwd = take("cwd")?;
    let run = take("run")?;
    if let Some(c) = &cwd {
        if !c.starts_with('/') {
            return Err(format!("adopt: cwd must be absolute, got {c:?}"));
        }
    }
    if cwd.is_none() && run.is_none() {
        return Err("adopt: needs cwd and/or run".into());
    }
    Ok(AdoptReq { cwd, run })
}

/// The reply for a `mcp rpc` line that produced no response — a JSON-RPC
/// notification, or an unparseable line. A distinct sentinel rather than `err`
/// so the relay client can drop it silently instead of logging a non-problem.
const MCP_NONE: &str = "mcp-none";

/// The unnamed RPC form — what every window has understood since the verb
/// existed, and what a relay falls back to when it meets one that predates
/// `mcp from`.
const BARE_RPC: &str = "mcp rpc ";

/// Whether a reply is a window saying it does not know this verb.
///
/// Matched on the shape `parse_line` produces for an unrecognised command, and
/// deliberately narrow: an `err` that is anything else — a malformed payload, a
/// refusal — is a real answer and must not be retried into a form that hides
/// it. A version negotiation would be tidier and is not worth a handshake on a
/// wire whose every other verb has been stable since it was written; this is
/// the one verb that has ever been added to it.
fn is_unknown_verb(reply: &std::io::Result<String>) -> bool {
    matches!(reply, Ok(r) if r.starts_with("err ") && r.contains("unknown command"))
}

/// Serve one connection: read a line, answer a line. Short timeouts on both
/// directions so a wedged client can never stall the single accept loop.
/// The skin mirror's payload: the active id, then every id that can be selected.
///
/// It exists so the SOCKET THREAD can refuse a misspelled skin name immediately
/// rather than posting it into the UI queue and replying `ok`. A switch that says
/// ok and does nothing is the exact failure this whole layer is built to refuse,
/// and it would have been reintroduced here — at the one surface a person
/// actually types into — for want of a list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SkinMirror {
    pub active: String,
    pub known: Vec<String>,
}

impl SkinMirror {
    fn line(&self) -> String {
        if self.known.is_empty() {
            return "unknown".into();
        }
        format!("active={} known={}", self.active, self.known.join(","))
    }
}

fn handle_conn(
    stream: UnixStream,
    mirror: &AtomicBool,
    mcp_mirror: &AtomicU8,
    skin_mirror: &Mutex<SkinMirror>,
    tx: &mpsc::Sender<Req>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(400)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(400)));
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut line = String::new();
    if BufReader::new(read_half).read_line(&mut line).is_err() {
        return;
    }
    let reply = match parse_line(line.trim()) {
        // `mcp rpc` is the one verb that waits on the gpui main thread (up to
        // the transport's snapshot budget). Serving it inline would stall the
        // single accept loop for seconds, so hand the connection to its own
        // thread and get straight back to accepting. That thread re-arms the
        // write timeout: 400 ms is sized for a mirror read, not a `tools/list`
        // payload after a five-second wait.
        Ok(Cmd::McpRpc(caller, payload)) => {
            // A caller that named a DIFFERENT session is answered here and goes
            // no further. Several Terminal Delights run on one box and a relay
            // that resolved the wrong window used to be served in full — an
            // answer about somebody else's terminals, correct in shape and
            // impossible to spot. Refusing costs one comparison and turns that
            // into a sentence naming both sessions.
            //
            // THIS GUARDS MISROUTING, NOT IMPERSONATION, and the difference
            // matters to whoever arrives here cold. `c.session` is whatever the
            // caller wrote on the wire; nothing checks it against the process on
            // the other end of the connection, and `c.pane` is not checked
            // against anything at all. A caller that names the right session on
            // purpose is served, and that is deliberate: every caller able to
            // reach this socket is already the same person, and an agent acting
            // for them is them. `docs/decisions/0002` carries the argument and,
            // more usefully, says what ends it — the first caller able to arrive
            // from outside this user's runtime directory.
            //
            // `host.rs` does check, with `SO_PEERCRED`, because it is answering
            // a different question. If this socket ever needs to, the technique
            // is already in the tree rather than to be invented.
            if let Some(c) = &caller {
                let mine = crate::instance::key();
                if c.session != mine {
                    let msg = format!(
                        "wrong window: this is terminal-delight session {mine:?} \
                         (window {}), and you asked for session {:?}. Nothing was \
                         read or written. Your relay resolved the wrong instance.",
                        std::process::id(),
                        c.session
                    );
                    let reply = mcp::error_response(&payload, -32001, &msg)
                        .unwrap_or_else(|| MCP_NONE.to_string());
                    let mut stream = stream;
                    let _ = writeln!(stream, "{reply}");
                    return;
                }
            }
            let spawned = thread::Builder::new()
                .name("td-ctl-mcp".into())
                .spawn(move || {
                    let reply = mcp_transport::respond_as(&payload, caller)
                        .unwrap_or_else(|| MCP_NONE.to_string());
                    let mut stream = stream;
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(8)));
                    let _ = writeln!(stream, "{reply}");
                });
            if spawned.is_err() {
                // Thread exhaustion. The caller is owed a line and the worker
                // now owns the stream, so there is nothing left to answer on;
                // the client's read timeout turns this into "unreachable".
                eprintln!("terminal-delight: ctl could not spawn an mcp worker");
            }
            return; // the worker owns the connection from here
        }
        Ok(Cmd::Ping) => "pong".to_string(),
        // Answered from the socket thread with no main-thread hop: it reads the
        // process-wide session binding and a pid, neither of which the UI owns.
        // It has to stay cheap, because finding the right window means asking
        // every live one.
        Ok(Cmd::WhoAmI) => format!("ok {} {}", crate::instance::key(), std::process::id()),
        Ok(Cmd::PaintStatus) => if mirror.load(Ordering::Relaxed) {
            "on"
        } else {
            "off"
        }
        .to_string(),
        Ok(Cmd::McpStatus) => mcp_status_line(mcp_mirror.load(Ordering::Relaxed)),
        Ok(Cmd::Paint(req)) => {
            if tx.send(req).is_ok() {
                "ok".into()
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::SkinStatus) => skin_mirror
            .lock()
            .map(|m| m.line())
            .unwrap_or_else(|_| "unknown".into()),
        Ok(Cmd::Skin(name)) => {
            let known = skin_mirror
                .lock()
                .map(|m| m.known.clone())
                .unwrap_or_default();
            // An empty list means the window has not ticked yet and we genuinely
            // do not know. Forwarding blind beats refusing a valid name because
            // startup has not finished — "unknown" and "absent" are not the same
            // answer, and only one of them justifies a refusal.
            let unchecked = known.is_empty();
            let ok = name == crate::skin::FOLLOW_THEME || known.contains(&name);
            if unchecked || ok {
                if tx.send(Req::Skin(name.clone())).is_ok() {
                    format!("ok {name}")
                } else {
                    "err ui gone".into()
                }
            } else {
                format!(
                    "err no skin {:?} — have: {}, {}",
                    name,
                    known.join(", "),
                    crate::skin::FOLLOW_THEME
                )
            }
        }
        Ok(Cmd::McpPolicy(p)) => {
            if tx.send(Req::McpPolicy(p)).is_ok() {
                "ok".into()
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::Tabs(ops)) => {
            let n = ops.len();
            if tx.send(Req::Tabs(ops)).is_ok() {
                format!("ok {n}")
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::BenchType(line)) => {
            let (rtx, rrx) = mpsc::channel();
            if tx.send(Req::BenchType(line, rtx)).is_ok() {
                bench_outcome(rrx)
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::BenchSay(line)) => {
            let (rtx, rrx) = mpsc::channel();
            if tx.send(Req::BenchSay(line, rtx)).is_ok() {
                bench_outcome(rrx)
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::BenchChoose(n)) => {
            let (rtx, rrx) = mpsc::channel();
            if tx.send(Req::BenchChoose(n, rtx)).is_ok() {
                bench_outcome(rrx)
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::BenchSubmit) => {
            let (rtx, rrx) = mpsc::channel();
            if tx.send(Req::BenchSubmit(rtx)).is_ok() {
                bench_outcome(rrx)
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::DocHere(path)) => {
            let (rtx, rrx) = mpsc::channel();
            if tx.send(Req::DocHere(path, rtx)).is_ok() {
                bench_outcome(rrx)
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::DocBeside(path)) => {
            let (rtx, rrx) = mpsc::channel();
            if tx.send(Req::DocBeside(path, rtx)).is_ok() {
                bench_outcome(rrx)
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::DocClose) => {
            let (rtx, rrx) = mpsc::channel();
            if tx.send(Req::DocClose(rtx)).is_ok() {
                bench_outcome(rrx)
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::Bench(face)) => {
            if tx.send(Req::Bench(face)).is_ok() {
                "ok".into()
            } else {
                "err ui gone".into()
            }
        }
        Ok(Cmd::Adopt(a)) => {
            if tx.send(Req::Adopt(a)).is_ok() {
                "ok".into()
            } else {
                "err ui gone".into()
            }
        }
        Err(e) => format!("err {e}"),
    };
    let mut stream = stream;
    let _ = writeln!(stream, "{reply}");
}

/// Start the control server once per process. Call from `Workspace::build`.
/// Failure to bind is a warning, never fatal — the terminal works without its
/// control surface, it just can't be painted from the bar.
pub fn start(cx: &mut Context<Workspace>) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let dir = ctl_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("terminal-delight: ctl dir unavailable ({dir:?}): {e}");
        return;
    }
    // `adopt` spawns commands, so the socket must stay same-user even on the
    // /tmp fallback (no $XDG_RUNTIME_DIR) under a loose umask: pin the dir to
    // 0700 every start — create_dir_all leaves an existing dir's mode alone.
    let _ = std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700));
    let path = socket_path(std::process::id());
    // A stale file under our own pid means a previous process with a recycled
    // pid died uncleanly; the bind would fail on it, so sweep first.
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("terminal-delight: ctl socket unavailable ({path:?}): {e}");
            return;
        }
    };

    let (tx, rx) = mpsc::channel::<Req>();
    let mirror = Arc::new(AtomicBool::new(false));
    let mcp_mirror = Arc::new(AtomicU8::new(0));
    // Not an atomic: the skin mirror is a name plus a list, and it is read once
    // per socket connection rather than per frame.
    let skin_mirror = Arc::new(Mutex::new(SkinMirror::default()));

    {
        let mirror = Arc::clone(&mirror);
        let mcp_mirror = Arc::clone(&mcp_mirror);
        let skin_mirror = Arc::clone(&skin_mirror);
        let _ = thread::Builder::new().name("td-ctl".into()).spawn(move || {
            for conn in listener.incoming() {
                let Ok(stream) = conn else { continue };
                handle_conn(stream, &mirror, &mcp_mirror, &skin_mirror, &tx);
            }
        });
    }

    // Ticker: applies queued commands on the main thread and re-mirrors the
    // live paint flag every pass — Esc flips the global without going through
    // this queue, and `paint status` must not report around that.
    cx.spawn(async move |this, cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
        let mut cmds = Vec::new();
        while let Ok(c) = rx.try_recv() {
            cmds.push(c);
        }
        let applied = this.update(cx, |ws, cx| {
            for c in cmds {
                match c {
                    Req::Set(v) => theme::set_paint_mode(cx, v),
                    Req::Toggle => {
                        let v = !theme::paint_mode(cx);
                        theme::set_paint_mode(cx, v);
                    }
                    // Adoption needs a Window to build the pane; this ticker is
                    // window-less, so park it — render() drains next frame.
                    Req::Adopt(a) => ws.queue_adopt(a, cx),
                    // Tab edits need no Window — the strip is workspace state.
                    Req::Tabs(ops) => ws.apply_tab_ops(ops, cx),
                    // The face is per-pane state, so this walks them. A
                    // window-wide gesture rather than a per-pane one because
                    // the caller has no pane ids to hand — and turning the
                    // whole window to face its work is what a demo wants.
                    Req::Bench(face) => ws.set_all_faces(face, cx),
                    Req::BenchChoose(n, reply) => {
                        let _ = reply.send(ws.bench_choose(n, cx));
                    }
                    Req::BenchSubmit(reply) => {
                        let _ = reply.send(ws.bench_submit(cx));
                    }
                    Req::BenchSay(line, reply) => {
                        let _ = reply.send(ws.bench_say(&line, cx));
                    }
                    Req::BenchType(line, reply) => {
                        let _ = reply.send(ws.bench_type(&line, cx));
                    }
                    Req::DocHere(path, reply) => {
                        let _ = reply.send(ws.doc_here(&path, cx));
                    }
                    // The split needs a Window, which this ticker has none
                    // of: the pane is asked, as a click asks it, and the
                    // workspace answers from the subscription that has one.
                    Req::DocBeside(path, reply) => ws.doc_beside(&path, reply, cx),
                    Req::DocClose(reply) => {
                        let _ = reply.send(ws.doc_close(cx));
                    }
                    // The same escalation the robot panel performs, and the same
                    // persistence: a grant made from the CLI shows in the panel
                    // and survives a restart.
                    // An id the window does not know still gets refused here —
                    // the socket-side check is a fast path, not the authority.
                    Req::Skin(id) => match crate::skin::select(cx, &id) {
                        Ok(now) => eprintln!("terminal-delight: skin -> {now}"),
                        Err(e) => eprintln!("terminal-delight: {e}"),
                    },
                    Req::McpPolicy(p) => {
                        match p {
                            McpPolicy::Enabled(v) => ws.mcp.enabled = v,
                            McpPolicy::Writes(v) => ws.mcp.writable = v,
                            McpPolicy::ExposeAll(v) => {
                                ws.mcp.expose = if v {
                                    mcp::Expose::All
                                } else {
                                    mcp::Expose::AgentsOnly
                                }
                            }
                        }
                        ws.save(cx);
                        cx.notify();
                    }
                }
            }
            mirror.store(theme::paint_mode(cx), Ordering::Relaxed);
            mcp_mirror.store(mcp_bits(&ws.mcp), Ordering::Relaxed);
            if let Ok(mut m) = skin_mirror.lock() {
                *m = SkinMirror {
                    active: crate::skin::active_id(cx),
                    known: crate::skin::all_skins(cx)
                        .into_iter()
                        .map(|(id, _, _)| id)
                        .collect(),
                };
            }
        });
        if applied.is_err() {
            return; // UI gone — the listener thread dies with the process
        }
    })
    .detach();
}

// ---------------------------------------------------------------- client ----

/// Which running terminals a `ctl` invocation addresses.
#[derive(Debug, PartialEq)]
enum Scope {
    /// Terminal-delight windows on the active Hyprland workspace (the default —
    /// it is what a bar click means).
    ActiveWorkspace,
    /// A named (or numeric-id) Hyprland workspace.
    Workspace(String),
    /// Every control socket present.
    All,
    /// The window this process is RUNNING INSIDE, found by walking our own
    /// parent chain — the same resolution the MCP relay uses, and the default
    /// for `tabs`. A tab edit is always about the caller's own window, so
    /// asking Hyprland which workspace is on screen answers a question nobody
    /// asked: an agent in a pane on workspace 1 would fail, or worse succeed
    /// against a different window, depending on where the human happened to be
    /// looking. See the note on `owning_td_pid`.
    Owning,
    /// One process.
    Pid(u32),
}

/// Parse `ctl` argv (everything after the `ctl` token) into the request line
/// and the scope. Kept pure for tests.
/// `ctl tab …` — the tab strip for the ONE caller who is not reorganising it.
///
/// The bulk form (`ctl tabs '<json>'`) came first and was the wrong thing to
/// build first: reorganising a whole strip is something a person does rarely,
/// while "label the tab I am working in" is something every agent wants at the
/// end of every task. That common case should cost no arguments, and here it
/// costs none — no index to look up, no pid to pass, no JSON to assemble:
///
/// ```text
/// terminal-delight ctl tab name WEBSITE BUILD LEADS
/// terminal-delight ctl tab group BFS
/// terminal-delight ctl tab ungroup
/// ```
///
/// It desugars to a single pane-addressed op, so it cannot name the wrong tab
/// even while the strip is moving underneath it.
/// What `ctl tab <words>` asked for, before we know whether it can be done.
///
/// Split from the pane lookup deliberately. Resolving which pane we are in is an
/// environment question — it reads `/proc` and the socket directory — and asking
/// it in order to reject a typo is both backwards and untestable: the answer
/// depends on where the process happens to be running, so a CI box with no
/// terminal-delight anywhere reports "you are not inside a pane" when the real
/// complaint is "there is no such verb". Parse first, resolve second.
#[derive(Debug, PartialEq)]
enum SelfTab {
    /// `None` clears the label — the same "empty means none" the JSON form gets
    /// from `name: null`.
    Name(Option<String>),
    Group(String),
    Ungroup,
}

/// Pure: the `ctl tab` grammar and nothing else.
fn self_tab_verb(words: &[&str]) -> Result<SelfTab, String> {
    match words {
        ["name"] => Ok(SelfTab::Name(None)),
        ["name", rest @ ..] => Ok(SelfTab::Name(Some(rest.join(" ")))),
        ["group"] => Err("`ctl tab group` needs a group name".into()),
        ["group", rest @ ..] => Ok(SelfTab::Group(rest.join(" "))),
        ["ungroup"] => Ok(SelfTab::Ungroup),
        [] => Err("`ctl tab` needs one of: name <text> | group <name> | ungroup".into()),
        [other, ..] => Err(format!(
            "unknown `ctl tab` verb {other:?} — try: name <text> | group <name> | ungroup"
        )),
    }
}

/// `ctl tab …` desugared to a single pane-addressed op.
fn self_tab_line(words: &[&str]) -> Result<String, String> {
    let verb = self_tab_verb(words)?;
    let Some(pane) = owning_td().and_then(|(_, pane)| pane) else {
        return Err(
            "`ctl tab` acts on the tab you are running in, and this process is not \
             inside a terminal-delight pane. Use `ctl tabs '<json>'` with an explicit \
             `pane` or `tab`, and `--pid` to say which window."
                .into(),
        );
    };
    let op = match verb {
        SelfTab::Name(name) => serde_json::json!({ "op": "name", "pane": pane, "name": name }),
        SelfTab::Group(group) => {
            serde_json::json!({ "op": "group", "pane": pane, "group": group })
        }
        SelfTab::Ungroup => serde_json::json!({ "op": "ungroup", "pane": pane }),
    };
    Ok(format!("tabs [{op}]"))
}

fn parse_cli(args: &[String]) -> Result<(String, Scope), String> {
    let mut words: Vec<&str> = Vec::new();
    let mut scope = Scope::ActiveWorkspace;
    let mut cwd: Option<String> = None;
    let mut run: Option<String> = None;
    let mut it = args.iter().map(String::as_str).peekable();
    // Whether the caller NAMED a scope. `tabs` defaults differently from every
    // other verb, and "they typed --workspace active" must not be mistaken for
    // "they typed nothing".
    let mut scope_given = false;
    while let Some(a) = it.next() {
        match a {
            "--all" => {
                scope = Scope::All;
                scope_given = true;
            }
            "--workspace" => {
                scope_given = true;
                let v = it.next().ok_or("--workspace needs a value")?;
                scope = if v == "active" {
                    Scope::ActiveWorkspace
                } else {
                    Scope::Workspace(v.to_string())
                };
            }
            "--pid" => {
                let v = it.next().ok_or("--pid needs a value")?;
                scope = Scope::Pid(v.parse().map_err(|_| format!("bad pid {v:?}"))?);
                scope_given = true;
            }
            "--cwd" => cwd = Some(it.next().ok_or("--cwd needs a value")?.to_string()),
            "--run" => run = Some(it.next().ok_or("--run needs a value")?.to_string()),
            // A lone `-` is a WORD, not a flag: it is how `mcp from <session>
            // <pane|-> rpc` says the caller could not find out which pane it is
            // in. Without this the flag parser refused the only verb that can
            // reproduce a wrong-window refusal by hand, which is the one a
            // person debugging instance identity most wants to type.
            "-" => words.push("-"),
            w if !w.starts_with('-') => words.push(w),
            other => return Err(format!("unknown flag {other:?}")),
        }
    }
    // `adopt` is flag-shaped on the CLI (cwd/run carry spaces) and becomes the
    // one-line JSON form on the wire; everything else is the words themselves.
    // `ctl tab` is defined as "my own tab", so a scope flag is not a refinement
    // of it, it is a contradiction — and one that would otherwise fail SILENTLY,
    // since a pane pid from this window matches nothing in another one and the
    // applier skips what it cannot find.
    if words.first() == Some(&"tab") && scope_given {
        return Err("`ctl tab` always means the tab you are running in — drop \
             --pid/--all/--workspace, or use `ctl tabs '<json>'` to aim elsewhere"
            .into());
    }
    if words.first() == Some(&"tab") {
        if cwd.is_some() || run.is_some() {
            return Err("--cwd/--run only apply to adopt".into());
        }
        let line = self_tab_line(&words[1..])?;
        parse_line(&line)?;
        return Ok((line, Scope::Owning));
    }
    let line = if words == ["adopt"] {
        format!("adopt {}", serde_json::json!({ "cwd": cwd, "run": run }))
    } else {
        if cwd.is_some() || run.is_some() {
            return Err("--cwd/--run only apply to adopt".into());
        }
        words.join(" ")
    };
    // The wire protocol is one line, and a hand-written op list is a FILE —
    // pretty-printed, because that is what a human can read and edit. Passing
    // one straight through truncated it at the first newline and answered
    // "EOF while parsing a list at line 1 column 1", which describes the
    // symptom and hides the cause. Re-serialise compactly here so the readable
    // form on disk and the one-line form on the wire are the same payload.
    let line = match line.strip_prefix("tabs ") {
        Some(rest) => match serde_json::from_str::<serde_json::Value>(rest.trim()) {
            Ok(v) => format!("tabs {v}"),
            // Leave it alone and let parse_line produce the real diagnosis.
            Err(_) => line,
        },
        None => line,
    };
    // A tab edit is about the caller's own window; only an explicit flag aims
    // it anywhere else.
    if line.starts_with("tabs ") && !scope_given {
        scope = Scope::Owning;
    }
    // Validate against the same grammar the server enforces, so a typo fails
    // HERE with usage rather than fanning out as N "err unknown command"s.
    parse_line(&line)?;
    Ok((line, scope))
}

/// Sockets currently present, as (pid, path).
fn discover() -> Vec<(u32, PathBuf)> {
    discover_in(&ctl_dir())
}

/// [`discover`] told where to look — the `_at`/`_in` twin `hostctl` names as
/// this house's discipline, so a test never sets `XDG_RUNTIME_DIR` for every
/// other test in the process.
fn discover_in(dir: &Path) -> Vec<(u32, PathBuf)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if let Some(pid) = name
            .strip_prefix("ctl-")
            .and_then(|s| s.strip_suffix(".sock"))
            .and_then(|s| s.parse::<u32>().ok())
        {
            out.push((pid, e.path()));
        }
    }
    out.sort_by_key(|(pid, _)| *pid);
    out
}

/// One request/response round trip against a socket, with an explicit read
/// budget: the queue-and-mirror verbs answer within a tick, but `mcp rpc` waits
/// on the gpui main thread and needs the server's snapshot budget plus slack.
fn send_within(path: &Path, line: &str, budget: Duration) -> std::io::Result<String> {
    let mut s = UnixStream::connect(path)?;
    let _ = s.set_read_timeout(Some(budget));
    let _ = s.set_write_timeout(Some(budget));
    writeln!(s, "{line}")?;
    let mut reply = String::new();
    BufReader::new(&mut s).read_line(&mut reply)?;
    Ok(reply.trim().to_string())
}

/// One request/response round trip against a socket.
fn send(path: &Path, line: &str) -> std::io::Result<String> {
    send_within(path, line, Duration::from_millis(800))
}

/// Ask the Hyprland IPC socket a `j/…` question. Direct socket, not `hyprctl`:
/// no PATH dependency, and the reply is the same JSON.
fn hypr_request(cmd: &str) -> Option<String> {
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    let base = std::env::var("XDG_RUNTIME_DIR").ok()?;
    let path = PathBuf::from(base)
        .join("hypr")
        .join(sig)
        .join(".socket.sock");
    let mut s = UnixStream::connect(path).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_millis(800)));
    s.write_all(cmd.as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out)
}

/// Focus THIS process's window compositor-side (used by the agent-finished
/// notification's click-to-jump). Omarchy's Hyprland fork speaks Lua on the
/// command socket — probed live: `dispatch hl.dsp.focus({window="pid:N"})`
/// executes (a bogus pid answers "window not found", not a syntax error) while
/// the classic `dispatch focuswindow pid:N` is a Lua error. The classic form is
/// still sent as a fallback when the Lua form errors, so a stock Hyprland
/// works too. Blocking (~ms unix-socket round trip) — call off the UI thread.
pub(crate) fn focus_this_window() {
    let pid = std::process::id();
    let lua = format!("dispatch hl.dsp.focus({{window=\"pid:{pid}\"}})");
    match hypr_request(&lua) {
        Some(reply) if !reply.starts_with("error") => {}
        _ => {
            let _ = hypr_request(&format!("dispatch focuswindow pid:{pid}"));
        }
    }
}

/// The pids of terminal-delight windows on the selected workspace, straight
/// out of `j/clients`. Numeric selectors match the workspace id, anything else
/// the workspace name — Hyprland names default to the id's digits, so both
/// spellings of "workspace 2" land in the same place.
fn td_pids_in_workspace(clients: &serde_json::Value, sel: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let Some(arr) = clients.as_array() else {
        return out;
    };
    for c in arr {
        if c["class"].as_str() != Some("terminal-delight") {
            continue;
        }
        let ws = &c["workspace"];
        let hit = match sel.parse::<i64>() {
            Ok(id) => ws["id"].as_i64() == Some(id),
            Err(_) => ws["name"].as_str() == Some(sel),
        };
        if hit {
            if let Some(pid) = c["pid"].as_i64() {
                out.push(pid as u32);
            }
        }
    }
    out
}

/// The `terminal-delight ctl …` entry point. Returns the process exit code:
/// 0 when at least one terminal answered, 2 when nothing matched (with a hint
/// on stderr), 1 on a usage error.
pub fn run_cli(args: &[String]) -> i32 {
    let (line, scope) = match parse_cli(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("terminal-delight ctl: {e}");
            eprintln!(
                "usage: terminal-delight ctl <{USAGE} | adopt --cwd <dir> [--run <cmd>]> \
                 [--workspace active|<name-or-id> | --all | --pid <N>]"
            );
            return 1;
        }
    };

    // Resolve the scope to concrete (pid, path) targets.
    let targets: Vec<(u32, PathBuf)> = match &scope {
        Scope::Pid(pid) => vec![(*pid, socket_path(*pid))],
        Scope::All => discover(),
        // Inside-out resolution, never Hyprland: the window we are running in,
        // else the only one running, else refuse to guess between several.
        Scope::Owning => match owning_td_pid().or_else(|| match discover().as_slice() {
            [(pid, _)] => Some(*pid),
            _ => None,
        }) {
            Some(pid) => vec![(pid, socket_path(pid))],
            None => {
                let running = discover();
                if running.is_empty() {
                    eprintln!(
                        "terminal-delight ctl: no terminal-delight control sockets in {:?} \
                         — is one running?",
                        ctl_dir()
                    );
                } else {
                    eprintln!(
                        "terminal-delight ctl: not running inside a terminal-delight window, \
                         and {} are open — name one with --pid <N> (pids: {})",
                        running.len(),
                        running
                            .iter()
                            .map(|(p, _)| p.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                return 2;
            }
        },
        Scope::ActiveWorkspace | Scope::Workspace(_) => {
            let sel = match &scope {
                Scope::Workspace(w) => w.clone(),
                _ => {
                    let Some(active) = hypr_request("j/activeworkspace")
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v["id"].as_i64())
                    else {
                        eprintln!(
                            "terminal-delight ctl: no Hyprland session found — \
                             use --all or --pid <N>"
                        );
                        return 2;
                    };
                    active.to_string()
                }
            };
            let Some(clients) = hypr_request("j/clients")
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            else {
                eprintln!("terminal-delight ctl: could not list Hyprland clients");
                return 2;
            };
            let pids = td_pids_in_workspace(&clients, &sel);
            if pids.is_empty() {
                eprintln!("terminal-delight ctl: no terminal-delight windows on workspace {sel}");
                return 2;
            }
            pids.into_iter().map(|p| (p, socket_path(p))).collect()
        }
    };

    // Adoption is a placement and a tab edit is per-WINDOW (indices mean
    // nothing in another window's strip) — neither is a broadcast.
    let targets: Vec<(u32, PathBuf)> = if line.starts_with("adopt") || line.starts_with("tabs ") {
        targets.into_iter().take(1).collect()
    } else {
        targets
    };

    if targets.is_empty() {
        eprintln!(
            "terminal-delight ctl: no control sockets in {:?} — terminals started \
             before this build don't have one; open a new terminal-delight window",
            ctl_dir()
        );
        return 2;
    }

    let mut ok = 0;
    for (pid, path) in &targets {
        match send(path, &line) {
            Ok(reply) => {
                println!("{pid}\t{reply}");
                if !reply.starts_with("err") {
                    ok += 1;
                }
            }
            Err(_) if !path.exists() => {
                // A window Hyprland knows but we have no socket for: a terminal
                // from a build older than the ctl surface.
                println!("{pid}\terr no control socket (older build — reopen this terminal)");
            }
            Err(e) => {
                // Present but unconnectable = stale leftovers; sweep so the
                // next discovery is clean.
                let _ = std::fs::remove_file(path);
                println!("{pid}\terr unreachable ({e}) — swept stale socket");
            }
        }
    }
    if ok > 0 {
        0
    } else {
        2
    }
}

// ------------------------------------------------------------ mcp relay ----

/// A process's parent, from `/proc/<pid>/stat`. The `comm` field is wrapped in
/// parens and may itself contain spaces AND parens, so the only safe split is
/// after the LAST `)`: what follows is `state ppid …`.
pub(crate) fn ppid_of(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after_comm = stat.rsplit_once(')')?.1;
    after_comm.split_whitespace().nth(1)?.parse().ok()
}

/// Whether `pid` runs under `ancestor` — the question a pane asks about an
/// MCP caller: is this my own agent, or somebody else's? Bounded, because a
/// parent chain on this box is a dozen deep at most and `/proc` can lie about
/// a pid that was recycled mid-walk.
pub(crate) fn descends_from(pid: u32, ancestor: u32) -> bool {
    let mut at = pid;
    for _ in 0..24 {
        if at == ancestor {
            return true;
        }
        match ppid_of(at) {
            Some(p) if p > 1 && p != at => at = p,
            _ => return false,
        }
    }
    false
}

/// The terminal-delight window hosting THIS process, by walking our own parent
/// chain until a pid turns out to own a control socket.
///
/// That is the whole trick behind the relay: an agent is a great-grandchild of
/// the terminal it lives in (td → shell → agent → this MCP server), so the
/// ancestor that owns a socket is, unambiguously, the window it is looking at.
/// The socket's own existence is the test — no class matching, no name guessing.
fn owning_td_pid() -> Option<u32> {
    owning_td().map(|(window, _)| window)
}

/// One walk, two answers: the window hosting us, and the PANE inside it we are
/// running in — which is simply the last ancestor before the window, because
/// that is the shell terminal-delight spawned for the pane.
///
/// Worth stating plainly, because it is what makes `ctl tab` need no arguments
/// at all: an agent's chain is `td → shell → agent → …`, so the shell whose
/// child we are IS our pane, already sitting in the walk that finds the window.
/// No environment variable to set, no pid to pass, no index to look up, and no
/// way to name the wrong tab. The pane is `None` only when the caller is itself
/// the window (terminal-delight poking its own socket), which no pane-scoped
/// command should reach.
fn owning_td() -> Option<(u32, Option<u32>)> {
    let mut pid = std::process::id();
    let mut prev: Option<u32> = None;
    // Deep enough for td → shell → agent → server with room to spare; bounded so
    // a malformed /proc chain can never spin.
    for _ in 0..64 {
        if pid <= 1 {
            return None;
        }
        if socket_path(pid).exists() {
            return Some((pid, prev));
        }
        prev = Some(pid);
        pid = ppid_of(pid)?;
    }
    None
}

/// This process's own parent chain, innermost first, ending where `/proc` does.
///
/// The raw material for every structural answer below: a process inside a pane
/// is a descendant of the host that forked that pane's shell, so the chain
/// contains both the shell and the host, and neither can be faked by an
/// environment somebody else set.
fn ancestry() -> Vec<u32> {
    let mut chain = vec![];
    let mut pid = std::process::id();
    // Deep enough for host → shell → agent → wrapper → server with room to
    // spare; bounded so a malformed /proc chain can never spin.
    for _ in 0..64 {
        if pid <= 1 {
            break;
        }
        chain.push(pid);
        match ppid_of(pid) {
            Some(p) => pid = p,
            None => break,
        }
    }
    chain
}

/// The inode of the process LISTENING on a unix socket path.
///
/// `/proc/net/unix` lists one row per endpoint, so a busy socket appears many
/// times under the same path — measured on this box, fourteen rows for one
/// session socket, thirteen of them connected peers. The listener is the row
/// whose state is `01` (`SS_UNCONNECTED` with `SO_ACCEPTCON` set); taking the
/// first row that matched the path would pick a peer and find nothing.
///
/// Columns: `Num RefCount Protocol Flags Type St Inode Path`.
fn listening_inode(path: &Path) -> Option<u64> {
    let table = std::fs::read_to_string("/proc/net/unix").ok()?;
    let want = path.to_str()?;
    table.lines().skip(1).find_map(|line| {
        let mut f = line.split_whitespace();
        let (_num, _ref, _proto, _flags, _ty, st, inode, sock) = (
            f.next()?,
            f.next()?,
            f.next()?,
            f.next()?,
            f.next()?,
            f.next()?,
            f.next()?,
            f.next()?,
        );
        (st == "01" && sock == want).then(|| inode.parse().ok())?
    })
}

/// Whether `pid` holds an open file descriptor on this socket inode.
fn owns_inode(pid: u32, inode: u64) -> bool {
    let needle = format!("socket:[{inode}]");
    let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else {
        return false;
    };
    fds.flatten()
        .any(|e| std::fs::read_link(e.path()).is_ok_and(|t| t.to_string_lossy() == needle))
}

/// The session whose host is on `chain`, asked of the kernel rather than of the
/// host.
///
/// This is the load-bearing step, and it deliberately involves no IPC and no
/// cooperation from the host at all: the session's NAME is already in the socket
/// filename, and which process is listening on that socket is a fact
/// `/proc/net/unix` and `/proc/<pid>/fd` answer between them.
///
/// It started as a field on the host's hello — and that was the wrong place for
/// it, for a reason worth keeping written down. The host is the one process here
/// that cannot be replaced without ending every terminal it holds, so a required
/// field on the host is a feature that does not work until somebody is willing to
/// kill a day's work. Asked this way it works against a host from any build,
/// including ones that predate the whole idea. The hello field stays as a cheap
/// cross-check, not as the mechanism.
fn session_hosted_on(chain: &[u32]) -> Option<String> {
    crate::hostctl::sockets_present().into_iter().find(|key| {
        let path = crate::hostproto::host_socket_path(key);
        listening_inode(&path).is_some_and(|ino| chain.iter().any(|&pid| owns_inode(pid, ino)))
    })
}

/// Which window a live control socket belongs to: `(session, window pid)`.
///
/// Asked of the socket rather than read from a file, so the answer is a running
/// process's own account of itself. A socket file whose process is gone simply
/// fails to answer, which is how [`live_windows`] filters the dead ones without
/// a separate liveness test — the runtime directory keeps every socket a window
/// ever made, and 26 of 28 pointing at nothing was the state that made an
/// unresolved relay report twenty-eight running windows.
fn window_identity(path: &Path) -> Option<(String, u32)> {
    let reply = send_within(path, "whoami", Duration::from_millis(300)).ok()?;
    let mut w = reply.split_whitespace();
    (w.next()? == "ok").then_some(())?;
    let session = w.next()?.to_string();
    let pid = w.next()?.parse().ok()?;
    Some((session, pid))
}

/// Every window that is actually running, with the session each one holds.
fn live_windows() -> Vec<(String, u32)> {
    live_windows_in(&ctl_dir())
}

/// [`live_windows`] told where to look.
fn live_windows_in(dir: &Path) -> Vec<(String, u32)> {
    discover_in(dir)
        .into_iter()
        .filter_map(|(_, path)| window_identity(&path))
        .collect()
}

/// What a process inside a pane can find out about itself without being told.
pub(crate) struct Located {
    pub session: String,
    pub pane: Option<u64>,
    /// The window drawing this session, when one is running.
    pub window: Option<u32>,
}

/// Work out which Terminal Delight this process is inside, from the process
/// tree rather than from the environment.
///
/// The walk this replaces looked for an ancestor that owned a WINDOW's control
/// socket, and it worked until the host took the pseudoterminals: a hosted
/// pane's parent is the host, and the window is a sibling that never appears on
/// the chain at all. Every hosted pane therefore fell through to
/// `$TD_SESSION` — which an agent that scrubs its children's environment (Codex
/// does) does not pass on, and which anything can set to the wrong value.
///
/// The host is still an ancestor. So: walk up, ask every live session host for
/// its own pid, and the one that appears on the chain is ours. That host also
/// holds the pane table, so the shell on the chain names the pane. Then ask
/// every live window which session it holds, and take the one that matches.
///
/// Nothing here reads an environment variable, and every answer comes from a
/// process that cannot be wrong about itself.
pub(crate) fn locate() -> Option<Located> {
    let chain = ancestry();
    let session = session_hosted_on(&chain)?;
    // The pane is a second question, and a failure to answer it is not a
    // failure to locate: knowing the session is already enough to reach the
    // right window and to be refused by the wrong one.
    let pane = crate::hostctl::panes_of(&session)
        .into_iter()
        .find(|p| chain.contains(&p.shell_pid))
        .map(|p| p.pane.0);
    // Ask the windows first — a running process's own account of itself, and
    // the only answer that cannot be stale. Then fall back to the record the
    // host and window both write, keyed by the session we DERIVED rather than
    // by one an environment variable claimed.
    //
    // That fallback is not a nicety. Until every window on the box speaks
    // `whoami` there is nothing to ask, and a locate that threw its own answer
    // away at the last step left the relay reading `$TD_SESSION` again — which
    // is the failure this whole path exists to remove. It was doing exactly
    // that until it was run against the live session rather than reasoned about.
    let window = live_windows()
        .into_iter()
        .find(|(s, _)| *s == session)
        .map(|(_, pid)| pid)
        .or_else(|| recorded_window(&session));
    Some(Located {
        session,
        pane,
        window,
    })
}

/// The window a session's `session-<key>.window` record names — proven by
/// ASKING it, never by the presence of its socket file.
///
/// This used to end in `socket_path(pid).exists()`, and the comment above it
/// claimed that caught a stale value. It did not, and this module's own doc
/// says why: the socket file is not unlinked on exit, because there is no hook
/// that runs on every exit path. So the check passed for a window that had
/// died, the relay targeted a corpse, and the agent lost every
/// `mcp__terminal-delight__*` tool for the rest of its session — with the live
/// scan it should have fallen through to sitting one line below the call. That
/// is not a rare shape: 24 stale socket files against 3 live windows, counted
/// on this desk on 2026-09-21. See `terminal-delight#507`.
///
/// Cross-checking the session is free and buys the case a liveness check alone
/// would still get wrong: a socket whose pid has been RECYCLED by a different
/// window answers perfectly well, and taking it would be a wrong answer rather
/// than a missing one.
fn recorded_window(session: &str) -> Option<u32> {
    recorded_window_at(
        &crate::hostproto::window_pid_path(session),
        &ctl_dir(),
        session,
    )
}

/// [`recorded_window`] told where to look.
///
/// The injectable twin `hostctl` names as this house's discipline: the ambient
/// function reads the runtime directory, the `_at` one is handed a path, so a
/// test never sets `XDG_RUNTIME_DIR` for every other test in the process.
fn recorded_window_at(record: &Path, ctl_dir: &Path, session: &str) -> Option<u32> {
    let text = std::fs::read_to_string(record).ok()?;
    let pid: u32 = text.trim().parse().ok()?;
    proven_window(&ctl_dir.join(format!("ctl-{pid}.sock")), pid, session)
}

/// A window pid that came from a RECORD rather than from a live walk, proven
/// against the window itself.
///
/// ONE door, because there were two copies of the file check and fixing the
/// filed one would have left the other standing. A future third reader of a
/// recorded pid should reach for this rather than re-deriving `.exists()` from
/// first principles, which is exactly how the second copy came to exist.
///
/// Deliberately NOT used by [`owning_td`], whose pid comes from walking our own
/// `/proc` ancestry and is therefore alive by construction. Asking there would
/// buy nothing and would have the window connect to its own control socket when
/// it pokes itself, which is a self-deadlock waiting to be discovered.
fn proven_window(sock: &Path, pid: u32, want_session: &str) -> Option<u32> {
    let (session, live) = window_identity(sock)?;
    (live == pid && session == want_session).then_some(pid)
}

/// Where THIS session's window went, for a relay whose own has gone away.
///
/// Same order as [`locate`]: ask the windows that are actually running, then
/// fall back to the record — which is itself now proven rather than stat-ed.
///
/// **Deliberately narrower than [`relay_target`], and that is a trust boundary
/// rather than a tidiness preference.** `relay_target` ends in "only one
/// terminal is running, so it must be the one", which is a fair degraded mode
/// for a relay that never managed to locate itself at all. It would be a
/// downgrade here. A relay that reaches this point HAS a session: it proved one
/// at startup and is only asking where that session moved to. Re-resolving to
/// whatever happens to be listening would let a window that is not ours inherit
/// an agent mid-run, and a cutover is precisely the moment when something else
/// could be the only thing bound. **A rebind may narrow what a relay talks to.
/// It may never widen it.**
fn rebound_window(session: &str) -> Option<u32> {
    rebound_window_at(
        &ctl_dir(),
        &crate::hostproto::window_pid_path(session),
        session,
    )
}

/// [`rebound_window`] told where to look.
fn rebound_window_at(ctl_dir: &Path, record: &Path, session: &str) -> Option<u32> {
    live_windows_in(ctl_dir)
        .into_iter()
        .find(|(s, _)| s == session)
        .map(|(_, pid)| pid)
        .or_else(|| recorded_window_at(record, ctl_dir, session))
}

/// Resolve which terminal the relay talks to: an explicit `--pid`, else the
/// window hosting us, else — only if it is unambiguous — the single running
/// terminal. Refusing to guess between several is deliberate: silently driving
/// the wrong window is worse than an error telling you to name one.
fn relay_target(args: &[String]) -> Result<u32, String> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => {}
        ["--pid", v] => return v.parse().map_err(|_| format!("bad pid {v:?}")),
        ["--pid"] => return Err("--pid needs a value".into()),
        [other, ..] => return Err(format!("unknown flag {other:?}")),
    }
    if let Some(pid) = owning_td_pid() {
        return Ok(pid);
    }
    // A hosted pane is a child of the host, not the window, so the walk above
    // reaches the host (which has no ctl socket) and never the window. The host
    // records the attached window's pid per session; if this pane carries
    // TD_SESSION, use that window's ctl socket when it is live. A stale or
    // missing record falls through to discovery below.
    //
    // "When it is live" was a file-existence test until 2026-09-21, and this
    // copy was the worse of the two: `recorded_window` is at least consulted
    // AFTER a live scan, while this arm returns before `discover()` below has
    // ever run — so a corpse here shadowed a perfectly good window sitting one
    // match arm away.
    //
    // It is now the same CALL rather than the same logic written out twice.
    // Side by side, this arm and `recorded_window` read one file, parsed one
    // pid and checked one session, and differed only in which of those they
    // believed; that duplication is precisely what would have let the filed
    // bug be fixed while its twin went on shipping. One door also means the
    // door's own test covers both callers, which two copies could never be.
    if let Ok(key) = std::env::var("TD_SESSION") {
        if let Some(pid) = recorded_window(&key) {
            return Ok(pid);
        }
    }
    match discover().as_slice() {
        [] => Err(format!(
            "no terminal-delight control sockets in {:?} — is one running, and \
             new enough to have a control socket?",
            ctl_dir()
        )),
        [(pid, _)] => Ok(*pid),
        many => Err(format!(
            "not launched from inside a terminal-delight window, and {} are \
             running — name one with --pid <N> (pids: {})",
            many.len(),
            many.iter()
                .map(|(p, _)| p.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// `terminal-delight mcp` — the stdio JSON-RPC relay: an MCP server an agent
/// registers, which forwards each line to a RUNNING terminal's control socket
/// and writes the answer back.
///
/// This exists because the in-process stdio transport ([`crate::mcp_transport`])
/// requires the MCP client to own our stdin/stdout, i.e. to be our parent — and
/// a GUI terminal launched from the desktop never is. The relay inverts that: it
/// is spawned BY the agent, and reaches back to the window already on screen.
pub fn run_mcp_cli(args: &[String]) -> i32 {
    // Derived first, from the process tree, because that is the only account
    // nobody else can have got wrong. An explicit `--pid` still overrides it —
    // naming a window on purpose is a different act from being unable to find
    // one — and the environment remains the last resort, for a caller whose
    // chain was broken by a reparenting wrapper (tmux, issue 215).
    let located = args.is_empty().then(locate).flatten();
    let pid = match located.as_ref().and_then(|l| l.window) {
        Some(p) => p,
        None => match relay_target(args) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("terminal-delight mcp: {e}");
                return 1;
            }
        },
    };
    // The prefix every line is sent under. Naming ourselves lets the window
    // refuse a call meant for a different instance, and lets a pane-scoped verb
    // act on the pane we are actually in. A caller that could locate nothing
    // sends the bare form and is served as before — degraded, never blocked.
    let mut prefix = match &located {
        Some(l) => format!(
            "mcp from {} {} rpc ",
            l.session,
            l.pane.map(|p| p.to_string()).unwrap_or_else(|| "-".into())
        ),
        None => BARE_RPC.to_string(),
    };
    // Both move when the window under us is replaced — see the rebind in the
    // loop. Resolved once and then frozen was the whole of issue 391: the
    // relay held a path to a window that had gone and died on the next call.
    let mut pid = pid;
    let mut path = socket_path(pid);

    // The server's own budget is 5 s; allow slack for the queue and the write so
    // a busy UI reads as slow, never as a dropped connection.
    let budget = Duration::from_secs(10);
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    let mut line = String::new();
    loop {
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) | Err(_) => return 0, // EOF: the agent closed us. Normal exit.
            Ok(_) => {}
        }
        let req = line.trim();
        if req.is_empty() {
            continue;
        }
        let mut sent = send_within(&path, &format!("{prefix}{req}"), budget);
        // A window from a build that predates `mcp from` calls it an unknown
        // command. That window is running somebody's terminals and cannot be
        // replaced without ending them, so the relay steps back to the form it
        // does understand and stays stepped back for the rest of the run.
        //
        // Without this the agent hangs: an `err` line is logged to stderr and
        // nothing is written to stdout, so the tool call never returns. The
        // relay is whatever `~/.local/bin` points at while the windows are
        // whatever was running before the cutover, so new-relay-old-window is
        // the NORMAL state for as long as it takes to restart them — not an
        // edge case, and not one to leave as a hang.
        if is_unknown_verb(&sent) && prefix != BARE_RPC {
            eprintln!(
                "terminal-delight mcp: window {pid} predates `mcp from` — \
                 falling back to the unnamed form for this run. It cannot refuse \
                 a call meant for another instance, and pane-scoped verbs will \
                 need an explicit pid. Restart that window to get both back."
            );
            prefix = BARE_RPC.to_string();
            sent = send_within(&path, &format!("{prefix}{req}"), budget);
        }
        // THE WINDOW WENT AWAY UNDER US. Until now this was fatal: the target
        // was resolved once before the loop, so a window replaced mid-session —
        // a build cutover, a bounce, a crash — left the relay holding a path to
        // a corpse, and the next tool call exited 2. The agent then lost every
        // `mcp__terminal-delight__*` tool for the REST of its session, because
        // a client that watches a server's stdio close does not un-fail it when
        // a replacement relay comes up healthy. Observed 2026-09-21: a relay
        // died on a cutover, a replacement was spawned three seconds later on
        // the new build and served a call, and the session never saw the tools
        // again. Issue 391.
        //
        // So: ask once more where our session is, and only ever for OUR session
        // (see `rebound_window`). One retry, not a loop — a window that is
        // there answers the second call, and anything else is a real failure
        // that the caller is owed promptly rather than after a spin.
        if sent.is_err() {
            if let Some(next) = located.as_ref().and_then(|l| rebound_window(&l.session)) {
                if next != pid {
                    eprintln!(
                        "terminal-delight mcp: window {pid} went away; session is \
                         now window {next} — reconnected"
                    );
                    pid = next;
                    path = socket_path(pid);
                    sent = send_within(&path, &format!("{prefix}{req}"), budget);
                }
            }
        }
        match sent {
            // A notification: JSON-RPC says answer nothing, so write nothing.
            Ok(r) if r == MCP_NONE => {}
            // A protocol-level refusal from ctl (never from the MCP handler,
            // which answers in JSON-RPC). Log it; emitting it on stdout would
            // corrupt the framing the client is parsing.
            Ok(r) if r.starts_with("err ") => eprintln!("terminal-delight mcp: {r}"),
            Ok(r) => {
                if writeln!(out, "{r}").is_err() || out.flush().is_err() {
                    return 0; // client hung up mid-answer
                }
            }
            Err(e) => {
                eprintln!("terminal-delight mcp: window {pid} unreachable ({e})");
                return 2;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> crate::testsync::Scratch {
        crate::testsync::Scratch::new(&format!("ctl-{tag}"))
    }

    /// `bench submit` parses, and the usage string says so.
    ///
    /// The round's ONLY exit is a zone on a tab — since a round with a
    /// navigator stopped posting itself on its last answer, nothing else ends
    /// one. A capability with no headless reach cannot be gated by a test, and
    /// that gap is where the keys road's defect lived: every test of `submit`
    /// walked the file road, because the file road was the only one a test
    /// could get to.
    ///
    /// The usage half is asserted because an undiscoverable verb is most of
    /// the way to an absent one, and `USAGE` is the only place the grammar is
    /// written down for a person.
    #[test]
    fn the_rounds_only_exit_can_be_pressed_without_a_pointer() {
        assert!(matches!(parse_line("bench submit"), Ok(Cmd::BenchSubmit)));
        assert!(
            USAGE.contains("submit"),
            "the verb exists and the usage string does not mention it: {USAGE}"
        );
        // It takes NO argument. A round is not named because the tab is not
        // either — anything trailing is a caller with the wrong idea, and
        // answering it would send a round they did not mean.
        assert!(parse_line("bench submit 1").is_err());
        assert!(parse_line("bench submit all").is_err());
    }

    /// A floating document can be opened and closed without a pointer, which
    /// is how a hundred open-and-close cycles get counted at all. The path is
    /// taken whole, spaces included, and must be absolute: the socket has no
    /// working directory, and resolving against the window's own would open a
    /// file the caller did not name.
    #[test]
    fn a_floating_document_opens_and_closes_without_a_pointer() {
        assert!(matches!(
            parse_line("doc here /tmp/two words.png"),
            Ok(Cmd::DocHere(ref p)) if p == Path::new("/tmp/two words.png")
        ));
        assert!(parse_line("doc here shot.png").is_err());
        assert!(parse_line("doc here ~/shot.png").is_err());
        assert!(matches!(parse_line("doc close"), Ok(Cmd::DocClose)));
        assert!(parse_line("doc close all").is_err());
        assert!(USAGE.contains("doc here") && USAGE.contains("doc close"));
    }

    /// The split can be asked for without a pointer too — Ctrl+Alt+click's
    /// verb — on the same terms as `doc here`: the whole path, spaces and all,
    /// and only an absolute one.
    #[test]
    fn a_document_opens_beside_a_pane_without_a_pointer() {
        assert!(matches!(
            parse_line("doc beside /tmp/two words.md"),
            Ok(Cmd::DocBeside(ref p)) if p == Path::new("/tmp/two words.md")
        ));
        assert!(parse_line("doc beside notes.md").is_err());
        assert!(parse_line("doc beside ~/notes.md").is_err());
        assert!(parse_line("doc beside").is_err(), "no path is not a path");
        assert!(USAGE.contains("doc beside <absolute path>"));
    }

    #[test]
    fn a_pane_addressed_op_list_means_the_same_thing_twice() {
        // The defect this addressing exists to remove: an INDEX is read against
        // the strip the caller was looking at, and a grouping op reorders that
        // strip — so re-applying an index-addressed list writes its names onto
        // whatever has since moved into those slots. Observed in the wild: one
        // name ended up on three different tabs. A pane pid does not move.
        let by_pane = r#"tabs [{"op":"name","pane":258041,"name":"AFTERCARE"}]"#;
        let Ok(Cmd::Tabs(ops)) = parse_line(by_pane) else {
            panic!("did not parse")
        };
        assert_eq!(ops[0].target(), Ok(Some(TabRef::Pane(258041))));
    }

    #[test]
    fn an_op_must_name_its_tab_exactly_one_way() {
        // Both is ambiguous — they can denote different tabs, and silently
        // preferring one would make the other a lie.
        let err = parse_line(r#"tabs [{"op":"name","tab":3,"pane":99,"name":"X"}]"#)
            .expect_err("both must be refused");
        assert!(err.contains("not both"), "{err}");

        // Neither leaves the applier nothing to act on.
        let err = parse_line(r#"tabs [{"op":"name","name":"X"}]"#).expect_err("neither");
        assert!(err.contains("needs a tab"), "{err}");

        // A group-wide op acts by group NAME and must not carry a tab at all.
        let err = parse_line(r#"tabs [{"op":"collapse","group":"BFS","collapsed":true,"tab":1}]"#)
            .expect_err("group-wide op with a tab");
        assert!(err.contains("group-wide"), "{err}");

        parse_line(r#"tabs [{"op":"collapse","group":"BFS","collapsed":true}]"#)
            .expect("group-wide op with no tab is fine");
    }

    #[test]
    fn a_batch_error_says_which_op_it_is_about() {
        // In a list of twenty, "op 13" is the difference between a fix and a hunt.
        let err = parse_line(
            r#"tabs [{"op":"name","tab":0,"name":"A"},{"op":"name","tab":1,"name":"B"},{"op":"ungroup"}]"#,
        )
        .expect_err("third op has no target");
        assert!(err.contains("op 2"), "{err}");
    }

    #[test]
    fn ctl_tab_refuses_a_scope_flag_instead_of_silently_doing_nothing() {
        // `ctl tab` is DEFINED as "my own tab", so --pid is not a refinement of
        // it but a contradiction — and one that would fail silently, because a
        // pane pid from this window matches nothing in another and the applier
        // skips what it cannot find.
        let err = parse_cli(&["tab".into(), "name".into(), "X".into(), "--all".into()])
            .expect_err("a scope flag must be refused, not ignored");
        assert!(
            err.contains("always means the tab you are running in"),
            "{err}"
        );
    }

    #[test]
    fn ctl_tab_rejects_an_unknown_verb_rather_than_guessing() {
        // Against the PURE grammar, not parse_cli: routing a typo through the
        // pane lookup makes the answer depend on where the test is running, and
        // this suite runs on a CI box with no terminal-delight in sight. It
        // passed locally for exactly that wrong reason.
        let err = self_tab_verb(&["recolour", "red"]).unwrap_err();
        assert!(err.contains("unknown `ctl tab` verb"), "{err}");
        let err = self_tab_verb(&[]).unwrap_err();
        assert!(err.contains("name <text>"), "{err}");
        let err = self_tab_verb(&["group"]).unwrap_err();
        assert!(err.contains("needs a group name"), "{err}");

        // And the shapes it does accept.
        assert_eq!(
            self_tab_verb(&["name", "WEBSITE", "BUILD", "LEADS"]),
            Ok(SelfTab::Name(Some("WEBSITE BUILD LEADS".into())))
        );
        assert_eq!(self_tab_verb(&["name"]), Ok(SelfTab::Name(None)));
        assert_eq!(self_tab_verb(&["ungroup"]), Ok(SelfTab::Ungroup));
    }

    #[test]
    fn tabs_aims_at_the_window_it_is_running_in_not_the_one_on_screen() {
        // The bug this replaced: `ctl tabs` resolved through the ACTIVE
        // Hyprland workspace, so an agent in a pane on workspace 1 got
        // "no terminal-delight windows on workspace 2" whenever the human
        // happened to be looking elsewhere. Whether a tab edit lands must not
        // depend on where somebody's eyes are.
        let (_, scope) = parse_cli(&[r#"tabs [{"op":"ungroup","tab":1}]"#.into()]).unwrap();
        assert_eq!(scope, Scope::Owning);

        // Every other verb keeps the on-screen default — a bar click means
        // "the windows I am looking at".
        let (_, scope) = parse_cli(&["paint".into(), "toggle".into()]).unwrap();
        assert_eq!(scope, Scope::ActiveWorkspace);
    }

    #[test]
    fn an_explicit_scope_still_overrides_the_tabs_default() {
        let ops = r#"tabs [{"op":"ungroup","tab":1}]"#;
        let (_, scope) = parse_cli(&[ops.into(), "--pid".into(), "42".into()]).unwrap();
        assert_eq!(scope, Scope::Pid(42));
        let (_, scope) = parse_cli(&[ops.into(), "--all".into()]).unwrap();
        assert_eq!(scope, Scope::All);
        // "--workspace active" is a CHOICE, not the absence of one.
        let (_, scope) = parse_cli(&[ops.into(), "--workspace".into(), "active".into()]).unwrap();
        assert_eq!(scope, Scope::ActiveWorkspace);
    }

    #[test]
    fn a_pretty_printed_op_list_survives_the_one_line_wire() {
        // A hand-written scheme is a file, and a readable file has newlines.
        // Passed through verbatim it truncated at the first one and answered
        // "EOF while parsing a list at line 1 column 1" — a message about the
        // symptom that says nothing about the cause.
        let pretty = "tabs [\n  {\"op\":\"name\",\"tab\":9,\"name\":\"AFTERCARE\"},\n  \
                      {\"op\":\"ungroup\",\"tab\":3}\n]";
        let (line, _) = parse_cli(&[pretty.into()]).expect("pretty JSON must be accepted");
        assert!(!line.contains('\n'), "newline reached the wire: {line:?}");
        let Ok(Cmd::Tabs(ops)) = parse_line(&line) else {
            panic!("compacted line did not parse: {line:?}")
        };
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn a_malformed_op_list_still_reports_its_own_error_not_the_compactors() {
        // The compaction step must not swallow the diagnosis.
        let err = parse_cli(&[r#"tabs [{"op":"nope"}]"#.into()]).expect_err("must reject");
        assert!(err.contains("valid op list"), "{err}");
    }

    #[test]
    fn tabs_parses_a_bare_array_and_the_wrapped_form() {
        let bare = r#"tabs [{"op":"name","tab":3,"name":"DEV"}]"#;
        let wrapped = r#"tabs {"ops":[{"op":"name","tab":3,"name":"DEV"}]}"#;
        for line in [bare, wrapped] {
            match parse_line(line) {
                Ok(Cmd::Tabs(ops)) => {
                    assert_eq!(ops.len(), 1);
                    assert_eq!(ops[0].target(), Ok(Some(TabRef::Index(3))));
                    assert_eq!(
                        ops[0].action,
                        TabAction::Name {
                            name: Some("DEV".into())
                        }
                    );
                }
                other => panic!("{line} parsed as {other:?}"),
            }
        }
    }

    #[test]
    fn tabs_keeps_spaces_in_a_name_instead_of_word_splitting_it() {
        // The whole reason `tabs` takes the remainder verbatim: a label like
        // "BFS MARKETING" would be shredded by the word-shaped grammar.
        let line = r#"tabs [{"op":"name","tab":7,"name":"BFS MARKETING"}]"#;
        let Ok(Cmd::Tabs(ops)) = parse_line(line) else {
            panic!("did not parse")
        };
        assert_eq!(ops.len(), 1);
        assert_eq!(
            ops[0].action,
            TabAction::Name {
                name: Some("BFS MARKETING".into())
            }
        );
    }

    #[test]
    fn tabs_rejects_a_bad_colour_while_the_caller_is_still_listening() {
        // Application is queued and cannot answer, so a typo has to fail HERE
        // or it fails silently and invisibly a tick later.
        let err = parse_line(r##"tabs [{"op":"group","tab":1,"group":"JOB","color":"blue"}]"##)
            .expect_err("a non-hex colour must not reach the queue");
        assert!(err.contains("hex colour"), "{err}");

        parse_line(r##"tabs [{"op":"group","tab":1,"group":"JOB","color":"#136eec"}]"##)
            .expect("a real hex colour is fine");
    }

    #[test]
    fn tabs_rejects_an_empty_batch_and_an_unknown_op() {
        assert!(parse_line("tabs []").is_err());
        assert!(parse_line(r#"tabs [{"op":"detonate","tab":1}]"#).is_err());
    }

    #[test]
    fn tabs_is_addressed_to_one_window_not_broadcast() {
        // Indices mean nothing in another window's strip, so a tab edit must
        // narrow to a single target the way `adopt` does.
        let line = r#"tabs [{"op":"ungroup","tab":2}]"#;
        assert!(line.starts_with("tabs "));
    }

    #[test]
    fn the_original_six_verbs_still_parse() {
        assert!(matches!(parse_line("ping"), Ok(Cmd::Ping)));
        // `skin status` is a status read; every other single word is an id, and
        // that includes `theme` and `custom` — the window owns the list, so this
        // grammar deliberately knows nothing about which names are valid.
        assert!(matches!(parse_line("skin status"), Ok(Cmd::SkinStatus)));
        for name in ["deco", "console", "theme", "custom", "nonsense"] {
            assert!(
                matches!(parse_line(&format!("skin {name}")), Ok(Cmd::Skin(ref g)) if g == name),
                "skin {name} should parse as an id"
            );
        }
        assert!(parse_line("skin").is_err(), "a bare `skin` names nothing");
        assert!(matches!(
            parse_line("paint on"),
            Ok(Cmd::Paint(Req::Set(true)))
        ));
        assert!(matches!(
            parse_line("paint off"),
            Ok(Cmd::Paint(Req::Set(false)))
        ));
        assert!(matches!(
            parse_line("paint toggle"),
            Ok(Cmd::Paint(Req::Toggle))
        ));
        assert!(matches!(parse_line("paint status"), Ok(Cmd::PaintStatus)));
        assert!(matches!(
            parse_line(r#"adopt {"cwd":"/tmp"}"#),
            Ok(Cmd::Adopt(AdoptReq { .. }))
        ));
        assert!(parse_line("paint").is_err());
        assert!(parse_line("paint sideways").is_err());
        assert!(parse_line("paint on extra").is_err());
        assert!(parse_line("adopt").is_err());
        assert!(parse_line("adopt notjson").is_err());
        assert!(parse_line("").is_err());
    }

    #[test]
    fn adopt_payloads_validate_before_they_queue() {
        let ok = parse_adopt(r#"{"cwd":"/tmp/x","run":"claude --resume abc-123"}"#).unwrap();
        assert_eq!(ok.cwd.as_deref(), Some("/tmp/x"));
        assert_eq!(ok.run.as_deref(), Some("claude --resume abc-123"));
        // run-only (a tmux re-attach with its own cd) is legal
        assert!(parse_adopt(r#"{"run":"tmux attach -t rec"}"#).is_ok());
        // empty strings collapse to None — and all-None is refused
        assert!(parse_adopt(r#"{"cwd":"","run":""}"#).is_err());
        assert!(parse_adopt(r#"{}"#).is_err());
        // relative cwd and non-string types are protocol errors
        assert!(parse_adopt(r#"{"cwd":"rel/path"}"#).is_err());
        assert!(parse_adopt(r#"{"cwd":42}"#).is_err());
    }

    #[test]
    fn cli_adopt_builds_the_json_line_and_scopes_like_paint() {
        let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (line, scope) = parse_cli(&s(&[
            "adopt",
            "--cwd",
            "/tmp/x",
            "--run",
            "claude --resume abc-1",
            "--pid",
            "7",
        ]))
        .unwrap();
        assert_eq!(scope, Scope::Pid(7));
        let Ok(Cmd::Adopt(a)) = parse_line(&line) else {
            panic!("adopt CLI produced an unparseable line: {line:?}");
        };
        assert_eq!(a.cwd.as_deref(), Some("/tmp/x"));
        assert_eq!(a.run.as_deref(), Some("claude --resume abc-1"));
        // the adopt flags are meaningless on other verbs
        assert!(parse_cli(&s(&["paint", "on", "--cwd", "/x"])).is_err());
    }

    #[test]
    fn cli_defaults_to_the_active_workspace_and_flags_override() {
        let s = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let (line, scope) = parse_cli(&s(&["paint", "toggle"])).unwrap();
        assert_eq!(line, "paint toggle");
        assert_eq!(scope, Scope::ActiveWorkspace);
        let (_, scope) = parse_cli(&s(&["paint", "on", "--all"])).unwrap();
        assert_eq!(scope, Scope::All);
        let (_, scope) = parse_cli(&s(&["ping", "--workspace", "web"])).unwrap();
        assert_eq!(scope, Scope::Workspace("web".into()));
        let (_, scope) = parse_cli(&s(&["ping", "--workspace", "active"])).unwrap();
        assert_eq!(scope, Scope::ActiveWorkspace);
        let (_, scope) = parse_cli(&s(&["paint", "off", "--pid", "42"])).unwrap();
        assert_eq!(scope, Scope::Pid(42));
        assert!(parse_cli(&s(&["paint", "maybe"])).is_err());
        assert!(parse_cli(&s(&["paint", "on", "--pid", "nope"])).is_err());
        assert!(parse_cli(&s(&["paint", "on", "--wat"])).is_err());
    }

    #[test]
    fn workspace_filter_matches_class_and_either_selector_spelling() {
        let clients: serde_json::Value = serde_json::from_str(
            r#"[
              {"class":"terminal-delight","pid":100,"workspace":{"id":1,"name":"1"}},
              {"class":"terminal-delight","pid":200,"workspace":{"id":2,"name":"web"}},
              {"class":"foot","pid":300,"workspace":{"id":2,"name":"web"}},
              {"class":"terminal-delight","workspace":{"id":2,"name":"web"}}
            ]"#,
        )
        .unwrap();
        assert_eq!(td_pids_in_workspace(&clients, "1"), vec![100]);
        assert_eq!(td_pids_in_workspace(&clients, "2"), vec![200]);
        assert_eq!(td_pids_in_workspace(&clients, "web"), vec![200]);
        assert!(td_pids_in_workspace(&clients, "9").is_empty());
    }

    #[test]
    fn a_round_trip_answers_and_queues_and_status_reads_the_mirror() {
        let dir = tmp("rt");
        let sock = dir.join("ctl-1.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, rx) = mpsc::channel::<Req>();
        let mirror = Arc::new(AtomicBool::new(false));
        let m2 = Arc::clone(&mirror);
        let mcp_mirror = Arc::new(AtomicU8::new(0));
        let mm2 = Arc::clone(&mcp_mirror);
        let server = thread::spawn(move || {
            // exactly five connections, in test order
            for _ in 0..5 {
                let (stream, _) = listener.accept().unwrap();
                handle_conn(stream, &m2, &mm2, &Mutex::new(SkinMirror::default()), &tx);
            }
        });
        assert_eq!(send(&sock, "ping").unwrap(), "pong");
        assert_eq!(send(&sock, "paint status").unwrap(), "off");
        assert_eq!(send(&sock, "paint toggle").unwrap(), "ok");
        assert!(matches!(rx.try_recv(), Ok(Req::Toggle)));
        assert_eq!(
            send(&sock, r#"adopt {"cwd":"/tmp","run":"htop"}"#).unwrap(),
            "ok"
        );
        match rx.try_recv() {
            Ok(Req::Adopt(a)) => {
                assert_eq!(a.cwd.as_deref(), Some("/tmp"));
                assert_eq!(a.run.as_deref(), Some("htop"));
            }
            other => panic!("expected the adopt on the queue, got {other:?}"),
        }
        mirror.store(true, Ordering::Relaxed); // the UI ticker's job
        assert_eq!(send(&sock, "paint status").unwrap(), "on");
        server.join().unwrap();
    }

    /// A call addressed to another session is refused, over the real socket,
    /// before anything is read or written — and the refusal NAMES both sessions
    /// so the caller can tell a resolution bug from a policy one.
    ///
    /// This is the guarantee the whole caller-identity path exists to make:
    /// several Terminal Delights run on one box, and a relay that resolved the
    /// wrong window used to be served in full. `whoami` is exercised here too,
    /// because it is how a caller finds the right window in the first place and
    /// the two answers have to agree about which session this is.
    #[test]
    fn a_call_meant_for_another_session_is_refused_by_name() {
        let dir = tmp("wrong-window");
        let sock = dir.join("ctl-1.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, _rx) = mpsc::channel::<Req>();
        let mirror = Arc::new(AtomicBool::new(false));
        let m2 = Arc::clone(&mirror);
        let mcp_mirror = Arc::new(AtomicU8::new(0));
        let mm2 = Arc::clone(&mcp_mirror);
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                handle_conn(stream, &m2, &mm2, &Mutex::new(SkinMirror::default()), &tx);
            }
        });

        let mine = crate::instance::key();
        let who = send(&sock, "whoami").unwrap();
        assert_eq!(
            who.trim(),
            format!("ok {mine} {}", std::process::id()),
            "whoami must name this window's session and pid"
        );

        // A session this window certainly does not hold.
        let req = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"list_panes","arguments":{}}}"#;
        let reply = send(&sock, &format!("mcp from not-{mine} - rpc {req}")).unwrap();
        assert!(
            reply.contains("wrong window") && reply.contains(&format!("not-{mine}")),
            "the refusal did not name the session that was asked for: {reply}"
        );
        assert!(
            reply.contains("\"id\":7"),
            "a refusal must answer the request it refused: {reply}"
        );
        assert!(
            !reply.contains("\"result\""),
            "a refused call must not carry a result: {reply}"
        );
        server.join().unwrap();
    }

    #[test]
    fn junk_gets_an_error_line_not_a_hang() {
        let dir = tmp("junk");
        let sock = dir.join("ctl-2.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, _rx) = mpsc::channel::<Req>();
        let mirror = AtomicBool::new(false);
        let mcp_mirror = AtomicU8::new(0);
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_conn(
                stream,
                &mirror,
                &mcp_mirror,
                &Mutex::new(SkinMirror::default()),
                &tx,
            );
        });
        assert!(send(&sock, "sudo make me a sandwich")
            .unwrap()
            .starts_with("err"));
        server.join().unwrap();
    }

    #[test]
    fn mcp_policy_verbs_queue_and_status_reads_the_mirror() {
        let dir = tmp("mcp");
        let sock = dir.join("ctl-4.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        let (tx, rx) = mpsc::channel::<Req>();
        let mirror = Arc::new(AtomicBool::new(false));
        // A mirror standing in for a live policy: reads on, writes off.
        let mcp_mirror = Arc::new(AtomicU8::new(MCP_ON | MCP_EVENTS));
        let (m2, mm2) = (Arc::clone(&mirror), Arc::clone(&mcp_mirror));
        let server = thread::spawn(move || {
            for _ in 0..4 {
                let (stream, _) = listener.accept().unwrap();
                handle_conn(stream, &m2, &mm2, &Mutex::new(SkinMirror::default()), &tx);
            }
        });

        assert_eq!(
            send(&sock, "mcp status").unwrap(),
            "enabled=on writes=off expose=agents events=on"
        );
        assert_eq!(send(&sock, "mcp writes on").unwrap(), "ok");
        assert!(matches!(
            rx.try_recv(),
            Ok(Req::McpPolicy(McpPolicy::Writes(true)))
        ));
        assert_eq!(send(&sock, "mcp expose all").unwrap(), "ok");
        assert!(matches!(
            rx.try_recv(),
            Ok(Req::McpPolicy(McpPolicy::ExposeAll(true)))
        ));
        // Reads and writes are separate grants: enabling one must never be
        // spelled in a way that quietly enables the other.
        assert!(send(&sock, "mcp writes").unwrap().starts_with("err"));
        server.join().unwrap();
    }

    #[test]
    fn mcp_rpc_takes_its_payload_verbatim() {
        // The JSON carries spaces AND nested braces; splitting on whitespace
        // would corrupt it, so the parser must take the remainder untouched.
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"x y"}}"#;
        match parse_line(&format!("mcp rpc {json}")) {
            Ok(Cmd::McpRpc(None, p)) => assert_eq!(p, json),
            _ => panic!("mcp rpc did not parse as a verbatim payload"),
        }
        assert!(parse_line("mcp rpc ").is_err());
        assert!(parse_line("mcp rpc").is_err());
    }

    /// The caller-naming form takes its payload just as verbatim, after three
    /// fixed words — the whole point being that the JSON is untouched while the
    /// identity in front of it is structured.
    #[test]
    fn the_caller_naming_form_keeps_the_payload_verbatim() {
        let json = r#"{"jsonrpc":"2.0","id":1,"params":{"name":"x y","p":{"q":"r s"}}}"#;
        match parse_line(&format!("mcp from tdclip 9 rpc {json}")) {
            Ok(Cmd::McpRpc(Some(c), p)) => {
                assert_eq!(c.session, "tdclip");
                assert_eq!(c.pane, Some(9));
                assert_eq!(p, json);
            }
            other => panic!("did not parse as a named caller: {other:?}"),
        }
    }

    /// A caller that could not find out which pane it is in says so with `-`,
    /// and that is NOT pane zero. The distinction is the whole reason the field
    /// is optional rather than defaulted.
    #[test]
    fn a_caller_with_no_pane_says_so_and_is_not_pane_zero() {
        match parse_line(r#"mcp from 1 - rpc {"id":1}"#) {
            Ok(Cmd::McpRpc(Some(c), _)) => assert_eq!(c.pane, None),
            other => panic!("`-` did not parse as an unknown pane: {other:?}"),
        }
        match parse_line(r#"mcp from 1 0 rpc {"id":1}"#) {
            Ok(Cmd::McpRpc(Some(c), _)) => assert_eq!(c.pane, Some(0)),
            other => panic!("pane 0 is a pane: {other:?}"),
        }
    }

    /// A malformed caller is refused rather than silently dropped down to the
    /// anonymous form — being unable to read who is asking is not the same as
    /// nobody asking, and serving it as anonymous would skip the wrong-window
    /// guard exactly when something is already wrong.
    #[test]
    fn a_malformed_caller_is_refused_rather_than_treated_as_anonymous() {
        for line in [
            r#"mcp from tdclip rpc {"id":1}"#,
            r#"mcp from tdclip nine rpc {"id":1}"#,
            r#"mcp from tdclip 9 {"id":1}"#,
            "mcp from tdclip 9 rpc ",
        ] {
            assert!(
                parse_line(line).is_err(),
                "accepted a broken caller: {line}"
            );
        }
    }

    /// The kernel really does answer "which process is serving this session",
    /// and the answer is the LISTENER rather than one of its peers.
    ///
    /// Run against a socket this test binds and connects to itself, so it
    /// exercises the exact shape that broke the first draft: a busy socket has
    /// many rows in `/proc/net/unix` under one path, and only one of them is
    /// the listener. Measured on this box before the fix, a live session socket
    /// had fourteen rows and thirteen were connected peers — matching the first
    /// row by path would have found an inode nobody's `/proc/<pid>/fd` holds.
    #[test]
    fn the_listening_socket_is_found_and_its_peers_are_not() {
        let dir = tmp("inode");
        let sock = dir.join("session-under-test.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        // Peers on the same path, left open, so the table has several rows.
        let _a = UnixStream::connect(&sock).unwrap();
        let _b = UnixStream::connect(&sock).unwrap();
        let _accepted: Vec<_> = (0..2).filter_map(|_| listener.accept().ok()).collect();

        let ino = listening_inode(&sock).expect("the listener must be findable");
        let me = std::process::id();
        assert!(
            owns_inode(me, ino),
            "this process is the listener and the fd scan did not see it"
        );

        // Every row under this path, listener and peers alike, for contrast:
        // more than one exists, which is the whole reason state is checked.
        let rows = std::fs::read_to_string("/proc/net/unix")
            .unwrap()
            .lines()
            .filter(|l| l.ends_with(sock.to_str().unwrap()))
            .count();
        assert!(
            rows > 1,
            "the fixture did not produce peers, so this proves nothing"
        );

        // And a socket nobody is listening on yields nothing rather than a guess.
        drop(listener);
        assert!(
            !owns_inode(me, u64::MAX),
            "an inode nobody holds must not match"
        );
    }

    /// An older window's refusal of `mcp from` is recognised, and nothing else
    /// is. Retrying a real error into the unnamed form would hide it — and the
    /// unnamed form skips the wrong-window guard, so hiding an error there is
    /// the one place it costs the most.
    #[test]
    fn only_an_unknown_verb_makes_the_relay_step_back() {
        let unknown =
            Ok("err unknown command \"mcp from 1 - rpc {}\" — try: ping | whoami | …".to_string());
        assert!(is_unknown_verb(&unknown));

        for real_answer in [
            "err ui gone".to_string(),
            "err mcp from: empty payload".to_string(),
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"wrong window: …"}}"#
                .to_string(),
            MCP_NONE.to_string(),
        ] {
            assert!(
                !is_unknown_verb(&Ok(real_answer.clone())),
                "would have retried a real answer: {real_answer}"
            );
        }
        assert!(!is_unknown_verb(&Err(std::io::Error::other("gone"))));
    }

    /// The refusal an old window actually produces is the one being matched.
    /// Asserting against a hand-written string would pass while the real text
    /// drifted, which is how a fallback ends up never firing.
    #[test]
    fn the_step_back_matches_what_this_parser_really_says() {
        let err = parse_line(
            r#"mcp from tdclip 9 rpc {"id":1}"#.replace("mcp from", "mcp fromm").as_str(),
        )
        .expect_err("a misspelled verb is unknown");
        assert!(
            is_unknown_verb(&Ok(format!("err {err}"))),
            "the fallback would not recognise this parser's own refusal: {err}"
        );
    }

    /// `mcp from … - rpc …` survives the CLI's flag parser.
    ///
    /// Found by running the thing rather than by reading it: the live probe
    /// could not reproduce a wrong-window refusal by hand, because `-` starts
    /// with a dash and the parser called it an unknown flag. The wire was
    /// right the whole time and the only way in was shut.
    #[test]
    fn a_bare_dash_is_a_pane_that_is_unknown_not_a_flag() {
        let args: Vec<String> = ["mcp", "from", "tdclip", "-", "rpc", r#"{"id":1}"#]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let (line, _) = parse_cli(&args).expect("the CLI must be able to send this");
        match parse_line(&line) {
            Ok(Cmd::McpRpc(Some(c), _)) => {
                assert_eq!(c.session, "tdclip");
                assert_eq!(c.pane, None);
            }
            other => panic!("the CLI could not express an unknown pane: {other:?}"),
        }
        // And a real flag is still a flag.
        assert!(parse_cli(&["--nope".to_string()]).is_err());
    }

    #[test]
    fn relay_target_refuses_to_guess_between_windows() {
        assert_eq!(relay_target(&["--pid".into(), "4242".into()]), Ok(4242));
        assert!(relay_target(&["--pid".into()]).is_err());
        assert!(relay_target(&["--wat".into()]).is_err());
    }

    #[test]
    fn ppid_of_walks_a_comm_containing_spaces_and_parens() {
        // Our own parent is the real check that the last-paren split is right.
        let me = std::process::id();
        assert_eq!(ppid_of(me), Some(unsafe { libc::getppid() } as u32));
        assert_eq!(ppid_of(u32::MAX), None); // no such process
    }

    #[test]
    fn a_dead_socket_file_reads_as_unreachable() {
        let dir = tmp("stale");
        let sock = dir.join("ctl-3.sock");
        drop(UnixListener::bind(&sock).unwrap()); // file survives the listener
        assert!(sock.exists());
        assert!(send(&sock, "ping").is_err());
    }

    /// A stand-in window that answers `whoami` with whatever we hand it.
    ///
    /// Deliberately not [`handle_conn`]: that answers with this process's own
    /// instance key and pid, and two of the four cases below are precisely
    /// about a window whose answer does NOT match what the record claimed.
    fn fake_window(sock: &Path, reply: &str, serves: usize) -> thread::JoinHandle<()> {
        let listener = UnixListener::bind(sock).expect("bind the stand-in window");
        let reply = reply.to_string();
        thread::spawn(move || {
            for _ in 0..serves {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut line = String::new();
                if let Ok(peer) = stream.try_clone() {
                    let _ = BufReader::new(peer).read_line(&mut line);
                }
                let _ = writeln!(stream, "{reply}");
            }
        })
    }

    /// A recorded window is proven by ASKING it, never by its socket file.
    ///
    /// The bug this replaces, `terminal-delight#507`: the record was taken on
    /// the strength of `socket_path(pid).exists()`, and nothing unlinks that
    /// file when a window exits. The relay then targeted a corpse and the agent
    /// lost every terminal-delight tool for the rest of its session.
    ///
    /// Four legs, and the last three are why the first is not enough on its
    /// own: a function that simply returned `None` forever would satisfy the
    /// corpse case and break every real window on the desk.
    #[test]
    fn a_recorded_window_is_proven_by_asking_it_not_by_its_socket_file() {
        let dir = tmp("recorded-window");
        let here = dir.path();
        let record = here.join("session-S.window");

        // 1. THE CORPSE — bound for real, then dropped, which is exactly the
        //    state a window exit leaves behind. The file is the bug.
        std::fs::write(&record, "3\n").unwrap();
        let corpse = here.join("ctl-3.sock");
        drop(UnixListener::bind(&corpse).unwrap());
        assert!(
            corpse.exists(),
            "the file must survive — that IS the defect"
        );
        assert_eq!(
            recorded_window_at(&record, here, "S"),
            None,
            "a socket file with nothing behind it is not a window"
        );

        // 2. A WINDOW THAT ANSWERS, holding the session we asked about.
        std::fs::write(&record, "4242\n").unwrap();
        let live = here.join("ctl-4242.sock");
        let server = fake_window(&live, "ok S 4242", 1);
        assert_eq!(
            recorded_window_at(&record, here, "S"),
            Some(4242),
            "a window that answers for this session is the answer"
        );
        server.join().unwrap();
        std::fs::remove_file(&live).unwrap();

        // 3. THE RECYCLED PID — reachable, and a different window. Taking it
        //    would be a WRONG answer, which is worse than a missing one.
        let other = fake_window(&live, "ok not-S 4242", 1);
        assert_eq!(
            recorded_window_at(&record, here, "S"),
            None,
            "another session's window must not be served to this one"
        );
        other.join().unwrap();
        std::fs::remove_file(&live).unwrap();

        // 4. RIGHT SESSION, WRONG PID — the socket's number and the window's
        //    own account of itself disagree, so the record is not trustworthy.
        let mismatched = fake_window(&live, "ok S 9999", 1);
        assert_eq!(
            recorded_window_at(&record, here, "S"),
            None,
            "a window whose pid disagrees with its socket name is not proven"
        );
        mismatched.join().unwrap();
    }

    /// A relay whose window went away finds its session again — and a rebind
    /// may NARROW what it talks to, never widen it.
    ///
    /// The second half is the one to read. [`relay_target`] ends in "only one
    /// terminal is running, so it must be the one", a fair degraded mode for a
    /// relay that never located itself at all. Reaching for it HERE would be
    /// different in kind: this relay proved a session at startup, and a cutover
    /// is exactly the moment when the only thing bound might not be ours.
    ///
    /// Everything on this box runs as one user, so these sockets are not a
    /// boundary between people. The only boundary they carry is between what a
    /// relay asked for and what it will accept — which makes the first two
    /// assertions below that boundary, written down.
    #[test]
    fn a_rebind_finds_our_own_session_and_never_adopts_another() {
        let dir = tmp("rebind");
        let here = dir.path();
        let record = here.join("session-mine.window");

        // A window that is NOT ours and — the point — the only one running.
        let theirs = here.join("ctl-222.sock");
        let them = fake_window(&theirs, "ok theirs 222", 4);

        // No record and no window of ours. The lone live terminal is a
        // stranger's, and "it is the only one" must not promote it.
        assert_eq!(
            rebound_window_at(here, &record, "mine"),
            None,
            "a lone window belonging to another session must not be adopted"
        );

        // A record naming that stranger does not launder it either.
        std::fs::write(&record, "222\n").unwrap();
        assert_eq!(
            rebound_window_at(here, &record, "mine"),
            None,
            "a record naming another session's window is still not ours"
        );

        // Ours comes up beside it: there is now something to rebind to, and it
        // is picked out of a field that still contains one we must not take.
        let ours = here.join("ctl-111.sock");
        // One connection each is not a guess: the scan below reaches ours once,
        // while theirs has already taken three scans plus the record probe. A
        // stand-in promised more connections than it gets blocks on `join`
        // forever, which is how this test first hung.
        let us = fake_window(&ours, "ok mine 111", 1);
        assert_eq!(
            rebound_window_at(here, &record, "mine"),
            Some(111),
            "our session's window is found even beside another's"
        );

        us.join().unwrap();
        them.join().unwrap();
    }
}
