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

## ⭐ Subscription usage — the other half of the same budget

The `</>` card has two faces, switched by the chips in its header. **savings** is
lean-ctx's compression ledger: tokens TD's own plumbing never had to send.
**usage** is what the subscriptions actually charged for the tokens that did go —
one tab per plan, with the ceiling each is up against.

Per subscription the card draws the plan and its limits (a meter apiece, coloured
by pressure rather than by theme, with the time until each window resets), today's
tokens and prompts, the last seven days, and where the tokens went by model. A
prepaid plan reports a draining credit balance instead of resetting windows.

**TD collects none of this.** It reads one JSON record per subscription out of a
state directory, and that is the entire contract:

```
${XDG_STATE_HOME:-~/.local/state}/omarchy/agents/usage/<id>.json
${XDG_STATE_HOME:-~/.local/state}/terminal-delight/agents/usage/<id>.json
```

Both are read, TD's own last, and a record in both resolves to the newer
`updatedAt`. The shape is Omarchy's — its `omarchy-agent-usage-<agent>` collectors
publish it and its agents widget reads it — so **on an Omarchy box TD picks up
records somebody else is already writing and refreshing, for free**.

**This does not make Omarchy a dependency.** The collectors are standalone Python 3
stdlib scripts that read the agent's own files (`~/.claude/projects`, Codex session
files) and ask the vendor for the authoritative limits; nothing in them touches
Hyprland, the bar, or Omarchy's runtime. On plain Ubuntu, Arch or Fedora the reader
half works unchanged — what is missing is only the writer, which is why the writer
is a plugin rather than a build dependency. Adding a subscription never touches TD
either: publish a record under a new `id` and the card gains a tab.

A refresh is optional. TD runs `omarchy-agent-usage-update` if it is on `PATH`,
else `td-agent-usage update`, else it draws what is on disk and names the writer it
is missing. It only refreshes when the newest record is over five minutes old, and
always on a pool thread — asking Anthropic's usage endpoint takes seconds, and the
card is up on the frame of the click.

- Evidence: `app/src/usage.rs` (record contract, discovery, countdowns; 13 tests),
  `main.rs::render_usage_body`, `main.rs::open_usage` / `refresh_usage`.
- Demo: `TD_USAGE_DEMO` (fictional). Dev: `TD_USAGE_LIVE` (this machine's real
  records — never for capture).

## Built-in: context-delight harvest

If the `cdx-mcp` binary is on `PATH`, TD auto-registers
**[context-delight](https://github.com/parker-brown-family/context-delight)** as a
plugin — harvest a live session into a portable `.cdx` / lean-ctx package, right
from the wall. `plugins.rs::builtin_context_delight` / `resolve_cdx_mcp`.

## Status

**Shipped** (client host, discovery, both built-ins). Per-agent LeanCTX attribution
is the tracked follow-up.
