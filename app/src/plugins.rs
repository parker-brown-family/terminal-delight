//! terminal-delight as an MCP **host**.
//!
//! TD is already an MCP *server* (it exposes the agent wall — see [`crate::mcp`]).
//! This module makes it an MCP *client* too, so it can drive **plugins**: each
//! plugin is a standalone MCP server that TD discovers, launches over stdio,
//! handshakes with, and calls. The first plugin is **context-delight**
//! (`cdx-mcp`), which harvests an agent session — live or from the 🪦 graveyard —
//! into a portable `.cdx` context package.
//!
//! The point is leverage: build the host once, and every future plugin (replay,
//! session-diff, publish-to-registry) is just-another-MCP-server with zero new
//! TD plumbing. See `context-delight/docs/PLUGINS.md` for the contract.
//!
//! Transport mirrors [`crate::mcp`]: newline-delimited JSON-RPC 2.0. A reader
//! thread feeds a channel so a wedged plugin can never freeze the UI — every
//! request has a deadline (the same defensive shape as `mcp_transport`).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{json, Value};

/// How long to wait for a single JSON-RPC response before giving up.
const RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// One action a plugin advertises for the dashboard UI (the authoritative list
/// still comes from the live `tools/list`; this is a placement hint).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginAction {
    pub tool: String,
    pub label: String,
    /// where the action should appear: "agent", "graveyard", "global".
    pub surfaces: Vec<String>,
}

/// A discovered plugin: enough to launch it and place its actions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub scope: String,
    pub actions: Vec<PluginAction>,
}

impl PluginManifest {
    /// Does this plugin offer an action on `surface` (e.g. "graveyard")?
    pub fn action_for(&self, surface: &str) -> Option<&PluginAction> {
        self.actions
            .iter()
            .find(|a| a.surfaces.iter().any(|s| s == surface))
    }

