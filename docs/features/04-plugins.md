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

**This does not make Omarchy a dependency, and the binary carries the proof.**
The collectors are standalone Python 3 stdlib scripts that read the agent's own
files (`~/.claude/projects`, Codex session files) and ask the vendor for the
authoritative limits; nothing in them touches Hyprland, the bar, or Omarchy's
runtime. So **terminal-delight ships them**, byte-identical and MIT-attributed
(`app/src/vendor/`, compiled in with `include_str!`), and can write the records
itself:

```
terminal-delight agent-usage update
```

That unpacks the collectors to `~/.cache/terminal-delight/agent-usage/`, runs each
one, and publishes what it prints to
`${XDG_STATE_HOME:-~/.local/state}/terminal-delight/agents/usage/`. `list` names
the collectors the binary carries; `where` prints the directories the panel reads.
A collector that fails never fails the run — a machine signed in to Claude and not
to Codex is the normal case. `python3` is a **soft** runtime requirement: without
it the panel says so and still draws whatever is on disk.

Adding a subscription never touches TD either: publish a record under a new `id`
and the card gains a tab.

A refresh is optional and never blocks a frame. TD runs
`omarchy-agent-usage-update` if it is on `PATH` — it is wired to that box's own
per-agent enable/disable settings, so using it respects a subscription the user
turned off — else a packaged `td-agent-usage`, else this binary's own
`agent-usage update`. It fires only when the newest record is over five minutes
old, and always on a pool thread: asking Anthropic's usage endpoint takes seconds,
and the card is up on the frame of the click with whatever it already has.

- Evidence: `app/src/usage.rs` (record contract, discovery, countdowns, the
  compiled-in collectors and their runner; 16 tests), `app/src/vendor/README.md`,
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
