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
}