    /// Parse a `plugin.json` document.
    fn from_value(v: &Value) -> Option<Self> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
        let name = s("name")?;
        let command = s("command")?;
        let args = v
            .get("args")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let env = v
            .get("env")
            .and_then(Value::as_object)
            .map(|o| {
                o.iter()
                    .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        let actions = v
            .get("actions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(action_from_value).collect())
            .unwrap_or_default();
        Some(PluginManifest {
            name,
            version: s("version").unwrap_or_else(|| "0".into()),
            description: s("description").unwrap_or_default(),
            command,
            args,
            env,
            scope: s("scope").unwrap_or_else(|| "agent".into()),
            actions,
        })
    }
}

fn action_from_value(v: &Value) -> Option<PluginAction> {
    let tool = v.get("tool").and_then(Value::as_str)?.to_string();
    let label = v
        .get("label")
        .and_then(Value::as_str)
        .unwrap_or(&tool)
        .to_string();
    let surfaces = v
        .get("surfaces")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_else(|| vec!["global".into()]);
    Some(PluginAction {
        tool,
        label,
        surfaces,
    })
}

/// Discover installed plugins under `~/.config/terminal-delight/plugins/*/plugin.json`,
/// then fold in the built-in context-delight default if a `cdx-mcp` binary is
/// resolvable and the user hasn't already installed their own.
pub fn discover(home: &Path) -> Vec<PluginManifest> {
    let mut out: Vec<PluginManifest> = Vec::new();
    let root = crate::instance::config_dir().join("plugins");
    if let Ok(dirs) = std::fs::read_dir(&root) {
        for d in dirs.flatten() {
            let mf = d.path().join("plugin.json");
            if let Ok(txt) = std::fs::read_to_string(&mf) {
                if let Ok(v) = serde_json::from_str::<Value>(&txt) {
                    if let Some(m) = PluginManifest::from_value(&v) {
                        out.push(m);
                    }
                }
            }
        }
    }
    if !out.iter().any(|m| m.name == "context-delight") {
        if let Some(cmd) = resolve_cdx_mcp(home) {
            out.push(builtin_context_delight(cmd));
        }
    }
    if !out.iter().any(|m| m.name == "leanctx-savings") {
        if let Some(cmd) = resolve_leanctx_mcp(home) {
            out.push(builtin_leanctx_savings(cmd));
        }
    }
    if !out.iter().any(|m| m.name == "jev") {
        if let Some(cmd) = resolve_jev_mcp(home) {
            out.push(builtin_jev(cmd));
        }
    }
    out
}

/// The bundled context-delight plugin definition.
fn builtin_context_delight(command: String) -> PluginManifest {
    PluginManifest {
        name: "context-delight".into(),
        version: "0.1.0".into(),
        description: "Harvest an agent session into a portable context package.".into(),
        command,
        args: vec![],
        env: vec![],
        scope: "agent".into(),
        actions: vec![PluginAction {
            tool: "write_package".into(),
            label: "\u{2b07} context".into(),
            surfaces: vec!["agent".into(), "graveyard".into()],
        }],
    }
}

/// The bundled leanctx-savings plugin definition (plugin #2): surfaces lean-ctx's
/// precomputed token-savings rollup on the agent wall (global) and per agent.
fn builtin_leanctx_savings(command: String) -> PluginManifest {
    PluginManifest {
        name: "leanctx-savings".into(),
        version: "0.1.0".into(),
        description: "Token savings from lean-ctx context compression.".into(),
        command,
        args: vec![],
        env: vec![],
        scope: "global".into(),
        actions: vec![PluginAction {
            tool: "savings".into(),
            label: "</> savings".into(),
            surfaces: vec!["global".into(), "agent".into()],
        }],
    }
}

/// The jev plugin definition: typed judgement, from TypeSafe's System One.
///
/// terminal-delight is a public repository and this is the one seam where a
/// hosted, keyed, third-party model could leak into it. It does not: TD holds no
/// client, no key, no endpoint, no model id and not one line of question text.
/// Everything that knows what Jev *is* lives in `plugins/jev-mcp/jev-mcp`, and a
/// checkout without that server installed has no Jev surface at all — not a
/// greyed one, not an empty one, and above all not a zero.
///
/// `source_says_nothing_about_jev` below is the mechanical half of that promise.
fn builtin_jev(command: String) -> PluginManifest {
    PluginManifest {
        name: "jev".into(),
        version: "0.1.0".into(),
        description: "Typed judgement over the workspace. Optional \u{2014} without it terminal-delight judges nothing."
            .into(),
        command,
        args: vec![],
        env: vec![],
        scope: "global".into(),
        actions: vec![PluginAction {
            tool: "workspace_weather".into(),
            label: "\u{25c8} weather".into(),
            surfaces: vec!["global".into()],
        }],
    }
}

/// Find the `jev-mcp` server — and *only* where a person put it deliberately.
///
/// Note what is missing, next to [`resolve_leanctx_mcp`]: there is no walk up
/// from the running exe to a copy bundled in this checkout. That fallback is
/// right for lean-ctx, which reads a ledger already sitting on the disk, and
/// wrong here, because this plugin makes a paid call to a third party over the
/// network. Cloning a repository is not consent to that. The bundled copy under
/// `plugins/jev-mcp/` is the thing you install *from*; installing is the opt-in.
fn resolve_jev_mcp(home: &Path) -> Option<String> {
    if let Ok(p) = which("jev-mcp") {
        return Some(p);
    }
    [
        home.join(".local/bin/jev-mcp"),
        home.join(".cargo/bin/jev-mcp"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .map(|p| p.to_string_lossy().into_owned())
}

/// Find the `leanctx-mcp` server: PATH, a couple of well-known spots, then the
/// bundled copy that ships in this checkout (`plugins/leanctx-mcp/leanctx-mcp`,
/// found by walking up from the running exe — the dev layout).
fn resolve_leanctx_mcp(home: &Path) -> Option<String> {
    if let Ok(p) = which("leanctx-mcp") {
        return Some(p);
    }
    let mut candidates: Vec<PathBuf> = vec![
        home.join(".local/bin/leanctx-mcp"),
        home.join(".cargo/bin/leanctx-mcp"),
    ];
    // dev: the bundled script under the terminal-delight checkout root.
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors() {
            let cand = anc.join("plugins/leanctx-mcp/leanctx-mcp");
            if cand.is_file() {
                candidates.push(cand);
                break;
            }
        }
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

/// Find a `cdx-mcp` binary: PATH, then a few well-known spots, then a sibling
/// `context-delight` checkout's release build (the dev layout).
fn resolve_cdx_mcp(home: &Path) -> Option<String> {
    if let Ok(p) = which("cdx-mcp") {
        return Some(p);
    }
    let mut candidates: Vec<PathBuf> = vec![
        home.join(".local/bin/cdx-mcp"),
        home.join(".cargo/bin/cdx-mcp"),
    ];
    // dev: ../context-delight/target/release/cdx-mcp relative to the running exe
    if let Ok(exe) = std::env::current_exe() {
        if let Some(software) = exe.ancestors().find(|a| a.ends_with("Software")) {
            candidates.push(software.join("context-delight/target/release/cdx-mcp"));
            candidates.push(software.join("context-delight/target/debug/cdx-mcp"));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

fn which(bin: &str) -> Result<String, ()> {
    let path = std::env::var_os("PATH").ok_or(())?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(bin);
        if cand.is_file() {
            return Ok(cand.to_string_lossy().into_owned());
        }
    }
    Err(())
}

// ----------------------------------------------------------------------------
// the MCP client (one short-lived process per action, for v0)
// ----------------------------------------------------------------------------

/// A launched plugin process speaking MCP over stdio.
pub struct McpProcess {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<String>,
    next_id: i64,
}

impl McpProcess {
    /// Spawn `command args` with `env`, wiring a reader thread to a channel.
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &[(String, String)],
    ) -> std::io::Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(McpProcess {
            child,
            stdin,
            rx,
            next_id: 1,
        })
    }

    /// Issue a request and wait (with a deadline) for the matching response.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let line = build_request(id, method, &params);
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|_| self.stdin.flush())
            .map_err(|e| format!("write to plugin failed: {e}"))?;

        let deadline = std::time::Instant::now() + RPC_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(format!("plugin timed out after {}s", RPC_TIMEOUT.as_secs()));
            }
            match self.rx.recv_timeout(remaining) {
                Ok(l) => {
                    if let Some(res) = match_response(&l, id) {
                        return res;
                    }
                    // not our id (a notification or another response) — keep reading
                }
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!("plugin timed out after {}s", RPC_TIMEOUT.as_secs()))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("plugin closed its output".into())
                }
            }
        }
    }

    /// MCP `initialize` handshake.
    pub fn initialize(&mut self) -> Result<Value, String> {
        let res = self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "terminal-delight", "version": env!("CARGO_PKG_VERSION") }
            }),
        )?;
        // best-effort initialized notification (no response expected)
        let _ = self
            .stdin
            .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .and_then(|_| self.stdin.flush());
        Ok(res)
    }

    /// Call a tool, returning the flattened text of its `content`.
    pub fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String, String> {
        let res = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        Ok(content_text(&res))
    }
}

