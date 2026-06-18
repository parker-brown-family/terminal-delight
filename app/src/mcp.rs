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
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            expose: Expose::AgentsOnly,
            events: true,
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
    pub fn label(&self) -> &'static str {
        match self {
            Expose::AgentsOnly => "agents only",
            Expose::All => "all panes",
        }
    }

    /// Cycle the policy for the panel toggle.
    pub fn next(self) -> Self {
        match self {
            Expose::AgentsOnly => Expose::All,
            Expose::All => Expose::AgentsOnly,
        }
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
    /// Foreground process cwd (falls back to the shell's), if readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Resumable agent command, e.g. `claude --resume <uuid>` — the stable
    /// identity AND the pointer to where this agent's tool-call transcript
    /// lives on disk. `None` for non-agent panes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// Whether this pane would be exposed under the current policy.
    pub exposed: bool,
}

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

/// The live data one request is answered from — built fresh per request on the
/// gpui main thread: the operator policy plus the current pane snapshot.
pub struct Snapshot {
    pub config: McpConfig,
    pub panes: Vec<PaneInfo>,
}

impl Snapshot {
    /// Panes the policy currently permits a connected agent to see.
    fn exposed(&self) -> Vec<&PaneInfo> {
        self.panes.iter().filter(|p| p.exposed).collect()
    }

    /// A snapshot that exposes nothing — used to answer snapshot-independent
    /// methods (initialize / tools/list / ping …) without a main-thread
    /// round-trip, and as the safe fallback when the UI has gone away.
    pub fn empty() -> Self {
        Self {
            config: McpConfig::default(),
            panes: vec![],
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
    let req = parse_req(line)?;
    // A notification (no id) is fire-and-forget — never answer it, even on error.
    let id = req.id.clone()?;
    Some(match dispatch(&req, snap, &tail) {
        Ok(result) => encode_ok(id, result),
        Err((code, msg)) => encode_err(id, code, msg),
    })
}

fn dispatch<F>(req: &Req, snap: &Snapshot, tail: &F) -> Result<Value, (i64, String)>
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
{
    match req.method.as_str() {
        "initialize" => Ok(initialize_result(&req.params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_defs() })),
        "tools/call" => tools_call(&req.params, snap, tail),
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
            "Read-only watch surface for terminal-delight's panes. `list_panes` \
             reports who is running where (mode, cwd, agent session); \
             `pane_events` tails an agent pane's own transcript for recent \
             tool calls. With logging enabled the server also pushes \
             `notifications/message` as agents appear, vanish, and call tools, \
             so you can react without polling. Nothing here can write to a terminal."
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
        }
    ])
}

fn tools_call<F>(params: &Value, snap: &Snapshot, tail: &F) -> Result<Value, (i64, String)>
where
    F: Fn(&PaneInfo, usize) -> Vec<ToolEvent>,
{
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((-32602, "tools/call requires a `name`".to_string()))?;
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match name {
        "list_panes" => Ok(list_panes(snap)),
        "pane_events" => Ok(pane_events(&args, snap, tail)),
        other => Err((-32602, format!("unknown tool: {other}"))),
    }
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
                format!(
                    "tab {} · {} · {} · {} · {}",
                    p.tab, p.title, p.mode, cwd, sess
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
        };
        let body = toml::to_string(&c).unwrap();
        // kebab-case on the enum: "all", not "All".
        assert!(body.contains("expose = \"all\""), "got: {body}");
        let back: McpConfig = toml::from_str(&body).unwrap();
        assert_eq!(c, back);
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
            cwd: Some("/work/x".into()),
            session: Some("claude --resume abc".into()),
            exposed,
        }
    }

    fn snap(enabled: bool, events: bool, panes: Vec<PaneInfo>) -> Snapshot {
        Snapshot {
            config: McpConfig {
                enabled,
                expose: Expose::AgentsOnly,
                events,
            },
            panes,
        }
    }

    fn no_tail(_: &PaneInfo, _: usize) -> Vec<ToolEvent> {
        vec![]
    }

    /// Parse a response line back to a Value for assertions.
    fn resp(line: &str) -> Value {
        serde_json::from_str(line).unwrap()
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
    fn tools_list_advertises_both_read_only_tools() {
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
            cwd: None,
            session: None,
            exposed: true,
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
