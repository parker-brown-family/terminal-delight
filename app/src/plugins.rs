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
    let root = home.join(".config/terminal-delight/plugins");
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
        // the synthetic fixture lives in the sibling checkout
        let exe = std::env::current_exe().unwrap();
        let software = exe.ancestors().find(|a| a.ends_with("Software")).unwrap();
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
}