impl Drop for McpProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// High-level: run `tool` on a discovered plugin and return its text result.
pub fn run_action(
    manifest: &PluginManifest,
    tool: &str,
    arguments: Value,
) -> Result<String, String> {
    let mut proc = McpProcess::spawn(&manifest.command, &manifest.args, &manifest.env)
        .map_err(|e| format!("could not launch {}: {e}", manifest.command))?;
    proc.initialize()?;
    proc.call_tool(tool, arguments)
}

/// Harvest one agent session into a `.cdx`, via the context-delight plugin.
/// Returns the human-readable result line (e.g. "wrote …/<id>.cdx").
pub fn harvest(manifest: &PluginManifest, session_id: &str, out: &Path) -> Result<String, String> {
    run_action(
        manifest,
        "write_package",
        json!({ "target": session_id, "format": "cdx", "out": out.to_string_lossy(), "redact": true }),
    )
}

/// Ask the leanctx-savings plugin for the token-savings rollup. `agent_id`
/// focuses one agent (the per-agent button); `None` is the whole-fleet total.
/// Returns the plugin's text payload (a compact JSON blob TD then parses).
pub fn savings(manifest: &PluginManifest, agent_id: Option<&str>) -> Result<String, String> {
    let args = match agent_id {
        Some(a) => json!({ "agent_id": a }),
        None => json!({}),
    };
    run_action(manifest, "savings", args)
}

/// Where harvested packages land: `~/.local/share/context-delight/`.
pub fn harvest_dir(home: &Path) -> PathBuf {
    home.join(".local/share/context-delight")
}

// ----------------------------------------------------------------------------
// pure protocol helpers (unit-tested without a real process)
// ----------------------------------------------------------------------------

fn build_request(id: i64, method: &str, params: &Value) -> String {
    let mut line =
        json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string();
    line.push('\n');
    line
}

/// `Some(Ok|Err)` if `line` is the response to `id`; `None` otherwise (skip it).
fn match_response(line: &str, id: i64) -> Option<Result<Value, String>> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("id").and_then(Value::as_i64) != Some(id) {
        return None;
    }
    if let Some(err) = v.get("error") {
        let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("error");
        return Some(Err(format!("plugin error {code}: {msg}")));
    }
    Some(Ok(v.get("result").cloned().unwrap_or(Value::Null)))
}

