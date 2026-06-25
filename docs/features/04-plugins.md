# Plugins — the extension surface

Terminal Delight is both an MCP **server** (watched by orchestrators) *and* an MCP
**client** (it drives plugins). Plugins are stdio MCP processes that surface extra
data on the agent wall, the graveyard, or globally.

## Why it matters

The wall isn't a closed box. Anything that speaks MCP can light up a surface in TD —
token-savings ledgers, context harvesters, your own tooling — discovered from a
manifest, launched on demand, with a wedged plugin unable to freeze the UI.

## Features

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **MCP client host** | Discovers, launches, and JSON-RPC-handshakes plugins over stdio; separate from the server transport | `app/src/plugins.rs` | Shipped |
| **Discovery** | Scans `~/.config/terminal-delight/plugins/*/plugin.json` + built-in fallbacks | `discover(home)` | Shipped |
| **Manifest** | Each plugin declares name/version/description/command/args/env/scope + per-surface **actions** (agent / graveyard / global) | `PluginManifest`, `action_for(surface)` | Shipped |
| **Timeout + isolation** | 10 s per tool call; isolated stdio; reader thread so a hung plugin can't block paint | `RPC_TIMEOUT`, `PluginClient` | Shipped |

## ⭐ Built-in: LeanCTX token-savings → [leanctx.com](https://leanctx.com/)

The `</> LeanCTX` plugin shows, on the wall, the **token savings** that
[lean-ctx](https://leanctx.com/) achieved by compressing each agent's context — a
live, quantified "this is how much cheaper your agents got." lean-ctx **precomputes**
the savings; TD reads and displays them. It's the flagship plugin and the canonical
backlink to leanctx.com.

- Evidence: `plugins.rs::builtin_leanctx_savings` / `resolve_leanctx_mcp`.
- Known gap: per-agent attribution (the ledger currently keys `agent_id:"local"`);
  full per-agent cost breakdown is the next step.
- Demo: `TD_SAVINGS_DEMO`.

## Built-in: context-delight harvest

If the `cdx-mcp` binary is on `PATH`, TD auto-registers
**[context-delight](https://github.com/parker-brown-family/context-delight)** as a
plugin — harvest a live session into a portable `.cdx` / lean-ctx package, right
from the wall. `plugins.rs::builtin_context_delight` / `resolve_cdx_mcp`.

## Status

**Shipped** (client host, discovery, both built-ins). Per-agent LeanCTX attribution
is the tracked follow-up.
