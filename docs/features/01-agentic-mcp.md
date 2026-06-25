# ⭐ Agentic / MCP — the read-only agent-monitoring surface

**This is the headline feature.** Terminal Delight exposes a Model Context Protocol
(MCP) server that lets an orchestrator agent **watch every agent pane** running
inside it — what each one is, what it's doing, how many tokens it's spent, and
whether it's blocked on a human — *without ever being able to write to a PTY.*

It's the difference between "I ran some agents and lost track" and "I can see my
whole fleet at a glance, from another agent."

## Why it matters

You can run a dozen coding agents (Claude Code, Codex) across tabs and splits. The
problem was never starting them — it's **knowing what they're all doing.** TD reads
each agent's **own on-disk transcript** (not a screen-scrape) and its status line,
and serves that as structured MCP data + a live push feed. An orchestrator can
monitor the fleet; you get the in-app wall. Read-only by design: TD observes, it
does not drive.

## What the MCP server exposes

| Tool / capability | What it does | Evidence | Opt-in |
|---|---|---|---|
| **`list_panes`** | Enumerate live panes: tab, title, foreground program (SHELL/CLAUDE/CODEX/REMOTE), pid, cwd, **durable session id**, exposed flag | `app/src/mcp.rs` `PaneInfo` | `TD_MCP` / UI toggle |
| **`pane_events`** (transcript tail) | Stream structured **tool-call events** from each agent's own JSONL transcript (`~/.claude/projects/<slug>/<id>.jsonl`, `~/.codex/sessions/**/rollout-*.jsonl`) | `app/src/mcp_tail.rs::transcript_for` | events toggle |
| **`get_pane_config`** | Read a pane's effective appearance: 10 grade channels (brightness/contrast/colour/text/bg/gamma/text-size/warp + crawl triplet), logo, menu-bar | `app/src/mcp.rs` `GradeReport` | read-only always |
| **`set_pane_config`** | *Optionally* mutate appearance remotely (dim, recolour, brand a pane with a logo) — **never a PTY write** | `ConfigPatch` PATCH shape | `TD_MCP_WRITE` only |
| **`search` / grep** | Substring-search every exposed pane's scrollback (lines-per-pane cap, 5s budget) | `mcp_transport::UiReq::Search` | read-only |
| **Push notifications** | `notifications/message` feed — agent state deltas pushed ~1×/sec; orchestrators don't poll | `mcp_transport.rs` notifier thread | — |

Protocol: JSON-RPC 2.0, MCP version `2025-06-18`, hand-rolled stdio transport
(reader / ticker / notifier / writer threads).

## The safety model (this is a selling point, not fine print)

- **Read-only by default.** The entire server is a `Snapshot` model — *there is no
  code path that writes to a PTY.* An agent watching the wall cannot execute
  anything. `set_pane_config` (behind `TD_MCP_WRITE`) only changes *appearance*.
- **Policy-gated exposure.** Default `Expose::AgentsOnly` — a plain root shell is
  never exposed; only agent panes are visible to the MCP client. Toggle to `All`
  explicitly. (`should_expose()` is unit-tested.)
- **Opt-in.** The server is **off** unless you enable it (`TD_MCP` or the robot
  panel toggle). Nothing leaves the box otherwise.
- **It only ever sees its own instance's panes.**

## How it resolves an agent (the durable-identity trick)

Panes are keyed by **session id, not pid** — so identity survives a crash or
restart within the same conversation. TD resolves the live session by reading the
agent process's *open file descriptor* for its transcript (`claude_session_from_fds`),
falling back to the cmdline `--resume` id, then the newest transcript for the cwd.
The result is `claude --resume <uuid>` / `codex resume <uuid>` — the exact command
to re-attach. (See `app/src/session.rs`; the idle-at-close edge is tracked in
issue #151.)

## Dynamic pane classification

Every frame, TD classifies each pane's foreground process — **SHELL / CLAUDE /
CODEX / REMOTE** (ssh/mosh/et/telnet) / named program — by matching `/proc/<pid>/comm`
and cmdline. That's what drives the program-coloured borders and the CLAUDE/CODEX
badges on the wall. `app/src/mcp.rs` `PaneMode::classify`.

## Try it

```bash
TD_MCP=1 terminal-delight        # enable the read-only MCP server
# point an orchestrator (or `mcp` client) at the stdio transport;
# call list_panes / pane_events to watch the fleet.
```

## Status

**Shipped.** This is the moat — the read-only stance, durable identity, and
transcript-as-source-of-truth are what make it trustworthy enough to leave on.