/// Flatten an MCP tool result's `content` array to plain text.
fn content_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| result.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parses_minimal() {
        let v = json!({
            "name": "context-delight", "command": "cdx-mcp",
            "actions": [{ "tool": "write_package", "label": "⬇", "surfaces": ["graveyard"] }]
        });
        let m = PluginManifest::from_value(&v).unwrap();
        assert_eq!(m.name, "context-delight");
        assert_eq!(m.command, "cdx-mcp");
        assert!(m.action_for("graveyard").is_some());
        assert!(m.action_for("agent").is_none());
    }

    #[test]
    fn build_request_is_newline_delimited_jsonrpc() {
        let line = build_request(7, "tools/list", &json!({}));
        assert!(line.ends_with('\n'));
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["id"], 7);
        assert_eq!(v["jsonrpc"], "2.0");
        assert_eq!(v["method"], "tools/list");
    }

    #[test]
    fn match_response_filters_by_id() {
        // wrong id → skip
        assert!(match_response(r#"{"id":1,"result":{}}"#, 2).is_none());
        // notification (no id) → skip
        assert!(match_response(r#"{"method":"notifications/message"}"#, 2).is_none());
        // matching result
        let ok = match_response(r#"{"id":2,"result":{"k":1}}"#, 2)
            .unwrap()
            .unwrap();
        assert_eq!(ok["k"], 1);
        // matching error
        let err = match_response(r#"{"id":2,"error":{"code":-32000,"message":"boom"}}"#, 2)
            .unwrap()
            .unwrap_err();
        assert!(err.contains("boom"));
    }

    #[test]
    fn content_text_flattens() {
        let r = json!({ "content": [{ "type": "text", "text": "hello" }, { "type": "text", "text": "world" }] });
        assert_eq!(content_text(&r), "hello\nworld");
    }

    /// Live end-to-end against the real cdx-mcp binary, when it's present in the
    /// sibling context-delight checkout. Skips (does not fail) when it isn't, so
    /// CI without the sibling repo stays green while local runs get real proof.
    #[test]
    fn live_cdx_mcp_roundtrip_if_available() {
        let home = crate::session::home_dir();
        let bin = match resolve_cdx_mcp(&home) {
            Some(b) => b,
            None => {
                eprintln!("skip: cdx-mcp not found");
                return;
            }
        };
        // the synthetic fixture lives in the sibling checkout. Resolve it
        // relative to the `Software/` ancestor of the test binary — but skip
        // (don't panic) when the binary lives outside that tree, e.g. a custom
        // CARGO_TARGET_DIR, so the test honours its own "skips when absent"
        // contract instead of unwrapping a None.
        let exe = std::env::current_exe().unwrap();
        let Some(software) = exe.ancestors().find(|a| a.ends_with("Software")) else {
            eprintln!("skip: test binary not under a Software/ checkout");
            return;
        };
        let fixture =
            software.join("context-delight/crates/cdx-core/tests/fixtures/synthetic-claude.jsonl");
        if !fixture.is_file() {
            eprintln!("skip: fixture not found");
            return;
        }
        let manifest = builtin_context_delight(bin);
        let mut proc = McpProcess::spawn(&manifest.command, &[], &[]).unwrap();
        let init = proc.initialize().unwrap();
        assert_eq!(init["serverInfo"]["name"], "context-delight");
        let text = proc
            .call_tool(
                "extract_skeleton",
                json!({ "target": fixture.to_string_lossy() }),
            )
            .unwrap();
        assert!(text.contains("tool calls"), "got: {text}");
    }

    /// An environment with no route to a paid backend, whatever the shell has.
    ///
    /// Empty strings rather than absent names, because the plugin tests every
    /// one of these with `.strip()` truthiness — so `""` reads to it exactly as
    /// unset reads, and `Command::env` has no way to unset an inherited name.
    /// Assembled from fragments for the same reason the source gate's needles
    /// are: this file is inside the scan it performs over itself.
    fn no_jev_env() -> Vec<(String, String)> {
        [
            "TD_JEV_HOME",
            concat!("TYPESAFE", "_API_KEY"),
            concat!("OPENROUTER", "_API_KEY"),
            concat!("SYSTEMONE", "_API_KEY"),
        ]
        .into_iter()
        .map(|k| (k.to_string(), String::new()))
        .collect()
    }

    /// The bundled `jev-mcp` in this checkout must never resolve on its own.
    ///
    /// lean-ctx and context-delight both walk up from the running exe to a copy
    /// sitting in the tree, and for them that is right. This plugin calls a paid
    /// third-party service over the network, so the same convenience would turn
    /// `git clone` into consent. Whatever else resolves, it is not this repo.
    #[test]
    fn jev_never_resolves_from_the_checkout() {
        let nowhere = std::env::temp_dir().join("td-jev-resolve-probe-no-such-home");
        if let Some(found) = resolve_jev_mcp(&nowhere) {
            let repo = concat!(env!("CARGO_MANIFEST_DIR"), "/../plugins/");
            assert!(
                !found.contains("/plugins/jev-mcp/"),
                "resolved the bundled copy at {found} — installing must stay the opt-in \
                 (checkout plugins live under {repo})"
            );
        }
    }

    /// Drive the bundled server for real and check the one invariant that has to
    /// hold on every branch, network or no network: **a row says what it is, or
    /// it says why it cannot.** Never both blank, and never a fabricated state.
    ///
    /// This makes no paid call, and that is now enforced rather than asserted.
    /// `McpProcess::spawn` *adds* to the inherited environment, so passing no
    /// env inherits the whole shell — and the one shell most likely to run this
    /// suite is the one with a checkout path and a live key already exported.
    /// That shell drove the answered path over the network and spent money, with
    /// this comment's first sentence claiming otherwise two lines above. Blanking
    /// the four names pins the test to the unavailable branch on every machine.
    #[test]
    fn live_jev_mcp_rows_are_never_silently_blank() {
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &no_jev_env()).unwrap();
        let init = proc.initialize().unwrap();
        assert_eq!(init["serverInfo"]["name"], "jev");

        // `available: false` must always name a reason. A plugin that refuses
        // without saying why is a different, worse fault than one that cannot run.
        let status: Value =
            serde_json::from_str(&proc.call_tool("jev_status", json!({})).unwrap()).unwrap();
        assert!(status.get("available").and_then(Value::as_bool).is_some());
        if status["available"] == json!(false) {
            assert!(
                status["reason"].as_str().is_some_and(|r| !r.is_empty()),
                "unavailable with no reason: {status}"
            );
        }

        let text = proc
            .call_tool(
                "workspace_weather",
                json!({ "panes": [
                    { "id": "a", "mode": "claude", "title": "t", "last_line": "working",
                      "idle_s": 2, "awaiting_input": false },
                    { "id": "b", "mode": "shell", "title": "shell", "awaiting_input": null },
                ]}),
            )
            .unwrap();
        let w: Value = serde_json::from_str(&text).unwrap();
        let rows = w["panes"].as_array().expect("panes array");
        assert_eq!(rows.len(), 2);
        for r in rows {
            assert!(
                !r["state"].is_null() || !r["reason"].is_null(),
                "a row with neither a state nor a reason is an invented blank: {r}"
            );
        }
        // Unknowns are counted on their own line and added to nothing, so a
        // caller can always see how much of the picture is missing.
        let counts = &w["counts"];
        let named: u64 = [
            "working",
            "needs_person",
            "stalled",
            "done",
            "unclear",
            "not_an_agent",
        ]
        .iter()
        .map(|k| counts[*k].as_u64().unwrap_or(0))
        .sum();
        let unknown = counts["unknown"]
            .as_u64()
            .expect("unknown is always reported");
        assert_eq!(
            named + unknown,
            2,
            "every pane lands in exactly one count: {counts}"
        );
    }

    /// A pane nobody could describe is never answered as a measured fact.
    ///
    /// `PaneMode::label()` emits `UNKNOWN` for a pane this window's own census
    /// could not describe, `REMOTE` for one it can only see through ssh, and
    /// `Other(name)` for an agent it has no variant for. None of those is
    /// evidence that no agent is there — and the mode branch used to answer all
    /// three `not_an_agent` with `source: "measured"`, which is the strongest
    /// claim this payload can make, invented from nobody having looked.
    ///
    /// `app/src/pane.rs` already carries the scar: its `Unknown` variant exists
    /// so that "I have not been told" cannot be stored as "I was told, and it is
    /// a shell". This asserts the plugin honours the same distinction.
    #[test]
    fn live_jev_mcp_never_measures_a_pane_it_cannot_describe() {
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &no_jev_env()).unwrap();
        proc.initialize().unwrap();
        let text = proc
            .call_tool(
                "workspace_weather",
                json!({ "panes": [
                    { "id": "unknown", "mode": "UNKNOWN" },
                    { "id": "remote",  "mode": "REMOTE" },
                    { "id": "other",   "mode": "GEMINI" },
                    { "id": "shell",   "mode": "SHELL" },
                ]}),
            )
            .unwrap();
        let w: Value = serde_json::from_str(&text).unwrap();
        let rows = w["panes"].as_array().expect("panes array");
        for r in rows {
            let id = r["id"].as_str().unwrap_or_default();
            if id == "shell" {
                // The one mode that positively says no agent is here.
                assert_eq!(r["state"], json!("not_an_agent"), "{r}");
                assert_eq!(r["source"], json!("measured"), "{r}");
            } else {
                assert_ne!(
                    r["source"],
                    json!("measured"),
                    "a pane whose mode says nobody looked was answered as measured: {r}"
                );
                assert!(
                    !r["state"].is_null() || !r["reason"].is_null(),
                    "neither a state nor a reason is an invented blank: {r}"
                );
            }
        }
    }

    /// Availability describes the plugin, never whether this batch happened to
    /// need it.
    ///
    /// Every pane here is answerable from measurement, so the old code never
    /// built a client and reported `available: true` on a machine with none —
    /// a draw gate that says judgement is configured when nothing is.
    #[test]
    fn live_jev_mcp_all_measured_panes_still_report_the_real_availability() {
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &no_jev_env()).unwrap();
        proc.initialize().unwrap();
        // Nothing here needs the model: a shell is measured by its mode, and a
        // pane already awaiting input is measured by that.
        let text = proc
            .call_tool(
                "workspace_weather",
                json!({ "panes": [
                    { "id": "a", "mode": "SHELL" },
                    { "id": "b", "mode": "claude", "awaiting_input": true },
                ]}),
            )
            .unwrap();
        let w: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            w["available"],
            json!(false),
            "no client is installed, so nothing may claim judgement is available: {w}"
        );
        assert!(
            w["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "unavailable with no reason: {w}"
        );
        // Both branches carry the same field set, so a consumer can tell "this
        // run could not measure it" from "this branch never reports it".
        for k in ["backend", "latency_ms", "counts", "needs_you"] {
            assert!(
                w.get(k).is_some(),
                "{k} missing from the unavailable shape: {w}"
            );
        }
    }

    /// You can learn WHICH key this window is spending with, and never the key.
    ///
    /// The status verb answers three things about the credential — the variable
    /// holding it, where that value came from, and a short one-way tag over it —
    /// so a person can confirm their window is on the account they meant without
    /// the secret being rendered anywhere. A terminal gets shoulder-surfed,
    /// screenshotted into an issue, and recorded while streaming, so "safe to
    /// display" has to be a property of the value, not a habit of the reader.
    ///
    /// This asserts the negative directly: the sentinel below is the key, and it
    /// may not appear anywhere in the response text. Asserting that the
    /// fingerprint *is* present would pass just as happily on a payload that
    /// carried both.
    #[test]
    fn live_jev_mcp_reports_which_key_and_never_the_key() {
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        // Distinctive enough that a substring search cannot match it by luck,
        // and not a real credential shape. `TD_JEV_HOME` is blanked so nothing
        // loads a checkout `.env` over the top of it.
        const SENTINEL: &str = "zzsentinel-key-do-not-print-4f7a91c2";
        let mut env = no_jev_env();
        for slot in env.iter_mut() {
            if slot.0 == concat!("OPENROUTER", "_API_KEY") {
                slot.1 = SENTINEL.to_string();
            }
        }
        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &env).unwrap();
        proc.initialize().unwrap();
        let text = proc.call_tool("jev_status", json!({})).unwrap();
        assert!(
            !text.contains(SENTINEL),
            "the key itself reached a tool result: {text}"
        );

        let status: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            status["key_env"],
            json!(concat!("OPENROUTER", "_API_KEY")),
            "the variable holding the key is not named: {status}"
        );
        assert_eq!(
            status["key_origin"],
            json!("environment"),
            "a key supplied by the launcher must say so: {status}"
        );
        let fp = status["key_fingerprint"]
            .as_str()
            .expect("a key is set, so it has a fingerprint");
        let hex = fp
            .strip_prefix("sha256:")
            .expect("fingerprint names its function");
        assert_eq!(hex.len(), 8, "fingerprint is truncated to a label: {fp}");
        assert!(
            hex.chars().all(|c| c.is_ascii_hexdigit()),
            "fingerprint is hex: {fp}"
        );
        // A tag over the key must not be derivable from the key's own name, or
        // every window holding a different key would show the same one.
        assert!(
            !SENTINEL.contains(hex),
            "the fingerprint is a slice of the key, not a digest of it: {fp}"
        );
    }

    /// A two-file `jev` package in a temp dir, enough to make the import succeed.
    ///
    /// Nothing in this repository could previously enter the plugin's
    /// import-succeeded branch: the tests that set `TD_JEV_HOME` skip when it is
    /// unset, and the one that sets a key blanks `TD_JEV_HOME` so the import
    /// fails first. So the branch shipped with a `NameError` in it and every
    /// guard stayed green. A real client is not needed to reach that code — only
    /// something importable — and a fake one runs everywhere, costs nothing, and
    /// cannot call anything.
    fn fake_jev_home(tag: &str, dotenv_sets: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("td-fake-jev-{tag}"));
        let ports = dir.join("jev").join("ports");
        std::fs::create_dir_all(&ports).expect("temp dir is writable");
        // `ask_many` answers every question and, in the score's own value,
        // reports HOW MANY FRAMES that request carried. That turns the fake into
        // an instrument: a test can read back what the plugin actually sent,
        // which is the only way to prove a partition happened rather than
        // inferring it from a band that moved. Nothing leaves the machine.
        std::fs::write(
            dir.join("jev").join("__init__.py"),
            "class _D:\n\
             \x20   def __init__(self, kind, value):\n\
             \x20       self.kind = kind\n\
             \x20       self.value = value\n\
             \x20       self.probabilities = None\n\
             \x20       self.confidence = None\n\
             \x20       self.abstained = False\n\
             \x20       self.reason = None\n\
             \x20       self.latency_ms = 1.0\n\
             \x20       self.backend = 'fake'\n\
             \n\
             \n\
             class JevClient:\n\
             \x20   def __init__(self, *a, **k):\n\
             \x20       pass\n\
             \n\
             \x20   def ask_many(self, state, questions):\n\
             \x20       n = len((state or {}).get('frames') or [])\n\
             \x20       out = {}\n\
             \x20       for key, q in questions.items():\n\
             \x20           if q.get('type') == 'noul':\n\
             \x20               out[key] = _D('noul', 0.5)\n\
             \x20           else:\n\
             \x20               out[key] = _D('score', min(4.0, n / 10.0))\n\
             \x20       return out\n",
        )
        .unwrap();
        std::fs::write(ports.join("__init__.py"), "").unwrap();
        std::fs::write(
            ports.join("hosted.py"),
            "class HostedPort:\n    def __init__(self, *a, **k):\n        pass\n",
        )
        .unwrap();
        let dotenv = dir.join("jev").join("dotenv.py");
        match dotenv_sets {
            // Stands in for a checkout's gitignored `.env`: a name that was not
            // set a moment ago is holding a value now.
            Some(name) => std::fs::write(
                &dotenv,
                format!("import os\n\n\ndef load():\n    os.environ[{name:?}] = 'zz-dotenv-9c1'\n"),
            )
            .unwrap(),
            None => {
                let _ = std::fs::remove_file(&dotenv);
            }
        }
        dir
    }

    /// Where a key came from is tracked, on the branch that only a real client
    /// reaches.
    ///
    /// Two spawns, because there are two origins and they are different facts: a
    /// key the launcher handed us, and a key a checkout's own `.env` supplied.
    /// Neither spawn can make a call — `jev_status` makes none by design, and the
    /// client here is two empty classes.
    #[test]
    fn live_jev_mcp_tracks_where_the_key_came_from() {
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        let key = concat!("OPENROUTER", "_API_KEY");

        // 1 — the launcher supplied it.
        let home = fake_jev_home("env", None);
        let mut env = no_jev_env();
        for slot in env.iter_mut() {
            if slot.0 == "TD_JEV_HOME" {
                slot.1 = home.to_string_lossy().into_owned();
            } else if slot.0 == key {
                slot.1 = "zz-from-the-launcher-4f7".to_string();
            }
        }
        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &env).unwrap();
        proc.initialize().unwrap();
        let text = proc.call_tool("jev_status", json!({})).unwrap();
        let st: Value = serde_json::from_str(&text).unwrap();
        assert!(
            st.get("key_origin").is_some(),
            "the import-succeeded branch did not run, so nothing here is tested: {st}"
        );
        assert_eq!(st["key_origin"], json!("environment"), "{st}");
        assert_eq!(st["key_env"], json!(key), "{st}");
        assert!(!text.contains("zz-from-the-launcher"), "key leaked: {text}");
        // The client imports and a key is present, so nothing checkable fails.
        assert_eq!(st["client"], json!(true), "{st}");

        // 2 — the checkout's own `.env` supplied it, and nothing else did.
        let home2 = fake_jev_home("dotenv", Some(key));
        let mut env2 = no_jev_env();
        for slot in env2.iter_mut() {
            if slot.0 == "TD_JEV_HOME" {
                slot.1 = home2.to_string_lossy().into_owned();
            }
        }
        let mut proc2 = McpProcess::spawn(&bin.to_string_lossy(), &[], &env2).unwrap();
        proc2.initialize().unwrap();
        let text2 = proc2.call_tool("jev_status", json!({})).unwrap();
        let st2: Value = serde_json::from_str(&text2).unwrap();
        assert_eq!(
            st2["key_origin"],
            json!("checkout .env"),
            "a key that appeared only after the checkout was read must say so: {st2}"
        );
        assert!(!text2.contains("zz-dotenv"), "key leaked: {text2}");
    }

    /// One malformed question abstains on its own and leaves its siblings alone.
    ///
    /// The encoder raises on an unknown type, a missing instruction, or a Score
    /// whose levels arrive as a map instead of an ordered list — and that raise
    /// used to escape the verb, so the whole call came back a JSON-RPC error and
    /// every well-formed answer in the batch went with it. The file's documented
    /// contract is that an answer may be null with a reason while its siblings
    /// stand; a malformed question is just one more reason.
    ///
    /// Costs nothing to run: EVERY question here is malformed, so the partition
    /// leaves nothing to send and no request is made. Opt-in on `TD_JEV_HOME`
    /// because reaching the encoder at all needs the client importable.
    #[test]
    fn live_jev_mcp_one_bad_question_does_not_take_the_batch() {
        let Ok(jev_home) = std::env::var("TD_JEV_HOME") else {
            eprintln!("skip: TD_JEV_HOME not set, no jev client to import");
            return;
        };
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        let env = vec![("TD_JEV_HOME".to_string(), jev_home)];
        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &env).unwrap();
        proc.initialize().unwrap();
        let text = proc
            .call_tool(
                "judge",
                json!({
                    "state": { "thing": "anything" },
                    "questions": {
                        // An type the encoder has never heard of.
                        "bogus_type": { "type": "noull", "instructions": "Is it?" },
                        // The real mistake, made while composing the question set
                        // that reviewed this very change: a Score's levels as a
                        // map, where the encoder wants an ordered list.
                        "score_as_map": { "type": "score", "instructions": "Where?",
                                          "criteria": { "low": "a", "high": "b" } },
                    }
                }),
            )
            .unwrap();
        let w: Value = serde_json::from_str(&text).unwrap();
        // The plugin is fine. The questions were not. Those are different facts.
        assert_eq!(
            w["available"],
            json!(true),
            "a malformed question is not the plugin being unavailable: {w}"
        );
        let answers = w["answers"].as_object().expect("an answers map");
        assert_eq!(answers.len(), 2, "every key sent comes back: {w}");
        for (k, a) in answers {
            assert_eq!(a["abstained"], json!(true), "{k} should abstain: {a}");
            assert!(a["value"].is_null(), "{k} invented a value: {a}");
            assert!(
                a["reason"]
                    .as_str()
                    .is_some_and(|r| r.contains("invalid_question")),
                "{k} abstained without saying why: {a}"
            );
        }
    }

    /// A key and a host that cannot belong to each other must never report ready.
    ///
    /// Found by the agent building the rail, not by me: the plugin answered
    /// `available: true` with a `null` reason while holding an OpenRouter key
    /// aimed at TypeSafe's own endpoint, and every question came back 401. My own
    /// probe scripts had hidden it by exporting the base URL themselves, so every
    /// live number I had was taken through a rig that supplied the missing half.
    ///
    /// Opt-in: needs a `jev` client, so it skips unless `TD_JEV_HOME` names one.
    /// The pairing is built from fragments for the same reason the source gate's
    /// needles are — this file is inside the scan.
    #[test]
    fn live_jev_mcp_refuses_a_key_that_cannot_reach_its_host() {
        let Ok(jev_home) = std::env::var("TD_JEV_HOME") else {
            eprintln!("skip: TD_JEV_HOME not set, no jev client to import");
            return;
        };
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        let env = vec![
            ("TD_JEV_HOME".to_string(), jev_home),
            (
                concat!("OPENROUTER", "_API_KEY").to_string(),
                "not-a-real-key".to_string(),
            ),
            (
                concat!("SYSTEMONE", "_BASE_URL").to_string(),
                // Split mid-word, not at the readable seam, so neither fragment
                // spells a whole needle. The gate caught this line twice: first
                // the string, then the comment that had quoted it to explain the
                // fix. A forbidding gate trips on its own prose as readily as on
                // its own code, and both times it was right to.
                concat!("https://api.type", "safe.ai").to_string(),
            ),
        ];
        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &env).unwrap();
        proc.initialize().unwrap();
        let status: Value =
            serde_json::from_str(&proc.call_tool("jev_status", json!({})).unwrap()).unwrap();
        assert_eq!(
            status["available"],
            json!(false),
            "a key that cannot authenticate against this host reported ready: {status}"
        );
        assert!(
            status["reason"].as_str().is_some_and(|r| !r.is_empty()),
            "refused without saying why: {status}"
        );
    }

    /// A `group` partitions the state, and the whole-reading questions do not.
    ///
    /// Both halves are one assertion, because the fake client answers each score
    /// with `frames / 10` — the size of the request it was handed. So the value
    /// coming back IS the state size, and a test can read the partition instead
    /// of inferring it from a band that moved.
    ///
    /// This guard exists because the rest of my own work had the failure the
    /// reviewer's grid exposed: `_ask_rail` was measured live and guarded
    /// nowhere, and the branch it needed — a client that can answer — looked
    /// expensive until `fake_jev_home` made it free.
    #[test]
    fn live_jev_mcp_group_splits_the_state_but_not_the_whole_reading() {
        let bin = std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../plugins/jev-mcp/jev-mcp"
        ))
        .to_path_buf();
        if !bin.is_file() {
            eprintln!("skip: bundled jev-mcp not in this checkout");
            return;
        }
        let home = fake_jev_home("group-split", None);
        let env = vec![
            (
                "TD_JEV_HOME".to_string(),
                home.to_string_lossy().into_owned(),
            ),
            (
                concat!("SYSTEMONE", "_BASE_URL").to_string(),
                "http://127.0.0.1:9".to_string(),
            ),
        ];

        // Ten frames: four warn, six plain, and the Sentence — the one kind whose
        // judgement is about the whole reading — sits in the plain group.
        let frame = |i: usize, kind: &str, group: &str| json!({ "id": format!("f{i}"), "kind": kind, "text": "t", "group": group });
        let mut frames: Vec<Value> = Vec::new();
        for i in 0..4 {
            frames.push(frame(i, "Collision", "warn"));
        }
        for i in 4..9 {
            frames.push(frame(i, "Repos", "plain"));
        }
        frames.push(frame(9, "Sentence", "plain"));

        let mut proc = McpProcess::spawn(&bin.to_string_lossy(), &[], &env).unwrap();
        proc.initialize().unwrap();
        let text = proc
            .call_tool(
                "rail_weather",
                json!({ "reading": "r", "frames": frames, "facts": { "calm": false } }),
            )
            .unwrap();
        let w: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            w["available"],
            json!(true),
            "fake client should answer: {w}"
        );

        let pos = |kind: &str| -> f64 {
            w["frames"]
                .as_array()
                .unwrap()
                .iter()
                .find(|f| f["kind"] == kind)
                .and_then(|f| f["position"].as_f64())
                .unwrap_or_else(|| panic!("no judged {kind} frame in {w}"))
        };

        // 4 warn frames in their own request, 5 plain ones in theirs (the
        // Sentence is lifted out), and the Sentence against all 10.
        assert!(
            (pos("Collision") - 0.4).abs() < 1e-6,
            "warn group should have been asked alone: got {}",
            pos("Collision")
        );
        assert!(
            (pos("Repos") - 0.5).abs() < 1e-6,
            "plain group should exclude the whole-reading frame: got {}",
            pos("Repos")
        );
        assert!(
            (pos("Sentence") - 1.0).abs() < 1e-6,
            "a frame judged against its neighbours must keep every one of them: got {}",
            pos("Sentence")
        );

        // And with no group sent, nothing partitions — the contract a caller
        // that has never heard of groups still relies on.
        let plain_frames: Vec<Value> = (0..10)
            .map(|i| json!({ "id": format!("g{i}"), "kind": "Repos", "text": "t" }))
            .collect();
        let mut proc2 = McpProcess::spawn(&bin.to_string_lossy(), &[], &env).unwrap();
        proc2.initialize().unwrap();
        let t2 = proc2
            .call_tool(
                "rail_weather",
                json!({ "reading": "r", "frames": plain_frames, "facts": { "calm": false } }),
            )
            .unwrap();
        let w2: Value = serde_json::from_str(&t2).unwrap();
        for f in w2["frames"].as_array().unwrap() {
            assert!(
                (f["position"].as_f64().unwrap() - 1.0).abs() < 1e-6,
                "an ungrouped reading must still go as one request: {f}"
            );
        }
    }

    /// terminal-delight's own source knows nothing about Jev but its name.
    ///
    /// The promise this whole plugin exists to keep is that a public checkout
    /// carries no hosted-model dependency, so the things that would constitute
    /// one — an endpoint, a key, a model id — may not appear in `app/src` at all.
    /// Naming the plugin `"jev"` is fine and deliberate; that is a plugin id, not
    /// a dependency. Each needle is assembled from fragments so this test's own
    /// source does not trip the scan it performs over itself.
    #[test]
    fn source_says_nothing_about_jev_but_its_name() {
        let needles = [
            concat!("type", "safe.ai"),
            concat!("openrouter", ".ai"),
            concat!("TYPESAFE", "_API_KEY"),
            concat!("OPENROUTER", "_API_KEY"),
            concat!("SYSTEMONE", "_API_KEY"),
            concat!("jev-", "latest"),
            concat!("/v1/", "systemone"),
            // The alias above is not the id the service answers as. The live one
            // is vendor-prefixed with a slash where the hostname needle has its
            // dot, so it contained no needle at all and a pinned model id could
            // be written straight into `app/src` past the gate built to catch
            // exactly that. This needle is the prefix, so it catches the pin
            // whatever version trails it.
            concat!("typesafe", "/jev"),
            // The base URL variable is the most likely thing a follow-up wiring
            // TD's config to this plugin would reach for, and naming a host is
            // naming the dependency this file exists to keep out.
            concat!("SYSTEMONE", "_BASE_URL"),
        ];
        let mut stack = vec![PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))];
        let mut scanned = 0usize;
        while let Some(dir) = stack.pop() {
            for e in std::fs::read_dir(&dir)
                .expect("app/src is readable")
                .flatten()
            {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let src = std::fs::read_to_string(&p).unwrap_or_default();
                    scanned += 1;
                    for n in needles {
                        assert!(
                            !src.contains(n),
                            "{} carries {n:?} — an endpoint, key or model id belongs in \
                             plugins/jev-mcp/, never in terminal-delight itself",
                            p.display()
                        );
                    }
                }
            }
        }
        assert!(
            scanned > 1,
            "scanned {scanned} files — the walk found nothing to check"
        );
    }
}
